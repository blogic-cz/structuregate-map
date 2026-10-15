//! A LIST OR AN OBJECT IS NODES, NEVER JSON TEXT.
//!
//! A row's cell that holds a structure - `expressions.ast`, `assignments.reads`, `enums.members` - is not
//! written to its table at all. It goes to a CHILD TABLE named `<table>__<column>`, one row per node of the
//! structure, so a template read joins a member in plain SQL and `json_each` is never needed:
//!
//! ```sql
//! SELECT n.owner, m.value FROM expressions__ast n
//!   JOIN expressions__ast k ON k.parent = n.id AND k.role = 'k' AND k.value = 'Read'
//!   JOIN expressions__ast m ON m.parent = n.id AND m.role = 'name';
//! ```
//!
//! EVERY VALUE IS A ROW, every member of an object included: `id` (`<owner>/<n>`, n its preorder index), `owner`
//! (the id of the row the cell belongs to), `parent` (the node it hangs off, NULL for the root), `role` (its key in
//! the parent object; an item of a member list carries the list's key too, NULL in a top-level list), `ord` (its
//! index in a list), `node` - `object`, `list`, `string`, `number`, `bool` or `null` - and `value`, a scalar's own
//! (a boolean 0/1). THE COLUMNS ARE FIXED: a member's NAME is a `role`, never a column, because names are data as
//! often as they are fields - an object literal's own keys, a property map, a document's headers - and a column per
//! name was a table past SQLite's column limit, with `Home` and `home` one column. The root of every cell is one
//! node, so `[]` and `{}` are rows too and an absent cell is no row at all.
//!
//! The child rows carry the parent's `file`, `owner_file` and `half`, so every delete that scopes the parent
//! (`drop_files`, `drop_half`, a partial run's `owner_file`) takes its nodes with it. `_cells` records which
//! columns of which table went where.
//!
//! NOTHING HERE RECURSES: a tree is as deep as the source made it, and a walk that recursed would trade a cap
//! for a stack overflow.

use super::schema;
use anyhow::Result;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Between a table and its column in a child table's name.
pub const SEP: &str = "__";
/// What records the child tables.
pub const REGISTRY: &str = "_cells";

/// The parent's columns a node row inherits, so the deletes that scope the parent scope it too.
const INHERITED: &[&str] = &["file", "owner_file"];

/// The table a column's structures are written to.
pub fn child(table: &str, column: &str) -> String {
    format!("{table}{SEP}{column}")
}

/// Is this a structure: a list, an object, or one kept as its text (`rawcells`)?
pub fn is_structured(value: &Value) -> bool {
    matches!(value, Value::Array(_) | Value::Object(_))
}

/// A cell kept as its text, decoded. No depth limit: the source decides how deep a tree is.
pub fn decoded(value: &Value) -> Value {
    match super::rawcells::text_of(value) {
        Some(text) => deep(text).unwrap_or(Value::Null),
        None => value.clone(),
    }
}

/// JSON text into a value, as deep as it goes.
pub fn deep(text: &str) -> serde_json::Result<Value> {
    use serde::Deserialize;
    let mut reader = serde_json::Deserializer::from_str(text);
    reader.disable_recursion_limit();
    let value = Value::deserialize(&mut reader)?;
    reader.end()?;
    Ok(value)
}

