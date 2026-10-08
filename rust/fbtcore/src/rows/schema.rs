//! THE SCHEMA IS DERIVED FROM THE ROWS, never declared here.
//!
//! Every table is whatever columns its rows carry, because the extractor decides what
//! a row is. A schema written twice disagrees with itself the first time a column is
//! added.

use anyhow::Result;
use rusqlite::{types::ToSqlOutput, types::Value as SqlValue, Connection};
use serde_json::Value;
use std::collections::HashSet;

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

/// One cell. A list or an object becomes JSON text, which SQLite can still search and
/// `json_each` can open.
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
        // A PAYLOAD CELL STILL IN THE TEXT NODE SENT - see `rawcells`: decoded here, one at a time, and spelled
        // exactly as a decoded one is.
        Some(other) if super::rawcells::text_of(other).is_some() => {
            let text = super::rawcells::text_of(other).unwrap_or("null");
            SqlValue::Text(super::pyjson::dumps(&serde_json::from_str(text).unwrap_or(Value::Null)))
        }
        // A list or an object, spelled as python spells it - see pyjson::dumps.
        Some(other) => SqlValue::Text(super::pyjson::dumps(other)),
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
