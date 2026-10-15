//! WHAT A RUN LEAVES FOR THE NEXT: the file graph a partial run starts from, and whether the recorded reads were
//! proved - kept in `_meta`, derived from the run's `Digest` (`digest.rs`).

use super::digest::Digest;
use super::deps;
use super::tsapply::{TsReceipt, LANG};
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::path::Path;

/// The graph, derived from the rows just written and kept in `_meta` for the next run to read.
///
/// `replaced` names the files a PARTIAL run re-extracted: the stored graph is repaired for those
/// and left alone everywhere else, because this run never saw the other rows.
///
/// DELETED AND THEN INSERTED, never `INSERT OR REPLACE`: `_meta` is `(key TEXT, value TEXT)` with
/// NO unique key, so REPLACE has nothing to replace ON and simply adds a second row. Every reader
/// takes the first one, so each run stored a graph nobody read and answered with the PREVIOUS run's.
fn remember_deps(db: &Connection, digest: &Digest, replaced: Option<&[String]>) {
    let read = |key: &str| -> Option<String> {
        db.query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| r.get::<_, String>(0))
            .ok()
    };
    let deps_key = format!("deps:{LANG}");
    let scope_key = format!("scope:{LANG}");

    let (edges, scope) = match replaced {
        None => (digest.graph(), digest.scope_files()),
        Some(files) => {
            // READ BACK AS JSON AND NOT AS A MAP TYPE: this crate's ordered map has no serde of its
            // own, and the stored graph is a plain object of string lists either way.
            let mut previous: indexmap::IndexMap<String, Vec<String>> = indexmap::IndexMap::new();
            if let Some(Value::Object(stored)) = read(&deps_key).and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
                for (target, dependents) in stored {
                    if let Value::Array(list) = dependents {
                        previous.insert(
                            target,
                            list.iter().filter_map(|d| d.as_str().map(String::from)).collect(),
                        );
                    }
                }
            }
            let held: Vec<String> = read(&scope_key)
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            (deps::merge(&previous, digest.graph(), files), deps::merge_scope(&held, digest.scope_files(), files))
        }
    };

    // A graph that could not be stored costs the next run its incremental parse and nothing else,
    // so it is not worth failing a map that is otherwise complete.
    let mut as_object = Map::new();
    for (target, dependents) in edges {
        as_object.insert(target, Value::from(dependents));
    }
    let compact = |v: &Value| serde_json::to_string(v).unwrap_or_else(|_| "null".into());
    let edges_json = compact(&Value::Object(as_object));
    let scope_json = compact(&Value::from(scope));
    for (key, value) in [(deps_key, edges_json), (scope_key, scope_json)] {
        let _ = db.execute("DELETE FROM _meta WHERE key = ?1", [key.as_str()]);
        let _ = db.execute(
            "INSERT INTO _meta (key, value) VALUES (?1, ?2)",
            [key.as_str(), value.as_str()],
        );
    }
}

/// WHICH FILES HAVE TO BE RE-EXTRACTED WHEN ONE CHANGES, stored beside the rows it is derived from so the
/// next run reads it instead of deriving it. A partial run REPAIRS it (`replaced`) rather than deriving
/// it: it holds a fraction of the rows, and reading the rest back costs close to a minute.
pub(super) fn remember_graph(receipt: &mut TsReceipt, db_path: &Path, digest: &Digest, replaced: Option<&[String]>) {
    let deps_started = std::time::Instant::now();
    if let Ok(db) = Connection::open(db_path) {
        remember_deps(&db, digest, replaced);
        // THE RECORDED READS ARE PROVED against every row written - see `reads::unread` - and only proved reads
        // let the next run stop short of every hop of dependents. A full run proves them; a partial one holds
        // only its own rows, so it can find them wrong and never right.
        let missed = digest.unread();
        let verdict = match missed.first() {
            Some((owner, named)) => {
                receipt.notes.push(format!(
                    "the typescript half's recorded reads miss {} file(s) its rows name, first {owner} -> {named} -                      the next run reads every dependent again",
                    missed.len()
                ));
                Some(format!("unproven: {owner} -> {named}"))
            }
            None => replaced.is_none().then(|| "proven".to_string()),
        };
        if let Some(verdict) = verdict {
            let key = format!("reads:{LANG}");
            let _ = db.execute("DELETE FROM _meta WHERE key = ?1", [key.as_str()]);
            let _ = db.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [key.as_str(), verdict.as_str()]);
        }
    }
    receipt
        .phases
        .insert("remember the graph".to_string(), deps_started.elapsed().as_millis() as u64);
}
