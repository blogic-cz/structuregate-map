//! The rules a half that replaces its rows WHOLE lives by.
//!
//! The C# half drops a file's rows by the `file` column and writes the files that
//! moved. The TypeScript half cannot: more than half of the tables it writes have no
//! `file` column — a binding hangs off a template node, a call off a member — so
//! dropping "the rows of the files that moved" would leave the rest of a re-parsed
//! file's rows behind, joined to nothing.
//!
//! So every row it writes carries `half`, and every run deletes exactly those before it
//! inserts. The other halves stamp `lang` and never `half`, and their rows are never
//! touched.

use super::schema;
use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

/// The column a whole-replacement half stamps on every row it writes.
pub const HALF_COLUMN: &str = "half";

/// The tables carrying a `half` column, which are the ones such a half may delete from.
///
/// `file_text` and its FTS5 shadows are excluded by prefix, as is anything starting with
/// `_`: those are bookkeeping, not rows.
pub fn tables_with_half(db: &Connection) -> Result<Vec<String>> {
    let mut stmt = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    let mut out = Vec::new();
    for name in names {
        if name.starts_with('_') || name.starts_with("file_text") {
            continue;
        }
        if schema::existing_columns(db, &name)?
            .iter()
            .any(|c| c == HALF_COLUMN)
        {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// Every row this half wrote, gone — and nothing else. Called before the new rows go in.
///
/// A half that only ever added rows would answer with rows describing a file that no
/// longer says that, which is worse than no rows because they look real.
pub fn drop_half(db: &Connection, half: &str) -> Result<()> {
    for table in tables_with_half(db)? {
        db.execute(
            &format!(
                "DELETE FROM \"{}\" WHERE \"{}\" = ?1",
                schema::escape(&table),
                HALF_COLUMN
            ),
            [half],
        )?;
    }
    Ok(())
}

/// Is this half the ONLY one with rows in this database?
///
/// IT DECIDES WHETHER THE IDS RESTART. A half that replaces its rows whole gains nothing
/// by continuing the counters and loses reproducibility: the same tree mapped twice
/// produced `renders` 1..N and then a fresh range above it, and `key_reach` sorts its gate
/// lists by id STRING, so they reordered and a 53-table gate reported thousands of changed rows
/// with no fact different. Restarting makes the numbering a function of the TREE.
///
/// IT IS NOT SAFE UNCONDITIONALLY. Nine id prefixes are shared with the C# half (`f`,
/// `c`, `x`, `fn`, `br`, `p`, `k`, `e`, `i`), so restarting `f:` in a database that also
/// holds C# files would hand out an id another half's rows already point at. The other
/// halves never write `half` — they stamp `lang` — so what says they are here is the
/// per-language tally `_meta` keeps.
pub fn alone(path: &Path, lang: &str) -> bool {
    if !path.exists() {
        return true;
    }
    let Ok(db) = Connection::open(path) else {
        return true;
    };
    let Ok(mut stmt) = db.prepare("SELECT key, value FROM _meta WHERE key LIKE 'files:%'") else {
        return true;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    }) else {
        return true;
    };

    let mine = format!("files:{lang}");
    for (key, value) in rows.flatten() {
        if key == mine {
            continue;
        }
        // A tally of zero is a half that has been here and left nothing, which is not
        // another half's rows to collide with.
        if value.trim().parse::<i64>().unwrap_or(0) > 0 {
            return false;
        }
    }
    true
}

/// One stored `_meta` value, or empty when it is not there.
pub fn meta(path: &Path, key: &str) -> String {
    if !path.exists() {
        return String::new();
    }
    let Ok(db) = Connection::open(path) else {
        return String::new();
    };
    db.query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| {
        r.get::<_, String>(0)
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::{apply, Batch};
    use serde_json::json;

    fn tmp(name: &str) -> crate::rows::TempDb {
        crate::rows::TempDb::new("half", name)
    }

    fn batch(lang: &str, half: Option<&str>, files: &[&str], rows: usize) -> Batch {
        let mut b = Batch {
            all: true,
            first: true,
            final_: true,
            lang: lang.to_string(),
            half: half.map(|h| h.to_string()),
            ..Default::default()
        };
        for (i, f) in files.iter().enumerate() {
            b.shas.insert((*f).to_string(), json!(format!("sha{i}")));
            b.tables
                .entry("files")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(json!({"id": format!("f:{}{i}", lang), "path": f, "sha": format!("sha{i}")}));
        }
        let bindings: Vec<_> = (0..rows)
            .map(|i| json!({"id": format!("b:{}{i}", lang), "name": format!("n{i}")}))
            .collect();
        b.tables.insert("bindings".to_string(), json!(bindings));
        b
    }

    #[test]
    fn a_whole_replacement_half_leaves_no_row_of_its_own_behind() {
        let db = tmp("whole");
        // Two runs of the same half. The second must REPLACE, not add: `bindings` has no
        // `file` column, so dropping by file would leave every row of the first run.
        apply(&db, ".", &batch("typescript", Some("typescript"), &["a.ts"], 3)).unwrap();
        let receipt = apply(&db, ".", &batch("typescript", Some("typescript"), &["a.ts"], 3)).unwrap();
        assert_eq!(receipt.written.get("bindings"), Some(&3));
    }

    #[test]
    fn it_never_touches_another_halfs_rows() {
        let db = tmp("other");
        // The C# half writes first, by file and stamped only with `lang`.
        apply(&db, ".", &batch("csharp", None, &["a.cs"], 2)).unwrap();
        // Then the whole-replacement half runs twice.
        apply(&db, ".", &batch("typescript", Some("typescript"), &["a.ts"], 3)).unwrap();
        let receipt = apply(&db, ".", &batch("typescript", Some("typescript"), &["a.ts"], 3)).unwrap();
        // Its own 3, plus the 2 the other half wrote and nobody deleted.
        assert_eq!(receipt.written.get("bindings"), Some(&5));
        assert_eq!(receipt.written.get("files"), Some(&2));
    }

    #[test]
    fn ids_may_restart_only_while_this_half_is_the_only_one_here() {
        let db = tmp("alone");
        assert!(alone(&db, "typescript"), "an empty database is nobody else's");

        apply(&db, ".", &batch("typescript", Some("typescript"), &["a.ts"], 1)).unwrap();
        assert!(alone(&db, "typescript"), "its own rows do not make it crowded");

        apply(&db, ".", &batch("csharp", None, &["a.cs"], 1)).unwrap();
        assert!(
            !alone(&db, "typescript"),
            "another half's files are here, so restarting `f:` would reuse an id it points at"
        );
    }

    #[test]
    fn the_setup_that_produced_the_rows_is_recorded_beside_them() {
        let db = tmp("setup");
        let mut b = batch("typescript", Some("typescript"), &["a.ts"], 1);
        b.setup = Some("toolchain-a".to_string());
        apply(&db, ".", &b).unwrap();
        // It lives OUTSIDE the tree, so no file moves when it changes and only this says so.
        assert_eq!(meta(&db, "setup:typescript"), "toolchain-a");
    }

    #[test]
    fn what_a_file_was_bound_through_comes_back_for_its_own_half_only() {
        let db = tmp("deps");
        let mut b = batch("python", None, &["main.py"], 1);
        b.deps = Some(r#"{"main.py":["lib/util.py"]}"#.to_string());
        apply(&db, ".", &b).unwrap();
        // A later half writing the same database must not take the key with it.
        apply(&db, ".", &batch("csharp", None, &["a.cs"], 1)).unwrap();
        let state = super::super::state(&db, "python", false);
        assert_eq!(state.deps.as_deref(), Some(r#"{"main.py":["lib/util.py"]}"#));
        assert_eq!(super::super::state(&db, "csharp", false).deps, None);
    }
}
