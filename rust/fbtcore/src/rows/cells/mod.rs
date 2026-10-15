//! A LIST OR AN OBJECT IS NODES, NEVER JSON TEXT.
//!
//! A row's cell that holds a structure - `expressions.ast`, `assignments.reads`, `enums.members` - is not
//! written to its table at all. It goes to a CHILD TABLE named `<table>__<column>`, one row per node of the
//! structure, so a template read joins a member in plain SQL and `json_each` is never needed:
//!
//! ```sql
//! SELECT n.owner, m.value FROM expressions__ast n
//!   JOIN expressions__ast k ON k.owner = n.owner AND k.parent = n.n AND k.role = 'k' AND k.value = 'Read'
//!   JOIN expressions__ast m ON m.owner = n.owner AND m.parent = n.n AND m.role = 'name';
//! ```
//!
//! EVERY VALUE IS A ROW, every member of an object included. A node is `(owner, n)`: `owner` the id of the row the
//! cell belongs to, `n` its preorder number in the cell - the root is 0 - and `parent` the `n` of the node it hangs
//! off (NULL for the root). `role` is its key in the parent object (an item of a member list carries the list's key
//! too, NULL in a top-level list), `ord` its index in a list, `node` one of `object`, `list`, `string`, `number`,
//! `bool`, `null`, and `value` a scalar's own (a boolean 0/1). THE COLUMNS ARE FIXED: a member's NAME is a `role`,
//! never a column - names are data as often as they are fields (an object literal's keys, a property map, a
//! document's headers), and a column per name was a table past SQLite's column limit, `Home` and `home` one
//! column. The root of every cell is one node, so `[]` and `{}` are rows too and an absent cell is no row at all.
//!
//! COMPACT, because there are tens of millions: `(owner, n)` is the primary key of a `WITHOUT ROWID` table - one
//! b-tree that holds the rows AND finds a cell, a node and a node's children (`owner = ? AND parent = ?` reads the
//! owner's few nodes off it) - and `value` is the only index. An `(owner, parent)` index cost a sixth of the map and a
//! quarter of the row write, and served no query the key does not.
//! A node carries no `file`, `owner_file` or `half`: every delete that scopes a row takes its nodes THROUGH the row
//! (`delete_children`), so nothing is stored twice. `_cells` records which columns of which table went where.
//!
//! NOTHING HERE RECURSES: a tree is as deep as the source made it, and a walk that recursed would trade a cap
//! for a stack overflow.

use super::schema;
use anyhow::Result;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, ToSql};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Between a table and its column in a child table's name.
pub const SEP: &str = "__";
/// What records the child tables.
pub const REGISTRY: &str = "_cells";

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

/// One node as the walk hands it out.
pub struct Node<'v> {
    pub n: i64,
    pub parent: Option<i64>,
    pub role: Option<&'v str>,
    pub ord: Option<i64>,
    pub node: &'static str,
    pub value: Value,
}

/// Every node of one structure, in PREORDER - `n` counts up from the root's 0. No recursion.
pub fn walk<'v>(value: &'v Value, mut emit: impl FnMut(Node<'v>) -> Result<()>) -> Result<()> {
    // (value, parent n, role, ord)
    let mut stack: Vec<(&'v Value, Option<i64>, Option<&'v str>, Option<i64>)> = vec![(value, None, None, None)];
    let mut n = 0i64;
    while let Some((value, parent, role, ord)) = stack.pop() {
        let mut kids: Vec<(&'v Value, Option<i64>, Option<&'v str>, Option<i64>)> = Vec::new();
        let (node, scalar) = match value {
            Value::Object(map) => {
                kids.extend(map.iter().map(|(key, member)| (member, Some(n), Some(key.as_str()), None)));
                ("object", Value::Null)
            }
            Value::Array(items) => {
                // The list's ROLE is its member name: an item of `args` says so, not just its index.
                kids.extend(items.iter().enumerate().map(|(i, item)| (item, Some(n), role, Some(i as i64))));
                ("list", Value::Null)
            }
            Value::Null => ("null", Value::Null),
            Value::Bool(b) => ("bool", Value::from(i64::from(*b))),
            Value::Number(_) => ("number", value.clone()),
            Value::String(_) => ("string", value.clone()),
        };
        emit(Node { n, parent, role, ord, node, value: scalar })?;
        n += 1;
        // Pushed reversed so they pop in order.
        stack.extend(kids.into_iter().rev());
    }
    Ok(())
}

