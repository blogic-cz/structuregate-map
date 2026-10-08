//! THE RUST HALF OF THE DEEP MAP: a `files` row per `.rs` file, its `consts`, `handlers`, `string_literals` and
//! `number_literals`, parsed by `syn` in this
//! process (`rsmap::consts`) and stored through the same door the payload halves use (`rows::apply`), so
//! what is recorded, what has gone and what disagrees are all scoped by `lang = 'rust'` exactly as theirs.
//!
//! INCREMENTAL LIKE THE OTHERS: a file whose sha the database already records is not parsed again, and its
//! rows stay. A disagreement after the write means the record is lying, so every rust file is read once
//! more - the database is never replaced, because the other halves' rows are in it.

use super::super::protocol::Collector;
use crate::rows;
use crate::rsmap;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

const LANG: &str = "rust";
/// Part of every file's sha: changing what a file's rows hold changes this, and every file is read again.
const ROWS_VERSION: &str = "rust-rows 3";

pub fn run(into: &mut Collector, db: &str, root: &str, files: &[(String, String)]) {
    let state = rows::state(Path::new(db), LANG, false);
    // NO RUST HERE AND NONE RECORDED: nothing to write, and no database to create for it.
    if files.is_empty() && state.shas.is_empty() {
        return;
    }
    let mut texts: Vec<(&str, &str, String)> = Vec::new();
    let mut shas = Map::new();
    for (rel, abs) in files {
        // An unreadable file is named by the file-level pass over the same tree; here it has no rows.
        let Ok(text) = std::fs::read_to_string(abs) else { continue };
        shas.insert(rel.clone(), Value::String(super::driven::folded(&format!("{ROWS_VERSION}\n{text}"))));
        texts.push((rel, abs, text));
    }
    for reset in [false, true] {
        let held = if state.rebuild || reset { BTreeMap::new() } else { state.shas.clone() };
        let stale: Vec<&(&str, &str, String)> =
            texts.iter().filter(|(rel, _, _)| held.get(*rel).map(String::as_str) != shas[*rel].as_str()).collect();
        let mut counters: BTreeMap<String, i64> = if state.rebuild { BTreeMap::new() } else { state.counters.clone() };
        let mut tables: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
        for (rel, _, text) in &stale {
            write_file(into, &mut tables, &mut counters, rel, text, shas[*rel].as_str().unwrap_or(""));
        }
        let batch = rows::Batch {
            all: true,
            first: true,
            final_: true,
            reset,
            shas: shas.clone(),
            read: stale.iter().map(|(rel, abs, _)| vec![rel.to_string(), abs.to_string()]).collect(),
            counters: counters.into_iter().map(|(k, v)| (k, Value::from(v))).collect(),
            tables: tables.into_iter().map(|(k, v)| (k.to_string(), Value::Array(v))).collect(),
            lang: LANG.to_string(),
            ..Default::default()
        };
        let receipt = match rows::apply(Path::new(db), root, &batch) {
            Ok(receipt) => receipt,
            Err(error) => {
                into.errors.push(format!("HALF      rust rows: the rows were not stored — {error}"));
                return;
            }
        };
        for (table, rows) in receipt.written {
            into.database.insert(table, rows);
        }
        if receipt.retry == 0 {
            return;
        }
        if reset {
            into.errors.push("HALF      rust rows: file(s) still disagree with the tree after a full rebuild".into());
            return;
        }
        into.notes.push(format!("the rust half is re-reading everything: {} file(s) disagreed with the tree after an incremental pass", receipt.retry));
    }
}

/// Ids are a prefix and a counter continued from the database, as every half hands them out.
fn next(counters: &mut BTreeMap<String, i64>, prefix: &str) -> String {
    let n = counters.entry(prefix.to_string()).or_insert(0);
    *n += 1;
    format!("{prefix}:{n}")
}

fn write_file(into: &mut Collector, tables: &mut BTreeMap<&str, Vec<Value>>, counters: &mut BTreeMap<String, i64>, rel: &str, text: &str, sha: &str) {
    let found = rsmap::rows_of(text);
    let file = next(counters, "f");
    let stem = Path::new(rel).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    // `mod.rs` is the module its folder names, as python's `__init__.py` is.
    let module = if stem == "mod" {
        Path::new(rel).parent().and_then(Path::file_name).map(|s| s.to_string_lossy().into_owned()).unwrap_or(stem)
    } else {
        stem
    };
    tables.entry("files").or_default().push(json!({
        "id": file, "path": rel, "module": module, "sha": sha,
        "lines": rsmap::lines::count(text).ok(), "errors": i64::from(found.is_err()),
    }));
    let rsmap::Rows { consts, handlers, literals } = match found {
        Ok(rows) => rows,
        Err((line, reason)) => {
            into.errors.push(format!("HALF      rust rows: {rel}:{line}: {reason}"));
            return;
        }
    };
    for c in consts {
        let id = next(counters, "k");
        tables.entry("consts").or_default().push(json!({
            "id": id, "file": file, "line": c.line, "name": c.name, "kind": c.kind, "owner": c.owner,
            "exported": c.exported, "type": c.ty, "source": c.source, "value": c.value,
            "value_kind": c.value_kind, "reads": c.reads, "calls": c.calls,
        }));
    }
    // `h`, the prefix the python and C# halves give a handler, continued from the database like theirs.
    for h in handlers {
        let id = next(counters, "h");
        tables.entry("handlers").or_default().push(json!({
            "id": id, "file": file, "cls": h.cls, "func": h.func, "line": h.line, "end_line": h.end_line,
            "try_line": h.try_line, "shape": h.shape, "types": h.types, "bare": i64::from(h.bare), "name": h.name,
            "name_read": i64::from(h.name_read), "passes": i64::from(h.passes), "raises": h.raises,
            "reraises": i64::from(h.reraises), "panics": i64::from(h.panics), "calls": h.calls, "test": i64::from(h.test),
        }));
    }
    // `s` and `n`, the prefixes every half gives these two tables.
    for l in literals {
        let (table, prefix) = if l.number.is_some() { ("number_literals", "n") } else { ("string_literals", "s") };
        let id = next(counters, prefix);
        let mut row = json!({
            "id": id, "file": file, "cls": l.cls, "func": l.func, "line": l.line, "value": l.value,
            "use": l.uses, "callee": l.callee, "target": l.target, "test": i64::from(l.test),
        });
        match l.number {
            Some(number) => row["number"] = json!(number),
            None => row["length"] = json!(l.value.chars().count()),
        }
        tables.entry(table).or_default().push(row);
    }
}
