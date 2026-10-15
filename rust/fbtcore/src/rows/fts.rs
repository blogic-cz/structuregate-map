//! A FULL-TEXT INDEX OVER EVERY ROW of the deep map, built on request.
//!
//! `--map-query` answers with SQL and is the right tool for "which bindings does this component have".
//! It is the wrong tool for "which row ANYWHERE mentions this word": that is a scan of every text column
//! of every table, and SQLite has an index for exactly it.
//!
//! TWO TABLES, AND THE SECOND IS WHAT MAKES THE FIRST USEFUL. `row_fts` is a CONTENTLESS FTS5 index - it
//! stores the index and not the text, so a million rows cost an index rather than a second copy of the
//! map - and a match comes back as a rowid, which says nothing on its own. `row_map` turns that rowid
//! into the row's own id and the table it came from.
//!
//! WHICH COLUMNS ARE INDEXED, and why it is not "all of them":
//!
//!   * an `id` is a handle, not text. Indexing it makes every row match its own number.
//!   * a JOIN column holds another row's handle, for the same reason. The map publishes which columns
//!     those are, under `_meta.spec:<half>.joins`, so this READS the answer rather than guessing it from
//!     a name - deciding by name would index `file` in one table and miss `target_ref` in the next.
//!   * a column holding a structure is NODES, in a table of its own (`cells`: `<table>__<column>`). A node
//!     table is never indexed as a table - a match there would name a node, not a row anyone asked about -
//!     and its text is skipped unless asked for (`with_json`, `--fbt-rows-fts-json`): then every text a
//!     row's nodes hold is indexed WITH THAT ROW, so a match still names the row the structure belongs to.
//!
//! Everything else is indexed when the value is TEXT IN THAT ROW - decided per value with `typeof()`,
//! because one column holds a number in one row and a name in the next, and SQLite has no declared type
//! to ask.

use super::{cells, schema};
use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

/// Rows per insert. The index is built in one transaction either way; this only bounds how much is
/// held while building it.
const BATCH: usize = 4000;

/// A node row's own bookkeeping: where it hangs, never what it says.
const NODE_COLUMNS: &[&str] = &["id", "owner", "parent", "role", "ord", "node", "file", "owner_file", "half", "lang"];

/// What a build did.
pub struct Indexed {
    /// How many rows the index holds, READ BACK OUT OF IT rather than counted while writing.
    pub rows: i64,
    /// Why nothing was built, when nothing was: no database, or a SQLite without FTS5.
    pub skipped: Option<String>,
}

/// The tables worth indexing: this tool's own bookkeeping and its two indexes are not among them.
fn indexable(db: &Connection) -> Result<Vec<String>> {
    let mut stmt = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let names = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out: Vec<String> = names
        .filter_map(|n| n.ok())
        .filter(|n| {
            !n.starts_with('_')
                && !n.starts_with("file_text")
                && !n.starts_with("row_map")
                && !n.starts_with("row_fts")
        })
        .filter(|n| cells::parent_of(db, n).is_none())
        .collect();
    out.sort();
    Ok(out)
}

/// The joins and the structure columns EVERY half published, merged.
///
/// A column is a join or a structure if ANY half says so: the halves name different tables, and one
/// that published no spec simply adds nothing.
fn spec(db: &Connection) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut joins, mut structures) = (BTreeSet::new(), BTreeSet::new());
    let Ok(mut stmt) = db.prepare("SELECT value FROM _meta WHERE key LIKE 'spec:%'") else {
        return (joins, structures);
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return (joins, structures);
    };
    for raw in rows.filter_map(|r| r.ok()) {
        let Ok(spec) = serde_json::from_str::<Value>(&raw) else { continue };
        if let Some(Value::Object(named)) = spec.get("joins") {
            joins.extend(named.keys().cloned());
        }
        if let Some(Value::Object(tables)) = spec.get("json_columns") {
            for (table, columns) in tables {
                if let Value::Array(columns) = columns {
                    for column in columns.iter().filter_map(Value::as_str) {
                        structures.insert(format!("{table}.{column}"));
                    }
                }
            }
        }
    }
    (joins, structures)
}

