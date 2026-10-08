//! Applying one run's rows to the map database.
//!
//! It runs in the process that already holds the rows: what once crossed a process boundary
//! as a payload file of tens of megabytes arrives here as memory the extractor already owns.

use super::half;
use super::schema::{self, cell};
use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// BUMP THIS whenever a row gains, loses or changes the meaning of a column. It is
/// what stops an incremental update leaving rows of the old shape beside the new.
/// Every half shares one database under this one version, and a version two halves
/// disagreed on would be a database each of them thinks the other built wrong.
pub const SCHEMA_VERSION: &str = "10";

pub struct Applied {
    /// Rows now in each table. Empty unless this batch said it was the last.
    pub written: BTreeMap<String, i64>,
    /// Files that disagree between the database and the tree. Anything but zero means
    /// the cache is lying.
    pub wrong: i64,
    /// Whether FTS5 was available for `file_text`.
    pub fts: bool,
    /// How long each step of this write took, in milliseconds - the TypeScript half reports them under its `write
    /// the rows` (over a minute on a large Angular workspace, several times v1.5.11's). Not spans: the C# half writes on its own thread.
    pub steps: Vec<(&'static str, u64)>,
}

/// Whether this database can be BUILT ON, which is not the same question as whether it
/// holds my rows.
///
/// A half with no rows of its own in a database that is otherwise fine must not
/// rebuild: the rebuild deletes the file, and the file is where the other extractor's
/// rows are.
pub fn usable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(db) = Connection::open(path) else {
        return false;
    };
    matches!(schema_version(&db).as_deref(), Some(SCHEMA_VERSION))
}

fn schema_version(db: &Connection) -> Option<String> {
    db.query_row("SELECT value FROM _meta WHERE key = 'schema'", [], |r| {
        r.get::<_, String>(0)
    })
    .ok()
}

/// `{rel -> sha}` already in the database FOR THIS LANGUAGE, or empty when there is
/// nothing to build on.
///
/// SCOPED BY LANGUAGE, because the other half's files are not this half's to reason
/// about: unscoped, every C# file in the tree came back as a python file that had gone
/// and every row of it was deleted.
pub fn recorded(path: &Path, lang: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if !path.exists() {
        return out;
    }
    let Ok(db) = Connection::open(path) else {
        return out;
    };
    if schema_version(&db).as_deref() != Some(SCHEMA_VERSION) {
        return out;
    }
    for (rel, sha) in files_of(&db, lang) {
        out.insert(rel, sha);
    }
    out
}

/// `(path, sha)` of the files one extractor owns, or of all of them when no language is
/// named.
///
/// A DATABASE WITH NO `files` TABLE IS A LEGITIMATE STATE, not an error: a half whose
/// rows belong to no single file can be the first to write.
fn files_of(db: &Connection, lang: &str) -> Vec<(String, String)> {
    let sql = if lang.is_empty() {
        "SELECT path, sha FROM files".to_string()
    } else {
        "SELECT path, sha FROM files WHERE lang = ?1".to_string()
    };
    let Ok(mut stmt) = db.prepare(&sql) else {
        return Vec::new();
    };
    let mapped = |r: &rusqlite::Row<'_>| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?));
    let rows = if lang.is_empty() {
        stmt.query_map([], mapped)
    } else {
        stmt.query_map([lang], mapped)
    };
    match rows {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// `{prefix -> highest number used}`, so an incremental run hands out ids that cannot
/// collide.
///
/// READ FROM `_meta`, NOT RECOMPUTED. Deriving it meant scanning the id column of every
/// table — every row of every table to learn a handful of numbers.
pub fn counters(path: &Path) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    if !path.exists() {
        return out;
    }
    let Ok(db) = Connection::open(path) else {
        return out;
    };
    let Ok(mut stmt) = db.prepare("SELECT key, value FROM _meta WHERE key LIKE 'counter:%'") else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    }) else {
        return out;
    };
    for row in rows.flatten() {
        if let Some((_, prefix)) = row.0.split_once(':')
            && let Ok(n) = row.1.parse::<i64>()
        {
            out.insert(prefix.to_string(), n);
        }
    }
    out
}