/// One structure as its node rows, appended to `out` in preorder.
pub fn flatten(owner: &str, value: &Value, inherit: &Map<String, Value>, out: &mut Vec<Value>) {
    // (value, parent id, role, ord)
    let mut stack: Vec<(&Value, Option<String>, Option<&str>, Option<usize>)> = vec![(value, None, None, None)];
    let mut n = 0usize;
    while let Some((value, parent, role, ord)) = stack.pop() {
        let id = format!("{owner}/{n}");
        n += 1;
        let mut row = inherit.clone();
        row.insert("id".into(), Value::String(id.clone()));
        row.insert("owner".into(), Value::String(owner.to_string()));
        row.insert("parent".into(), parent.map(Value::String).unwrap_or(Value::Null));
        row.insert("role".into(), role.map(|r| Value::String(r.to_string())).unwrap_or(Value::Null));
        row.insert("ord".into(), ord.map(Value::from).unwrap_or(Value::Null));
        let mut kids: Vec<(&Value, Option<String>, Option<&str>, Option<usize>)> = Vec::new();
        let (node, scalar) = match value {
            Value::Object(map) => {
                kids.extend(map.iter().map(|(key, member)| (member, Some(id.clone()), Some(key.as_str()), None)));
                ("object", Value::Null)
            }
            Value::Array(items) => {
                // The list's ROLE is its member name: an item of `args` says so, not just its index.
                kids.extend(items.iter().enumerate().map(|(i, item)| (item, Some(id.clone()), role, Some(i))));
                ("list", Value::Null)
            }
            Value::Null => ("null", Value::Null),
            Value::Bool(b) => ("bool", Value::from(i64::from(*b))),
            Value::Number(_) => ("number", value.clone()),
            Value::String(_) => ("string", value.clone()),
        };
        row.insert("node".into(), Value::from(node));
        row.insert("value".into(), scalar);
        out.push(Value::Object(row));
        // Pushed reversed so they pop in order: preorder, which is also the rowid order they are read back in.
        stack.extend(kids.into_iter().rev());
    }
}

/// Which columns hold a structure in some row, and which hold a non-null scalar in some row. A column in both
/// keeps its scalars in the table and its structures in the child table.
pub fn structured_columns(rows: &[Value]) -> (Vec<String>, std::collections::HashSet<String>) {
    let mut structured: Vec<String> = Vec::new();
    let mut scalar = std::collections::HashSet::new();
    for object in rows.iter().filter_map(Value::as_object) {
        for (key, value) in object {
            if is_structured(value) {
                if !structured.contains(key) {
                    structured.push(key.clone());
                }
            } else if !value.is_null() && !scalar.contains(key) {
                scalar.insert(key.clone());
            }
        }
    }
    (structured, scalar)
}

/// How many node rows are held before they are written: a large Angular workspace's `ast` is millions.
pub const NODE_CHUNK: usize = 50_000;

/// The node rows held for one column, written to its child table and cleared. The parent's stamp goes with
/// them, never `lang`: that is a `files` column.
pub fn write_nodes(db: &Connection, table: &str, column: &str, nodes: &mut Vec<Value>, stamp: &[(String, Value)]) -> Result<()> {
    if nodes.is_empty() {
        return Ok(());
    }
    let child_table = child(table, column);
    let stamp: Vec<(String, Value)> = stamp.iter().filter(|(name, _)| name != "lang").cloned().collect();
    super::store::insert(db, &child_table, nodes, &stamp)?;
    index_owner(db, &child_table)?;
    nodes.clear();
    Ok(())
}

/// The parent columns a node row inherits from the row its cell belongs to.
pub fn inherited(row: &Map<String, Value>) -> Map<String, Value> {
    INHERITED.iter().filter_map(|c| row.get(*c).filter(|v| !v.is_null()).map(|v| (c.to_string(), v.clone()))).collect()
}

/// `_cells`, made once.
pub fn register(db: &Connection, table: &str, column: &str) -> Result<()> {
    db.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {REGISTRY} (tbl TEXT, col TEXT, child TEXT, UNIQUE (tbl, col))"
    ))?;
    db.execute(
        &format!("INSERT OR IGNORE INTO {REGISTRY} (tbl, col, child) VALUES (?1, ?2, ?3)"),
        [table, column, &child(table, column)],
    )?;
    Ok(())
}

/// The owner index a child table is read by. Made with the table.
pub fn index_owner(db: &Connection, child_table: &str) -> Result<()> {
    let t = schema::escape(child_table);
    db.execute_batch(&format!("CREATE INDEX IF NOT EXISTS \"ix_{t}_owner\" ON \"{t}\" (owner)"))?;
    Ok(())
}