pub fn build(db_path: &Path, with_json: bool) -> Result<Indexed> {
    if !db_path.exists() {
        return Ok(Indexed { rows: 0, skipped: Some(format!("no database at {}", db_path.display())) });
    }
    let db = Connection::open(db_path)
        .with_context(|| format!("cannot open the map database {}", db_path.display()))?;
    db.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")?;
    db.execute_batch("DROP TABLE IF EXISTS row_map; DROP TABLE IF EXISTS row_fts;")?;
    if db
        .execute_batch("CREATE VIRTUAL TABLE row_fts USING fts5(text, content='')")
        .is_err()
    {
        return Ok(Indexed {
            rows: 0,
            skipped: Some("this sqlite has no FTS5, so the row index cannot be built".to_string()),
        });
    }
    db.execute_batch("CREATE TABLE row_map (rid INTEGER PRIMARY KEY, id TEXT, tbl TEXT)")?;

    let (joins, structures) = spec(&db);
    let tx = db.unchecked_transaction()?;
    let mut rid: i64 = 0;
    for table in indexable(&db)? {
        let columns = schema::existing_columns(&db, &table)?;
        let wanted: Vec<&String> = columns
            .iter()
            .filter(|c| {
                let qualified = format!("{table}.{c}");
                *c != "id"
                    && *c != "half"
                    && !joins.contains(&qualified)
                    && (with_json || !structures.contains(&qualified))
            })
            .collect();
        // A row's structures, when asked for: every text its nodes hold, read by the row's owner key.
        let nodes: Vec<String> = if with_json {
            cells::columns_of(&db, &table)
                .into_iter()
                .filter(|(c, _)| !joins.contains(&format!("{table}.{c}")))
                .map(|(_, child)| child)
                .collect()
        } else {
            Vec::new()
        };
        if wanted.is_empty() && nodes.is_empty() {
            continue;
        }
        let mut readers = Vec::new();
        for child in &nodes {
            let held = schema::existing_columns(&db, child)?;
            let texts: Vec<String> = held
                .iter()
                .filter(|c| !NODE_COLUMNS.contains(&c.as_str()))
                .map(|c| {
                    let e = schema::escape(c);
                    format!("CASE WHEN typeof(\"{e}\") = 'text' THEN \"{e}\" ELSE '' END")
                })
                .collect();
            if texts.is_empty() {
                continue;
            }
            readers.push((
                tx.prepare(&format!("SELECT {} FROM \"{}\" WHERE owner = ?1 ORDER BY rowid", texts.join(", "), schema::escape(child)))?,
                texts.len(),
            ));
        }
        let has_id = columns.iter().any(|c| c == "id");
        // `typeof()` PER VALUE, not per column: one column holds a number in one row and a name in
        // the next. The id travels beside the text so a match can be turned back into a row.
        let mut picked: Vec<String> = wanted
            .iter()
            .map(|c| {
                let e = schema::escape(c);
                format!("CASE WHEN typeof(t.\"{e}\") = 'text' THEN t.\"{e}\" ELSE '' END")
            })
            .collect();
        // The owner the node rows name the row by, last.
        picked.push(super::dups::owner_sql(&db, &table, "t"));
        let sql = format!(
            "SELECT {}, {} FROM \"{}\" t",
            if has_id { "t.id" } else { "NULL" },
            picked.join(", "),
            schema::escape(&table)
        );

        let mut stmt = tx.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        // (rid, the row's own id, its text). The table is the same for the whole batch.
        //
        // THE ID IS CARRIED AS SQLITE HANDED IT OVER, not turned into text here: `render_path.id`
        // is an INTEGER and most others are TEXT, and `row_map.id` is a TEXT column. Binding the
        // value as it came lets SQLite apply the same affinity it applies to any other writer -
        // reading it as a string instead failed on every integer id and quietly indexed many thousands of
        // derived rows with no id at all.
        let mut batch: Vec<(i64, rusqlite::types::Value, String)> = Vec::with_capacity(BATCH);
        while let Some(row) = rows.next()? {
            let id: rusqlite::types::Value = row.get(0)?;
            let mut parts: Vec<String> = Vec::with_capacity(wanted.len());
            for index in 1..=wanted.len() {
                if let Ok(text) = row.get::<_, String>(index)
                    && !text.is_empty()
                {
                    parts.push(text);
                }
            }
            if !readers.is_empty() {
                let owner: String = row.get(wanted.len() + 1)?;
                for (reader, width) in readers.iter_mut() {
                    let mut held = reader.query([&owner])?;
                    while let Some(node) = held.next()? {
                        for index in 0..*width {
                            if let Ok(text) = node.get::<_, String>(index)
                                && !text.is_empty()
                            {
                                parts.push(text);
                            }
                        }
                    }
                }
            }
            if parts.is_empty() {
                continue;
            }
            rid += 1;
            batch.push((rid, id, parts.join(" ")));
            if batch.len() >= BATCH {
                flush(&tx, &table, &mut batch)?;
            }
        }
        flush(&tx, &table, &mut batch)?;
    }
    tx.execute_batch(
        "CREATE INDEX ix_rowmap_id ON row_map(id); CREATE INDEX ix_rowmap_tbl ON row_map(tbl);",
    )?;
    tx.commit()?;

    let rows = db.query_row("SELECT count(*) FROM row_map", [], |r| r.get(0))?;
    Ok(Indexed { rows, skipped: None })
}

fn flush(
    db: &Connection,
    table: &str,
    batch: &mut Vec<(i64, rusqlite::types::Value, String)>,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    {
        let mut into_map = db.prepare("INSERT INTO row_map (rid, id, tbl) VALUES (?1, ?2, ?3)")?;
        let mut into_fts = db.prepare("INSERT INTO row_fts (rowid, text) VALUES (?1, ?2)")?;
        for (rid, id, text) in batch.iter() {
            into_map.execute(rusqlite::params![rid, id, table])?;
            into_fts.execute(rusqlite::params![rid, text])?;
        }
    }
    batch.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn matches(path: &Path, word: &str) -> Vec<(String, String)> {
        let db = Connection::open(path).unwrap();
        let mut stmt = db.prepare("SELECT m.id, m.tbl FROM row_fts f JOIN row_map m ON m.rid = f.rowid WHERE row_fts MATCH ?1 ORDER BY m.id").unwrap();
        stmt.query_map([word], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect()
    }

    #[test]
    fn a_structure_is_indexed_with_its_row_only_when_asked_and_a_node_table_never_on_its_own() {
        let dir = std::env::temp_dir().join(format!("fbt-rowfts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("map.sqlite");
        let _ = std::fs::remove_file(&path);
        {
            let db = Connection::open(&path).unwrap();
            let rows = [json!({"id": "e:1", "source": "cart.total", "ast": {"k": "Read", "name": "basket"}})];
            super::super::store::insert_rows(&db, "expressions", &rows).unwrap();
        }
        let built = build(&path, false).unwrap();
        assert_eq!(built.rows, 1);
        assert_eq!(matches(&path, "cart"), vec![("e:1".to_string(), "expressions".to_string())]);
        // Not asked for: the node's text is in no row, and the node table is no row of its own.
        assert!(matches(&path, "basket").is_empty());
        build(&path, true).unwrap();
        assert_eq!(matches(&path, "basket"), vec![("e:1".to_string(), "expressions".to_string())]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