/// How many files disagree between the database and the tree — a recorded sha that has
/// moved, a row for a file that is gone, or a file the tree has and the database does
/// not.
fn verify(db: &Connection, shas: &Map<String, Value>, lang: &str) -> i64 {
    let stored: BTreeMap<String, String> = files_of(db, lang).into_iter().collect();
    let mut wrong = 0i64;
    for (rel, sha) in &stored {
        if shas.get(rel).and_then(|v| v.as_str()) != Some(sha.as_str()) {
            wrong += 1;
        }
    }
    wrong + shas.keys().filter(|rel| !stored.contains_key(*rel)).count() as i64
}

/// Every row belonging to these files, gone — the rows, the file record and the source.
///
/// THIS IS WHAT MAKES INCREMENTAL HONEST. A database that only ever added rows would
/// answer with rows describing files that no longer say that, which is worse than no
/// database because the rows look real.
///
/// SCOPED BY `lang`. A path is not owned by one half: when the plain TypeScript half hands
/// a file over to the Angular one, both have held `src/main.ts`, and a drop by path alone
/// deleted the rows the Angular half had just written for it. The source text is shared by
/// path, so it goes only when no half records the file any more.
fn drop_files(db: &Connection, rels: &BTreeSet<String>, lang: &str) -> Result<()> {
    if rels.is_empty() {
        return Ok(());
    }
    let marks = vec!["?"; rels.len()].join(",");
    let mut params: Vec<&dyn rusqlite::ToSql> =
        rels.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    let owned = if lang.is_empty() {
        String::new()
    } else {
        params.push(&lang as &dyn rusqlite::ToSql);
        " AND lang = ?".to_string()
    };

    // AN ID IS NOT A NUMBER. The extractors hand out `f:1`, `c:7`, `fn:12` — a prefix
    // and a counter, as TEXT — so reading this column as an integer finds nothing, the
    // deletes below run against an empty list, and the old rows of a re-read file stay
    // beside the new ones. The table then holds every version of every row it has ever
    // been given, and only a count gives it away.
    let ids: Vec<rusqlite::types::Value> = {
        let sql = format!("SELECT id FROM files WHERE path IN ({marks}){owned}");
        match db.prepare(&sql) {
            Ok(mut stmt) => stmt
                .query_map(params.as_slice(), |r| r.get::<_, rusqlite::types::Value>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            // No `files` table yet: nothing of this half's is recorded, nothing to drop.
            Err(_) => Vec::new(),
        }
    };

    if !ids.is_empty() {
        let id_marks = vec!["?"; ids.len()].join(",");
        let id_params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        for table in schema::listable_tables(db)? {
            if table == "files" {
                continue;
            }
            let columns = schema::existing_columns(db, &table)?;
            if columns.iter().any(|c| c == "file") {
                db.execute(
                    &format!(
                        "DELETE FROM \"{}\" WHERE file IN ({id_marks})",
                        schema::escape(&table)
                    ),
                    id_params.as_slice(),
                )?;
            }
        }
    }
    let _ = db.execute(&format!("DELETE FROM files WHERE path IN ({marks}){owned}"), params.as_slice());
    let _ = db.execute(
        &format!("DELETE FROM file_text WHERE path IN ({marks}) AND path NOT IN (SELECT path FROM files)"),
        &params[..rels.len()],
    );
    Ok(())
}

/// One table's rows, INSERTED WITHOUT BEING COPIED.
///
/// `stamp` is what this side adds to every row of the table - `lang`, `half` - and it is applied
/// WHILE THE PARAMETERS ARE BOUND rather than by rewriting the rows first. Building a stamped
/// copy was the plainest way to say it and it doubled the biggest table in memory: the full run
/// of a large map peaked at many GB, and one of those copies was this one.
///
/// A stamped column the rows already carry is overwritten, which is what inserting into the row
/// object did. The column list is SORTED (`columns_of`), so appending here changes no schema.
fn insert(db: &Connection, table: &str, rows: &[Value], stamp: &[(String, Value)]) -> Result<()> {
    let mut columns = schema::columns_of(rows);
    if columns.is_empty() {
        return Ok(());
    }
    for (name, _) in stamp {
        if !columns.contains(name) {
            columns.push(name.clone());
        }
    }
    columns.sort_by(|a, b| (a != "id", a).cmp(&(b != "id", b)));
    let mut existing = schema::existing_columns(db, table)?;
    if existing.is_empty() {
        schema::create(db, table, &columns)?;
        existing = columns.clone();
    } else {
        schema::widen(db, table, &mut existing, &columns)?;
    }

    let use_columns: Vec<&String> = columns.iter().filter(|c| existing.contains(c)).collect();
    let marks = vec!["?"; use_columns.len()].join(", ");
    let quoted = use_columns
        .iter()
        .map(|c| format!("\"{}\"", schema::escape(c)))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO \"{}\" ({}) VALUES ({})",
        schema::escape(table),
        quoted,
        marks
    );

    let mut stmt = db.prepare(&sql)?;
    for row in rows {
        let object = row.as_object();
        let values: Vec<_> = use_columns
            .iter()
            .map(|c| {
                let stamped = stamp.iter().find(|(name, _)| name == *c).map(|(_, v)| v);
                cell(stamped.or_else(|| object.and_then(|o| o.get(c.as_str()))))
            })
            .collect();
        stmt.execute(rusqlite::params_from_iter(values))?;
    }
    Ok(())
}

