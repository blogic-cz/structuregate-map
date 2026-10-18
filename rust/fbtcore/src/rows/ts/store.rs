//! The rows this half already wrote, served back the way node held them in memory.
//!
//! The closure is derived from EVERY other table, so it needs every row. In node that
//! meant keeping the whole store alive, which is fine on a full run and impossible on an
//! incremental one: node cannot read this database, and handing the rows back costs
//! about as much JSON as a `JSON.parse` can hold at all. Running the closure
//! where the rows already live is what makes "re-parse only the affected files" work.
//!
//! THE SHAPE IS NODE'S, NOT SQLITE'S, because the passes were ported line for line and a
//! second translation would be a second place to be wrong:
//!   * a column the spec calls JSON is decoded, so a test for "is this a list" still
//!     answers about a list;
//!   * SQL NULL arrives as `Value::Null`, which is the single absence node's
//!     `=== null || === undefined` collapsed to;
//!   * `half` is dropped. No pass reads it, and a row carrying it would differ from the
//!     row node walked — the kind of difference that is found late.

use super::super::cells;
use super::super::half::HALF_COLUMN;
use super::super::schema;
use anyhow::Result;
use rusqlite::Connection;
#[cfg(test)]
use serde_json::Map;
use serde_json::Value;
use std::cell::RefCell;
use super::super::{FastMap, FastSet};
use std::collections::HashMap;
use std::rc::Rc;

mod row;
pub use row::{null_literal, Cell, Row, Schema};
#[cfg(test)]
use row::shared;

pub struct Store<'a> {
    db: Option<&'a Connection>,
    half: String,
    /// Rows are read ONCE per table and cached: the passes walk the same tables
    /// repeatedly (`template_nodes` is read by five of them), and re-querying turned a
    /// tenth of a second into minutes in the first draft.
    cache: RefCell<FastMap<String, Rc<Vec<Row>>>>,
    /// Which columns hold a structure, as the half itself recorded when it wrote them.
    json_columns: FastMap<String, FastSet<String>>,
    /// ...and which hold a boolean, which SQLite stores as 0 and 1.
    bool_columns: FastMap<String, FastSet<String>>,
    names: FastSet<String>,
    payload: bool,
    /// The payload's table names IN THE ORDER IT SENT THEM, kept only by a store that was handed
    /// the rows and will hand them back. `cache` is a `HashMap` and has no order to give back.
    #[cfg(test)]
    order: Vec<String>,
    /// What a pass builds from the rows and several passes ask for, built once and shared (`shared`).
    memo: RefCell<FastMap<&'static str, Rc<dyn std::any::Any>>>,
    /// Derived rows this run produced, by table.
    pub emitted: HashMap<String, Vec<Row>>,
    /// WHAT THE PASSES READ: every table asked for, and each read's column names and flags (`Schema::reads`).
    /// Kept past `release`, which lets the rows go - see `reads`.
    asked: RefCell<Vec<(String, Option<Asked>)>>,
}

/// One table's columns as read, and whether its rows were scoped by `half` (a table without the column is read
/// whole, other halves' rows included).
type Asked = (Vec<String>, Rc<[std::cell::Cell<bool>]>, bool);

/// WHAT THE CLOSURE READ, table by table: the columns any pass asked a row of. A table asked for and holding no
/// rows is here too, with no columns - that it was empty is part of the answer. `unscoped` names a table read
/// whole, another half's rows with it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Reads {
    pub tables: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    pub unscoped: std::collections::BTreeSet<String>,
}

