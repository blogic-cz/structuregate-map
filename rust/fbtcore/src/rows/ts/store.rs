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

use super::super::half::HALF_COLUMN;
use super::super::schema;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// The column names of one table, held ONCE instead of once per row.
pub struct Schema {
    names: Vec<String>,
    at: HashMap<String, usize>,
}

impl Schema {
    fn of(names: Vec<String>) -> Rc<Schema> {
        let at = names.iter().cloned().zip(0..).collect();
        Rc::new(Schema { names, at })
    }
}

/// ONE ROW OF A TABLE.
///
/// `Held` is what a table hands back: the names live in the table's single `Schema` and the row
/// is nothing but its cells. This is the shape that matters - a million rows of a dozen columns is
/// over ten million key strings and a million hash tables when every row is a map of its own,
/// measured at gigabytes of decoded payload for a file of hundreds of MB, on a machine with less than that free.
///
/// `Built` is a row a pass is making. There are far fewer (about a fifth), they are written
/// rather than walked, and giving each one a schema of its own would cost more than it saves.
///
/// AN ABSENT COLUMN IS NOT A NULL ONE, and a shared schema gives every row every column - so a
/// cell is `Option<Value>`: `None` where this row never had that column, `Some(Null)` where it
/// holds null. The distinction is free: `Value` has spare discriminants, so `Option<Value>` is
/// the same 24 bytes. Collapsing the two would be the bug this half keeps finding - a row that
/// says nothing about a column reads as a row that says "nothing".
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Held { schema: Rc<Schema>, cells: Vec<Option<Cell>> },
    Built(Map<String, Value>),
}

/// ONE CELL OF A HELD ROW: a value, or a list/object STILL IN ITS JSON TEXT, decoded the first time a pass
/// asks for it and kept decoded after that.
///
/// THE CLOSURE READS EVERY ROW BUT NOT EVERY CELL. `expressions.ast` is hundreds of MB of text on a large Angular workspace and
/// gigabytes decoded, yet the passes look an AST up through a gate, a node or a role - a few thousand of hundreds of thousands.
/// Decoding them all up front was most of the closure's memory.
#[derive(Clone)]
pub enum Cell {
    Value(Value),
    Json(Box<str>, std::cell::OnceCell<Value>),
}

impl Cell {
    fn json(text: String) -> Cell {
        Cell::Json(text.into_boxed_str(), std::cell::OnceCell::new())
    }

    fn value(&self) -> &Value {
        match self {
            Cell::Value(v) => v,
            Cell::Json(text, decoded) => decoded.get_or_init(|| structure(text)),
        }
    }

    /// Moved out for the WRITE: a cell nobody decoded goes as its text, which the store spells exactly as it
    /// spells a decoded one (`rawcells`).
    fn into_value(self) -> Value {
        match self {
            Cell::Value(v) => v,
            Cell::Json(text, decoded) => decoded.into_inner().unwrap_or_else(|| super::super::rawcells::wrap(text.into())),
        }
    }
}

impl PartialEq for Cell {
    fn eq(&self, other: &Cell) -> bool {
        self.value() == other.value()
    }
}

impl std::fmt::Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value().fmt(f)
    }
}

/// A payload cell, as a held cell: text the payload kept stays text until it is read. A FIXTURE'S door now:
/// every run's closure reads the rows back from the database (`rows/tsapply.rs`).
#[cfg(test)]
fn cell_of(value: Value) -> Cell {
    match super::super::rawcells::text_of(&value) {
        Some(text) => Cell::json(text.to_string()),
        None => Cell::Value(value),
    }
}

impl Default for Row {
    fn default() -> Row {
        Row::Built(Map::new())
    }
}

impl std::fmt::Debug for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.names.iter()).finish()
    }
}

impl PartialEq for Schema {
    fn eq(&self, other: &Schema) -> bool {
        self.names == other.names
    }
}

impl Row {
    pub fn new() -> Row {
        Row::Built(Map::new())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Row::Held { schema, cells } => {
                let at = *schema.at.get(key)?;
                cells.get(at)?.as_ref().map(Cell::value)
            }
            Row::Built(map) => map.get(key),
        }
    }

    /// Setting a column on a row the table handed out COPIES IT OUT of the shared shape first.
    /// No pass does this - the tables are walked, not edited - and the one caller that adds a
    /// column (`emit`, stamping `half`) is working on a row it built itself.
    pub fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        if let Row::Held { .. } = self {
            *self = Row::Built(self.to_map());
        }
        match self {
            Row::Built(map) => map.insert(key, value),
            Row::Held { .. } => unreachable!("just made it Built"),
        }
    }

    /// The row as a plain map, which is what the storing side writes.
    pub fn to_map(&self) -> Map<String, Value> {
        match self {
            Row::Held { schema, cells } => {
                let mut map = Map::new();
                for (index, name) in schema.names.iter().enumerate() {
                    if let Some(Some(cell)) = cells.get(index) {
                        map.insert(name.clone(), cell.value().clone());
                    }
                }
                map
            }
            Row::Built(map) => map.clone(),
        }
    }

    /// The same, consuming the row so the cells are moved rather than copied.
    pub fn into_map(self) -> Map<String, Value> {
        match self {
            Row::Held { schema, cells } => {
                let mut map = Map::new();
                for (index, cell) in cells.into_iter().enumerate() {
                    if let (Some(name), Some(cell)) = (schema.names.get(index), cell) {
                        map.insert(name.clone(), cell.into_value());
                    }
                }
                map
            }
            Row::Built(map) => map,
        }
    }
}

