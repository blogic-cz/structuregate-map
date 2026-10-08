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
        if let Ok(mut stmt) = conn.prepare("SELECT path, reads, surface, reads_deep FROM files WHERE half = ?1") {
            loaded = true;
            let rows = stmt.query_map([half], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            });
            for (path, reads, surface, deep) in rows.into_iter().flatten().flatten() {
                for (text, into) in [(reads, &mut readers), (deep, &mut deep_readers)] {
                    if let Some(text) = text
                        && let Ok(Value::Array(list)) = serde_json::from_str::<Value>(&text)
                    {
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
pub fn unread(tables: &Map<String, Value>, rebuilt: &[String]) -> Vec<(String, String)> {
    let rows_of = |name: &str| tables.get(name).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object);
    let text = |row: &Map<String, Value>, name: &str| match row.get(name) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };
    let mut path_of: IndexMap<String, String> = IndexMap::new();
    let mut reads: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for row in rows_of("files") {
        let (Some(id), Some(path)) = (text(row, "id"), text(row, "path")) else { continue };
        if row.contains_key("reads") {
            reads.insert(path.clone(), super::deps::list(row.get("reads")).into_iter().collect());
        }
        path_of.insert(id, path);
    }
    let owner = |row: &Map<String, Value>| text(row, "owner_file").or_else(|| text(row, "file"));
    let mut owner_of: IndexMap<String, String> = IndexMap::new();
    for (name, rows) in tables {
        for row in rows.as_array().into_iter().flatten().filter_map(Value::as_object) {
            let Some(id) = text(row, "id") else { continue };
            let home = if name == "files" { Some(id.clone()) } else { owner(row) };
            if let Some(home) = home {
                owner_of.insert(id, home);
            }
        }
    }
    let scope: IndexSet<String> = super::deps::scope_files(tables).into_iter().collect();
    let template = |p: &str| p.ends_with(".html");
    let answered = |p: &str| template(p) || scope.contains(p) || p.starts_with("node_modules/") || p.contains("/node_modules/");
    let mut seen: IndexSet<(String, String)> = IndexSet::new();
    for (name, rows) in tables {
        if name == "files" || rebuilt.iter().any(|r| r == name) {
            continue;
        }
        for row in rows.as_array().into_iter().flatten().filter_map(Value::as_object) {
            let Some(own) = owner(row).and_then(|o| path_of.get(&o).cloned()) else { continue };
            let Some(read) = reads.get(&own) else { continue };
            if template(&own) {
                continue;
            }
            for (column, value) in row {
                if column == "id" || column == "owner_file" {
                    continue;
                }
                let Value::String(v) = value else { continue };
                let Some(named) = owner_of.get(v).and_then(|o| path_of.get(o)) else { continue };
                if *named != own && !answered(named) && !read.contains(named) {
                    seen.insert((own.clone(), named.clone()));
                }
            }
        }
    }
    seen.into_iter().collect()
}

/// How a refusal by `dangling` begins, so the caller asks again over every hop rather than reporting an error.
pub const STOPPED_SHORT: &str = "a run that stopped short of every hop";

/// THE REFERENCES A PARTIAL RUN WOULD LEAVE POINTING AT NOTHING, before it writes: the first one found, and how
/// many there are.
///
/// A file read again keeps its rows' ids where they are claimable (`carry`), but not every row is - and a row
/// in a file that was NOT read again, naming one of those ids, would then name a row that is gone. It can only
/// be a row of a file that pointed into a replaced one, and `deps` (the stored id graph) names exactly those,
/// so only their rows are looked through, for only the ids that are about to vanish.
pub fn dangling(
    db: &Connection,
    half: &str,
    replaced: &[String],
    tables: &Map<String, Value>,
    deps: &Map<String, Value>,
) -> anyhow::Result<Option<(usize, String)>> {
    use super::half::{tables_with_half, HALF_COLUMN};
    use super::schema::escape;
    let file_ids = |paths: &IndexSet<String>| -> anyhow::Result<IndexSet<String>> {
        let mut out = IndexSet::new();
        let mut stmt = db.prepare("SELECT id FROM files WHERE half = ?1 AND path = ?2")?;
        for p in paths {
            for id in stmt.query_map([half, p.as_str()], |r| r.get::<_, String>(0))?.flatten() {
                out.insert(id);
            }
        }
        Ok(out)
    };
    let gone_paths: IndexSet<String> = replaced.iter().cloned().collect();
    let mut watchers: IndexSet<String> = IndexSet::new();
    for p in &gone_paths {
        for d in deps.get(p).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if !gone_paths.contains(d) {
                watchers.insert(d.to_string());
            }
        }
    }
    if watchers.is_empty() {
        return Ok(None);
    }
    let mut kept: IndexSet<String> = IndexSet::new();
    for rows in tables.values().filter_map(Value::as_array) {
        for row in rows.iter().filter_map(Value::as_object) {
            if let Some(Value::String(id)) = row.get("id") {
                kept.insert(id.clone());
            }
        }
    }
    let replaced_ids = file_ids(&gone_paths)?;
    let watcher_ids = file_ids(&watchers)?;
    let names = tables_with_half(db)?;
    let owned = |table: &str| -> anyhow::Result<bool> {
        let stmt = db.prepare(&format!("SELECT * FROM \"{}\" LIMIT 0", escape(table)))?;
        let has = |c: &str| stmt.column_names().iter().any(|n| *n == c);
        Ok(has("owner_file") && has("id"))
    };
    let marks = |n: usize| vec!["?"; n].join(",");
    let mut vanished: IndexSet<String> = IndexSet::new();
    for table in names.iter().filter(|t| owned(t).unwrap_or(false)) {
        let sql = format!(
            "SELECT id FROM \"{}\" WHERE \"{HALF_COLUMN}\" = ? AND owner_file IN ({})",
            escape(table),
            marks(replaced_ids.len())
        );
        let mut stmt = db.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&half];
        params.extend(replaced_ids.iter().map(|i| i as &dyn rusqlite::ToSql));
        for id in stmt.query_map(params.as_slice(), |r| r.get::<_, Option<String>>(0))?.flatten().flatten() {
            if !kept.contains(&id) {
                vanished.insert(id);
            }
        }
    }
    if vanished.is_empty() {
        return Ok(None);
    }
    let mut found: Option<String> = None;
    let mut count = 0;
    for table in names.iter().filter(|t| owned(t).unwrap_or(false)) {
        let sql = format!(
            "SELECT * FROM \"{}\" WHERE \"{HALF_COLUMN}\" = ? AND owner_file IN ({})",
            escape(table),
            marks(watcher_ids.len())
        );
        let mut stmt = db.prepare(&sql)?;
        let width = stmt.column_count();
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&half];
        params.extend(watcher_ids.iter().map(|i| i as &dyn rusqlite::ToSql));
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            for i in 0..width {
                if let Ok(Some(text)) = row.get::<_, Option<String>>(i)
                    && vanished.contains(&text)
                {
                    count += 1;
                    found.get_or_insert_with(|| format!("{table} -> {text}"));
                }
            }
        }
    }
    Ok(found.map(|f| (count, f)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_kept_row_naming_an_id_the_run_replaces_away_is_found_and_one_kept_is_not() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE files (id TEXT, path TEXT, half TEXT);
             CREATE TABLE classes (id TEXT, file TEXT, owner_file TEXT, half TEXT);
             CREATE TABLE calls (id TEXT, file TEXT, owner_file TEXT, target TEXT, half TEXT);
             INSERT INTO files VALUES ('f:1', 'a.ts', 'typescript'), ('f:2', 'b.ts', 'typescript');
             INSERT INTO classes VALUES ('c:1', 'f:2', 'f:2', 'typescript'), ('c:2', 'f:2', 'f:2', 'typescript');
             INSERT INTO calls VALUES ('x:1', 'f:1', 'f:1', 'c:1', 'typescript');",
        )
        .unwrap();
        let deps = json!({"b.ts": ["a.ts"]});
        // b.ts is read again and keeps c:1: nothing a.ts holds is left pointing at nothing.
        let kept = json!({"classes": [{"id": "c:1", "file": "f:2"}]});
        let found = dangling(&db, "typescript", &["b.ts".to_string()], kept.as_object().unwrap(), deps.as_object().unwrap());
        assert_eq!(found.unwrap(), None);
        // b.ts comes back with c:1 under a new id: a.ts's call would name a row that is gone.
        let renumbered = json!({"classes": [{"id": "c:9", "file": "f:2"}, {"id": "c:2", "file": "f:2"}]});
        let found = dangling(&db, "typescript", &["b.ts".to_string()], renumbered.as_object().unwrap(), deps.as_object().unwrap());
        assert_eq!(found.unwrap(), Some((1, "calls -> c:1".to_string())));
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