impl<'a> Store<'a> {
    /// The rows read out of the database, which is what a PARTIAL run needs: the
    /// unchanged files' rows are only there.
    pub fn from_db(db: &'a Connection, half: &str) -> Result<Store<'a>> {
        let json_columns = spec_columns(db, half, "json_columns");
        let bool_columns = spec_columns(db, half, "bool_columns");
        let names = table_names(db)?;
        Ok(Store {
            db: Some(db),
            half: half.to_string(),
            cache: RefCell::new(FastMap::default()),
            json_columns,
            bool_columns,
            names,
            payload: false,
            #[cfg(test)]
            order: Vec::new(),
            memo: RefCell::new(FastMap::default()),
            emitted: HashMap::new(),
            asked: RefCell::new(Vec::new()),
        })
    }

    /// A list the half's extraction was configured with (`_meta` `options:<half>`, e.g. `i18n_calls`), empty
    /// when the database holds none - a store handed its rows has no `_meta` to read.
    pub fn option_list(&self, name: &str) -> Vec<String> {
        let Some(db) = self.db else { return Vec::new() };
        let Some(text) = crate::rows::meta::get(db, &format!("options:{}", self.half)) else { return Vec::new() };
        let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        parsed.get(name).and_then(Value::as_array).into_iter().flatten()
            .filter_map(|v| v.as_str().map(str::to_string)).collect()
    }

    /// The rows exactly as node built them — already decoded, so nothing is parsed a
    /// second time. The closure cannot tell the two sources apart, which is the point.
    ///
    /// THE PAYLOAD IS CONSUMED, not copied. A full run's payload is hundreds of MB of JSON and some
    /// gigabytes of parsed rows; borrowing it and cloning each row held two of those at once,
    /// which is most of the peak a real run was measured at. `take_tables` hands them back.
    #[cfg(test)]
    pub fn from_payload(tables: Map<String, Value>, half: &str) -> Store<'a> {
        let mut cache = FastMap::default();
        let mut order = Vec::with_capacity(tables.len());
        for (name, rows) in tables {
            let list: Vec<Row> = match rows {
                Value::Array(rows) => shared(
                    rows.into_iter()
                        .filter_map(|r| match r {
                            Value::Object(o) => Some(o),
                            _ => None,
                        })
                        .map(|mut r| {
                            r.remove(HALF_COLUMN);
                            r
                        })
                        .collect(),
                ),
                _ => Vec::new(),
            };
            order.push(name.clone());
            cache.insert(name, Rc::new(list));
        }
        Store {
            db: None,
            half: half.to_string(),
            cache: RefCell::new(cache),
            json_columns: FastMap::default(),
            bool_columns: FastMap::default(),
            names: FastSet::default(),
            payload: true,
            order,
            memo: RefCell::new(FastMap::default()),
            emitted: HashMap::new(),
            asked: RefCell::new(Vec::new()),
        }
    }

    /// The rows this store was handed, GIVEN BACK IN THE ORDER THEY ARRIVED and without a copy.
    ///
    /// `Rc::try_unwrap` is what makes it free: the passes take `Rc` clones while they walk and
    /// drop them, so by the time the closure is done this store holds the only reference. A table
    /// somebody still holds is cloned rather than refused - being right is worth one copy.
    ///
    /// The `half` column each row arrived with was dropped on the way in and is NOT restored:
    /// the storing side stamps it, and a row carrying its own would be trusted instead.
    #[cfg(test)]
    pub fn take_tables(&mut self) -> Map<String, Value> {
        let mut out = Map::new();
        let mut cache = self.cache.borrow_mut();
        for name in std::mem::take(&mut self.order) {
            let Some(held) = cache.remove(&name) else { continue };
            let rows = Rc::try_unwrap(held).unwrap_or_else(|shared| (*shared).clone());
            out.insert(
                name,
                Value::Array(rows.into_iter().map(|r| Value::Object(r.into_map())).collect()),
            );
        }
        out
    }

    /// Every row this half wrote to `name`, IN THE ORDER IT WROTE THEM.
    ///
    /// ROW ORDER IS PART OF THE ANSWER. `loadEdges` walks `renders` in extraction order
    /// and the walk below it is order-sensitive; `rowid` is that order, because the rows
    /// went in as the payload listed them. An `ORDER BY id` would sort `r:1000` before
    /// `r:2` and quietly reorder the DAG.
    pub fn table(&self, name: &str) -> Rc<Vec<Row>> {
        if let Some(hit) = self.cache.borrow().get(name) {
            return Rc::clone(hit);
        }
        // A table the payload does not carry is one this half wrote no rows to, which is
        // the same answer the database gives for one it does not hold.
        let rows = if self.payload || !self.names.contains(name) {
            self.asked.borrow_mut().push((name.to_string(), None));
            Vec::new()
        } else {
            // A TABLE THAT WOULD NOT READ is answered empty, and is UNSCOPED in `reads`: nothing may be concluded
            // from what the closure saw of it.
            self.read(name).unwrap_or_else(|_| {
                self.asked.borrow_mut().push((name.to_string(), Some((Vec::new(), Rc::from(Vec::new()), false))));
                Vec::new()
            })
        };
        let shared = Rc::new(rows);
        self.cache
            .borrow_mut()
            .insert(name.to_string(), Rc::clone(&shared));
        shared
    }