/// The source itself, FTS5-indexed beside the rows.
///
/// THE ROWS POINT AT FILES; THIS CARRIES THEM. It is the one thing the file-level map
/// refuses to do, and the reason this database exists.
/// THE SOURCES TO INDEX - either already in hand, or still on disk and named.
///
/// A large workspace is most of a GB of source, and holding all of it in order to insert one
/// row at a time cost that much of the peak - on a machine that then paged, which made the
/// store several times slower. The TypeScript half names its files and lets this side read each one as it
/// indexes it; the C# half already holds the text it has just parsed, and passes it straight in.
pub enum Texts<'a> {
    Held(&'a BTreeMap<String, String>),
    OnDisk { root: &'a Path, rels: &'a BTreeSet<String> },
}

impl Texts<'_> {
    fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Texts::Held(held) => Box::new(held.keys().map(String::as_str)),
            Texts::OnDisk { rels, .. } => Box::new(rels.iter().map(String::as_str)),
        }
    }

    /// The text of one file, or `None` when it cannot be read.
    ///
    /// A FILE THAT CANNOT BE READ IS SKIPPED, not fatal - the same rule the half that collected
    /// the names already applied. It now also loses whatever text was stored for it, where before
    /// an unreadable file kept the previous run's: no row is better than a row nothing stands
    /// behind, and the parse that named it read it seconds earlier.
    fn read(&self, rel: &str) -> Option<Cow<'_, str>> {
        match self {
            Texts::Held(held) => held.get(rel).map(|t| Cow::Borrowed(t.as_str())),
            Texts::OnDisk { root, .. } => {
                let full = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                let bytes = std::fs::read(full).ok()?;
                Some(Cow::Owned(String::from_utf8_lossy(&bytes).into_owned()))
            }
        }
    }
}

fn search(db: &Connection, texts: &Texts<'_>) -> Result<i64> {
    if db
        .execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS file_text USING fts5(path, content)")
        .is_err()
    {
        return Ok(-1);
    }
    // A TEXT BEING WRITTEN REPLACES THE ONE STORED. `file_text` is an FTS5 table with no key to
    // conflict on, and the half that replaces its rows WHOLE never reaches the per-file drop that
    // would have cleared it - so a partial run inserted a second copy of every file it re-read.
    //
    // `path` CANNOT BE INDEXED, because this is a virtual table: `WHERE path = ?` reads every
    // document in it. One of those per file is quadratic, and it cost most of an hour of a large
    // write - and grew with the square of the file count. So: nothing is cleared when the
    // table is empty, which is every full run, and the paths are cleared in BATCHES when it is
    // not. A handful of scans rather than one per file, and seconds instead of minutes.
    let held: i64 = db
        .query_row("SELECT count(*) FROM file_text", [], |r| r.get(0))
        .unwrap_or(0);
    if held > 0 {
        let paths: Vec<&str> = texts.paths().collect();
        // Under SQLITE_MAX_VARIABLE_NUMBER, with room to spare.
        for chunk in paths.chunks(900) {
            let marks = vec!["?"; chunk.len()].join(",");
            let params: Vec<&dyn rusqlite::ToSql> =
                chunk.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
            db.execute(
                &format!("DELETE FROM file_text WHERE path IN ({marks})"),
                params.as_slice(),
            )?;
        }
    }
    {
        // ONE FILE IS HELD AT A TIME. The read, the insert and the drop are one step, so the
        // whole workspace is never in memory at once.
        let mut stmt = db.prepare("INSERT INTO file_text (path, content) VALUES (?1, ?2)")?;
        let paths: Vec<&str> = texts.paths().collect();
        for rel in paths {
            if let Some(text) = texts.read(rel) {
                stmt.execute((rel, text.as_ref()))?;
            }
        }
    }
    Ok(db.query_row("SELECT count(*) FROM file_text", [], |r| r.get(0))?)
}

