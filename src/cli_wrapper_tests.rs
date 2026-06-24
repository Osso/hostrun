use serde_json::json;
use tempfile::TempDir;

use super::HostrunSession;

#[test]
fn cli_program_proxy_returns_lazy_command_builder() {
    let session = HostrunSession::new().expect("session");

    let result = session.eval("cli.dmidecode();").expect("builder");

    assert_eq!(result.result_type, "completed");
    assert_eq!(
        result.value,
        Some(json!({
            "program": "dmidecode",
            "args": []
        }))
    );
}

#[test]
fn cli_command_builder_run_returns_command_approval() {
    let session = HostrunSession::new().expect("session");

    let result = session.eval("cli.dmidecode().run();").expect("approval");

    assert_eq!(result.result_type, "needs_approval");
    assert_dmidecode_approval(result.approval.expect("approval"));
}

#[test]
fn run_program_proxy_executes_without_capture() {
    let session = HostrunSession::new().expect("session");

    let result = session.eval("run.dmidecode();").expect("approval");

    assert_eq!(result.result_type, "needs_approval");
    assert_dmidecode_approval(result.approval.expect("approval"));
}

#[test]
fn sudo_program_proxy_uses_sudo_binary_literally() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("cli.sudo('dmidecode', '-t', 'system').run();")
        .expect("approval");

    assert_eq!(result.result_type, "needs_approval");
    let approval = result.approval.expect("approval");
    assert_eq!(approval.id, "cli.sudo:sudo dmidecode -t system");
    assert_eq!(approval.tool, "cli.sudo");
    assert_eq!(approval.summary, "Run sudo dmidecode -t system");
    assert_eq!(
        approval.args,
        json!({
            "program": "sudo",
            "args": ["dmidecode", "-t", "system"]
        })
    );
}

#[test]
fn tools_sudo_uses_authsudo() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.sudo(cli.dmidecode('-t', 'system')).run();")
        .expect("approval");

    assert_eq!(result.result_type, "needs_approval");
    let approval = result.approval.expect("approval");
    assert_eq!(approval.id, "cli.authsudo:authsudo dmidecode -t system");
    assert_eq!(approval.tool, "cli.authsudo");
    assert_eq!(
        approval.summary,
        "Run authsudo dmidecode -t system (stdout text, stderr text)"
    );
    assert_eq!(
        approval.args,
        json!({
            "program": "authsudo",
            "args": ["dmidecode", "-t", "system"],
            "stdout": { "type": "text" },
            "stderr": { "type": "text" }
        })
    );
}

#[test]
fn tools_sudo_captures_stdout_and_stderr_by_default() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.sudo(cli.ls()).run();")
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "authsudo",
            "args": ["ls"],
            "stdout": { "type": "text" },
            "stderr": { "type": "text" }
        })
    );
}

#[test]
fn tools_sudo_preserves_command_builder_io_overrides() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.sudo(cli.dmidecode('-t', 'system').stdout.capture()).run();")
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "authsudo",
            "args": ["dmidecode", "-t", "system"],
            "stdout": { "type": "capture" },
            "stderr": { "type": "text" }
        })
    );
}

#[test]
fn tools_tmux_open_returns_new_session_approval() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.tmux.open('work', { cwd: '/tmp/project', command: 'nvim' });")
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(approval.tool, "cli.tmux");
    assert_eq!(
        approval.args,
        json!({
            "program": "tmux",
            "args": ["new-session", "-d", "-s", "work", "-c", "/tmp/project", "nvim"]
        })
    );
}

#[test]
fn tools_tmux_close_returns_kill_session_approval() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.tmux.close('work:1.2');")
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(approval.tool, "cli.tmux");
    assert_eq!(
        approval.args,
        json!({
            "program": "tmux",
            "args": ["kill-session", "-t", "work:1.2"]
        })
    );
}

#[test]
fn tools_tmux_send_uses_literal_keys_and_enter_by_default() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.tmux.send('work', 'cargo test');")
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(approval.tool, "cli.tmux");
    // In literal (-l) mode the Enter keypress must be a separate send-keys
    // call, otherwise tmux types the word "Enter" instead of pressing it.
    // This first approval is the literal command text without Enter.
    assert_eq!(
        approval.args,
        json!({
            "program": "tmux",
            "args": ["send-keys", "-t", "work", "-l", "cargo test"]
        })
    );
}

