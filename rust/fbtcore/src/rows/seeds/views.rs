//! ONE TYPED VIEW PER SEEDED TABLE - `seed_<schema>_<name>`, a column per column any seed row of it sets, beside
//! `id, file, line, database, condition, status`. `sql_seeds` keeps each row's columns and values as two LISTS
//! (`sql_seeds__columns`, `sql_seeds__row_values`, one node per item) in the order its INSERT wrote them, and two INSERTs into one table can list them in different orders:
//! a join on `ProductID` had to find the column's position row by row. The view does it once, for every row.
//!
//! VIEWS, NOT TABLES: they hold nothing, so they cannot disagree with `sql_seeds`, and they are dropped and made
//! again whenever it is rewritten. A `#temp` is no table and gets none.

use super::schema;
use crate::rows::dups::{items_sql, Column};
use rusqlite::types::Value;
use rusqlite::Connection;
use std::collections::BTreeMap;

pub const PREFIX: &str = "seed_";

/// Every `seed_` view dropped, and one made per seeded table. The number made.
pub fn write(conn: &Connection) -> rusqlite::Result<usize> {
    let old: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'view' AND name LIKE 'seed\\_%' ESCAPE '\\'")?;
        stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    for name in old {
        conn.execute_batch(&format!("DROP VIEW IF EXISTS \"{}\"", quoted(&name)))?;
    }
    // EVERY COLUMN A ROW OF THE TABLE SETS, in the order they are first met: a table seeded by two INSERTs with
    // different column lists has the union of both.
    let mut tables: BTreeMap<(String, String), (String, String, Vec<String>)> = BTreeMap::new();
    {
        let mut list = Column::new(conn, "sql_seeds", "columns")?;
        let mut stmt = conn.prepare("SELECT schema, name, id FROM sql_seeds WHERE status <> 'temp' ORDER BY id")?;
        let rows: Vec<(String, String, String)> =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        for (schema_name, name, id) in rows {
            let columns = list.strings(&id, &Value::Null)?;
            let schema_name = schema(&schema_name);
            let entry = tables
                .entry((schema_name.to_lowercase(), name.to_lowercase()))
                .or_insert_with(|| (schema_name.clone(), name.clone(), Vec::new()));
            for column in columns {
                if !entry.2.iter().any(|c| c.eq_ignore_ascii_case(&column)) {
                    entry.2.push(column);
                }
            }
        }
    }
    // A value is the item of `row_values` at the position its column has in `columns`.
    let (names, values) = (items_sql(conn, "sql_seeds", "columns", &["value"]), items_sql(conn, "sql_seeds", "row_values", &["value"]));
    let mut made = 0;
    let mut taken: Vec<String> = Vec::new();
    for (schema_name, name, columns) in tables.values() {
        let view = format!("{PREFIX}{}_{}", schema_name, name);
        // A NAME IS A VIEW'S ONCE: `seed_a_b_c` is both `a.b_c` and `a_b.c`, and the second is left out rather
        // than made under a name that says the wrong table.
        if taken.iter().any(|t| t.eq_ignore_ascii_case(&view)) {
            continue;
        }
        let mut select = String::from(
            "SELECT s.id AS id, f.path AS file, s.line AS line, s.database AS database, s.condition AS condition, s.status AS status",
        );
        for column in columns {
            // A SEED COLUMN NAMED LIKE ONE OF THE VIEW'S OWN (`status`, `file`) would hide it; it is named for its
            // table instead.
            let shown = if ["id", "file", "line", "database", "condition", "status"].iter().any(|c| c.eq_ignore_ascii_case(column)) {
                format!("{name}_{column}")
            } else {
                column.clone()
            };
            select.push_str(&format!(
                ", (SELECT v.value FROM {names} c JOIN {values} v ON v.owner = c.owner AND v.ord = c.ord \
                 WHERE c.owner = s.id AND lower(c.value) = '{}' ORDER BY c.ord LIMIT 1) AS \"{}\"",
                literal(&column.to_lowercase()),
                quoted(&shown)
            ));
        }
        select.push_str(&format!(
            " FROM sql_seeds s LEFT JOIN files f ON f.id = s.file WHERE s.status <> 'temp' \
             AND lower(CASE s.schema WHEN '' THEN 'dbo' ELSE s.schema END) = '{}' AND lower(s.name) = '{}'",
            literal(&schema_name.to_lowercase()),
            literal(&name.to_lowercase())
        ));
        conn.execute_batch(&format!("CREATE VIEW \"{}\" AS {select}", quoted(&view)))?;
        taken.push(view);
        made += 1;
    }
    Ok(made)
}

fn quoted(name: &str) -> String {
    name.replace('"', "\"\"")
}

fn literal(text: &str) -> String {
    text.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_column_is_read_by_name_whatever_its_position_in_the_row() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE files (id, path); INSERT INTO files VALUES ('f:1', 'Seed.sql');").unwrap();
        // The seeds as `seeds::write` stores them: the two lists are nodes.
        let seed = |id: &str, line: i64, condition: &str, schema: &str, name: &str, columns: serde_json::Value, values: serde_json::Value, status: &str| {
            serde_json::json!({"id": id, "file": "f:1", "line": line, "database": "Main", "condition": condition, "schema": schema,
                               "name": name, "columns": columns, "row_values": values, "status": status})
        };
        let rows = [
            seed("sd:1", 1, "", "Sales", "Products", serde_json::json!(["ProductID", "Name"]), serde_json::json!([1, "Widget"]), "bound"),
            seed("sd:2", 2, "US", "Sales", "Products", serde_json::json!(["Name", "ProductID", "Status"]), serde_json::json!(["Old", 503, 1]), "bound"),
            seed("sd:3", 3, "", "", "#tmp", serde_json::json!(["A"]), serde_json::json!([1]), "temp"),
        ];
        crate::rows::store::insert_rows(&conn, "sql_seeds", &rows).unwrap();
        assert_eq!(write(&conn).unwrap(), 1);
        let rows: Vec<(i64, String, String, Option<i64>)> = conn
            .prepare("SELECT ProductID, Name, condition, Products_Status FROM seed_Sales_Products ORDER BY line")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows, vec![(1, "Widget".into(), "".into(), None), (503, "Old".into(), "US".into(), Some(1))]);
        // Made again, never twice.
        assert_eq!(write(&conn).unwrap(), 1);
    }
}
