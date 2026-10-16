//! A CLOSURE DERIVED AGAIN IS WRITTEN AS WHAT CHANGED, not deleted and written whole.
//!
//! An edit that moves the closure at all (`unchanged`) usually moves a few of its rows: a component's paths, the
//! keys under one gate. Deleting the four tables and writing every row back was a third of a partial run on a
//! large workspace - hundreds of thousands of rows and their node rows, nearly all of them what was already there.
//! So each row the closure emitted is held against the rows its table holds, BY CONTENT as SQLite would store it
//! (`store::insert`: a scalar as its cell, a list or an object as its node rows), and only the difference is
//! written: a stored row no emitted row matches goes with its nodes, an emitted row no stored row matches goes in.
//! Equal rows are matched one for one, so a row emitted twice is stored twice. What the tables hold afterwards is
//! the emitted rows exactly; only their rowids say which were there before.

use super::cells;
use super::half::HALF_COLUMN;
use super::schema;
use anyhow::Result;
use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::HashMap;

type Digest = [u8; 32];

fn encode(hasher: &mut blake3::Hasher, value: &Sql) {
    match value {
        Sql::Null => {
            hasher.update(b"N");
        }
        Sql::Integer(i) => {
            hasher.update(b"I");
            hasher.update(&i.to_le_bytes());
        }
        Sql::Real(f) => {
            hasher.update(b"F");
            hasher.update(&f.to_bits().to_le_bytes());
        }
        Sql::Text(t) => {
            hasher.update(b"T");
            hasher.update(&(t.len() as u64).to_le_bytes());
            hasher.update(t.as_bytes());
        }
        Sql::Blob(b) => {
            hasher.update(b"B");
            hasher.update(&(b.len() as u64).to_le_bytes());
            hasher.update(b);
        }
    }
}

fn name(hasher: &mut blake3::Hasher, tag: &[u8], column: &str) {
    hasher.update(tag);
    hasher.update(&(column.len() as u64).to_le_bytes());
    hasher.update(column.as_bytes());
}

fn sql_of(value: &Value) -> Sql {
    match schema::cell(Some(value)) {
        rusqlite::types::ToSqlOutput::Owned(v) => v,
        _ => Sql::Null,
    }
}

/// One node row, as `(n, parent, role, ord, node, value)` - the order `cells::COLUMNS` stores it in.
fn node(hasher: &mut blake3::Hasher, cells: [Sql; 6]) {
    hasher.update(b"n");
    for cell in &cells {
        encode(hasher, cell);
    }
}

/// A ROW AS SQLITE HOLDS IT: every non-NULL scalar by column name, then every structure's node rows by column
/// name - both in name order. An absent cell and a NULL one are the same row once stored, so neither is hashed.
fn row_digest(scalars: &mut [(String, Sql)], structures: &mut [(String, Digest)]) -> Digest {
    scalars.sort_by(|a, b| a.0.cmp(&b.0));
    structures.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = blake3::Hasher::new();
    for (column, value) in scalars.iter().filter(|(_, v)| !matches!(v, Sql::Null)) {
        name(&mut hasher, b"S", column);
        encode(&mut hasher, value);
    }
    for (column, digest) in structures.iter() {
        name(&mut hasher, b"C", column);
        hasher.update(digest);
    }
    *hasher.finalize().as_bytes()
}

/// An emitted row, hashed as `store::insert` would write it.
fn emitted_digest(object: &Map<String, Value>, half: &str) -> Result<Digest> {
    let mut scalars: Vec<(String, Sql)> = Vec::new();
    let mut structures: Vec<(String, Digest)> = Vec::new();
    for (column, value) in object {
        if column == HALF_COLUMN {
            continue;
        }
        if cells::is_structured(value) {
            let mut hasher = blake3::Hasher::new();
            cells::walk(&cells::decoded(value), |n| {
                let role = n.role.map(|r| Sql::Text(r.to_string())).unwrap_or(Sql::Null);
                node(&mut hasher, [Sql::Integer(n.n), n.parent.map(Sql::Integer).unwrap_or(Sql::Null), role, n.ord.map(Sql::Integer).unwrap_or(Sql::Null), Sql::Text(n.node.to_string()), sql_of(&n.value)]);
                Ok(())
            })?;
            structures.push((column.clone(), *hasher.finalize().as_bytes()));
        } else {
            scalars.push((column.clone(), sql_of(value)));
        }
    }
    // THE STAMP the store puts on every row, whatever the row said.
    scalars.push((HALF_COLUMN.to_string(), Sql::Text(half.to_string())));
    Ok(row_digest(&mut scalars, &mut structures))
}

fn owner_text(owner: &Sql) -> String {
    match owner {
        Sql::Text(t) => t.clone(),
        Sql::Integer(i) => i.to_string(),
        other => format!("{other:?}"),
    }
}

