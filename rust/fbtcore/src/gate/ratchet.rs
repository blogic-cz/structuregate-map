//! THE SIZE RATCHET. A flat limit applied to a codebase that already exceeds it means a red build from the
//! first commit, so existing debt is RECORDED and allowed to shrink only:
//!
//!   * a file NOT in the baseline must stay under the CEILING (90 % of the limit) - new debt is rejected;
//!   * a file IN the baseline must not grow past the count recorded there;
//!   * a baseline file that drops under the ceiling must be REMOVED, so the list is a shrinking to-do list.
//!
//! THE CEILING, NOT THE LIMIT, because the gate stops at the ceiling: a list frozen from the limit left every
//! file in the last tenth red on arrival, and told a debt paid down into it to leave the list while failing it.
//!
//! Without the third rule a baseline is just a permanent exemption list with extra steps.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Recorded {
    pub files: BTreeMap<String, i64>,
    pub dirs: BTreeMap<String, i64>,
}

/// A missing baseline is an EMPTY ratchet: nothing recorded, nothing exempt. One that exists and cannot be
/// READ is the opposite - the ratchet is not applied at all - so it is reported, never thrown: a gate that
/// throws instead of reporting is a gate that gets switched off.
pub fn load(path: Option<&str>, problems: &mut Vec<String>) -> Recorded {
    let Some(path) = path else { return Recorded::default() };
    if !std::path::Path::new(path).is_file() {
        return Recorded::default();
    }
    let parsed = std::fs::read_to_string(path)
        .map_err(|e| ("IOException", e.to_string()))
        .and_then(|text| {
            serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).map_err(|e| ("JsonException", e.to_string()))
        });
    let root = match parsed {
        Ok(root) => root,
        Err((kind, why)) => {
            problems.push(format!(
                "baseline `{path}` could not be read ({kind}), so the ratchet is NOT being applied: {} — fix the \
                 file, or rewrite it with --update-baseline",
                why.lines().next().unwrap_or("")
            ));
            return Recorded::default();
        }
    };
    let section = |name: &str| -> BTreeMap<String, i64> {
        root.get(name)
            .and_then(Value::as_object)
            .map(|s| s.iter().filter_map(|(k, v)| v.as_i64().map(|n| (k.clone(), n))).collect())
            .unwrap_or_default()
    };
    Recorded { files: section("files"), dirs: section("dirs") }
}

/// The three verdicts, for files or for folders - the shape is identical, so the code is. `over` keeps the
/// walk's order, and the sort is STABLE, so equal counts are named in the order the tree has them.
pub fn judge(problems: &mut Vec<String>, counts: &[(String, i64)], over: &[(String, i64)],
    baseline: &BTreeMap<String, i64>, ceiling: i64, unit: &str, headline: &str) {
    let mut by_count: Vec<&(String, i64)> = over.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1));
    for (key, n) in by_count {
        match baseline.get(key) {
            None => problems.push(format!("{n}  {key} — {unit} {headline}")),
            Some(was) if n > was => problems.push(format!(
                "{n} (baseline {was})  {key}   +{} — baseline {unit} GREW; existing debt may only shrink", n - was)),
            Some(_) => {}
        }
    }
    let now: BTreeMap<&str, i64> = counts.iter().map(|(k, n)| (k.as_str(), *n)).collect();
    for key in baseline.keys() {
        let n = now.get(key.as_str()).copied().unwrap_or(0);
        if n < ceiling {
            problems.push(format!(
                "{n}  {key}   ({unit} now under the ceiling {ceiling}) — remove it from the baseline so the list keeps shrinking"
            ));
        }
    }
}

/// The baseline, rewritten from what is at or over the ceiling now.
pub fn write(path: &str, max_lines: i64, max_files: i64, files: &[(String, i64)], dirs: &[(String, i64)]) -> Result<(), String> {
    let mut out = Map::new();
    out.insert("_comment".into(), json!(
        "Files/folders ALREADY at or over the ceiling (90 % of the limit) when the gate was introduced. This list may only SHRINK. Do not add \
         an entry to silence the gate on a new file — split it instead."));
    out.insert("max_source_lines".into(), json!(max_lines));
    out.insert("max_files_per_dir".into(), json!(max_files));
    out.insert("files".into(), counts(files));
    out.insert("dirs".into(), counts(dirs));
    let text = serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("the baseline could not be written to {path} ({e})"))
}

/// Largest first, then by key - the order a reader shrinks the list in.
pub fn counts(counts: &[(String, i64)]) -> Value {
    let mut ordered: Vec<&(String, i64)> = counts.iter().collect();
    ordered.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Value::Object(ordered.into_iter().map(|(k, v)| (k.clone(), json!(v))).collect())
}
