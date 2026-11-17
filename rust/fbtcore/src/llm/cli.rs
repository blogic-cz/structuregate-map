//! A PROGRAM AS AN `Llm`: one process per question, started with the arguments configured, the request as one JSON
//! object on stdin (`system_prompt`, `user_message`, `schema`, `want`, and `tools` when the question offers any), ONE
//! JSON object back on stdout - `{"ok": true, "data"}` (with `tool_calls`, at the top or in `usage`, when it counts them)
//! or `{"ok": false, "error_type", "message"}` - and its exit code saying which failure it was (2 unavailable, 3 an
//! invalid reply, 4 a timeout, 5 refused). Its log goes to stderr, which is kept only to say why when stdout is not
//! that object. The question never goes on the command line: a document's text has no business in a process list.

use super::{Answer, Ask, Failed, Llm};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The program and its arguments.
#[derive(Debug, Clone)]
pub struct Cli {
    pub exe: PathBuf,
    pub args: Vec<String>,
}

impl Llm for Cli {
    fn ask_json(&self, ask: &Ask) -> Result<Answer, Failed> {
        let mut child = Command::new(&self.exe)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Failed::Unavailable(format!("{} could not be started ({e}) - set {} to a program that answers a JSON request on stdin", self.exe.display(), super::ENV)))?;
        let want = if ask.schema["type"] == "array" { "array" } else { "object" };
        let mut request = json!({ "system_prompt": ask.system, "user_message": ask.message, "schema": ask.schema, "want": want });
        if !ask.tools.is_empty() {
            request["tools"] = json!(ask.tools);
        }
        // WRITTEN WHILE IT READS: a stdin larger than the pipe would block a writer that waits for the process first.
        let mut stdin = child.stdin.take().ok_or_else(|| Failed::Other("no stdin to write the question to".into()))?;
        let writer = std::thread::spawn(move || stdin.write_all(request.to_string().as_bytes()));
        let out = child.wait_with_output().map_err(|e| Failed::Other(format!("{} did not finish ({e})", self.exe.display())))?;
        let _ = writer.join();
        answer(out.status.code(), &out.stdout, &out.stderr)
    }
}

/// What the CLI said, read by its object first and its exit code second.
pub(super) fn answer(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<Answer, Failed> {
    let text = String::from_utf8_lossy(stdout);
    let said: Option<Value> = text.lines().rev().find_map(|l| serde_json::from_str::<Value>(l.trim()).ok().filter(Value::is_object));
    if let Some(said) = &said
        && said["ok"] == true
    {
        let tool_calls = said["tool_calls"].as_u64().or_else(|| said["usage"]["tool_calls"].as_u64());
        return Ok(Answer { data: said["data"].clone(), tool_calls });
    }
    let why = match &said {
        Some(s) => {
            let kind = s["error_type"].as_str().unwrap_or("error");
            let mut why = format!("{kind}: {}", s["message"].as_str().unwrap_or(""));
            if let Some(errors) = s["errors"].as_array().filter(|e| !e.is_empty()) {
                why.push_str(&format!(" ({})", errors.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")));
            }
            // A FAILURE AFTER TOOL CALLS says how many: an answer refused after eight queries is not one never asked.
            if let Some(n) = s["tool_calls"].as_u64().or_else(|| s["usage"]["tool_calls"].as_u64()) {
                why.push_str(&format!(" [after {n} tool call(s)]"));
            }
            why
        }
        None => {
            let tail: Vec<&str> = std::str::from_utf8(stderr).unwrap_or("").lines().rev().take(3).collect();
            format!("no answer object (exit {}): {}", code.map_or("none".into(), |c| c.to_string()), tail.into_iter().rev().collect::<Vec<_>>().join(" | "))
        }
    };
    Err(match code {
        Some(2) => Failed::Unavailable(why),
        Some(3) => Failed::Invalid(why),
        Some(4) => Failed::Timeout(why),
        Some(5) => Failed::Rejected(why),
        _ => Failed::Other(why),
    })
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
