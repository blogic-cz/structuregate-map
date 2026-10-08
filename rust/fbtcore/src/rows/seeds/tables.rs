//! THE MAP'S TABLES a seed row is checked against - per database, with their columns in declaration order,
//! which of them an INSERT without a column list writes, and their constant DEFAULTs.

use super::value::{decimal, Val};
use super::{Pending, Seed};
use super::{num, text};
use rusqlite::Connection;
use std::collections::HashMap;

struct Column {
    name: String,
    /// An INSERT without a column list skips an identity or a computed column.
    written: bool,
    default: Option<Val>,
}

pub struct Tables {
    by_name: HashMap<String, Vec<(String, String)>>,
    columns: HashMap<String, Vec<Column>>,
}

impl Tables {
    pub fn load(conn: &Connection) -> Tables {
        let mut tables = Tables { by_name: HashMap::new(), columns: HashMap::new() };
        for row in super::query(conn, "SELECT id, database, schema, name FROM sql_objects WHERE kind IN ('table', 'view')").unwrap_or_default() {
            let key = format!("{}.{}", text(&row[2]), text(&row[3])).to_lowercase();
            tables.by_name.entry(key).or_default().push((text(&row[0]), text(&row[1])));
        }
        for row in super::query(conn, "SELECT object, name, identity, computed, default_expr FROM sql_columns ORDER BY object, position").unwrap_or_default() {
            tables.columns.entry(text(&row[0])).or_default().push(Column {
                name: text(&row[1]),
                written: num(&row[2]) == 0 && text(&row[3]).is_empty(),
                default: literal(&text(&row[4])),
            });
        }
        tables
    }

    /// The table in THE SCRIPT'S OWN database; a script of no project takes the only one there is.
    pub fn find(&self, database: &str, schema_name: &str) -> String {
        let hits = self.by_name.get(&schema_name.to_lowercase()).map(Vec::as_slice).unwrap_or_default();
        let own: Vec<&(String, String)> = hits.iter().filter(|h| h.1.eq_ignore_ascii_case(database)).collect();
        if own.len() == 1 {
            own[0].0.clone()
        } else if database.is_empty() && hits.len() == 1 {
            hits[0].0.clone()
        } else {
            String::new()
        }
    }

    pub fn columns_of(&self, database: &str, schema_name: &str, written: bool) -> Vec<String> {
        if schema_name.is_empty() {
            return Vec::new();
        }
        self.columns
            .get(&self.find(database, schema_name))
            .map(|cols| cols.iter().filter(|c| !written || c.written).map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    /// The row as the table holds it after the INSERT: an INSERT with no column list names the writable
    /// columns in order, and a column it leaves out gets the table's constant DEFAULT - a row whose script
    /// never writes `IsArchived` holds its `DEFAULT 0`.
    pub fn completed(&self, row: &Pending, schema: &str, name: &str) -> Pending {
        let known = self.columns.get(&self.find(&row.database, &format!("{schema}.{name}")));
        let known: &[Column] = known.map(Vec::as_slice).unwrap_or_default();
        let mut cols: Vec<String> =
            if !row.columns.is_empty() { row.columns.clone() } else { known.iter().filter(|c| c.written).map(|c| c.name.clone()).collect() };
        let mut values = row.values.clone();
        let mut defaulted = Vec::new();
        if cols.len() == values.len() {
            for column in known {
                let Some(fallback) = column.default.as_ref().filter(|_| column.written) else { continue };
                if cols.iter().any(|c| c.eq_ignore_ascii_case(&column.name)) {
                    continue;
                }
                cols.push(column.name.clone());
                values.push(fallback.clone());
                defaulted.push(column.name.clone());
            }
        }
        Pending { columns: cols, values, defaulted, ..row.clone() }
    }

    pub fn seed(&self, row: Pending, schema: &str, name: &str) -> Seed {
        let id = self.find(&row.database, &format!("{schema}.{name}"));
        let known = self.columns.get(&id);
        let cols = &row.columns;
        let mut problems = Vec::new();
        if id.is_empty() {
            problems.push(format!("{schema}.{name} is in no project of the map"));
        }
        if cols.len() != row.values.len() {
            problems.push(format!("{} column(s) for {} value(s)", cols.len(), row.values.len()));
        }
        let unknown: Vec<&String> = if id.is_empty() {
            Vec::new()
        } else {
            cols.iter().filter(|c| !known.is_some_and(|k| k.iter().any(|k| k.name.eq_ignore_ascii_case(c)))).collect()
        };
        if !unknown.is_empty() {
            problems.push(format!("not columns of {schema}.{name}: {}", unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
        let open: Vec<String> = row
            .values
            .iter()
            .enumerate()
            .filter(|(_, v)| v.0 == "expr" || v.0 == "unset")
            .map(|(i, v)| {
                let column = cols.get(i).cloned().unwrap_or_else(|| format!("#{}", i + 1));
                format!("{column} = {}{}", v.1, if v.0 == "unset" { " (not set in this chain)" } else { "" })
            })
            .collect();
        if !open.is_empty() {
            problems.push(format!("not known before run time: {}", open.join(", ")));
        }
        if !row.dropped.is_empty() {
            problems.push(format!("not carried into the table: {}", row.dropped.join(", ")));
        }
        let status = if problems.is_empty() { "bound" } else { "partial" };
        // A NOTE, not a problem: the value is known, only what the database can store of it is not.
        let varchar: Vec<&String> =
            row.values.iter().enumerate().filter(|(i, v)| v.0 == "vstr" && *i < cols.len()).map(|(i, _)| &cols[i]).collect();
        if !varchar.is_empty() {
            problems.push(format!(
                "a string without N, so the database's code page decides which characters it keeps: {}",
                varchar.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        Seed { row, schema: schema.into(), name: name.into(), object: id, status: status.into(), detail: problems.join("; ") }
    }
}

/// A DEFAULT that is a constant - `0`, `((1))`, `N'EU'`, `NULL` - as a value; anything the server computes
/// (`GETDATE()`, `NEWID()`) is none.
fn literal(text: &str) -> Option<Val> {
    let mut value = text.trim();
    while value.len() > 1 && value.starts_with('(') && value.ends_with(')') {
        value = value[1..value.len() - 1].trim();
    }
    if value.is_empty() {
        return None;
    }
    if value.eq_ignore_ascii_case("NULL") {
        return Some(("null".into(), String::new()));
    }
    let quoted = if value.len() >= 2 && value[..2].eq_ignore_ascii_case("N'") { &value[1..] } else { value };
    if quoted.len() >= 2 && quoted.starts_with('\'') && quoted.ends_with('\'') {
        return Some(("str".into(), quoted[1..quoted.len() - 1].replace("''", "'")));
    }
    if value.parse::<i64>().is_ok() {
        return Some(("int".into(), value.into()));
    }
    decimal(value).map(|_| ("num".into(), value.into()))
}
