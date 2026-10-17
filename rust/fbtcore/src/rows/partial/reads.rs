//! WHAT EACH FILE'S EXTRACTION READ, handed to the next partial run - and proved against the rows on a full one.
//!
//! node records `files.reads` (every in-tree file the checker answered from while a file was extracted) and
//! `files.surface` (its declaration emit, hashed) - see `src/TsRows/TsSetup/TsReads.mjs`. A partial run re-reads a
//! file when something it READ changed, which is a smaller set than every hop of dependents and, unlike a hop
//! count, loses nothing: a file is kept only when nothing it read moved.
//!
//! THE READS ARE A RECORDING, and a recording can miss a way in. So a FULL run holds them against the one graph
//! that cannot miss what it describes - every id one file's rows name in another (`deps::graph`) - and a reference
//! the reads do not cover is an error: the file holding it would not be re-read when the file it names changed.
//!
//! A TEMPLATE IS NOT READ THROUGH THE CHECKER: the template pass runs from its component, so the two are
//! re-extracted together (`coupled`), whatever else changed.

use indexmap::{IndexMap, IndexSet};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Value};
use std::path::Path;

/// `readers`: path -> the files that read it. `deep_readers`: path -> the files that asked the checker about a
/// node inside it. `surfaces`: path -> its surface. `coupled`: a template and its component, each way. `loaded`
/// is false when the columns could not be read - and then nothing here may be trusted to cut anything short.
/// `shapes`: path -> its shape (`TsSetup/TsShape.mjs`), which decides whether an edit to a file a template
/// resolves through can go the short way; read on its own, so a database without the column loses nothing else.
pub struct Recorded {
    pub readers: Value,
    pub deep_readers: Value,
    pub surfaces: Value,
    pub shapes: Value,
    pub coupled: Value,
    pub loaded: bool,
}

fn to_value(graph: IndexMap<String, IndexSet<String>>) -> Value {
    let mut out = Map::new();
    for (key, set) in graph {
        let mut list: Vec<String> = set.into_iter().collect();
        list.sort();
        out.insert(key, Value::from(list));
    }
    Value::Object(out)
}

/// Read back from the database. A database written before the reads were recorded has no such columns, and
/// answers nothing - which the reader takes as "read everything".
pub fn recorded(db: &Path, half: &str) -> Recorded {
    let mut readers: IndexMap<String, IndexSet<String>> = IndexMap::new();
    let mut deep_readers: IndexMap<String, IndexSet<String>> = IndexMap::new();
    let mut loaded = false;
    let mut surfaces = Map::new();
    let mut shapes = Map::new();
    let mut coupled: IndexMap<String, IndexSet<String>> = IndexMap::new();
    if let Ok(conn) = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        // `reads` and `reads_deep` are lists, so nodes (`files__reads`, `files__reads_deep`), read by the file's id.
        // A database whose files never recorded them answers nothing, as before.
        let held = super::dups::has_columns(&conn, "files", &["reads", "surface", "reads_deep"]);
        let lists = super::dups::Column::new(&conn, "files", "reads").and_then(|r| Ok((r, super::dups::Column::new(&conn, "files", "reads_deep")?)));
        let scalar = |c: &str| super::dups::scalar_sql(&conn, "files", "f", c);
        let sql = format!(
            "SELECT f.path, {}, f.surface, {}, {} FROM files f WHERE f.half = ?1",
            scalar("reads"),
            scalar("reads_deep"),
            super::dups::owner_sql(&conn, "files", "f")
        );
        if held
            && let Ok((mut reads_of, mut deep_of)) = lists
            && let Ok(mut stmt) = conn.prepare(&sql)
        {
            loaded = true;
            let rows = stmt.query_map([half], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, rusqlite::types::Value>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, rusqlite::types::Value>(3)?,
                    r.get::<_, String>(4)?,
                ))
            });
            let rows: Vec<_> = rows.into_iter().flatten().flatten().collect();
            for (path, reads, surface, deep, owner) in rows {
                for (column, held, into) in [(&mut reads_of, reads, &mut readers), (&mut deep_of, deep, &mut deep_readers)] {
                    if let Ok(Value::Array(list)) = column.value(&owner, &held) {
                        for read in list.iter().filter_map(Value::as_str) {
                            into.entry(read.to_string()).or_default().insert(path.clone());
                        }
                    }
                }
                if let Some(surface) = surface {
                    surfaces.insert(path, Value::String(surface));
                }
            }
        }
        if let Ok(mut stmt) = conn.prepare("SELECT path, shape FROM files WHERE half = ?1 AND shape IS NOT NULL") {
            let rows = stmt.query_map([half], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
            for (path, shape) in rows.into_iter().flatten().flatten() {
                shapes.insert(path, Value::String(shape));
            }
        }
        let sql = "SELECT tf.path, cf.path FROM templates t JOIN files tf ON tf.id = t.file \
                   JOIN classes c ON c.id = t.class JOIN files cf ON cf.id = c.file \
                   WHERE t.half = ?1 AND tf.path <> cf.path";
        if let Ok(mut stmt) = conn.prepare(sql) {
            let rows = stmt.query_map([half], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
            for (template, component) in rows.into_iter().flatten().flatten() {
                coupled.entry(template.clone()).or_default().insert(component.clone());
                coupled.entry(component).or_default().insert(template);
            }
        }
    }
    Recorded {
        readers: to_value(readers),
        deep_readers: to_value(deep_readers),
        surfaces: Value::Object(surfaces),
        shapes: Value::Object(shapes),
        coupled: to_value(coupled),
        loaded,
    }
}