/// One structure as its node rows (maps), appended to `out` - what a test reads; the store writes through `Writer`.
#[cfg(test)]
pub fn flatten(owner: &str, value: &Value, out: &mut Vec<Value>) {
    let _ = walk(value, |node| {
        let mut row = Map::new();
        row.insert("owner".into(), Value::String(owner.to_string()));
        row.insert("n".into(), Value::from(node.n));
        row.insert("parent".into(), node.parent.map(Value::from).unwrap_or(Value::Null));
        row.insert("role".into(), node.role.map(|r| Value::String(r.to_string())).unwrap_or(Value::Null));
        row.insert("ord".into(), node.ord.map(Value::from).unwrap_or(Value::Null));
        row.insert("node".into(), Value::from(node.node));
        row.insert("value".into(), node.value);
        out.push(Value::Object(row));
        Ok(())
    });
}

/// A ROW'S OWNER KEY AS SQLITE HOLDS IT: an INTEGER when its text is a decimal integer (`render_path` numbers its
/// rows), TEXT otherwise (`x:12`, `#5`). The column has no declared type, so `b.owner = rp.id` compares like with
/// like - a TEXT `owner` matched no numbered row at all, in a join or a delete. Every writer and reader goes through
/// here, so the two never disagree.
pub fn owner_value(owner: &str) -> SqlValue {
    match owner.parse::<i64>() {
        Ok(n) if n.to_string() == owner => SqlValue::Integer(n),
        _ => SqlValue::Text(owner.to_string()),
    }
}

fn owner_key(owner: &SqlValue) -> String {
    match owner {
        SqlValue::Text(t) => t.clone(),
        SqlValue::Integer(i) => i.to_string(),
        SqlValue::Real(f) => f.to_string(),
        other => format!("{other:?}"),
    }
}

/// THE COLUMNS OF EVERY CHILD TABLE, fixed and in this order.
pub const COLUMNS: &[&str] = &["owner", "n", "parent", "role", "ord", "node", "value"];

/// A child table, made once with its two indexes and registered.
pub fn create(db: &Connection, table: &str, column: &str) -> Result<String> {
    let child_table = child(table, column);
    let t = schema::escape(&child_table);
    db.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS \"{t}\" (owner NOT NULL, n INTEGER NOT NULL, parent INTEGER, role TEXT, \
         ord INTEGER, node TEXT NOT NULL, value, PRIMARY KEY (owner, n)) WITHOUT ROWID;"
    ))?;
    register(db, table, column)?;
    Ok(child_table)
}

/// THE `value` INDEX, made AFTER the rows are in - by `store::write` once per write, by `store::insert_rows` per
/// table. Built during the insert, every node landed at a random place in it, and on a large workspace that was
/// most of the time the store spent; one sort over the finished table is a fraction of that. `IF NOT EXISTS`, so a
/// partial run inserts into the index a full run built.
pub fn index_values(db: &Connection) -> Result<()> {
    if schema::existing_columns(db, REGISTRY).map(|c| c.is_empty()).unwrap_or(true) {
        return Ok(());
    }
    let children: Vec<String> = {
        let mut stmt = db.prepare(&format!("SELECT child FROM {REGISTRY}"))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for child_table in children {
        let t = schema::escape(&child_table);
        db.execute_batch(&format!("CREATE INDEX IF NOT EXISTS \"ix_{t}_value\" ON \"{t}\" (value)"))?;
    }
    Ok(())
}

/// Writes one column's structures straight into its child table: one prepared statement, every node bound as the
/// walk reaches it - no row map per node. Made once per column per insert.
pub struct Writer<'c> {
    stmt: rusqlite::Statement<'c>,
}