pub struct WriteRequest<'a> {
    pub tables: &'a Map<String, Value>,
    pub texts: &'a Texts<'a>,
    pub root: &'a str,
    pub shas: &'a Map<String, Value>,
    pub gone: &'a BTreeSet<String>,
    /// A full run REPLACES the file. The caller decides it, never this side: with a
    /// second extractor writing the same database, a half that rebuilt on its own would
    /// take the other half's rows with it.
    pub full: bool,
    pub counters: &'a Map<String, Value>,
    pub lang: &'a str,
    /// Set when this half REPLACES ITS ROWS WHOLE rather than by file. Every row it
    /// writes is stamped with it and every row already carrying it is deleted first.
    /// See `half.rs` for why the TypeScript half cannot drop by file.
    pub half: Option<&'a str>,
    /// Recorded as `setup:<lang>`. It lives OUTSIDE the mapped tree - compiler versions,
    /// a config file - so no file moves when it changes and only this key says it did.
    pub setup: Option<&'a str>,
    /// MORE `_meta` KEYS THE HALF ITSELF DERIVED — the join spec, the provenance block. They are
    /// merged into the same read-then-rewrite below, because a second writer of that table would
    /// be a second chance to lose another half's keys.
    pub extra: &'a BTreeMap<String, String>,
    /// A PARTIAL RUN HAS ALREADY MADE ITS OWN ROOM, and must not have this one make more. A
    /// whole-replacement half normally drops every row it owns before inserting; a partial one
    /// removed exactly the affected files' rows first and is about to put those back. Letting the
    /// drop run anyway emptied the map: 53 tables of rows deleted to insert the few that moved.
    pub keep_existing: bool,
    /// Only the batch that says it is the last one counts the tables. A count is a scan
    /// of the whole table, and a hundred batches into a database of millions of rows
    /// means counting the same rows a hundred times.
    pub counts: bool,
}

