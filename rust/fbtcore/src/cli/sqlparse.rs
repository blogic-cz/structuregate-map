//! `--sql-parse <file|->`: ONE SQL TEXT, read as the SQL half reads a file of a tree - no tree, no database file, no
//! host - and printed as JSON: its statements, what they touch, the columns they name, their shape, their trees and
//! the parser's errors. For a service handed one query at a time, which otherwise wrote it into a temp tree, built
//! a database and queried it back.
//!
//! THE SAME CODE AS THE MAP, not a second implementation of it: the half's rows are written into an IN-MEMORY
//! database through the store's own insert, the statement pass (`rows/sqlshape/`) and its views run over it, and
//! what is printed is read back out of it. Exit 1 when the text does not parse.

use super::Out;
use crate::mapper::{Deep, Free};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::io::Read;

pub fn run(source: &str, dialect: &str, deep: Deep, free: Free, out: &mut Out) -> i64 {
    let (text, name) = if source == "-" {
        let mut text = String::new();
        if let Err(why) = std::io::stdin().read_to_string(&mut text) {
            out.error(format!("structuregate: --sql-parse could not read stdin - {why}"));
            return 2;
        }
        (text, "-".to_string())
    } else {
        match std::fs::read_to_string(source) {
            Ok(text) => (text, std::path::Path::new(source).file_name().map_or_else(|| source.to_string(), |n| n.to_string_lossy().into_owned())),
            Err(why) => {
                out.error(format!("structuregate: --sql-parse could not read {source} - {why}"));
                return 2;
            }
        }
    };
    let reply = crate::mapper::callback(deep, free, &json!({ "mode": "sql-parse", "text": text, "name": name, "dialect": dialect }).to_string());
    let payload: Value = serde_json::from_str(&reply).unwrap_or_default();
    let errors: Vec<&str> = payload["errors"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !errors.is_empty() || !payload["tables"].is_object() {
        for error in errors {
            out.error(format!("structuregate: {error}"));
        }
        out.error("structuregate: --sql-parse got no rows back from the SQL half".into());
        return 2;
    }
    match report(&payload, dialect) {
        Ok((report, broken)) => {
            out.json(report, true);
            i64::from(broken)
        }
        Err(why) => {
            out.error(format!("structuregate: --sql-parse could not derive the statements - {why}"));
            2
        }
    }
}

/// What the text is, and whether it failed to parse.
fn report(payload: &Value, dialect: &str) -> anyhow::Result<(Value, bool)> {
    let mut conn = Connection::open_in_memory()?;
    for (table, rows) in payload["tables"].as_object().into_iter().flatten() {
        if let Some(rows) = rows.as_array() {
            crate::rows::store::insert_rows(&conn, table, rows)?;
        }
    }
    crate::rows::sqlshape::run_on(&mut conn)?;
    let list = |sql: &str| rows(&conn, sql);
    let diagnostics = list("SELECT line, name, kind, source AS message FROM diagnostics ORDER BY rowid")?;
    let broken = diagnostics.iter().any(|d| d["kind"] == "error");
    let report = json!({
        "dialect": dialect,
        "statements": list("SELECT id, parent, object, batch, ordinal, line, end_line, col, kind, source, fingerprint FROM sql_statements ORDER BY rowid")?,
        "objects": list("SELECT id, kind, schema, name, parent, line, end_line FROM sql_objects ORDER BY rowid")?,
        "refs": list("SELECT statement, object, line, action, schema, name, \"column\" FROM sql_refs ORDER BY rowid")?,
        "column_refs": list("SELECT statement, object, line, role, schema, name, source_kind, alias, \"column\" FROM sql_column_refs ORDER BY rowid")?,
        "clauses": list("SELECT statement, kind, line FROM sql_clauses ORDER BY statement, line")?,
        "joins": list("SELECT statement, type, joined, schema, name, alias, on_line, line FROM sql_joins ORDER BY statement, line")?,
        "functions": list("SELECT statement, name, windowed, line FROM sql_functions ORDER BY statement, line")?,
        "ctes": list("SELECT statement, name, line FROM sql_ctes ORDER BY statement, line")?,
        "nodes": list("SELECT statement, n, parent, ord, kind, value, line FROM sql_nodes ORDER BY rowid")?,
        "diagnostics": diagnostics,
    });
    Ok((report, broken))
}

/// Every row of a query as an object by column name. A table this text gave no rows to is an empty list.
fn rows(conn: &Connection, sql: &str) -> anyhow::Result<Vec<Value>> {
    let mut stmt = match conn.prepare(sql) {
        Ok(stmt) => stmt,
        Err(rusqlite::Error::SqliteFailure(_, Some(why))) if why.starts_with("no such table") => return Ok(Vec::new()),
        Err(why) => return Err(why.into()),
    };
    let names: Vec<String> = stmt.column_names().iter().map(|n| n.to_string()).collect();
    let mut found = stmt.query([])?;
    let mut all = Vec::new();
    while let Some(row) = found.next()? {
        let mut object = Map::new();
        for (i, name) in names.iter().enumerate() {
            let value = match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(r) => json!(r),
                ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Blob(_) => Value::Null,
            };
            object.insert(name.clone(), value);
        }
        all.push(Value::Object(object));
    }
    Ok(all)
}
