use std::fs;

use serde_json::json;

use super::HostrunSession;

#[test]
fn tools_require_lazy_loads_function_modules_once() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            r#"
            const module = tools.require('demo', (module) => {
              ctx.demoLoads = (ctx.demoLoads ?? 0) + 1;
              module.exports = { loads: ctx.demoLoads, value: 'loaded' };
            });
            module;
            "#,
        )
        .expect("load module");

    assert_eq!(result.value, Some(json!({ "loads": 1, "value": "loaded" })));

    let result = session
        .eval("tools.require('demo').loads;")
        .expect("cached module");
    assert_eq!(result.value, Some(json!(1)));

    let result = session.eval("ctx.demoLoads;").expect("load count");
    assert_eq!(result.value, Some(json!(1)));
}

#[test]
fn tools_require_loads_file_modules_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let module_path = dir.path().join("helper.js");
    fs::write(
        &module_path,
        "ctx.fileLoads = (ctx.fileLoads ?? 0) + 1;\nmodule.exports = { answer: 42, loads: ctx.fileLoads };",
    )
    .expect("write module");
    let module_path = module_path.to_str().expect("utf-8 path");
    let session = HostrunSession::new_auto_approve().expect("session");

    let result = session
        .eval(&format!(
            "tools.require('file-demo', {}).answer;",
            json!(module_path)
        ))
        .expect("load file module");

    assert_eq!(result.value, Some(json!(42)));

    let result = session
        .eval("tools.require('file-demo').loads;")
        .expect("cached file module");
    assert_eq!(result.value, Some(json!(1)));
}

#[test]
fn tools_require_requires_loader_for_unknown_modules() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            r#"
            try {
              tools.require('missing');
            } catch (error) {
              error.message;
            }
            "#,
        )
        .expect("captured error");

    assert_eq!(
        result.value,
        Some(json!("tools.require module is not loaded: missing"))
    );
}

#[test]
fn tools_require_loads_builtin_sheetjs() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            r#"
            const XLSX = tools.require('sheetjs');
            const workbook = XLSX.utils.book_new();
            const sheet = XLSX.utils.aoa_to_sheet([
              ['Name', 'Value'],
              ['alpha', 42]
            ]);
            XLSX.utils.book_append_sheet(workbook, sheet, 'Data');
            const bytes = XLSX.write(workbook, { bookType: 'xlsx', type: 'array' });
            const parsed = XLSX.read(bytes, { type: 'array' });
            XLSX.utils.sheet_to_json(parsed.Sheets.Data, { header: 1 });
            "#,
        )
        .expect("sheetjs workbook round trip");

    assert_eq!(
        result.value,
        Some(json!([["Name", "Value"], ["alpha", 42]]))
    );
}

#[test]
fn tools_require_loads_builtin_xlsx_alias() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("typeof tools.require('xlsx').read;")
        .expect("xlsx alias");

    assert_eq!(result.value, Some(json!("function")));
}