pub fn write(path: &Path, req: &WriteRequest<'_>) -> Result<Applied> {
    if let Some(folder) = path.parent()
        && !folder.as_os_str().is_empty()
    {
        std::fs::create_dir_all(folder)?;
    }
    // A FILE THAT WAS NEVER A DATABASE OF ROWS - no schema stamp - can still hold another half's `skip:` key: a half
    // with nothing to store (a tree with no Angular workspace) records only that. Replaced, it took the key with it,
    // and the first turn after every cold run started node to hear "nothing to do" again. Carried over - only from
    // such a file: a rebuild of a usable database (a schema upgrade) drops every half's rows, so their keys go too.
    let carried = if req.full && path.exists() && !usable(path) { skip_keys(path) } else { BTreeMap::new() };
    if req.full && path.exists() {
        // The `-wal` and `-shm` siblings go with it; leaving them would have SQLite
        // recover a journal belonging to a database that no longer exists.
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    let db = Connection::open(path)
        .with_context(|| format!("cannot open the map database {}", path.display()))?;
    db.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")?;

    let tx = db.unchecked_transaction()?;
    let mut steps: Vec<(&'static str, u64)> = Vec::new();
    let mut step = std::time::Instant::now();
    let mut lap = |name: &'static str, steps: &mut Vec<(&'static str, u64)>| {
        steps.push((name, step.elapsed().as_millis() as u64));
        step = std::time::Instant::now();
    };

    if !req.full && !req.keep_existing {
        match req.half {
            // WHOLE REPLACEMENT. Not "the rows of the files that moved": most of this
            // half's tables have no `file` column to drop by.
            Some(half) => half::drop_half(&db, half)?,
            None => {
                let mut drop: BTreeSet<String> =
                    req.texts.paths().map(str::to_string).collect();
                drop.extend(req.gone.iter().cloned());
                drop_files(&db, &drop, req.lang)?;
            }
        }
    }
    lap("delete the old rows", &mut steps);

    for (table, rows) in req.tables {
        let Some(rows) = rows.as_array() else { continue };
        if rows.is_empty() {
            continue;
        }
        // `lang` and `half` are stamped HERE rather than trusted from the extractor.
        // `lang` is what scopes "already recorded", "gone" and "disagrees"; `half` is
        // what a whole-replacement half deletes by, and a row that forgot it would
        // survive every future run of the half that wrote it.
        let mut stamp: Vec<(String, Value)> = Vec::new();
        if table == "files" {
            stamp.push(("lang".to_string(), Value::String(req.lang.to_string())));
        }
        if let Some(h) = req.half {
            stamp.push((half::HALF_COLUMN.to_string(), Value::String(h.to_string())));
        }
        insert(&db, table, rows, &stamp)?;
    }
    lap("insert", &mut steps);

    let found = search(&db, req.texts)?;
    lap("file text", &mut steps);

    let mut written = BTreeMap::new();
    if req.counts {
        for table in schema::listable_tables(&db)? {
            written.insert(table.clone(), schema::count_of(&db, &table));
        }
        written.insert("file_text".to_string(), found);
    }
    lap("count the tables", &mut steps);
    let wrong = if req.counts { verify(&db, req.shas, req.lang) } else { 0 };
    lap("verify", &mut steps);

    write_meta(&db, req, found, &written, &carried)?;
    tx.commit()?;
    lap("commit", &mut steps);
    Ok(Applied { written, wrong, fts: found >= 0, steps })
}

/// The `skip:<lang>` keys a file holds - see `write`.
fn skip_keys(path: &Path) -> BTreeMap<String, String> {
    let Ok(db) = Connection::open(path) else { return BTreeMap::new() };
    let Ok(mut stmt) = db.prepare("SELECT key, value FROM _meta WHERE key LIKE 'skip:%'") else { return BTreeMap::new() };
    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// `_meta`, READ THEN REWRITTEN.
///
/// The other extractor's keys are in here too, and a `DELETE FROM _meta` followed by
/// this half's own rows would leave the database claiming it holds only C# — or only
/// python — whichever half happened to write last.
fn write_meta(
    db: &Connection,
    req: &WriteRequest<'_>,
    found: i64,
    written: &BTreeMap<String, i64>,
    carried: &BTreeMap<String, String>,
) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS _meta (key TEXT, value TEXT)")?;

    let mut meta: BTreeMap<String, String> = BTreeMap::new();
    {
        let mut stmt = db.prepare("SELECT key, value FROM _meta")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows.flatten() {
            meta.insert(row.0, row.1);
        }
    }

    let root = std::path::absolute(req.root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| req.root.to_string());
    for (key, value) in req.extra.iter().chain(carried) {
        meta.insert(key.clone(), value.clone());
    }
    meta.insert("schema".into(), SCHEMA_VERSION.into());
    meta.insert("root".into(), root);
    meta.insert(format!("files:{}", req.lang), req.shas.len().to_string());
    meta.insert(
        "fts5".into(),
        if found >= 0 { "yes".into() } else { "no - this build has no FTS5, so file_text is absent".to_string() },
    );
    if req.counts {
        meta.insert("files".into(), schema::count_of(db, "files").to_string());
        meta.insert(
            "rows".into(),
            written.values().filter(|n| **n > 0).sum::<i64>().to_string(),
        );
    }
    if let Some(setup) = req.setup {
        meta.insert(format!("setup:{}", req.lang), setup.to_string());
    }
    for (prefix, value) in req.counters {
        let n = value.as_i64().unwrap_or(0);
        meta.insert(format!("counter:{prefix}"), n.to_string());
    }

    db.execute_batch("DELETE FROM _meta")?;
    {
        let mut stmt = db.prepare("INSERT INTO _meta (key, value) VALUES (?1, ?2)")?;
        for (key, value) in &meta {
            stmt.execute((key, value))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