#[test]
fn tools_tmux_send_literal_presses_enter_in_separate_call() {
    let temp_dir = TempDir::new().expect("temp dir");
    let fake_tmux = temp_dir.path().join("tmux");
    let log = temp_dir.path().join("calls.log");
    std::fs::write(
        &fake_tmux,
        format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
    )
    .expect("write fake tmux");
    make_executable(&fake_tmux);
    let session = HostrunSession::new_auto_approve().expect("session");

    session
        .eval(&format!(
            "tools.tmux.with({{ executable: '{}' }}).send('work', 'cargo test')",
            fake_tmux.display()
        ))
        .expect("tmux send");

    let calls = std::fs::read_to_string(&log).expect("read calls log");
    let lines: Vec<&str> = calls.lines().collect();
    assert_eq!(
        lines,
        vec!["send-keys -t work -l cargo test", "send-keys -t work Enter"]
    );
}

#[test]
fn tools_tmux_send_non_literal_keeps_single_call() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.tmux.send('work', 'C-c', { literal: false });")
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(approval.tool, "cli.tmux");
    assert_eq!(
        approval.args,
        json!({
            "program": "tmux",
            "args": ["send-keys", "-t", "work", "C-c", "Enter"]
        })
    );
}

#[test]
fn tools_tmux_capture_requests_stdout_text() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.tmux.capture('work', { start: -40, end: -1, joinWrappedLines: true });")
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(approval.tool, "cli.tmux");
    assert_eq!(
        approval.args,
        json!({
            "program": "tmux",
            "args": ["capture-pane", "-p", "-t", "work", "-S", "-40", "-E", "-1", "-J"],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn tools_tmux_capture_returns_command_stdout() {
    let session = HostrunSession::new_auto_approve().expect("session");

    let result = session
        .eval("tools.tmux.with({ executable: 'printf' }).capture('work');")
        .expect("tmux capture");

    assert_eq!(
        result.value,
        Some(json!({
            "program": "printf",
            "args": ["capture-pane", "-p", "-t", "work"],
            "success": true,
            "exitCode": 0,
            "stdout": "capture-pane",
            "stdoutMeta": {
                "bytes": 12,
                "capturedBytes": 12,
                "truncated": false
            }
        }))
    );
}

#[test]
fn tools_tmux_run_extracts_last_marked_output() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            r#"
            __hostrun_tmuxExtractRunResult(
              "$ printf __HOSTRUN_TMUX_START_1__ __HOSTRUN_TMUX_END_1__\n__HOSTRUN_TMUX_START_1__\nfile-a\nfile-b\n__HOSTRUN_TMUX_END_1__:7\n",
              "__HOSTRUN_TMUX_START_1__",
              "__HOSTRUN_TMUX_END_1__"
            )
            "#,
        )
        .expect("extract result");

    assert_eq!(
        result.value,
        Some(json!({
            "stdout": "file-a\nfile-b",
            "exitCode": 7
        }))
    );
}

#[test]
fn tools_tmux_run_sends_command_and_returns_output() {
    let temp_dir = TempDir::new().expect("temp dir");
    let fake_tmux = temp_dir.path().join("tmux");
    std::fs::write(
        &fake_tmux,
        "#!/bin/sh\nif [ \"$1\" = \"capture-pane\" ]; then\n  printf '%s\\n' '__HOSTRUN_TMUX_START_1__' 'remote-a' 'remote-b' '__HOSTRUN_TMUX_END_1__:0'\nfi\n",
    )
    .expect("write fake tmux");
    make_executable(&fake_tmux);
    let session = HostrunSession::new_auto_approve().expect("session");

    let result = session
        .eval(&format!(
            "tools.tmux.with({{ executable: '{}' }}).run('work', 'ls', {{ pollMs: 0 }})",
            fake_tmux.display()
        ))
        .expect("tmux run");

    assert_eq!(
        result.value,
        Some(json!({
            "stdout": "remote-a\nremote-b",
            "exitCode": 0,
            "timedOut": false
        }))
    );
}

#[test]
fn command_builder_env_is_redacted_in_approval() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("cli.printenv('TOKEN').env('TOKEN', 'plain').stdout.text();")
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(
        approval.summary,
        "Run printenv TOKEN (env TOKEN, stdout text)"
    );
    assert_eq!(
        approval.args,
        json!({
            "program": "printenv",
            "args": ["TOKEN"],
            "env": { "TOKEN": "[redacted]" },
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn tools_ssh_plain_password_uses_sshpass_env() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            "tools.ssh({
              host: 'router',
              user: 'root',
              password: 'none',
              passwordMode: 'plain'
            }).run(cli.echo('hello'));",
        )
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(approval.tool, "cli.sshpass");
    assert_eq!(
        approval.summary,
        "Run sshpass -e ssh -o ControlMaster=auto -o ControlPath=~/.ssh/hostrun-%C -o ControlPersist=120s root@router 'echo' 'hello' (env SSHPASS, stdout text, stderr text)"
    );
    assert_eq!(
        approval.args,
        json!({
            "program": "sshpass",
            "args": [
                "-e",
                "ssh",
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPath=~/.ssh/hostrun-%C",
                "-o",
                "ControlPersist=120s",
                "root@router",
                "'echo' 'hello'"
            ],
            "env": { "SSHPASS": "[redacted]" },
            "stdout": { "type": "text" },
            "stderr": { "type": "text" }
        })
    );
}

