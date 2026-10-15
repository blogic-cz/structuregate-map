//! `_meta`, THE ONE WAY IN AND OUT. A key's value is TEXT when it is a scalar - a version, a count, a path, a
//! hash - and NODES when it is a list or an object: the structure goes to `_meta__value` through `cells` (owner =
//! the key, `_meta.value` NULL), never into the cell as JSON text, as every other structure in the map does.
//!
//! `get` hands a structure back as compact JSON TEXT, because what reads one either parses it or hands it on
//! whole (a half's state, a lens). Every read and write of `_meta` goes through here, so a structure stored as
//! nodes is never read as a NULL by a reader that only knew the column.

use super::{child, deep, is_structured, Reader, Writer};
use crate::rows::schema;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

pub const TABLE: &str = "_meta";
pub const COLUMN: &str = "value";

/// `_meta`, made once.
pub fn ensure(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS _meta (key TEXT, value TEXT)")?;
    Ok(())
}

fn nodes_table() -> String {
    child(TABLE, COLUMN)
}

fn has_nodes(db: &Connection) -> bool {
    schema::existing_columns(db, &nodes_table()).map(|c| !c.is_empty()).unwrap_or(false)
}

/// A value read back: the text, or the structure its nodes rebuild, as compact JSON.
fn value_of(reader: &mut Option<Reader<'_>>, key: &str, text: Option<String>) -> Option<String> {
    if let Some(text) = text {
        return Some(text);
    }
    let value = reader.as_mut()?.get(key).ok()??;
    serde_json::to_string(&value).ok()
}

fn reader(db: &Connection) -> Option<Reader<'_>> {
    if has_nodes(db) { Reader::new(db, &nodes_table()).ok() } else { None }
}

/// One key's value, or None when it is not there (or `_meta` is not).
pub fn get(db: &Connection, key: &str) -> Option<String> {
    let text: Option<String> = db.query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| r.get(0)).ok()?;
    let mut nodes = if text.is_none() { reader(db) } else { None };
    value_of(&mut nodes, key, text)
}

/// Every `(key, value)` whose key matches `like` (SQL LIKE; None for all of them), in the order they were written.
pub fn scan(db: &Connection, like: Option<&str>) -> Vec<(String, String)> {
    let sql = match like {
        Some(_) => "SELECT key, value FROM _meta WHERE key LIKE ?1 ORDER BY rowid",
        None => "SELECT key, value FROM _meta ORDER BY rowid",
    };
    let Ok(mut stmt) = db.prepare(sql) else { return Vec::new() };
    let mapped = |r: &rusqlite::Row<'_>| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?));
    let rows: Vec<(String, Option<String>)> = match like {
        Some(pattern) => stmt.query_map([pattern], mapped).map(|r| r.flatten().collect()),
        None => stmt.query_map([], mapped).map(|r| r.flatten().collect()),
    }
    .unwrap_or_default();
    let mut nodes = reader(db);
    rows.into_iter().filter_map(|(key, text)| value_of(&mut nodes, &key, text).map(|v| (key, v))).collect()
}

/// One key, gone with its nodes.
pub fn delete(db: &Connection, key: &str) -> Result<()> {
    ensure(db)?;
    if has_nodes(db) {
        db.execute(&format!("DELETE FROM \"{}\" WHERE owner = ?1", nodes_table()), [key])?;
    }
    db.execute("DELETE FROM _meta WHERE key = ?1", [key])?;
    Ok(())
}

/// Every key, gone with its nodes - the start of a whole rewrite.
pub fn clear(db: &Connection) -> Result<()> {
    ensure(db)?;
    if has_nodes(db) {
        db.execute_batch(&format!("DELETE FROM \"{}\"", nodes_table()))?;
    }
    db.execute_batch("DELETE FROM _meta")?;
    Ok(())
}

/// One key REPLACED: a list or an object as nodes, anything else as text. `_meta` has no unique key, so the old
/// row is deleted first - an `INSERT OR REPLACE` only added a second one.
pub fn put(db: &Connection, key: &str, value: &str) -> Result<()> {
    delete(db, key)?;
    let structure = deep(value).ok().filter(is_structured);
    match structure {
        Some(structure) => {
            db.execute("INSERT INTO _meta (key, value) VALUES (?1, NULL)", [key])?;
            Writer::new(db, TABLE, COLUMN)?.tree(key, &structure)?;
        }
        None => {
            db.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [key, value])?;
        }
    }
    Ok(())
}

/// A value that is already a structure, put without a trip through its text.
pub fn put_value(db: &Connection, key: &str, value: &Value) -> Result<()> {
    match value {
        Value::String(text) => put(db, key, text),
        other => put(db, key, &serde_json::to_string(other)?),
    }
}

/// Whether `_meta` is there at all.
pub fn exists(db: &Connection) -> bool {
    db.query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1", [TABLE], |_| Ok(())).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_structure_is_nodes_and_comes_back_the_same() {
        let db = Connection::open_in_memory().unwrap();
        let text = r#"{"b":[1,2.5,"x",null,true,{}],"a":{"z":[]},"k":"v"}"#;
        put(&db, "spec:x", text).unwrap();
        put(&db, "schema", "12").unwrap();
        put(&db, "skip:x", "abc\nnote").unwrap();
        let held: Option<String> = db.query_row("SELECT value FROM _meta WHERE key = 'spec:x'", [], |r| r.get(0)).unwrap();
        assert_eq!(held, None, "a structure never lands in the cell as text");
        let nodes: i64 = db.query_row("SELECT count(*) FROM _meta__value WHERE owner = 'spec:x'", [], |r| r.get(0)).unwrap();
        assert!(nodes > 5);
        assert_eq!(get(&db, "spec:x").as_deref(), Some(text));
        assert_eq!(get(&db, "schema").as_deref(), Some("12"));
        assert_eq!(get(&db, "skip:x").as_deref(), Some("abc\nnote"));
        assert_eq!(get(&db, "absent"), None);
        // Replaced, not added to.
        put(&db, "spec:x", "[1]").unwrap();
        assert_eq!(get(&db, "spec:x").as_deref(), Some("[1]"));
        let keys: Vec<String> = scan(&db, None).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["schema", "skip:x", "spec:x"]);
        assert_eq!(scan(&db, Some("sp%")), vec![("spec:x".to_string(), "[1]".to_string())]);
        clear(&db).unwrap();
        let left: i64 = db.query_row("SELECT count(*) FROM _meta__value", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "a clear takes the nodes too");
    }
}
