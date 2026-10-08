//! `--map-check`: the findings a map holds, printed with their severity, and the exit code they decide - the same for
//! a map this run just wrote and for one `--map-if-stale` kept because nothing moved. A kept map that was not checked
//! let a red tree pass on the very next turn of every hook.

use serde_json::Value;
use std::path::Path;

/// The findings as `(text, error)`, then a count line. The errors go to STDERR, where a Claude Code hook shows them -
/// a failing hook discards stdout. 1 when any is an error.
pub fn report(findings: &[(String, bool)], out: &mut Vec<(&str, String)>) -> i64 {
    let mut errors = 0;
    for (text, error) in findings {
        if *error {
            errors += 1;
            out.push(("e", format!("  error: {text}")));
        } else {
            out.push(("o", format!("  note : {text}")));
        }
    }
    out.push(("o", format!("  {} finding(s), {errors} of them a state that cannot be legitimate", findings.len())));
    i64::from(errors > 0)
}

/// What a written map recorded, judged by its text as the run that wrote it judged it. `None` when the map cannot be
/// read - the caller parses again rather than pass a check it could not make.
pub fn kept(map: &Path) -> Option<Vec<(String, bool)>> {
    let text = std::fs::read_to_string(map).ok()?;
    let written: Value = serde_json::from_str(&text).ok()?;
    let findings = written.get("findings")?.as_array()?;
    Some(findings.iter().filter_map(Value::as_str).map(|t| (t.to_string(), crate::graph::is_error(t))).collect())
}