#[test]
fn tools_ssh_rejects_password_without_plain_mode() {
    let session = HostrunSession::new().expect("session");

    session
        .eval("tools.ssh({ host: 'router', password: 'none' }).run(cli.hostname());")
        .expect_err("plain password mode should be explicit");
}

#[test]
fn tools_ssh_cli_returns_lazy_builder_with_persistent_defaults() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.ssh({ host: 'router', port: 2222 }).cli(cli.hostname()).text();")
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "ssh",
            "args": [
                "-p",
                "2222",
                "-o",
                "BatchMode=yes",
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPath=~/.ssh/hostrun-%C",
                "-o",
                "ControlPersist=120s",
                "router",
                "'hostname'"
            ],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn tools_powershell_composes_with_ssh_using_encoded_command() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            r#"tools.ssh({ host: 'desktop' })
              .cli(tools.powershell("Test-Path 'C:\\World of Warcraft\\_retail_\\Interface\\AddOns'"))
              .text();"#,
        )
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "ssh",
            "args": [
                "-o",
                "BatchMode=yes",
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPath=~/.ssh/hostrun-%C",
                "-o",
                "ControlPersist=120s",
                "desktop",
                "powershell -NoProfile -EncodedCommand VABlAHMAdAAtAFAAYQB0AGgAIAAnAEMAOgBcAFcAbwByAGwAZAAgAG8AZgAgAFcAYQByAGMAcgBhAGYAdABcAF8AcgBlAHQAYQBpAGwAXwBcAEkAbgB0AGUAcgBmAGEAYwBlAFwAQQBkAGQATwBuAHMAJwA="
            ],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn tools_ssh_opt_outs_disable_multiplex_and_batch_mode() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            "tools.ssh({ host: 'router', multiplex: false, batchMode: false })
              .cli(cli.hostname()).text();",
        )
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "ssh",
            "args": ["router", "'hostname'"],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn tools_ssh_explicit_options_override_matching_defaults() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            "tools.ssh({ host: 'router', options: ['ControlMaster=no', 'BatchMode no'] })
              .cli(cli.hostname()).text();",
        )
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "ssh",
            "args": [
                "-o",
                "ControlMaster=no",
                "-o",
                "BatchMode no",
                "-o",
                "ControlPath=~/.ssh/hostrun-%C",
                "-o",
                "ControlPersist=120s",
                "router",
                "'hostname'"
            ],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn browser_open_builds_browser_cli_command() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.browser.open('https://example.com').run();")
        .expect("approval");

    let approval = result.approval.expect("approval");
    assert_eq!(
        approval.id,
        "cli.browser-cli:browser-cli open https://example.com"
    );
    assert_eq!(approval.tool, "cli.browser-cli");
    assert_eq!(
        approval.args,
        json!({
            "program": "browser-cli",
            "args": ["open", "https://example.com"]
        })
    );
}