/// What a full run's rows name that the file extracting them never read, as `owner -> named` pairs.
///
/// A row belongs to the file whose extraction made it (`owner_file`, else `file`), and it is made again only
/// when that file is read again. So everything the row names - the file it describes (`file`) and the owner
/// of every row whose id it holds - has to be the owner itself or among the owner's reads; anything else is a
/// file whose change would leave the row as it was. A template is read with its component, not through the
/// checker (see the header), and a file this half never extracted records no reads at all.
///
/// THREE THINGS ARE NAMED WITHOUT BEING READ, and each is answered elsewhere: a file that declares what a
/// template resolves (`deps::scope_files`) turns any change of it into a full run; a table a rollup rebuilds
/// (`rebuilt`) is derived again whole; and `node_modules` is not walked, so nothing in it ever reads as changed.
#[cfg(test)]
pub fn unread(tables: &Map<String, Value>, rebuilt: &[String]) -> Vec<(String, String)> {
    // ONE DERIVATION, streamed or held - see `digest.rs`.
    super::digest::Digest::over(tables, rebuilt).unread()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_reads_a_file_recorded_come_back_from_their_nodes() {
        let db = crate::rows::TempDb::new("reads", "nodes");
        {
            let conn = Connection::open(&*db).unwrap();
            let files = [
                json!({"id": "f:1", "path": "a.ts", "half": "typescript", "reads": ["b.ts"], "surface": "s1", "reads_deep": []}),
                json!({"id": "f:2", "path": "b.ts", "half": "typescript", "reads": [], "surface": "s2", "reads_deep": ["a.ts"]}),
            ];
            crate::rows::store::insert_rows(&conn, "files", &files).unwrap();
        }
        let got = recorded(&db, "typescript");
        assert!(got.loaded);
        assert_eq!(got.readers, json!({"b.ts": ["a.ts"]}));
        assert_eq!(got.deep_readers, json!({"a.ts": ["b.ts"]}));
        assert_eq!(got.surfaces, json!({"a.ts": "s1", "b.ts": "s2"}));
    }

    #[test]
    fn a_row_naming_a_file_its_owner_never_read_is_named_and_one_it_read_is_not() {
        let tables = json!({
            "files": [
                {"id": "f:1", "path": "a.ts", "reads": ["b.ts"]},
                {"id": "f:2", "path": "b.ts", "reads": []},
                {"id": "f:3", "path": "c.ts", "reads": []},
                {"id": "f:4", "path": "c.html"}
            ],
            "classes": [{"id": "c:1", "file": "f:2"}],
            "calls": [
                {"id": "x:1", "file": "f:1", "target": "c:1"},
                {"id": "x:2", "file": "f:3", "target": "c:1"},
                {"id": "x:3", "file": "f:2", "owner_file": "f:3"},
                {"id": "x:4", "file": "f:4", "target": "c:1"}
            ]
        });
        let missed = unread(tables.as_object().unwrap(), &[]);
        assert_eq!(missed, vec![("c.ts".to_string(), "b.ts".to_string())]);
    }
}
