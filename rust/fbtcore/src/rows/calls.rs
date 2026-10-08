//! THE DATABASE CALLS every half makes, as functions: what is recorded, a batch applied, the rows a partial
//! run keeps, the TypeScript state and payload, the search index and the atlas. Each answers with the JSON its
//! `extern "C"` door in `lib.rs` hands the caller, so a half driven from rust and one driven from C# get the
//! same answer - one body, two callers.

use super::{carry, fts, tsapply};
use std::path::Path;

/// What the database already records for one half: the sha of every file it holds and the id counters.
pub fn state(db: &Path, lang: &str, whole: bool) -> Result<String, String> {
    serde_json::to_string(&super::state(db, lang, whole)).map_err(|e| format!("state could not be serialised: {e}"))
}

/// One batch of rows, stored; the receipt back.
pub fn apply(db: &Path, root: &str, payload: &[u8]) -> Result<String, String> {
    let batch: super::Batch = serde_json::from_slice(payload).map_err(|e| format!("the rows could not be read - {e}"))?;
    let receipt = super::apply(db, root, &batch).map_err(|e| format!("{e:#}"))?;
    serde_json::to_string(&receipt).map_err(|e| format!("receipt could not be serialised: {e}"))
}

/// The rows of every file a partial run is NOT re-extracting, written to `target`. AN UNREADABLE PLAN IS
/// NOT AN EMPTY ONE: read as "nothing is affected" it would carry every row.
pub fn carry(db: &Path, lang: &str, plan: &str, target: &str) -> Result<String, String> {
    if target.is_empty() {
        return Err("no file to carry the rows into".to_string());
    }
    let affected: Vec<String> = if plan.is_empty() {
        Vec::new()
    } else {
        let text = std::fs::read_to_string(plan).map_err(|e| format!("the plan could not be read - {e}"))?;
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("the plan could not be read - {e}"))?;
        parsed
            .get("affected")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    let carried = carry::carry_over(db, &affected, Path::new(target), lang).map_err(|e| format!("{e:#}"))?;
    serde_json::to_string(&carried).map_err(|e| format!("the receipt could not be serialised: {e}"))
}

/// What the TypeScript half has to know before it parses.
pub fn ts_state(db: &Path, lang: &str) -> Result<String, String> {
    serde_json::to_string(&super::ts_state(db, lang)).map_err(|e| format!("the state could not be serialised: {e}"))
}

/// One TypeScript payload stored, parsed from a MAPPING of the file node wrote. Reading it whole held hundreds of MB, and
/// streaming it through a reader cost seconds of per-byte overhead; mapped pages are the file's own, so the OS
/// can drop them. The parse is timed here, the only side that knows when it began.
pub fn ts_apply(db: &Path, root: &str, payload: &str) -> Result<String, String> {
    let file = std::fs::File::open(payload).map_err(|e| format!("the payload {payload} could not be opened - {e}"))?;
    let started = std::time::Instant::now();
    // SAFETY: node has exited and nothing writes the payload while it is mapped; the mapping is dropped here.
    let mapped = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| format!("the payload {payload} could not be mapped - {e}"))?;
    let parsed: tsapply::TsPayload = serde_json::from_slice(&mapped).map_err(|e| format!("the rows could not be read - {e}"))?;
    drop(mapped);
    let parse_ms = started.elapsed().as_millis() as u64;
    let mut receipt = tsapply::apply(db, root, parsed).map_err(|e| format!("{e:#}"))?;
    receipt.phases.insert("parse the payload".to_string(), parse_ms);
    serde_json::to_string(&receipt).map_err(|e| format!("the receipt could not be serialised: {e}"))
}

/// The full-text index over every row.
pub fn search(db: &Path, with_json: bool) -> Result<String, String> {
    let built = fts::build(db, with_json).map_err(|e| format!("{e:#}"))?;
    let mut out = serde_json::Map::new();
    out.insert("rows".to_string(), serde_json::Value::from(built.rows));
    if let Some(why) = built.skipped {
        out.insert("skipped".to_string(), serde_json::Value::String(why));
    }
    serde_json::to_string(&serde_json::Value::Object(out)).map_err(|e| format!("the receipt could not be serialised: {e}"))
}

/// The atlas beside the map.
pub fn atlas(db: &Path, into: &str) -> Result<String, String> {
    let written = crate::atlas::write(db, Path::new(into)).map_err(|e| format!("{e:#}"))?;
    let mut out = serde_json::Map::new();
    out.insert("projects".to_string(), serde_json::Value::from(written.projects));
    out.insert("routes".to_string(), serde_json::Value::from(written.routes));
    out.insert("boundaries".to_string(), serde_json::Value::from(written.boundaries));
    if let Some(why) = written.skipped {
        out.insert("skipped".to_string(), serde_json::Value::String(why));
    }
    serde_json::to_string(&serde_json::Value::Object(out)).map_err(|e| format!("the receipt could not be serialised: {e}"))
}