#[test]
fn browser_get_helpers_capture_text() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.browser.get('title').text();")
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "browser-cli",
            "args": ["get", "title"],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn browser_snapshot_and_screenshot_build_expected_flags() {
    let session = HostrunSession::new().expect("session");

    let snapshot = session
        .eval("tools.browser.snapshot({ mini: true, interactive: true, depth: 4 }).text();")
        .expect("snapshot approval");

    assert_eq!(
        snapshot.approval.expect("approval").args,
        json!({
            "program": "browser-cli",
            "args": ["snapshot", "--mini", "--interactive", "--depth", "4"],
            "stdout": { "type": "text" }
        })
    );

    let screenshot = session
        .eval("tools.browser.screenshot('/tmp/page.jpg', { full: true }).run();")
        .expect("screenshot approval");

    assert_eq!(
        screenshot.approval.expect("approval").args,
        json!({
            "program": "browser-cli",
            "args": ["screenshot", "--full", "/tmp/page.jpg"]
        })
    );
}

#[test]
fn browser_runtime_helpers_capture_json() {
    let session = HostrunSession::new().expect("session");

    let console = session
        .eval("tools.browser.console({ reload: true, waitMs: 3000 }).json();")
        .expect("console approval");

    assert_eq!(
        console.approval.expect("approval").args,
        json!({
            "program": "browser-cli",
            "args": ["--json", "runtime", "console", "--reload", "--wait-ms", "3000"],
            "stdout": { "type": "text" }
        })
    );

    let exceptions = session
        .eval("tools.browser.exceptions({ reload: true }).json();")
        .expect("exceptions approval");

    assert_eq!(
        exceptions.approval.expect("approval").args,
        json!({
            "program": "browser-cli",
            "args": ["--json", "runtime", "exceptions", "--reload"],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn browser_tabs_build_nested_commands() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("tools.browser.tabs.switch(2).run();")
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "browser-cli",
            "args": ["tabs", "switch", "2"]
        })
    );
}

#[test]
fn run_proxy_string_call_explains_correct_api() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("run('dmidecode -t system')")
        .expect("run as a string call should explain the proxy API");

    let value = result.value.expect("explanation");
    assert_eq!(value["ok"], json!(false));
    assert!(
        value["use"]
            .as_array()
            .unwrap()
            .contains(&json!("run.dmidecode('-t', 'system')"))
    );
    assert!(value["use"].as_array().unwrap().contains(&json!(
        "tools.sudo(cli.dmidecode('-t', 'system')).run() for privileged commands"
    )));
}

fn assert_dmidecode_approval(approval: super::HostrunApprovalRequest) {
    assert_eq!(approval.id, "cli.dmidecode:dmidecode");
    assert_eq!(approval.tool, "cli.dmidecode");
    assert_eq!(approval.summary, "Run dmidecode");
    assert_eq!(
        approval.args,
        json!({
            "program": "dmidecode",
            "args": []
        })
    );
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .expect("fake tmux metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("fake tmux permissions");
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) {}

#[test]
fn sqlite_query_wrapper_builds_json_sqlite_command() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval("sqlite.query('/tmp/app.db', 'select * from users').stdout.json();")
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "sqlite3",
            "args": ["-json", "/tmp/app.db", "select * from users"],
            "stdout": { "type": "text" }
        })
    );
}

#[test]
fn kubectl_get_wrapper_builds_json_get_command() {
    let session = HostrunSession::new().expect("session");

    let result = session
        .eval(
            "kubectl.get('pods', { namespace: 'default', allNamespaces: true })
                .stdout.json();",
        )
        .expect("approval");

    assert_eq!(
        result.approval.expect("approval").args,
        json!({
            "program": "kubectl",
            "args": ["get", "pods", "--namespace", "default", "--all-namespaces", "-o", "json"],
            "stdout": { "type": "text" }
        })
    );
}