    /// What every pass so far read - see `Reads`.
    pub fn reads(&self) -> Reads {
        let mut out = Reads::default();
        for (name, asked) in self.asked.borrow().iter() {
            let columns = out.tables.entry(name.clone()).or_default();
            let Some((names, flags, scoped)) = asked else { continue };
            columns.extend(names.iter().zip(flags.iter()).filter(|(_, f)| f.get()).map(|(n, _)| n.clone()));
            if !scoped {
                out.unscoped.insert(name.clone());
            }
        }
        out
    }

    /// Forget every table read so far, and every structure decoded in them. A table asked for again is read
    /// again - the same rows, from the same database - so this changes what is held, never what is answered.
    pub fn release(&mut self) {
        if self.db.is_some() {
            self.cache.get_mut().clear();
        }
        self.memo.get_mut().clear();
    }

    /// WHAT SEVERAL PASSES BUILD FROM THE SAME ROWS, built by the first to ask and handed to the rest. The rows do
    /// not change while the passes run, so the answer is the one each would have built - and building it again
    /// rebuilt the same trees from their node rows each time (`gate_tsrows::TsRows`). Gone with `release`.
    pub fn shared<T: 'static>(&self, key: &'static str, make: impl FnOnce(&Store<'a>) -> T) -> Rc<T> {
        if let Some(held) = self.memo.borrow().get(key).cloned().and_then(|h| h.downcast::<T>().ok()) {
            return held;
        }
        let made = Rc::new(make(self));
        self.memo.borrow_mut().insert(key, made.clone());
        made
    }

