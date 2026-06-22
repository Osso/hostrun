use std::io::BufRead;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

use hostrun::HostrunEvalArguments;
use hostrun::HostrunExecutionContext;
use hostrun::HostrunSessionStore;
use hostrun::run_eval_tool;
use serde_json::json;

fn main() {
    let sessions = Arc::new(Mutex::new(HostrunSessionStore::new_auto_approve()));
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => run_line(&sessions, &line),
            Err(error) => json!({ "type": "error", "error": error.to_string() }),
        };

        let _ = writeln!(stdout, "{response}");
        let _ = stdout.flush();
    }
}

fn run_line(sessions: &Arc<Mutex<HostrunSessionStore>>, line: &str) -> serde_json::Value {
    let input = match serde_json::from_str::<HostrunEvalArguments>(line) {
        Ok(input) => input,
        Err(error) => return json!({ "type": "error", "error": error.to_string() }),
    };

    match run_eval_tool(sessions, &input, HostrunExecutionContext::default()) {
        Ok(value) => value,
        Err(error) => json!({ "type": "error", "error": error.to_string() }),
    }
}
