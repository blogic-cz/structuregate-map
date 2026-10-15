//! WHAT A RUN LEAVES FOR THE NEXT: the file graph a partial run starts from, and whether the recorded reads were
//! proved - kept in `ts_deps`, `ts_scope` and `_meta`, derived from the run's `Digest` (`digest.rs`).

use super::digest::Digest;
use super::deps;
use super::tsapply::{TsReceipt, LANG};
use indexmap::IndexMap;
use rusqlite::Connection;
use std::path::Path;

/// THE GRAPH'S TABLE: one row per edge, `target` the file depended on and `dependent` the file re-read when it moves,
/// in the graph's own order - a target's rows together, targets in the order the graph holds them. A target with
/// no dependent left is one row with a NULL `dependent`, so the stored graph comes back exactly as it went in.
pub const DEPS: &str = "ts_deps";
/// Every file a template resolves through (`deps::scope_files`), one row each.
pub const SCOPE: &str = "ts_scope";

/// The graph the last run stored, in its order - empty when there is none.
pub fn stored_graph(db: &Connection) -> IndexMap<String, Vec<String>> {
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    let Ok(mut stmt) = db.prepare(&format!("SELECT target, dependent FROM {DEPS} ORDER BY rowid")) else { return out };
    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))) else { return out };
    for (target, dependent) in rows.flatten() {
        out.entry(target).or_default().extend(dependent);
    }
    out
}

/// The scope set the last run stored, in its order.
pub fn stored_scope(db: &Connection) -> Vec<String> {
    let Ok(mut stmt) = db.prepare(&format!("SELECT path FROM {SCOPE} ORDER BY rowid")) else { return Vec::new() };
    stmt.query_map([], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// Both, REWRITTEN WHOLE: a repair can move a target, and the order is part of what is kept.
fn store(db: &Connection, edges: &IndexMap<String, Vec<String>>, scope: &[String]) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {DEPS} (target TEXT NOT NULL, dependent TEXT); \
         CREATE INDEX IF NOT EXISTS ix_{DEPS}_target ON {DEPS} (target); \
         CREATE TABLE IF NOT EXISTS {SCOPE} (path TEXT NOT NULL); \
         DELETE FROM {DEPS}; DELETE FROM {SCOPE};"
    ))?;
    {
        let mut edge = tx.prepare(&format!("INSERT INTO {DEPS} (target, dependent) VALUES (?1, ?2)"))?;
        for (target, dependents) in edges {
            if dependents.is_empty() {
                edge.execute(rusqlite::params![target, Option::<&str>::None])?;
            }
            for dependent in dependents {
                edge.execute([target, dependent])?;
            }
        }
        let mut path = tx.prepare(&format!("INSERT INTO {SCOPE} (path) VALUES (?1)"))?;
        for file in scope {
            path.execute([file])?;
        }
    }
    tx.commit()
}

/// The graph, derived from the rows just written and kept in `ts_deps`/`ts_scope` for the next run to read.
///
/// `replaced` names the files a PARTIAL run re-extracted: the stored graph is repaired for those
/// and left alone everywhere else, because this run never saw the other rows.
fn remember_deps(db: &Connection, digest: &Digest, replaced: Option<&[String]>) {
    let (edges, scope) = match replaced {
        None => (digest.graph(), digest.scope_files()),
        Some(files) => (
            deps::merge(&stored_graph(db), digest.graph(), files),
            deps::merge_scope(&stored_scope(db), digest.scope_files(), files),
        ),
    };
    // A graph that could not be stored costs the next run its incremental parse and nothing else,
    // so it is not worth failing a map that is otherwise complete.
    let _ = store(db, &edges, &scope);
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
            let _ = crate::rows::meta::put(&db, &format!("reads:{LANG}"), &verdict);
        }
    }
    receipt
        .phases
        .insert("remember the graph".to_string(), deps_started.elapsed().as_millis() as u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graph_comes_back_in_its_own_order_an_empty_target_included() {
        let db = Connection::open_in_memory().unwrap();
        let mut edges: IndexMap<String, Vec<String>> = IndexMap::new();
        edges.insert("z.ts".into(), vec!["a.ts".into(), "b.ts".into()]);
        edges.insert("a.ts".into(), Vec::new());
        edges.insert("m.ts".into(), vec!["z.ts".into()]);
        let scope = vec!["b.ts".to_string(), "a.ts".to_string()];
        store(&db, &edges, &scope).unwrap();
        assert_eq!(stored_graph(&db), edges);
        assert_eq!(stored_scope(&db), scope);
        // Rewritten whole, never appended to.
        edges.shift_remove("z.ts");
        store(&db, &edges, &scope[..1]).unwrap();
        assert_eq!(stored_graph(&db), edges);
        assert_eq!(stored_scope(&db), ["b.ts"]);
        assert!(stored_graph(&Connection::open_in_memory().unwrap()).is_empty(), "no table is no graph");
    }
}
