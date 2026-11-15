//! A LIST OF FACTS OUT OF THE TABLES OF A SECTION. A document's table rarely has the header a GFM table expects:
//! A document export can write an EMPTY header row and put `Product code` in the first body row, then split the one
//! list over many tables, each under a bold paragraph. So a named column is the first row - header or body -
//! that holds a cell with that text, and the rows after it are the data; a table holding no such row continues
//! the one before it, with the same columns.

use super::config::Column;
use super::reader::Found;
use serde_json::{Map, Value};

/// One fact: where it is, the subsection it sits under, its key and label, and the row as `{header: cell}`.
#[derive(Debug)]
pub struct Row {
    pub at: usize,
    pub subsection: String,
    pub key: String,
    pub label: String,
    pub cells: Map<String, Value>,
}

/// Where the columns are: the key's, the label's, and the header cells naming every column.
#[derive(Clone)]
struct Columns {
    key: usize,
    label: Option<usize>,
    header: Vec<String>,
}

pub fn rows(tables: &[Found], key: &Column, label: &Option<Column>) -> Result<Vec<Row>, String> {
    let mut out = Vec::new();
    let mut carried: Option<Columns> = None;
    for table in tables {
        let (columns, data) = match key {
            Column::At(n) => {
                let header = if table.head { table.rows.first().map(|r| r.1.clone()).unwrap_or_default() } else { Vec::new() };
                let label = label.as_ref().and_then(|l| position(l, &header));
                (Columns { key: n - 1, label, header }, if table.head { &table.rows[1.min(table.rows.len())..] } else { table.rows })
            }
            Column::Name(name) => match table.rows.iter().position(|(_, cells)| cells.iter().any(|c| same(c, name))) {
                Some(h) => {
                    let header = table.rows[h].1.clone();
                    let key = header.iter().position(|c| same(c, name)).unwrap_or(0);
                    let label = label.as_ref().and_then(|l| position(l, &header));
                    (Columns { key, label, header }, &table.rows[h + 1..])
                }
                None => match &carried {
                    Some(columns) => (columns.clone(), if table.head { &table.rows[1.min(table.rows.len())..] } else { table.rows }),
                    None => {
                        let first: Vec<String> = table.rows.iter().take(2).map(|(_, r)| format!("[{}]", r.join(" | "))).collect();
                        return Err(format!("no row of the first table holds a cell `{name}` (it starts {})", first.join(", ")));
                    }
                },
            },
        };
        for (at, cells) in data {
            let key_text = cells.get(columns.key).cloned().unwrap_or_default();
            if key_text.is_empty() {
                continue;
            }
            let label_text = columns.label.and_then(|l| cells.get(l).cloned()).unwrap_or_default();
            let named: Map<String, Value> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let name = columns.header.get(i).filter(|h| !h.is_empty()).cloned().unwrap_or_else(|| (i + 1).to_string());
                    (name, Value::String(c.clone()))
                })
                .collect();
            out.push(Row { at: *at, subsection: table.subsection.clone(), key: key_text, label: label_text, cells: named });
        }
        carried = Some(columns);
    }
    Ok(out)
}

fn position(column: &Column, header: &[String]) -> Option<usize> {
    match column {
        Column::At(n) => Some(n - 1),
        Column::Name(name) => header.iter().position(|c| same(c, name)),
    }
}

fn same(cell: &str, name: &str) -> bool {
    cell.trim().to_lowercase() == name.trim().to_lowercase()
}
