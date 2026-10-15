//! ONE ROW OF A TABLE THE CLOSURE READS, and its cells - the shape `Store` (`store.rs`) hands every pass. Apart from
//! the store only so that file stays under the line ceiling.

use super::super::super::cells;
use serde_json::{Map, Value};
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
use std::rc::Rc;

/// The column names of one table, held ONCE instead of once per row - and, for a column written as nodes, where
/// its structures are read from, so no cell carries a handle of its own.
pub struct Schema {
    pub(super) names: Vec<String>,
    pub(super) at: HashMap<String, usize>,
    /// Per column: the child table a structure column's cells are rebuilt from (`rows/cells/mod.rs`).
    nodes: Vec<Option<cells::Source>>,
    /// The column a row's nodes are owned by (`id`); a row without one owns them by its rowid.
    id_at: Option<usize>,
}

impl Schema {
    #[cfg(test)]
    pub(super) fn of(names: Vec<String>) -> Rc<Schema> {
        Rc::new(Schema::with(names, Vec::new(), None))
    }

    pub(super) fn with(names: Vec<String>, mut nodes: Vec<Option<cells::Source>>, id_at: Option<usize>) -> Schema {
        let at = names.iter().cloned().zip(0..).collect();
        nodes.resize_with(names.len(), || None);
        Schema { names, at, nodes, id_at }
    }
}

/// ONE ROW OF A TABLE.
///
/// `Held` is what a table hands back: the names live in the table's single `Schema` and the row
/// is nothing but its cells. This is the shape that matters - a million rows of a dozen columns is
/// over ten million key strings and a million hash tables when every row is a map of its own,
/// measured at gigabytes of decoded payload for a file of hundreds of MB, on a machine with less than that free.
/// `rowid` is what owns the nodes of a row whose table has no `id`.
///
/// `Built` is a row a pass is making. There are far fewer (about a fifth), they are written
/// rather than walked, and giving each one a schema of its own would cost more than it saves.
///
/// AN ABSENT COLUMN IS NOT A NULL ONE, and a shared schema gives every row every column - so a
/// cell is `Option<Cell>`: `None` where this row never had that column, `Some(Null)` where it
/// holds null. The distinction is free: `Cell` has spare discriminants, so `Option<Cell>` is
/// the same size. Collapsing the two would be the bug this half keeps finding - a row that
/// says nothing about a column reads as a row that says "nothing".
#[derive(Clone)]
pub enum Row {
    Held { schema: Rc<Schema>, cells: Box<[Option<Cell>]>, rowid: i64 },
    Built(Map<String, Value>),
}

/// ONE CELL OF A HELD ROW: a value, or a list/object STILL IN ITS JSON TEXT or its nodes, decoded the first time
/// a pass asks for it and kept decoded after that.
///
/// THE CLOSURE READS EVERY ROW BUT NOT EVERY CELL. `expressions.ast` is hundreds of MB of text on a large Angular workspace and
/// gigabytes decoded, yet the passes look an AST up through a gate, a node or a role - a few thousand of hundreds of thousands.
/// Decoding them all up front was most of the closure's memory.
///
/// NO WIDER THAN A VALUE: every row of every table read is made of these, and a structure's cell carrying its
/// own source handle, owner string and decoded slot inline made each cell of every column half as large again.
#[derive(Clone)]
pub enum Cell {
    Value(Value),
    Json(Box<(Box<str>, std::cell::OnceCell<Value>)>),
    /// A structure written as nodes: rebuilt from the schema's source for the row's owner the first time a pass reads it.
    Nodes(std::cell::OnceCell<Box<Value>>),
}

/// Where a held cell sits, which is what a cell written as nodes is rebuilt from.
struct At<'r> {
    schema: &'r Schema,
    cells: &'r [Option<Cell>],
    rowid: i64,
    index: usize,
}

impl At<'_> {
    /// The structure this cell's nodes hold, rebuilt: the owner is the row's `id`, or its rowid in a table without one.
    fn rebuild(&self) -> Value {
        let Some(source) = self.schema.nodes.get(self.index).and_then(Option::as_ref) else { return Value::Null };
        let id = self.schema.id_at.and_then(|i| self.cells.get(i)).and_then(Option::as_ref);
        let owner = match id {
            Some(Cell::Value(Value::String(id))) => id.clone(),
            Some(Cell::Value(Value::Number(n))) if n.is_i64() => n.to_string(),
            _ => format!("#{}", self.rowid),
        };
        source.get(&owner).unwrap_or(Value::Null)
    }
}

impl Cell {
    pub(super) fn json(text: String) -> Cell {
        Cell::Json(Box::new((text.into_boxed_str(), std::cell::OnceCell::new())))
    }

    fn value(&self, at: &At<'_>) -> &Value {
        match self {
            Cell::Value(v) => v,
            Cell::Json(held) => held.1.get_or_init(|| structure(&held.0)),
            Cell::Nodes(decoded) => decoded.get_or_init(|| Box::new(at.rebuild())),
        }
    }