impl std::ops::Index<&str> for Row {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&Value::Null)
    }
}

/// Every row of one table, sharing the names their columns go by.
#[cfg(test)]
fn shared(rows: Vec<Map<String, Value>>) -> Vec<Row> {
    // THE NAMES IN THE ORDER THEY WERE FIRST SEEN, over every row: a column only the last row
    // carries is still a column of the table.
    let mut names: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for row in &rows {
        for key in row.keys() {
            if seen.insert(key.clone()) {
                names.push(key.clone());
            }
        }
    }
    let schema = Schema::of(names);
    rows.into_iter()
        .map(|mut row| {
            let cells = schema.names.iter().map(|n| row.remove(n).map(cell_of)).collect();
            Row::Held { schema: Rc::clone(&schema), cells }
        })
        .collect()
}


pub struct Store<'a> {
    db: Option<&'a Connection>,
    half: String,
    /// Rows are read ONCE per table and cached: the passes walk the same tables
    /// repeatedly (`template_nodes` is read by five of them), and re-querying turned a
    /// tenth of a second into minutes in the first draft.
    cache: RefCell<HashMap<String, Rc<Vec<Row>>>>,
    /// Which columns hold a structure, as the half itself recorded when it wrote them.
    json_columns: HashMap<String, HashSet<String>>,
    /// ...and which hold a boolean, which SQLite stores as 0 and 1.
    bool_columns: HashMap<String, HashSet<String>>,
    names: HashSet<String>,
    payload: bool,
    /// The payload's table names IN THE ORDER IT SENT THEM, kept only by a store that was handed
    /// the rows and will hand them back. `cache` is a `HashMap` and has no order to give back.
    #[cfg(test)]
    order: Vec<String>,
    /// Derived rows this run produced, by table.
    pub emitted: HashMap<String, Vec<Row>>,
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
            cache: RefCell::new(HashMap::new()),
            json_columns,
            bool_columns,
            names,
            payload: false,
            #[cfg(test)]
            order: Vec::new(),
            emitted: HashMap::new(),
        })
    }

    /// The rows exactly as node built them — already decoded, so nothing is parsed a
    /// second time. The closure cannot tell the two sources apart, which is the point.
    ///
    /// THE PAYLOAD IS CONSUMED, not copied. A full run's payload is hundreds of MB of JSON and some
    /// gigabytes of parsed rows; borrowing it and cloning each row held two of those at once,
    /// which is most of the peak a real run was measured at. `take_tables` hands them back.
    #[cfg(test)]
    pub fn from_payload(tables: Map<String, Value>, half: &str) -> Store<'a> {
        let mut cache = HashMap::new();
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
            json_columns: HashMap::new(),
            bool_columns: HashMap::new(),
            names: HashSet::new(),
            payload: true,
            order,
            emitted: HashMap::new(),
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
            Vec::new()
        } else {
            self.read(name).unwrap_or_default()
        };
        let shared = Rc::new(rows);
        self.cache
            .borrow_mut()
            .insert(name.to_string(), Rc::clone(&shared));
        shared
    }

    fn read(&self, name: &str) -> Result<Vec<Row>> {
        let db = self.db.expect("a database-backed store has a connection");
        let columns = schema::existing_columns(db, name)?;
        let structured = self.json_columns.get(name);
        let has_half = columns.iter().any(|c| c == HALF_COLUMN);
        let keep: Vec<String> = columns.into_iter().filter(|c| c != HALF_COLUMN).collect();

        let select = keep
            .iter()
            .map(|c| format!("\"{}\"", schema::escape(c)))
            .collect::<Vec<_>>()
            .join(", ");
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
        let schema = Schema::of(keep.clone());
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
            Ok(Row::Held { schema: Rc::clone(&schema), cells })
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

fn table_names(db: &Connection) -> Result<HashSet<String>> {
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
fn spec_columns(db: &Connection, half: &str, key: &str) -> HashMap<String, HashSet<String>> {
    let mut out = HashMap::new();
    let text: String = match db.query_row(
        "SELECT value FROM _meta WHERE key = ?1",
        [format!("spec:{half}")],
        |r| r.get(0),
    ) {
        Ok(t) => t,
        Err(_) => return out,
    };
    let Ok(spec) = serde_json::from_str::<Value>(&text) else {
        return out;
    };
    let Some(columns) = spec.get(key).and_then(|c| c.as_object()) else {
        return out;
    };
    for (table, names) in columns {
        let set: HashSet<String> = names
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

/// Text back to the structure it holds, or the text itself when it holds none.
///
/// The rule: only `[` or `{` can open one, and
/// text that opens like one but does not parse is a string that happens to look like a
/// structure.
fn text_of_mut(value: &mut Value) -> &mut String {
    match value {
        Value::String(text) => text,
        _ => unreachable!("only called on a string cell"),
    }
}

fn structure(text: &str) -> Value {
    let Some(head) = text.chars().next() else {
        return Value::String(text.to_string());
    };
    if head != '[' && head != '{' {
        return Value::String(text.to_string());
    }
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
