//! THE SCHEMA IS DERIVED FROM THE ROWS, never declared here.
//!
//! Every table is whatever columns its rows carry, because the extractor decides what
//! a row is. A schema written twice disagrees with itself the first time a column is
//! added.

use anyhow::Result;
use rusqlite::{types::ToSqlOutput, types::Value as SqlValue, Connection};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// The columns that join to `files.id`, indexed wherever they appear.
///
/// Named once rather than per table: every row an extractor produces carries `file`,
/// and an unindexed join column is the difference between an index probe and a full
/// scan of the largest table.
pub const JOIN_COLUMNS: &[&str] = &[
    "file", "cls", "func", "name", "module", "callee", "target", "path", "qualname", "kind",
    "value", "lang", "symbol", "type", "project", "call", "const", "const_kind", "object",
    "parent",
];

/// One scalar cell. A list or an object is not a cell: it is nodes in a child table (`rows/cells/mod.rs`).
pub fn cell(raw: Option<&Value>) -> ToSqlOutput<'static> {
    let value = match raw {
        None | Some(Value::Null) => SqlValue::Null,
        Some(Value::Bool(b)) => SqlValue::Integer(if *b { 1 } else { 0 }),
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else {
                SqlValue::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        Some(Value::String(s)) => SqlValue::Text(s.clone()),
        // A LIST OR AN OBJECT NEVER REACHES A COLUMN: `store::insert` writes it as nodes (`cells`).
        Some(_) => SqlValue::Null,
    };
    ToSqlOutput::Owned(value)
}

/// Every column any row in the table carries, `id` first so a table reads like one.
pub fn columns_of(rows: &[Value]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut known: HashSet<&str> = HashSet::new();
    for row in rows {
        if let Some(object) = row.as_object() {
            for key in object.keys() {
                if known.insert(key.as_str()) {
                    seen.push(key.clone());
                }
            }
        }
    }
    seen.sort_by(|a, b| (a != "id", a).cmp(&(b != "id", b)));
    seen
}

/// The columns a table already has, in declaration order. Empty when it does not exist.
pub fn existing_columns(db: &Connection, table: &str) -> Result<Vec<String>> {
    let sql = format!("PRAGMA table_info(\"{}\")", escape(table));
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn create(db: &Connection, table: &str, columns: &[String]) -> Result<()> {
    let quoted: Vec<String> = columns.iter().map(|c| format!("\"{}\"", escape(c))).collect();
    db.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        escape(table),
        quoted.join(", ")
    ))?;
    for column in columns {
        if column == "id" || JOIN_COLUMNS.contains(&column.as_str()) {
            index(db, table, column)?;
        }
    }
    Ok(())
}

/// The columns the table is MISSING, added to it.
///
/// TWO EXTRACTORS FILL THE SAME TABLES and they do not carry identical columns — a C#
/// `using` has a `kind` a python `import` does not. Whichever half ran first would
/// otherwise decide the shape of the table, and the second half's extra column would be
/// dropped on every insert: the row would land, silently missing the one field the
/// query was written for.
pub fn widen(db: &Connection, table: &str, existing: &mut Vec<String>, columns: &[String]) -> Result<()> {
    for column in columns {
        if existing.iter().any(|e| e == column) {
            continue;
        }
        db.execute_batch(&format!(
            "ALTER TABLE \"{}\" ADD COLUMN \"{}\"",
            escape(table),
            escape(column)
        ))?;
        existing.push(column.clone());
        if JOIN_COLUMNS.contains(&column.as_str()) {
            index(db, table, column)?;
        }
    }
    Ok(())
}

/// EVERY COLUMN A HALF'S SPEC DECLARES A JOIN (`spec:<half>` -> `joins`, keyed `table.column`), plus the columns a
/// reader looks a row up by (`extra`), indexed - the map is opened read-only, so a consumer cannot add one, and
/// `WHERE calls.target_id = ?` scanned the table. A name the spec gives with no table or column here is skipped:
/// the spec also names node tables and columns a run may not have produced.
/// AN INDEX IS SORTED ON EVERY CORE: SQLite's sorter merges its runs on worker threads (`PRAGMA threads`) - but only
/// when the page cache leaves it runs to merge. Under the write's own large cache it sorted every index in memory on
/// one thread; with the default cache and four threads `expressions__ast`'s two indexes took two thirds of the time.
/// `then` is the cache the caller goes back to (KiB, as `cache_size` takes a negative one); no index differs.
pub fn sorted_on_every_core<T>(db: &Connection, then: i64, build: impl FnOnce() -> Result<T>) -> Result<T> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
    db.execute_batch(&format!("PRAGMA cache_size = -2000; PRAGMA threads = {threads};"))?;
    let built = build();
    db.execute_batch(&format!("PRAGMA threads = 0; PRAGMA cache_size = -{then};"))?;
    built
}

pub fn index_declared(db: &Connection, extra: &[(&str, &str)]) -> Result<usize> {
    sorted_on_every_core(db, 2000, || declared(db, extra))
}

fn declared(db: &Connection, extra: &[(&str, &str)]) -> Result<usize> {
    let mut wanted: BTreeSet<(String, String)> = extra.iter().map(|(t, c)| (t.to_string(), c.to_string())).collect();
    for (_, raw) in super::meta::scan(db, Some("spec:%")) {
        let Ok(spec) = serde_json::from_str::<Value>(&raw) else { continue };
        for key in spec.get("joins").and_then(Value::as_object).into_iter().flat_map(|j| j.keys()) {
            if let Some((table, column)) = key.split_once('.') {
                wanted.insert((table.to_string(), column.to_string()));
            }
        }
    }
    let mut made = 0;
    let mut columns: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (table, column) in wanted {
        if !columns.contains_key(&table) {
            columns.insert(table.clone(), existing_columns(db, &table)?);
        }
        if columns[&table].contains(&column) {
            index(db, &table, &column)?;
            made += 1;
        }
    }
    Ok(made)
}

/// One column's index, made when it is not there yet - what a reader that probes the column by value needs.
pub fn index_column(db: &Connection, table: &str, column: &str) -> Result<()> {
    index(db, table, column)
}

fn index(db: &Connection, table: &str, column: &str) -> Result<()> {
    db.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS \"ix_{t}_{c}\" ON \"{t}\" (\"{c}\")",
        t = escape(table),
        c = escape(column)
    ))?;
    Ok(())
}

/// A table or column name inside a quoted identifier. SQLite escapes a double quote by
/// doubling it; nothing else needs touching, and the names come from the extractor
/// rather than from a user.
pub fn escape(name: &str) -> String {
    name.replace('"', "\"\"")
}

/// The tables a half may be asked to count.
///
/// Anything starting with `_` is this tool's own bookkeeping, and `file_text_*` are the
/// shadow tables FTS5 creates beside `file_text` itself.
pub fn listable_tables(db: &Connection) -> Result<Vec<String>> {
    let mut stmt = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let name = row?;
        if !name.starts_with('_') && !name.starts_with("file_text_") {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// How many rows a table holds, or 0 when it is not there at all.
pub fn count_of(db: &Connection, table: &str) -> i64 {
    db.query_row(
        &format!("SELECT count(*) FROM \"{}\"", escape(table)),
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}
