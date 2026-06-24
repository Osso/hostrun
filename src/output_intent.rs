use std::fs;
use std::path::Path;

use serde_json::Value;
use serde_json::json;

use crate::fs_capability::resolve_path;
use crate::session::HostrunSessionError;

const CAPTURE_LIMIT_BYTES: usize = 64 * 1024;

pub(crate) fn apply_output_intent(
    result: &mut serde_json::Map<String, Value>,
    name: &str,
    intent: Option<&Value>,
    bytes: &[u8],
    cwd: &Path,
) -> Result<(), HostrunSessionError> {
    let Some(intent) = intent else {
        return Ok(());
    };
    match intent
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("capture")
    {
        "capture" | "text" => capture_output(result, name, bytes),
        "lines" => capture_lines(result, name, bytes),
        "file" => write_output_file(result, name, intent, bytes, cwd),
        "tee" => tee_output(result, name, intent, bytes, cwd),
        other => Err(HostrunSessionError::Eval(format!(
            "unsupported {name} output intent: {other}"
        ))),
    }
}

fn capture_output(
    result: &mut serde_json::Map<String, Value>,
    name: &str,
    bytes: &[u8],
) -> Result<(), HostrunSessionError> {
    let captured = bounded_capture(bytes);
    result.insert(
        name.to_string(),
        Value::String(String::from_utf8_lossy(captured).to_string()),
    );
    insert_capture_metadata(result, name, bytes.len(), captured.len());
    Ok(())
}

fn capture_lines(
    result: &mut serde_json::Map<String, Value>,
    name: &str,
    bytes: &[u8],
) -> Result<(), HostrunSessionError> {
    let captured = bounded_capture(bytes);
    let text = String::from_utf8_lossy(captured);
    result.insert(name.to_string(), json!(text.lines().collect::<Vec<_>>()));
    insert_capture_metadata(result, name, bytes.len(), captured.len());
    Ok(())
}

fn write_output_file(
    result: &mut serde_json::Map<String, Value>,
    name: &str,
    intent: &Value,
    bytes: &[u8],
    cwd: &Path,
) -> Result<(), HostrunSessionError> {
    let path = resolve_path(cwd, field_as_string(intent, "path"));
    fs::write(&path, bytes).map_err(|error| {
        HostrunSessionError::Eval(format!(
            "failed to write {name} to {}: {error}",
            path.display()
        ))
    })?;
    result.insert(
        name.to_string(),
        json!({ "path": path, "bytes": bytes.len() }),
    );
    Ok(())
}

fn tee_output(
    result: &mut serde_json::Map<String, Value>,
    name: &str,
    intent: &Value,
    bytes: &[u8],
    cwd: &Path,
) -> Result<(), HostrunSessionError> {
    let path = resolve_path(cwd, field_as_string(intent, "path"));
    fs::write(&path, bytes).map_err(|error| {
        HostrunSessionError::Eval(format!(
            "failed to tee {name} to {}: {error}",
            path.display()
        ))
    })?;
    capture_output(result, name, bytes)?;
    result.insert(
        format!("{name}File"),
        json!({ "path": path, "bytes": bytes.len() }),
    );
    Ok(())
}

fn bounded_capture(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.len().min(CAPTURE_LIMIT_BYTES)]
}

fn insert_capture_metadata(
    result: &mut serde_json::Map<String, Value>,
    name: &str,
    bytes: usize,
    captured_bytes: usize,
) {
    result.insert(
        format!("{name}Meta"),
        json!({
            "bytes": bytes,
            "capturedBytes": captured_bytes,
            "truncated": captured_bytes < bytes
        }),
    );
}

fn field_as_string(args: &Value, field: &str) -> String {
    args.get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    use tempfile::tempdir;

    #[test]
    fn capture_and_lines_intents_record_metadata() {
        let cwd = tempdir().unwrap();
        let mut result = Map::new();

        apply_output_intent(
            &mut result,
            "stdout",
            Some(&json!({"type": "capture"})),
            b"one\ntwo\n",
            cwd.path(),
        )
        .unwrap();
        apply_output_intent(
            &mut result,
            "stderr",
            Some(&json!({"type": "lines"})),
            b"err\nwarn\n",
            cwd.path(),
        )
        .unwrap();

        assert_eq!(result["stdout"], json!("one\ntwo\n"));
        assert_eq!(result["stdoutMeta"]["bytes"], 8);
        assert_eq!(result["stderr"], json!(["err", "warn"]));
        assert_eq!(result["stderrMeta"]["truncated"], false);
    }

    #[test]
    fn missing_intent_leaves_result_unchanged() {
        let cwd = tempdir().unwrap();
        let mut result = Map::new();

        apply_output_intent(&mut result, "stdout", None, b"ignored", cwd.path()).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn file_and_tee_intents_write_resolved_paths() {
        let cwd = tempdir().unwrap();
        let mut result = Map::new();

        apply_output_intent(
            &mut result,
            "stdout",
            Some(&json!({"type": "file", "path": "out.txt"})),
            b"saved",
            cwd.path(),
        )
        .unwrap();
        apply_output_intent(
            &mut result,
            "stderr",
            Some(&json!({"type": "tee", "path": "err.txt"})),
            b"shown",
            cwd.path(),
        )
        .unwrap();

        assert_eq!(fs::read(cwd.path().join("out.txt")).unwrap(), b"saved");
        assert_eq!(fs::read(cwd.path().join("err.txt")).unwrap(), b"shown");
        assert_eq!(result["stdout"]["bytes"], 5);
        assert_eq!(result["stderr"], json!("shown"));
        assert_eq!(result["stderrFile"]["bytes"], 5);
    }

    #[test]
    fn unsupported_intent_reports_field_name() {
        let cwd = tempdir().unwrap();
        let mut result = Map::new();

        let err = apply_output_intent(
            &mut result,
            "stderr",
            Some(&json!({"type": "bad"})),
            b"",
            cwd.path(),
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("unsupported stderr output intent: bad")
        );
    }

    #[test]
    fn file_intent_reports_write_errors() {
        let cwd = tempdir().unwrap();
        let mut result = Map::new();

        let err = apply_output_intent(
            &mut result,
            "stdout",
            Some(&json!({"type": "file", "path": "missing/out.txt"})),
            b"saved",
            cwd.path(),
        )
        .unwrap_err();

        assert!(err.to_string().contains("failed to write stdout"));
    }
}
