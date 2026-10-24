//! THE SLICE'S ROWS, copied from the base map through an attached connection - nothing is re-extracted.
//!
//! Every table is created as the base declares it. A table with a `file` column keeps the rows of the sliced files (a
//! `file` is a `files` id, or a map path where a table keeps paths); one with an `owner_file` and no `file` (the
//! TypeScript half's), the rows those files own; one with neither is a WHOLE-PROGRAM table (a closure, a graph, the
//! http and sql links, `_meta`) and is copied whole, so none of them is truncated. A node table (`<table>__<column>`)
//! keeps the node rows of the rows its table kept - by `id`, or by `#<rowid>` where a row has none, so every row keeps
//! its ROWID. `files` is copied whole, with `sliced` = 1 on the slice's files. The full-text tables are left out: the
//! slice says it holds no text (`options:text` = 0), as a `--map-no-text` map does.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::{BTreeSet, HashSet};

fn columns(db: &Connection, schema: &str, table: &str) -> Result<Vec<String>> {
    let mut stmt = db.prepare(&format!("SELECT name FROM \"{schema}\".pragma_table_info(?1)"))?;
    let rows = stmt.query_map([table], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn quoted(names: &[String]) -> String {
    names.iter().map(|c| format!("\"{}\"", c.replace('"', "\"\""))).collect::<Vec<_>>().join(", ")
}

pub fn write(base: &str, out_db: &str, kept: &BTreeSet<String>) -> Result<()> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(format!("{out_db}{suffix}"));
    }
    let db = Connection::open(out_db).with_context(|| format!("open {out_db}"))?;
    db.execute("ATTACH DATABASE ?1 AS b", [base])?;
    db.execute_batch("BEGIN; CREATE TEMP TABLE slice_keep (id TEXT PRIMARY KEY, path TEXT);")?;
    {
        let mut add = db.prepare("INSERT INTO temp.slice_keep (id, path) SELECT id, path FROM b.files WHERE id = ?1")?;
        for id in kept {
            add.execute([id])?;
        }
    }
    let tables: Vec<(String, String)> = {
        let mut stmt = db.prepare("SELECT name, sql FROM b.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' AND sql IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    // THE FULL-TEXT TABLES AND THEIR SHADOWS: a virtual table and every `<name>_*` table it keeps.
    let virtuals: Vec<String> = tables.iter().filter(|(_, sql)| sql.trim_start().to_uppercase().starts_with("CREATE VIRTUAL")).map(|(n, _)| n.clone()).collect();
    let text = |name: &str| virtuals.iter().any(|v| name == v || name.starts_with(&format!("{v}_")));
    let mut whole: HashSet<String> = HashSet::new();
    let mut nodes: Vec<String> = Vec::new();
    for (name, sql) in &tables {
        if text(name) {
            continue;
        }
        db.execute_batch(sql).with_context(|| format!("create {name}"))?;
        if name.contains("__") {
            nodes.push(name.clone());
            continue;
        }
        let held = columns(&db, "b", name)?;
        let rowid = !sql.to_uppercase().contains("WITHOUT ROWID");
        let (into, from) = if rowid { (format!("rowid, {}", quoted(&held)), format!("rowid, {}", quoted(&held))) } else { (quoted(&held), quoted(&held)) };
        let filter = if name == "files" {
            whole.insert(name.clone());
            String::new()
        } else if held.iter().any(|c| c == "file") {
            " WHERE file IN (SELECT id FROM temp.slice_keep) OR file IN (SELECT path FROM temp.slice_keep)".to_string()
        } else if held.iter().any(|c| c == "owner_file") {
            " WHERE owner_file IN (SELECT id FROM temp.slice_keep)".to_string()
        } else {
            whole.insert(name.clone());
            String::new()
        };
        db.execute_batch(&format!("INSERT INTO main.\"{name}\" ({into}) SELECT {from} FROM b.\"{name}\"{filter};")).with_context(|| format!("copy {name}"))?;
    }
    for name in &nodes {
        let parent = &name[..name.find("__").unwrap_or(name.len())];
        let held = columns(&db, "b", name)?;
        let filter = if whole.contains(parent) || columns(&db, "main", parent)?.is_empty() {
            String::new()
        } else {
            let by_id = if columns(&db, "main", parent)?.iter().any(|c| c == "id") { format!("owner IN (SELECT id FROM main.\"{parent}\") OR ") } else { String::new() };
            format!(" WHERE {by_id}owner IN (SELECT '#' || rowid FROM main.\"{parent}\")")
        };
        db.execute_batch(&format!("INSERT INTO main.\"{name}\" ({cols}) SELECT {cols} FROM b.\"{name}\"{filter};", cols = quoted(&held)))
            .with_context(|| format!("copy {name}"))?;
    }
    db.execute_batch("ALTER TABLE main.files ADD COLUMN sliced; UPDATE main.files SET sliced = (id IN (SELECT id FROM temp.slice_keep));")?;
    let indexes: Vec<String> = {
        let mut stmt = db.prepare("SELECT sql FROM b.sqlite_master WHERE type = 'index' AND sql IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let made: HashSet<String> = tables.iter().filter(|(n, _)| !text(n)).map(|(n, _)| n.clone()).collect();
    for sql in indexes {
        // An index of a table the slice left out (full text) is not made.
        let on = sql.split(" ON ").nth(1).map(|t| t.split('(').next().unwrap_or("").trim().trim_matches('"').to_string()).unwrap_or_default();
        if made.contains(&on) {
            db.execute_batch(&sql)?;
        }
    }
    db.execute_batch("COMMIT; DETACH DATABASE b;")?;
    crate::rows::meta::put(&db, crate::rows::cells::text::KEY, "0")?;
    Ok(())
}