    fn read(&self, name: &str) -> Result<Vec<Row>> {
        let db = self.db.expect("a database-backed store has a connection");
        let columns = schema::existing_columns(db, name)?;
        let structured = self.json_columns.get(name);
        let has_half = columns.iter().any(|c| c == HALF_COLUMN);
        let keep: Vec<String> = columns.into_iter().filter(|c| c != HALF_COLUMN).collect();

        // THE STRUCTURES ARE NODES (`rows/cells/mod.rs`): each such column is a cell rebuilt from its child table
        // when a pass first reads it. The owner is the row's id, or its rowid in a table without one.
        let nodes: Vec<(String, cells::Source)> = cells::columns_of(db, name)
            .into_iter()
            .map(|(column, child)| Ok((column, cells::Source::new(db, &child)?)))
            .collect::<Result<_>>()?;
        let id_at = keep.iter().position(|c| c == "id");
        let mut select = keep
            .iter()
            .map(|c| format!("\"{}\"", schema::escape(c)))
            .collect::<Vec<_>>()
            .join(", ");
        select.push_str(", rowid");
        let width = keep.len();
        let sql = if has_half {
            format!(
                "SELECT {select} FROM \"{}\" WHERE \"{HALF_COLUMN}\" = ?1 ORDER BY rowid",
                schema::escape(name)
            )
        } else {
            format!(
                "SELECT {select} FROM \"{}\" ORDER BY rowid",
                schema::escape(name)
            )
        };

        // STRAIGHT INTO THE HELD SHAPE. A SQL row has every column, so the schema is the SELECT's and every
        // cell is `Some` - what `shared` makes of a map per row, without the map: building hundreds of thousands of them
        // first, each with its own key strings, took the exe to several GB on a partial run of a large consumer.
        let mut names = keep.clone();
        let mut sources: Vec<Option<cells::Source>> = Vec::new();
        let mut node_at: Vec<usize> = Vec::new();
        for (column, source) in nodes {
            let at = names.iter().position(|n| *n == column).unwrap_or_else(|| {
                names.push(column.clone());
                names.len() - 1
            });
            sources.resize_with(names.len(), || None);
            sources[at] = Some(source);
            node_at.push(at);
        }
        let schema = Rc::new(Schema::with(names, sources, id_at));
        let (names, flags) = schema.reads();
        self.asked.borrow_mut().push((name.to_string(), Some((names, flags, has_half))));
        let decode: Vec<bool> = keep.iter().map(|c| structured.is_some_and(|cols| cols.contains(c.as_str()))).collect();
        // A BOOLEAN COMES BACK AN INTEGER, and a pass that asks `== true` of it gets false: `is_default` did, and
        // every switch's default case lost its key on every run that read the rows back - all partial ones.
        let booleans = self.bool_columns.get(name);
        let truth: Vec<bool> = keep.iter().map(|c| booleans.is_some_and(|cols| cols.contains(c.as_str()))).collect();
        let mut stmt = db.prepare(&sql)?;
        let map_row = |record: &rusqlite::Row<'_>| -> rusqlite::Result<Row> {
            let mut cells = Vec::with_capacity(decode.len());
            for (index, structured) in decode.iter().enumerate() {
                let raw: rusqlite::types::Value = record.get(index)?;
                let mut value = from_sql(raw);
                if truth[index] && let Value::Number(n) = &value {
                    match n.as_i64() {
                        Some(0) => value = Value::Bool(false),
                        Some(1) => value = Value::Bool(true),
                        _ => {}
                    }
                }
                // A LIST OR AN OBJECT STAYS TEXT until a pass reads it - see `Cell`.
                if *structured && let Value::String(text) = &value && matches!(text.as_bytes().first(), Some(b'[' | b'{')) {
                    cells.push(Some(Cell::json(std::mem::take(text_of_mut(&mut value)))));
                    continue;
                }
                cells.push(Some(Cell::Value(value)));
            }
            if !node_at.is_empty() {
                cells.resize(schema.names.len(), None);
                for at in &node_at {
                    // A column that held a scalar in this row keeps it; only a NULL one is a structure's.
                    if matches!(&cells[*at], Some(Cell::Value(v)) if !v.is_null()) {
                        continue;
                    }
                    cells[*at] = Some(Cell::Nodes(std::cell::OnceCell::new()));
                }
            }
            let rowid = record.get::<_, i64>(width)?;
            Ok(Row::Held { schema: Rc::clone(&schema), cells: cells.into_boxed_slice(), rowid })
        };
        let rows: Vec<Row> = if has_half {
            stmt.query_map([&self.half], map_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], map_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    /// One derived row, STAMPED WITH THE HALF, exactly as node's store stamps every row
    /// it creates.
    ///
    /// THE STAMP IS WHAT MAKES THE ROW DELETABLE. `drop_half` clears this half's rows
    /// before the new ones go in and finds a table by looking for this column — so a
    /// table written without it is never cleared and every rebuild APPENDS. The four
    /// closure tables came out at exactly twice their size on the second run
    /// (`render_path` held two rows for every path), and neither check in place caught it: the
    /// row-by-row comparison skips `half` as housekeeping, and the 53-table gate treats
    /// it as a field only the new map has. Only rebuilding twice shows it.
    pub fn emit(&mut self, name: &str, mut row: Row) {
        row.insert(HALF_COLUMN.to_string(), Value::String(self.half.clone()));
        self.emitted.entry(name.to_string()).or_default().push(row);
    }
}

fn from_sql(raw: rusqlite::types::Value) -> Value {
    use rusqlite::types::Value as V;
    match raw {
        V::Null => Value::Null,
        V::Integer(i) => Value::from(i),
        V::Real(f) => Value::from(f),
        V::Text(t) => Value::String(t),
        // A blob is not something any pass reads; carried as its lossy text so the row
        // still has the column rather than silently losing it.
        V::Blob(b) => Value::String(String::from_utf8_lossy(&b).into_owned()),
    }
}

fn table_names(db: &Connection) -> Result<FastSet<String>> {
    let mut stmt = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let names = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(names
        .filter_map(|n| n.ok())
        .filter(|n| !n.starts_with('_') && !n.starts_with("file_text"))
        .collect())
}

/// Which columns hold a structure, as the half itself recorded when it wrote them.
///
/// NOT GUESSED FROM THE VALUE. A source string can SPELL a structure — `return '{}'` is
/// stored as `{}` either way — so deciding per value would decode a string literal into
/// an empty object for the one row in a million that writes one. The spec is the half's
/// own statement of which columns it encoded.
fn spec_columns(db: &Connection, half: &str, key: &str) -> FastMap<String, FastSet<String>> {
    let mut out = FastMap::default();
    let Some(text) = crate::rows::meta::get(db, &format!("spec:{half}")) else { return out };
    let Ok(spec) = serde_json::from_str::<Value>(&text) else {
        return out;
    };
    let Some(columns) = spec.get(key).and_then(|c| c.as_object()) else {
        return out;
    };
    for (table, names) in columns {
        let set: FastSet<String> = names
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        out.insert(table.clone(), set);
    }
    out
}

fn text_of_mut(value: &mut Value) -> &mut String {
    match value {
        Value::String(text) => text,
        _ => unreachable!("only called on a string cell"),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
