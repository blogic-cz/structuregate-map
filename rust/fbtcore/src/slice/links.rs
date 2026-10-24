//! WHICH FILES A SLICE HOLDS: the listed ones and the files whose rows a RESOLVED link points into them, hop by hop.
//!
//! A link is a column whose value is a declaration's row id, read back to that row's file through every table holding
//! `id` and `file` (the id's prefix names the table: `fn:12` is a `functions` row) - a call's `target_id` and `callee_id`,
//! a literal's or function's `target_id`, a `sql_links` row's `object` - or a column naming a file outright: an
//! import's `resolved_file`, an imported name's `declared_file`, a C# `file_refs` row's `target`; `http_links` joins a
//! request to the endpoint it reaches. A `file` value is a `files` id or a map path (`http_calls` keeps paths), so both
//! are read back to the id. A dependent is the file the link is written IN; the walk follows links backwards from the
//! listed files, `hops` deep, a visited set ending it. Nothing is matched by a name.

use anyhow::Result;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A column of a table that names a row (joined through the id tables) or a file (read as one).
const ROW_LINKS: [(&str, &str); 8] = [
    ("calls", "target_id"),
    ("calls", "callee_id"),
    ("string_literals", "target_id"),
    ("number_literals", "target_id"),
    ("template_literals", "target_id"),
    ("functions", "target_id"),
    ("sql_links", "object"),
    ("sql_links", "column_id"),
];
const FILE_LINKS: [(&str, &str); 3] = [("imports", "resolved_file"), ("import_names", "declared_file"), ("file_refs", "target")];

fn columns(db: &Connection, table: &str) -> Vec<String> {
    let Ok(mut stmt) = db.prepare("SELECT name FROM pragma_table_info(?1)") else { return vec![] };
    stmt.query_map([table], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// `files` read both ways: id -> id and path -> id.
fn file_ids(db: &Connection) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut stmt = db.prepare("SELECT id, path FROM files")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, path) = row?;
        out.insert(path, id.clone());
        out.insert(id.clone(), id);
    }
    Ok(out)
}

/// The listed paths as `files` ids, and the ones the base map does not hold - by PATH only, an id spelled in the list is no file.
pub fn resolve(db: &Connection, listed: &BTreeSet<String>) -> Result<(BTreeSet<String>, Vec<String>)> {
    let mut by_path = HashMap::new();
    let mut stmt = db.prepare("SELECT id, path FROM files")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, path) = row?;
        by_path.insert(path, id);
    }
    let mut found = BTreeSet::new();
    let mut unknown = Vec::new();
    for path in listed {
        match by_path.get(path) {
            Some(id) => {
                found.insert(id.clone());
            }
            None => unknown.push(path.clone()),
        }
    }
    Ok((found, unknown))
}

/// The tables a row id is read back through: every one with `id` and `file`, by the prefix its ids carry.
fn id_tables(db: &Connection) -> Result<BTreeMap<String, Vec<String>>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let tables: Vec<String> = {
        let mut stmt = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '%\\_\\_%' ESCAPE '\\' AND name <> 'files'")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for table in tables {
        let held = columns(db, &table);
        if !(held.iter().any(|c| c == "id") && held.iter().any(|c| c == "file")) {
            continue;
        }
        let first: Option<String> = db.query_row(&format!("SELECT id FROM \"{table}\" WHERE id IS NOT NULL LIMIT 1"), [], |r| r.get(0)).ok();
        if let Some(prefix) = first.as_deref().and_then(|id| id.split_once(':')).map(|(p, _)| p.to_string()) {
            out.entry(prefix).or_default().push(table);
        }
    }
    Ok(out)
}

/// Every link as (the file it is written in, the file it points into), both as `files` ids.
fn edges(db: &Connection) -> Result<BTreeSet<(String, String)>> {
    let ids = file_ids(db)?;
    let norm = |v: String| ids.get(&v).cloned();
    let mut out = BTreeSet::new();
    let mut add = |rows: Vec<(String, String)>| {
        for (from, to) in rows {
            if let (Some(f), Some(t)) = (norm(from), norm(to)) {
                if f != t {
                    out.insert((f, t));
                }
            }
        }
    };
    let pairs = |sql: &str| -> Result<Vec<(String, String)>> {
        let mut stmt = db.prepare(sql)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    };
    let targets = id_tables(db)?;
    for (table, column) in ROW_LINKS {
        let held = columns(db, table);
        if !(held.iter().any(|c| c == "file") && held.iter().any(|c| c == column)) {
            continue;
        }
        for (prefix, into) in &targets {
            for target in into {
                add(pairs(&format!(
                    "SELECT DISTINCT a.file, b.file FROM \"{table}\" a JOIN \"{target}\" b ON b.id = a.\"{column}\" \
                     WHERE a.\"{column}\" LIKE '{prefix}:%' AND a.file IS NOT NULL AND b.file IS NOT NULL"
                ))?);
            }
        }
    }
    for (table, column) in FILE_LINKS {
        let held = columns(db, table);
        if held.iter().any(|c| c == "file") && held.iter().any(|c| c == column) {
            add(pairs(&format!("SELECT DISTINCT file, \"{column}\" FROM \"{table}\" WHERE file IS NOT NULL AND \"{column}\" IS NOT NULL"))?);
        }
    }
    if ["http_links", "http_calls", "http_endpoints"].iter().all(|t| !columns(db, t).is_empty()) {
        add(pairs(
            "SELECT DISTINCT c.file, e.file FROM http_links l JOIN http_calls c ON c.id = l.call JOIN http_endpoints e ON e.id = l.endpoint \
             WHERE c.file IS NOT NULL AND e.file IS NOT NULL",
        )?);
    }
    Ok(out)
}

/// The listed files and their dependents, `hops` deep.
pub fn dependents(db: &Connection, listed: &BTreeSet<String>, hops: usize) -> Result<BTreeSet<String>> {
    let mut into: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in edges(db)? {
        into.entry(to).or_default().push(from);
    }
    let mut kept = listed.clone();
    let mut front: Vec<String> = listed.iter().cloned().collect();
    for _ in 0..hops {
        let mut next = Vec::new();
        for file in &front {
            for from in into.get(file).into_iter().flatten() {
                if kept.insert(from.clone()) {
                    next.push(from.clone());
                }
            }
        }
        if next.is_empty() {
            break;
        }
        front = next;
    }
    Ok(kept)
}