/// Every stored row of this half: `digest -> rowids` (several when rows repeat), and each rowid's node owner.
fn stored(db: &Connection, table: &str, half: &str) -> Result<HashMap<Digest, Vec<(i64, String)>>> {
    let mut out: HashMap<Digest, Vec<(i64, String)>> = HashMap::new();
    let columns = schema::existing_columns(db, table)?;
    if columns.is_empty() {
        return Ok(out);
    }
    // THE NODES FIRST, ONE SCAN PER CHILD TABLE in key order: a row's structure is its owner's run of nodes.
    let mut nodes: Vec<(String, HashMap<String, Digest>)> = Vec::new();
    for (column, child) in cells::columns_of(db, table) {
        let mut by_owner: HashMap<String, Digest> = HashMap::new();
        let mut stmt = db.prepare(&format!("SELECT owner, n, parent, role, ord, node, value FROM \"{}\" ORDER BY owner, n", schema::escape(&child)))?;
        let mut rows = stmt.query([])?;
        let mut current: Option<(String, blake3::Hasher)> = None;
        while let Some(r) = rows.next()? {
            let owner = owner_text(&r.get::<_, Sql>(0)?);
            if current.as_ref().is_none_or(|(o, _)| *o != owner) {
                if let Some((o, h)) = current.take() {
                    by_owner.insert(o, *h.finalize().as_bytes());
                }
                current = Some((owner, blake3::Hasher::new()));
            }
            let (_, hasher) = current.as_mut().expect("just set");
            node(hasher, [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?]);
        }
        if let Some((o, h)) = current.take() {
            by_owner.insert(o, *h.finalize().as_bytes());
        }
        nodes.push((column, by_owner));
    }
    let plain: Vec<&String> = columns.iter().filter(|c| c.as_str() != HALF_COLUMN).collect();
    let select: Vec<String> = plain.iter().map(|c| format!("t.\"{}\"", schema::escape(c))).collect();
    let sql = format!(
        "SELECT {}, t.rowid, {} FROM \"{}\" t WHERE t.\"{HALF_COLUMN}\" = ?1",
        select.join(", "),
        cells::owner_expr(db, table, "t"),
        schema::escape(table)
    );
    let mut stmt = db.prepare(&sql)?;
    let mut rows = stmt.query([half])?;
    while let Some(r) = rows.next()? {
        let mut scalars: Vec<(String, Sql)> = Vec::with_capacity(plain.len() + 1);
        for (i, column) in plain.iter().enumerate() {
            scalars.push(((*column).clone(), r.get(i)?));
        }
        scalars.push((HALF_COLUMN.to_string(), Sql::Text(half.to_string())));
        let rowid: i64 = r.get(plain.len())?;
        let owner = owner_text(&r.get::<_, Sql>(plain.len() + 1)?);
        // A STRUCTURE'S COLUMN HOLDS NULL in its row: the row says nothing there but through its nodes.
        let mut structures: Vec<(String, Digest)> = Vec::new();
        for (column, by_owner) in &nodes {
            if let Some(d) = by_owner.get(&owner) {
                structures.push((column.clone(), *d));
            }
        }
        out.entry(row_digest(&mut scalars, &mut structures)).or_default().push((rowid, owner));
    }
    Ok(out)
}

/// What a delta write did, for the receipt.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Delta {
    pub kept: usize,
    pub deleted: usize,
    pub inserted: usize,
}

/// THE ROWS TO WRITE, after deleting every stored row of `table` no emitted row matches - and the count of each.
/// The caller writes what comes back (`store::write`); nothing here inserts, so a row goes in the way every row does.
pub fn reconcile(db: &Connection, table: &str, emitted: Vec<Value>, half: &str) -> Result<(Vec<Value>, Delta)> {
    let mut held = stored(db, table, half)?;
    let mut fresh: Vec<Value> = Vec::new();
    let mut delta = Delta::default();
    for row in emitted {
        let Some(object) = row.as_object() else { continue };
        let digest = emitted_digest(object, half)?;
        match held.get_mut(&digest).and_then(Vec::pop) {
            Some(_) => delta.kept += 1,
            None => fresh.push(row),
        }
    }
    let gone: Vec<(i64, String)> = held.into_values().flatten().collect();
    if !gone.is_empty() {
        let children: Vec<String> = cells::columns_of(db, table).into_iter().map(|(_, c)| c).collect();
        let mut drop_row = db.prepare(&format!("DELETE FROM \"{}\" WHERE rowid = ?1", schema::escape(table)))?;
        let mut drop_nodes: Vec<rusqlite::Statement<'_>> = children
            .iter()
            .map(|c| db.prepare(&format!("DELETE FROM \"{}\" WHERE owner = ?1", schema::escape(c))))
            .collect::<rusqlite::Result<_>>()?;
        for (rowid, owner) in &gone {
            let owner = cells::owner_value(owner);
            for stmt in drop_nodes.iter_mut() {
                stmt.execute([&owner])?;
            }
            drop_row.execute([rowid])?;
        }
    }
    delta.deleted = gone.len();
    delta.inserted = fresh.len();
    Ok((fresh, delta))
}

/// EVERY TABLE AT ONCE - `tables` and any other `emitted` names - in one unjournalled transaction, as the store writes:
/// the rows to write, by table, and what was kept, deleted and left to insert in all of them.
pub fn reconcile_tables(db: &Connection, tables: &[&str], mut emitted: Map<String, Value>, half: &str) -> Result<(Map<String, Value>, Delta)> {
    db.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; BEGIN")?;
    let mut names: std::collections::BTreeSet<String> = tables.iter().map(|t| t.to_string()).collect();
    names.extend(emitted.keys().cloned());
    let mut fresh = Map::new();
    let mut total = Delta::default();
    for table in names {
        let rows = match emitted.remove(&table) {
            Some(Value::Array(rows)) => rows,
            _ => Vec::new(),
        };
        let (rows, delta) = reconcile(db, &table, rows, half)?;
        total.kept += delta.kept;
        total.deleted += delta.deleted;
        total.inserted += delta.inserted;
        fresh.insert(table, Value::Array(rows));
    }
    db.execute_batch("COMMIT")?;
    Ok((fresh, total))
}

#[cfg(test)]
#[path = "delta_tests.rs"]
mod tests;