    /// The structure this cell holds, rebuilt and NOT kept - None when it is a plain value or already decoded.
    fn undecoded(&self, at: &At<'_>) -> Option<Value> {
        match self {
            Cell::Value(_) => None,
            Cell::Json(held) => held.1.get().is_none().then(|| structure(&held.0)),
            Cell::Nodes(decoded) => decoded.get().is_none().then(|| at.rebuild()),
        }
    }

    /// Moved out for the WRITE: a cell nobody decoded goes as its text, which the store spells exactly as it
    /// spells a decoded one (`rawcells`).
    fn into_value(self, at: &At<'_>) -> Value {
        match self {
            Cell::Value(v) => v,
            Cell::Json(held) => {
                let (text, decoded) = *held;
                decoded.into_inner().unwrap_or_else(|| super::super::super::rawcells::wrap(text.into()))
            }
            Cell::Nodes(decoded) => decoded.into_inner().map(|v| *v).unwrap_or_else(|| at.rebuild()),
        }
    }
}

/// A payload cell, as a held cell: text the payload kept stays text until it is read. A FIXTURE'S door now:
/// every run's closure reads the rows back from the database (`rows/tsapply.rs`).
#[cfg(test)]
pub(super) fn cell_of(value: Value) -> Cell {
    match super::super::super::rawcells::text_of(&value) {
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

/// Two rows are equal when they are the same shape and every column reads the same - absent apart from null.
impl PartialEq for Row {
    fn eq(&self, other: &Row) -> bool {
        match (self, other) {
            (Row::Held { schema: a, .. }, Row::Held { schema: b, .. }) => a == b && a.names.iter().all(|n| self.get(n) == other.get(n)),
            (Row::Built(a), Row::Built(b)) => a == b,
            _ => false,
        }
    }
}

impl std::fmt::Debug for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Row::Held { schema, .. } => {
                let cells: Vec<Option<&Value>> = schema.names.iter().map(|n| self.get(n)).collect();
                f.debug_struct("Held").field("schema", schema).field("cells", &cells).finish()
            }
            Row::Built(map) => f.debug_tuple("Built").field(map).finish(),
        }
    }
}

impl Row {
    pub fn new() -> Row {
        Row::Built(Map::new())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Row::Held { schema, cells, rowid } => {
                let index = *schema.at.get(key)?;
                let at = At { schema, cells, rowid: *rowid, index };
                match cells.get(index)? { Some(c) => Some(c.value(&at)), None => None }
            }
            Row::Built(map) => map.get(key),
        }
    }

    /// A cell READ ONCE AND NOT KEPT: a structure nobody decoded yet is rebuilt for `f` and dropped after it, where `get`
    /// would hold it decoded for the rest of the closure. For a scan that asks something of every row's tree; the answer
    /// is the one `get` gives.
    pub fn peek<R>(&self, key: &str, f: impl FnOnce(Option<&Value>) -> R) -> R {
        if let Row::Held { schema, cells, rowid } = self
            && let Some(&index) = schema.at.get(key)
            && let Some(Some(cell)) = cells.get(index)
            && let Some(fresh) = cell.undecoded(&At { schema, cells, rowid: *rowid, index })
        {
            return f(Some(&fresh));
        }
        f(self.get(key))
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
            Row::Held { schema, .. } => {
                let mut map = Map::new();
                for name in &schema.names {
                    if let Some(value) = self.get(name) {
                        map.insert(name.clone(), value.clone());
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
            Row::Held { schema, cells, rowid } => {
                let mut map = Map::new();
                // THE OWNER IS READ BEFORE ANY CELL MOVES: a structure nobody decoded is rebuilt from it.
                let rebuilt: Vec<Option<Value>> = (0..cells.len())
                    .map(|index| match &cells[index] {
                        Some(Cell::Nodes(decoded)) if decoded.get().is_none() => Some(At { schema: &schema, cells: &cells, rowid, index }.rebuild()),
                        _ => None,
                    })
                    .collect();
                let at = At { schema: &schema, cells: &[], rowid, index: 0 };
                for ((index, cell), fresh) in cells.into_vec().into_iter().enumerate().zip(rebuilt) {
                    if let (Some(name), Some(cell)) = (schema.names.get(index), cell) {
                        map.insert(name.clone(), fresh.unwrap_or_else(|| cell.into_value(&at)));
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
pub(super) fn shared(rows: Vec<Map<String, Value>>) -> Vec<Row> {
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
            Row::Held { schema: Rc::clone(&schema), cells, rowid: 0 }
        })
        .collect()
}

/// Text back to the structure it holds, or the text itself when it holds none.
///
/// The rule: only `[` or `{` can open one, and
/// text that opens like one but does not parse is a string that happens to look like a
/// structure.
pub(super) fn structure(text: &str) -> Value {
    let Some(head) = text.chars().next() else {
        return Value::String(text.to_string());
    };
    if head != '[' && head != '{' {
        return Value::String(text.to_string());
    }
    cells::deep(text).unwrap_or_else(|_| Value::String(text.to_string()))
}
