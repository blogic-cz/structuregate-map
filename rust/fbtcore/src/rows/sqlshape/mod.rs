//! WHAT A SQL STATEMENT IS, derived from its normalized tree (`sql_nodes`, one node per clause, table, column,
//! function, operator and literal, written by the SQL half - `src/Map/Sql/SqlTree.cs`): its fingerprint, the columns
//! it names (`sql_column_refs`, `columns.rs`) and its shape as views (`sql_clauses`, `sql_joins`, `sql_functions`,
//! `sql_ctes` - `views.rs`).
//!
//! EXACT ON A PARTIAL RUN BY CONSTRUCTION: the half writes `fingerprint = ''` on every statement it reads, the
//! store drops a re-read file's rows by `file` - `sql_column_refs` too - before writing its new ones, and this pass
//! reads only the statements still `''`, the ones written this run. A statement it fills is never read again until
//! its file moves. The views hold nothing.

mod columns;
mod fingerprint;
mod views;

use rusqlite::{Connection, OptionalExtension};

/// `sql_column_refs`, in the order its rows are written. `file` is what the store drops a re-read file's rows by.
const COLUMN_REFS: [&str; 11] = ["file", "object", "statement", "line", "role", "schema", "name", "source_kind", "alias", "column", "database"];

/// One node of a statement's tree, as `sql_nodes` holds it. `n` is its preorder number, the root 0.
#[derive(Clone, Debug)]
pub struct Node {
    pub n: i64,
    pub parent: Option<i64>,
    pub kind: String,
    pub value: String,
    pub line: i64,
}

pub use fingerprint::of as fingerprint;

fn has_table(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

/// Fills what the statements written this run are; the number of statements it filled.
pub fn run(db: &str) -> anyhow::Result<usize> {
    let mut conn = Connection::open(db)?;
    if !has_table(&conn, "sql_statements")? || !has_table(&conn, "sql_nodes")? {
        return Ok(0);
    }
    // A NODE IS FOUND BY ITS STATEMENT AND ITS PARENT - this pass reads a statement's nodes, a view a node's children.
    // The store indexes every `parent` column alone, which no reader of this table asks: a node's `parent` is a number
    // within its statement. It is dropped, once per database - the store makes it only with the table.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS ix_sql_nodes_statement ON sql_nodes (statement, parent); DROP INDEX IF EXISTS ix_sql_nodes_parent",
    )?;
    // THE TABLE EXISTS WHEREVER STATEMENTS DO, rows or none: a query of it never fails on a tree with no column named.
    crate::rows::schema::create(&conn, "sql_column_refs", &COLUMN_REFS.map(String::from))?;
    views::write(&conn)?;
    let text = |r: &rusqlite::Row, i: usize| -> rusqlite::Result<String> { Ok(r.get::<_, Option<String>>(i)?.unwrap_or_default()) };
    let fresh: Vec<[String; 4]> = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.file, s.object, f.database FROM sql_statements s LEFT JOIN files f ON f.id = s.file \
             WHERE s.fingerprint = '' ORDER BY s.rowid",
        )?;
        stmt.query_map([], |r| Ok([text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?]))?.collect::<rusqlite::Result<_>>()?
    };
    if fresh.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    {
        let mut read = tx.prepare("SELECT n, parent, kind, value, line FROM sql_nodes WHERE statement = ?1 ORDER BY n")?;
        let mut stamp = tx.prepare("UPDATE sql_statements SET fingerprint = ?2 WHERE id = ?1")?;
        let mut named = tx.prepare(&format!(
            "INSERT INTO sql_column_refs ({}) VALUES ({})",
            COLUMN_REFS.map(|c| format!("\"{c}\"")).join(", "),
            vec!["?"; COLUMN_REFS.len()].join(", ")
        ))?;
        for [id, file, object, database] in &fresh {
            let nodes: Vec<Node> = read
                .query_map([id], |r| {
                    Ok(Node {
                        n: r.get(0)?,
                        parent: r.get(1)?,
                        kind: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        value: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        line: r.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            for c in columns::of(&nodes, "dbo") {
                named.execute(rusqlite::params![file, object, id, c.line, c.role, c.schema, c.name, c.source_kind, c.alias, c.column, database])?;
            }
            stamp.execute((id, fingerprint(&nodes)))?;
        }
    }
    tx.commit()?;
    Ok(fresh.len())
}
