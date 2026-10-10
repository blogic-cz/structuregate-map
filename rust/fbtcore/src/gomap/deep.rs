//! THE GO HALF OF THE DEEP MAP: a `files` row per `.go` file, its `functions`, `calls`, `consts` and `string_literals`
//! (`rows.rs`), parsed by `gosyn` in this process and stored through the door every half uses (`rows::apply`), so what
//! is recorded, what has gone and what disagrees are all scoped by `lang = 'go'`.
//!
//! INCREMENTAL BY SHA, WITH ONE EXCEPTION. A call's `target_path` names ANOTHER file - the one of its package that
//! declares the name - so a file whose own bytes did not move can still bind differently once a neighbour has gained,
//! lost or renamed a function. When ANY `.go` file is new, changed or gone, every `.go` file is read again (as the plain
//! TypeScript half re-reads the importers of a file that moved); a tree that moved nothing parses nothing.

use super::rows::{self, Index};
use crate::mapper::protocol::Collector;
use crate::rows as store;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

const LANG: &str = "go";
/// Part of every file's sha: changing what a file's rows hold changes this, and every file is read again.
const ROWS_VERSION: &str = "go-rows 1";

pub(crate) fn run(into: &mut Collector, db: &str, root: &str, files: &[(String, String)]) {
    let state = store::state(Path::new(db), LANG, false);
    // NO GO HERE AND NONE RECORDED: nothing to write, and no database to create for it.
    if files.is_empty() && state.shas.is_empty() {
        return;
    }
    let mut texts: Vec<(&str, &str, String)> = Vec::new();
    let mut shas = Map::new();
    for (rel, abs) in files {
        // An unreadable file is named by the file-level pass over the same tree; here it has no rows.
        let Ok(text) = std::fs::read_to_string(abs) else { continue };
        shas.insert(rel.clone(), Value::String(folded(&format!("{ROWS_VERSION}\n{text}"))));
        texts.push((rel, abs, text));
    }
    for reset in [false, true] {
        let held = if state.rebuild || reset { BTreeMap::new() } else { state.shas.clone() };
        let moved = texts.iter().any(|(rel, _, _)| held.get(*rel).map(String::as_str) != shas[*rel].as_str())
            || held.keys().any(|rel| !shas.contains_key(rel));
        let stale: Vec<&(&str, &str, String)> = if moved { texts.iter().collect() } else { Vec::new() };
        let index = if moved { index_of(Path::new(root), &texts) } else { Index::new() };
        let mut counters: BTreeMap<String, i64> = if state.rebuild { BTreeMap::new() } else { state.counters.clone() };
        let mut tables: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
        for (rel, abs, text) in &stale {
            write_file(into, &mut tables, &mut counters, Path::new(root), rel, abs, text, shas[*rel].as_str().unwrap_or(""), &index);
        }
        let batch = store::Batch {
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
        let receipt = match store::apply(Path::new(db), root, &batch) {
            Ok(receipt) => receipt,
            Err(error) => {
                into.errors.push(format!("HALF      go rows: the rows were not stored — {error}"));
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
            into.errors.push("HALF      go rows: file(s) still disagree with the tree after a full rebuild".into());
            return;
        }
        into.notes.push(format!("the go half is re-reading everything: {} file(s) disagreed with the tree after an incremental pass", receipt.retry));
    }
}

/// SHA-256 of a text, as 16 lowercase hex digits - the form every half records.
fn folded(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Every top-level func of the run, by folder: what a call binds to. A `_test.go` declares nothing (`mod.rs`), and the
/// files are taken in path order so a name two build variants declare always answers with the same one.
fn index_of(root: &Path, texts: &[(&str, &str, String)]) -> Index {
    let mut ordered: Vec<&(&str, &str, String)> = texts.iter().filter(|(rel, _, _)| !rel.ends_with("_test.go")).collect();
    ordered.sort_by(|a, b| a.0.cmp(b.0));
    let mut index = Index::new();
    for (rel, abs, text) in ordered {
        let folder = rows::folder_of(root, Path::new(abs));
        for name in rows::declared_funcs(text) {
            index.entry((folder.clone(), name)).or_insert_with(|| rel.to_string());
        }
    }
    index
}

/// Ids are a prefix and a counter continued from the database, as every half hands them out.
fn next(counters: &mut BTreeMap<String, i64>, prefix: &str) -> String {
    let n = counters.entry(prefix.to_string()).or_insert(0);
    *n += 1;
    format!("{prefix}:{n}")
}

#[allow(clippy::too_many_arguments)]
fn write_file(into: &mut Collector, tables: &mut BTreeMap<&str, Vec<Value>>, counters: &mut BTreeMap<String, i64>, root: &Path,
              rel: &str, abs: &str, text: &str, sha: &str, index: &Index) {
    let found = rows::rows_of(text, root, Path::new(abs), index);
    let file = next(counters, "f");
    let package = found.as_ref().map(|r| r.package.clone()).unwrap_or_default();
    tables.entry("files").or_default().push(json!({
        "id": file, "path": rel, "module": package, "sha": sha,
        "lines": super::lines::count(text.trim_start_matches('\u{feff}')).ok(), "errors": i64::from(found.is_err()),
    }));
    let rows::Rows { functions, calls, consts, literals, .. } = match found {
        Ok(rows) => rows,
        Err((line, reason)) => {
            into.errors.push(format!("HALF      go rows: {rel}:{line}: {reason}"));
            return;
        }
    };
    for f in functions {
        let id = next(counters, "fn");
        tables.entry("functions").or_default().push(json!({
            "id": id, "file": file, "line": f.line, "end_line": f.end_line, "name": f.name, "qualname": f.qualname,
            "kind": f.kind, "args": f.args, "exported": i64::from(f.exported), "statements": f.statements,
        }));
    }
    for c in calls {
        let id = next(counters, "call");
        tables.entry("calls").or_default().push(json!({
            "id": id, "file": file, "func": c.func, "line": c.line, "callee": c.callee, "target_path": c.target_path,
            "target_name": c.target_name, "args": c.args, "source": c.source,
        }));
    }
    for c in consts {
        let id = next(counters, "k");
        tables.entry("consts").or_default().push(json!({
            "id": id, "file": file, "line": c.line, "name": c.name, "kind": c.kind, "exported": i64::from(c.exported), "value": c.value,
        }));
    }
    for l in literals {
        let id = next(counters, "s");
        tables.entry("string_literals").or_default().push(json!({
            "id": id, "file": file, "cls": l.cls, "func": l.func, "line": l.line, "length": l.value.chars().count(), "value": l.value,
            "test": i64::from(rel.ends_with("_test.go")),
        }));
    }
}