/// Which columns of `table` went to child tables: `(column, child table)`.
pub fn columns_of(db: &Connection, table: &str) -> Vec<(String, String)> {
    let sql = format!("SELECT col, child FROM {REGISTRY} WHERE tbl = ?1 ORDER BY rowid");
    let Ok(mut stmt) = db.prepare(&sql) else { return Vec::new() };
    stmt.query_map([table], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

/// Every child table of `table`, dropped with its record - for a table that is itself dropped and made again.
pub fn drop_children(db: &Connection, table: &str) -> Result<()> {
    for (_, child_table) in columns_of(db, table) {
        db.execute_batch(&format!("DROP TABLE IF EXISTS \"{}\"", schema::escape(&child_table)))?;
    }
    if schema::existing_columns(db, REGISTRY).map(|c| !c.is_empty()).unwrap_or(false) {
        db.execute(&format!("DELETE FROM {REGISTRY} WHERE tbl = ?1"), [table])?;
    }
    Ok(())
}

/// The table a child table belongs to, when it is one.
pub fn parent_of(db: &Connection, child_table: &str) -> Option<String> {
    db.query_row(&format!("SELECT tbl FROM {REGISTRY} WHERE child = ?1"), [child_table], |r| r.get(0)).ok()
}

/// One node row as read back: its columns by name.
pub struct NodeRows {
    names: Vec<String>,
    at: HashMap<String, usize>,
}

impl NodeRows {
    pub fn new(names: Vec<String>) -> NodeRows {
        let at = names.iter().cloned().zip(0..).collect();
        NodeRows { names, at }
    }

    /// The structure these rows are, in the order they were written (preorder). None for no rows.
    pub fn rebuild(&self, rows: Vec<Vec<SqlValue>>) -> Option<Value> {
        if rows.is_empty() {
            return None;
        }
        let col = |name: &str| self.at.get(name).copied();
        let (id_at, parent_at, role_at, node_at, value_at) = (col("id")?, col("parent")?, col("role")?, col("node")?, col("value")?);
        let mut index: HashMap<String, usize> = HashMap::with_capacity(rows.len());
        let mut kids: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
        let mut root = None;
        for (i, row) in rows.iter().enumerate() {
            if let SqlValue::Text(id) = &row[id_at] {
                index.insert(id.clone(), i);
            }
            match &row[parent_at] {
                SqlValue::Text(parent) => {
                    if let Some(&p) = index.get(parent) {
                        kids[p].push(i);
                    }
                }
                _ => root = root.or(Some(i)),
            }
        }
        // BOTTOM UP, without recursion: a node's children come after it in preorder, so walking backwards
        // finds every child built before its parent.
        let mut built: Vec<Option<Value>> = vec![None; rows.len()];
        for i in (0..rows.len()).rev() {
            let row = &rows[i];
            let kind = match &row[node_at] {
                SqlValue::Text(t) => t.as_str(),
                _ => "null",
            };
            let value = match kind {
                "object" => {
                    let mut map = Map::new();
                    for &k in &kids[i] {
                        if let SqlValue::Text(role) = &rows[k][role_at] {
                            map.insert(role.clone(), built[k].take().unwrap_or(Value::Null));
                        }
                    }
                    Value::Object(map)
                }
                "list" => Value::Array(kids[i].iter().map(|&k| built[k].take().unwrap_or(Value::Null)).collect()),
                "bool" => Value::Bool(matches!(row[value_at], SqlValue::Integer(i) if i != 0)),
                "string" | "number" => scalar(&row[value_at]),
                _ => Value::Null,
            };
            built[i] = Some(value);
        }
        built[root?].take()
    }
}

fn scalar(cell: &SqlValue) -> Value {
    match cell {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => Value::from(*i),
        SqlValue::Real(f) => Value::from(*f),
        SqlValue::Text(t) => Value::String(t.clone()),
        SqlValue::Blob(b) => Value::String(String::from_utf8_lossy(b).into_owned()),
    }
}

/// Reads one child table back, cell by cell.
pub struct Reader<'c> {
    rows: NodeRows,
    stmt: rusqlite::Statement<'c>,
}

impl<'c> Reader<'c> {
    pub fn new(db: &'c Connection, child_table: &str) -> Result<Reader<'c>> {
        let names = schema::existing_columns(db, child_table)?;
        let stmt = db.prepare(&format!(
            "SELECT * FROM \"{}\" WHERE owner = ?1 ORDER BY rowid",
            schema::escape(child_table)
        ))?;
        Ok(Reader { rows: NodeRows::new(names), stmt })
    }

    /// The structure `owner`'s cell held, or None when it held none.
    pub fn get(&mut self, owner: &str) -> Result<Option<Value>> {
        let width = self.rows.names.len();
        let rows: Vec<Vec<SqlValue>> = self
            .stmt
            .query_map([owner], |r| (0..width).map(|i| r.get::<_, SqlValue>(i)).collect())?
            .collect::<rusqlite::Result<_>>()?;
        Ok(self.rows.rebuild(rows))
    }
}

/// A child table read LAZILY, cell by cell, over a connection of its own - what a held row decodes from
/// the first time a pass asks (`rows/ts/store.rs`). A database with no file to open again (one in memory)
/// is read whole instead.
pub enum Source {
    Open { conn: Connection, sql: String, rows: NodeRows },
    Read(HashMap<String, Value>),
}

impl Source {
    pub fn new(db: &Connection, child_table: &str) -> Result<Source> {
        let names = schema::existing_columns(db, child_table)?;
        match db.path().filter(|p| !p.is_empty()) {
            Some(path) => Ok(Source::Open {
                conn: Connection::open(path)?,
                sql: format!("SELECT * FROM \"{}\" WHERE owner = ?1 ORDER BY rowid", schema::escape(child_table)),
                rows: NodeRows::new(names),
            }),
            None => {
                let parent = parent_of(db, child_table).unwrap_or_default();
                let column = child_table.strip_prefix(&format!("{parent}{SEP}")).unwrap_or_default().to_string();
                Ok(Source::Read(load_all(db, &parent, &column)?))
            }
        }
    }

    /// The structure `owner`'s cell held, or None.
    pub fn get(&self, owner: &str) -> Option<Value> {
        match self {
            Source::Read(all) => all.get(owner).cloned(),
            Source::Open { conn, sql, rows } => {
                let width = rows.names.len();
                let mut stmt = conn.prepare_cached(sql).ok()?;
                let held: Vec<Vec<SqlValue>> = stmt
                    .query_map([owner], |r| (0..width).map(|i| r.get::<_, SqlValue>(i)).collect())
                    .ok()?
                    .filter_map(|r| r.ok())
                    .collect();
                rows.rebuild(held)
            }
        }
    }
}

/// One column's structure for one row, or None.
pub fn load(db: &Connection, table: &str, column: &str, owner: &str) -> Result<Option<Value>> {
    let child_table = child(table, column);
    if schema::existing_columns(db, &child_table)?.is_empty() {
        return Ok(None);
    }
    Reader::new(db, &child_table)?.get(owner)
}

/// Every row's structure in one column, by owner - for a small table read whole.
pub fn load_all(db: &Connection, table: &str, column: &str) -> Result<HashMap<String, Value>> {
    let child_table = child(table, column);
    let names = schema::existing_columns(db, &child_table)?;
    let mut out = HashMap::new();
    if names.is_empty() {
        return Ok(out);
    }
    let owner_at = names.iter().position(|n| n == "owner").unwrap_or(0);
    let width = names.len();
    let rows = NodeRows::new(names);
    let mut stmt = db.prepare(&format!("SELECT * FROM \"{}\" ORDER BY owner, rowid", schema::escape(&child_table)))?;
    let mut current: Option<String> = None;
    let mut held: Vec<Vec<SqlValue>> = Vec::new();
    let flush = |owner: Option<String>, held: &mut Vec<Vec<SqlValue>>, out: &mut HashMap<String, Value>| {
        if let (Some(owner), Some(value)) = (owner, rows.rebuild(std::mem::take(held))) {
            out.insert(owner, value);
        }
    };
    let mut query = stmt.query([])?;
    while let Some(r) = query.next()? {
        let row: Vec<SqlValue> = (0..width).map(|i| r.get::<_, SqlValue>(i)).collect::<rusqlite::Result<_>>()?;
        let owner = match &row[owner_at] {
            SqlValue::Text(t) => t.clone(),
            other => format!("{other:?}"),
        };
        if current.as_deref() != Some(owner.as_str()) {
            flush(current.take(), &mut held, &mut out);
            current = Some(owner);
        }
        held.push(row);
    }
    flush(current, &mut held, &mut out);
    Ok(out)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
