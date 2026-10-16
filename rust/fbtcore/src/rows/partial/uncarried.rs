//! THE STRUCTURES A CARRIED ROW IS HANDED BACK WITHOUT: node's own word, through the plan
//! (`UNREAD_WHEN_CARRIED` in `src/TsRows/TsSetup/TsCarried.mjs`), on the cells no pass of a partial run looks inside.
//!
//! Rebuilding a structure from its node rows was most of the carry - a TypeScript expression's tree above all - and
//! node then parsed it only to hold it. A cell named here goes out as its KIND alone (`a` a list, `o` an object), which
//! node turns into a stub that refuses every look inside: a run that reads one anyway is asked again with every cell
//! carried (`MAP-UNCARRIED`, `whole`). The kind is the cell's ROOT node (`n = 0`), read once per child table for every
//! owner at once - never one query per row, which is what the rebuild cost.

use anyhow::Result;
use indexmap::IndexMap;
use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// How one carried cell goes out.
pub(super) enum Cell {
    /// Rebuilt from its node rows, as every cell was.
    Whole,
    /// As its kind alone: `a` a list, `o` an object.
    Stub(&'static str),
    /// Stubbed, and the row holds no structure there: absent, as a rebuild would have left it.
    Absent,
}

/// A cell stubbed on every carried row, or only on the rows whose `column` holds `value`.
enum When {
    Always,
    Where(String, String),
}

pub(super) struct Uncarried {
    cells: IndexMap<String, IndexMap<String, When>>,
    /// The kind of every owner's structure, by child table: read on first use.
    roots: HashMap<String, HashMap<String, &'static str>>,
}

impl Uncarried {
    /// Nothing stubbed: every cell carried, as a run asked again after a stub was read is.
    pub(super) fn none() -> Uncarried {
        Uncarried { cells: IndexMap::new(), roots: HashMap::new() }
    }

    /// `{table: {column: true | [column, value]}}`, as the plan states it; anything else in it is no stub.
    pub(super) fn from_plan(spec: Option<&Value>) -> Uncarried {
        let mut cells = IndexMap::new();
        for (table, columns) in spec.and_then(Value::as_object).into_iter().flatten() {
            let mut of: IndexMap<String, When> = IndexMap::new();
            for (column, when) in columns.as_object().into_iter().flatten() {
                let when = match when {
                    Value::Bool(true) => When::Always,
                    Value::Array(pair) => match (pair.first().and_then(Value::as_str), pair.get(1).and_then(Value::as_str), pair.len()) {
                        (Some(on), Some(value), 2) => When::Where(on.to_string(), value.to_string()),
                        _ => continue,
                    },
                    _ => continue,
                };
                of.insert(column.clone(), when);
            }
            if !of.is_empty() {
                cells.insert(table.clone(), of);
            }
        }
        Uncarried { cells, roots: HashMap::new() }
    }

    /// How `row`'s cell `column` - its node rows in `child`, under `owner` - goes out.
    pub(super) fn cell(&mut self, db: &Connection, table: &str, column: &str, child: &str, owner: &str, row: &Map<String, Value>) -> Result<Cell> {
        let stubbed = match self.cells.get(table).and_then(|c| c.get(column)) {
            None => false,
            Some(When::Always) => true,
            Some(When::Where(on, value)) => row.get(on.as_str()).and_then(Value::as_str) == Some(value.as_str()),
        };
        if !stubbed {
            return Ok(Cell::Whole);
        }
        if !self.roots.contains_key(child) {
            let read = roots(db, child)?;
            self.roots.insert(child.to_string(), read);
        }
        Ok(self.roots.get(child).and_then(|r| r.get(owner)).map_or(Cell::Absent, |kind| Cell::Stub(kind)))
    }
}

/// Every owner's root node in one child table: `a` for a list, `o` for an object. The ROOT is the one node with no
/// parent, and asked as such the `(role, parent)` index answers it: `n = 0` scanned every node of the table, which on
/// `expressions__ast` cost what the stubs saved.
fn roots(db: &Connection, child: &str) -> Result<HashMap<String, &'static str>> {
    let mut stmt = db.prepare(&format!("SELECT owner, node FROM \"{}\" WHERE role IS NULL AND parent IS NULL", super::schema::escape(child)))?;
    let mut out = HashMap::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let owner = match row.get::<_, Sql>(0)? {
            Sql::Integer(i) => i.to_string(),
            Sql::Text(t) => t,
            _ => continue,
        };
        let kind = match row.get::<_, String>(1)?.as_str() {
            "list" => "a",
            "object" => "o",
            _ => continue,
        };
        out.insert(owner, kind);
    }
    Ok(out)
}
