//! ONE TYPED VIEW PER SEEDED TABLE - `seed_<schema>_<name>`, a column per column any seed row of it sets, beside
//! `id, file, line, database, condition, status`. `sql_seeds` keeps each row's columns and values as two JSON
//! arrays in the order its INSERT wrote them, and two INSERTs into one table can list them in different orders:
//! a join on `ProductID` had to find the column's position row by row. The view does it once, for every row.
//!
//! VIEWS, NOT TABLES: they hold nothing, so they cannot disagree with `sql_seeds`, and they are dropped and made
//! again whenever it is rewritten. A `#temp` is no table and gets none.

use super::schema;
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
        let mut stmt = conn.prepare("SELECT schema, name, columns FROM sql_seeds WHERE status <> 'temp' ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (schema_name, name, columns) = row?;
            let schema_name = schema(&schema_name);
            let entry = tables
                .entry((schema_name.to_lowercase(), name.to_lowercase()))
                .or_insert_with(|| (schema_name.clone(), name.clone(), Vec::new()));
            for column in serde_json::from_str::<Vec<String>>(&columns).unwrap_or_default() {
                if !entry.2.iter().any(|c| c.eq_ignore_ascii_case(&column)) {
                    entry.2.push(column);
                }
            }
        }
    }
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
                ", (SELECT json_extract(s.row_values, '$[' || c.key || ']') FROM json_each(s.columns) c \
                 WHERE lower(c.value) = '{}') AS \"{}\"",
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
        conn.execute_batch(
            "CREATE TABLE files (id, path); INSERT INTO files VALUES ('f:1', 'Seed.sql');
             CREATE TABLE sql_seeds (id, file, line, database, condition, schema, name, columns, row_values, status);
             INSERT INTO sql_seeds VALUES ('sd:1', 'f:1', 1, 'Main', '', 'Sales', 'Products', '[\"ProductID\",\"Name\"]', '[1,\"Widget\"]', 'bound');
             INSERT INTO sql_seeds VALUES ('sd:2', 'f:1', 2, 'Main', 'US', 'Sales', 'Products', '[\"Name\",\"ProductID\",\"Status\"]', '[\"Old\",503,1]', 'bound');
             INSERT INTO sql_seeds VALUES ('sd:3', 'f:1', 3, 'Main', '', '', '#tmp', '[\"A\"]', '[1]', 'temp');",
        )
        .unwrap();
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
