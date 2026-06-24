use std::path::Path;
use std::path::PathBuf;

use serde_json::Map;
use serde_json::Value;
use serde_json::json;

use crate::fs_capability::resolve_path;
use crate::session::HostrunSessionError;

pub(crate) fn split_command_payload(args: Value) -> (Vec<Value>, Option<Value>) {
    match args {
        Value::Array(args) => (args, None),
        Value::Object(mut payload) if payload.contains_key("args") => {
            let cli_args = match payload.remove("args").unwrap_or(Value::Null) {
                Value::Array(args) => args,
                Value::Null => Vec::new(),
                other => vec![other],
            };
            if payload.is_empty() {
                (cli_args, None)
            } else {
                (cli_args, Some(Value::Object(payload)))
            }
        }
        Value::Null => (Vec::new(), None),
        other => (vec![other], None),
    }
}

pub(crate) fn command_args(program: &str, args: Vec<Value>, io: Option<Value>) -> Value {
    let mut payload = json!({
        "program": program,
        "args": args,
    });
    if let (Value::Object(payload), Some(Value::Object(io))) = (&mut payload, io) {
        payload.extend(io);
    }
    payload
}

pub(crate) fn payload_args(
    payload: &serde_json::Map<String, Value>,
) -> Result<Vec<String>, HostrunSessionError> {
    let values = payload
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    values.iter().map(arg_to_string).collect()
}

pub(crate) fn payload_cwd(
    payload: &serde_json::Map<String, Value>,
    session_cwd: &Path,
) -> Result<PathBuf, HostrunSessionError> {
    let Some(cwd) = payload.get("cwd") else {
        return Ok(session_cwd.to_path_buf());
    };
    let Some(cwd) = cwd.as_str() else {
        return Err(HostrunSessionError::Eval(
            "cli cwd must be a string".to_string(),
        ));
    };
    Ok(resolve_path(session_cwd, cwd))
}

pub(crate) fn payload_env(
    payload: &serde_json::Map<String, Value>,
) -> Result<Vec<(String, String)>, HostrunSessionError> {
    let Some(env) = payload.get("env") else {
        return Ok(Vec::new());
    };
    let Value::Object(env) = env else {
        return Err(HostrunSessionError::Eval(
            "cli env must be an object".to_string(),
        ));
    };
    env.iter()
        .map(|(key, value)| Ok((key.clone(), arg_to_string(value)?)))
        .collect()
}

pub(crate) fn redact_env_values(payload: &mut Value) {
    let Value::Object(payload) = payload else {
        return;
    };
    let Some(Value::Object(env)) = payload.get_mut("env") else {
        return;
    };
    let redacted = env
        .keys()
        .map(|key| (key.clone(), Value::String("[redacted]".to_string())))
        .collect::<Map<String, Value>>();
    *env = redacted;
}

fn arg_to_string(value: &Value) -> Result<String, HostrunSessionError> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null => Ok(String::new()),
        Value::Array(_) | Value::Object(_) => Err(HostrunSessionError::Eval(format!(
            "cli arguments must be scalar argv values, got {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn split_payload_handles_array_null_scalar_and_io_object() {
        assert_eq!(
            split_command_payload(json!(["one", 2])).0,
            vec![json!("one"), json!(2)]
        );
        assert_eq!(split_command_payload(Value::Null), (Vec::new(), None));
        assert_eq!(
            split_command_payload(json!("--version")),
            (vec![json!("--version")], None)
        );

        let (args, io) = split_command_payload(json!({
            "args": "--json",
            "stdout": { "type": "capture" }
        }));

        assert_eq!(args, vec![json!("--json")]);
        assert_eq!(io, Some(json!({ "stdout": { "type": "capture" } })));
    }

    #[test]
    fn command_args_merges_object_io_only() {
        assert_eq!(
            command_args("git", vec![json!("status")], Some(json!({"cwd": "/repo"}))),
            json!({"program": "git", "args": ["status"], "cwd": "/repo"})
        );
        assert_eq!(
            command_args("git", vec![], Some(json!("ignored"))),
            json!({"program": "git", "args": []})
        );
    }

    #[test]
    fn payload_args_accepts_scalar_values_and_rejects_nested_values() {
        let payload = json!({"args": ["a", 2, true, null]});
        let args = payload_args(payload.as_object().unwrap()).unwrap();
        assert_eq!(args, vec!["a", "2", "true", ""]);

        let payload = json!({"args": [{"bad": true}]});
        let err = payload_args(payload.as_object().unwrap()).unwrap_err();
        assert!(err.to_string().contains("scalar argv values"));
    }

    #[test]
    fn payload_cwd_defaults_and_validates_string_values() {
        let session_cwd = Path::new("/tmp/session");
        let empty = serde_json::Map::new();
        assert_eq!(payload_cwd(&empty, session_cwd).unwrap(), session_cwd);

        let payload = json!({"cwd": "child"});
        assert_eq!(
            payload_cwd(payload.as_object().unwrap(), session_cwd).unwrap(),
            Path::new("/tmp/session/child")
        );

        let payload = json!({"cwd": 42});
        let err = payload_cwd(payload.as_object().unwrap(), session_cwd).unwrap_err();
        assert!(err.to_string().contains("cwd must be a string"));
    }

    #[test]
    fn payload_env_defaults_redacts_and_rejects_non_objects() {
        let empty = serde_json::Map::new();
        assert!(payload_env(&empty).unwrap().is_empty());

        let payload = json!({"env": {"TOKEN": "secret", "RETRIES": 3}});
        let env = payload_env(payload.as_object().unwrap()).unwrap();
        assert_eq!(
            env,
            vec![
                ("RETRIES".into(), "3".into()),
                ("TOKEN".into(), "secret".into())
            ]
        );

        let mut payload = payload;
        redact_env_values(&mut payload);
        assert_eq!(
            payload["env"],
            json!({"RETRIES": "[redacted]", "TOKEN": "[redacted]"})
        );

        let payload = json!({"env": ["TOKEN=secret"]});
        let err = payload_env(payload.as_object().unwrap()).unwrap_err();
        assert!(err.to_string().contains("env must be an object"));

        let mut scalar = json!("ignored");
        redact_env_values(&mut scalar);
        assert_eq!(scalar, json!("ignored"));
    }
}