impl<'c> Writer<'c> {
    pub fn new(db: &'c Connection, table: &str, column: &str) -> Result<Writer<'c>> {
        let child_table = create(db, table, column)?;
        let marks = vec!["?"; COLUMNS.len()].join(", ");
        let stmt = db.prepare(&format!("INSERT INTO \"{}\" ({}) VALUES ({marks})", schema::escape(&child_table), COLUMNS.join(", ")))?;
        Ok(Writer { stmt })
    }

    /// One cell's structure, every node a row.
    pub fn tree(&mut self, owner: &str, value: &Value) -> Result<()> {
        let stmt = &mut self.stmt;
        let owner = owner_value(owner);
        walk(value, |node| {
            stmt.execute(rusqlite::params![owner, node.n, node.parent, node.role, node.ord, node.node, schema::cell(Some(&node.value))])?;
            Ok(())
        })
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

/// Which columns of `table` went to child tables: `(column, child table)`.
pub fn columns_of(db: &Connection, table: &str) -> Vec<(String, String)> {
    let sql = format!("SELECT col, child FROM {REGISTRY} WHERE tbl = ?1 ORDER BY rowid");
    let Ok(mut stmt) = db.prepare(&sql) else { return Vec::new() };
    stmt.query_map([table], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

/// The SQL a row of `table` (aliased `alias`) is owned by: its id, or `'#' || rowid` for a table without one.
///
/// The id as it is, because `owner` holds it as it is (`owner_value`): a delete through numbered rows reached none of
/// their nodes when the two were compared as text and number. A row whose id is NULL is owned by its rowid, as
/// `store::insert` names it.
pub fn owner_expr(db: &Connection, table: &str, alias: &str) -> String {
    let rowid = format!("'#' || {alias}.rowid");
    let has_id = schema::existing_columns(db, table).map(|c| c.iter().any(|c| c == "id")).unwrap_or(false);
    if has_id {
        format!("(CASE WHEN {alias}.id IS NOT NULL THEN {alias}.id ELSE {rowid} END)")
    } else {
        rowid
    }
}

/// THE NODES OF THE ROWS ABOUT TO GO, deleted first - `where_sql` is the same WHERE the caller deletes `table`'s rows
/// by, its parameters the same. A node carries no scope of its own, so this is how every delete reaches it; run it
/// BEFORE the rows go, while they still say which nodes are theirs.
pub fn delete_children(db: &Connection, table: &str, where_sql: &str, params: &[&dyn ToSql]) -> Result<usize> {
    let children = columns_of(db, table);
    if children.is_empty() {
        return Ok(0);
    }
    let owner = owner_expr(db, table, "t");
    let mut gone = 0;
    for (_, child_table) in children {
        gone += db.execute(
            &format!(
                "DELETE FROM \"{}\" WHERE owner IN (SELECT {owner} FROM \"{}\" t WHERE {where_sql})",
                schema::escape(&child_table),
                schema::escape(table)
            ),
            params,
        )?;
    }
    Ok(gone)
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

/// What a node read back carries: `n`, `parent`, `role`, `node`, `value` - the SELECT every reader here runs.
const READ: &str = "n, parent, role, node, value";

fn read_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Option<i64>, Option<String>, String, SqlValue)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
}

type NodeRow = (i64, Option<i64>, Option<String>, String, SqlValue);

/// The structure one cell's node rows are, read in `n` order. None for no rows.
pub fn rebuild(rows: Vec<NodeRow>) -> Option<Value> {
    if rows.is_empty() {
        return None;
    }
    // `n` is the preorder number, 0.. without a gap, so a node's place in `rows` IS its n.
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
    for (i, row) in rows.iter().enumerate() {
        if let Some(p) = row.1.and_then(|p| usize::try_from(p).ok()).filter(|&p| p < i) {
            kids[p].push(i);
        }
    }
    // BOTTOM UP, without recursion: a node's children come after it in preorder.
    let mut built: Vec<Option<Value>> = vec![None; rows.len()];
    for i in (0..rows.len()).rev() {
        let (_, _, _, node, value) = &rows[i];
        let value = match node.as_str() {
            "object" => {
                let mut map = Map::new();
                for &k in &kids[i] {
                    if let Some(role) = &rows[k].2 {
                        map.insert(role.clone(), built[k].take().unwrap_or(Value::Null));
                    }
                }
                Value::Object(map)
            }
            "list" => Value::Array(kids[i].iter().map(|&k| built[k].take().unwrap_or(Value::Null)).collect()),
            "bool" => Value::Bool(matches!(value, SqlValue::Integer(v) if *v != 0)),
            "string" | "number" => scalar(value),
            _ => Value::Null,
        };
        built[i] = Some(value);
    }
    built[0].take()
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
    stmt: rusqlite::Statement<'c>,
}

impl<'c> Reader<'c> {
    pub fn new(db: &'c Connection, child_table: &str) -> Result<Reader<'c>> {
        let stmt = db.prepare(&format!("SELECT {READ} FROM \"{}\" WHERE owner = ?1 ORDER BY n", schema::escape(child_table)))?;
        Ok(Reader { stmt })
    }

    /// The structure `owner`'s cell held, or None when it held none.
    pub fn get(&mut self, owner: &str) -> Result<Option<Value>> {
        let rows: Vec<NodeRow> = self.stmt.query_map([owner_value(owner)], read_row)?.collect::<rusqlite::Result<_>>()?;
        Ok(rebuild(rows))
    }
}

/// A child table read LAZILY, cell by cell, over a connection of its own - what a held row decodes from
/// the first time a pass asks (`rows/ts/store.rs`). A database with no file to open again (one in memory)
/// is read whole instead.
pub enum Source {
    Open { conn: Connection, sql: String },
    Read(HashMap<String, Value>),
}

impl Source {
    pub fn new(db: &Connection, child_table: &str) -> Result<Source> {
        match db.path().filter(|p| !p.is_empty()) {
            Some(path) => Ok(Source::Open {
                conn: {
                    let c = Connection::open(path)?;
                    if std::env::var("SG_MMAP").is_ok() {
                        c.execute_batch("PRAGMA mmap_size = 68719476736")?;
                    }
                    c
                },
                sql: format!("SELECT {READ} FROM \"{}\" WHERE owner = ?1 ORDER BY n", schema::escape(child_table)),
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
            Source::Open { conn, sql } => {
                let t0 = std::time::Instant::now();
                let mut stmt = conn.prepare_cached(sql).ok()?;
                let held: Vec<NodeRow> = stmt.query_map([owner_value(owner)], read_row).ok()?.filter_map(|r| r.ok()).collect();
                let out = rebuild(held);
                if probe::on() { probe::record(sql.split('"').nth(1).unwrap_or(""), owner, t0.elapsed().as_nanos()); }
                out
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
    let mut out = HashMap::new();
    if schema::existing_columns(db, &child_table)?.is_empty() {
        return Ok(out);
    }
    let mut stmt = db.prepare(&format!("SELECT owner, {READ} FROM \"{}\" ORDER BY owner, n", schema::escape(&child_table)))?;
    let mut current: Option<String> = None;
    let mut held: Vec<NodeRow> = Vec::new();
    let mut query = stmt.query([])?;
    while let Some(r) = query.next()? {
        let owner = owner_key(&r.get::<_, SqlValue>(0)?);
        if current.as_deref() != Some(owner.as_str()) {
            if let (Some(done), Some(value)) = (current.take(), rebuild(std::mem::take(&mut held))) {
                out.insert(done, value);
            }
            current = Some(owner);
        }
        held.push((r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?));
    }
    if let (Some(done), Some(value)) = (current, rebuild(held)) {
        out.insert(done, value);
    }
    Ok(out)
}

pub mod meta;
pub mod probe;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
