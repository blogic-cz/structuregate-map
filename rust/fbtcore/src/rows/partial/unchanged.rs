//! A PARTIAL RUN KEEPS THE CLOSURE IT STORED WHEN NOTHING THE CLOSURE READ HAS MOVED.
//!
//! The closure (`render_path`, `key_reach`, `gate_values`, `gate_features`) is derived from every row of the half,
//! so a partial run derived it whole - most of the run, for an edit that changed nothing it reads: a comment, a
//! blank line. What it reads is MEASURED, not listed: every column a pass asked a row of (`Store::reads`), recorded
//! beside the closure in `_closure_reads` by the run that derived it. The closure is a function of those cells, the
//! source text the dead-key trace scans, and what the half was configured with - so when all three are what they
//! were, the stored closure is the one a derivation would produce, and it stays.
//!
//! COMPARED AS STORED, before against after, never against the payload: the rows a partial run REMOVES (the affected
//! files', the tables node rebuilt whole, and the stylesheet pass's own) are hashed table by table in rowid order
//! before they go, and the rows it WRITES in their place after - every read column, a structure as its node rows,
//! every id with it. One cell apart and the closure is derived. So is everything this cannot see: a table read whole
//! (another half's rows with it), no record from the last derivation, a different configuration. A run that fails
//! between the two leaves no record (`forget` runs first), so the next one derives.

use super::cells;
use super::half::HALF_COLUMN;
use super::schema;
use super::ts::sourcetrace;
use super::ts::store::Reads;
use anyhow::Result;
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, ToSql};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// `(tbl, col)`: a column the last derivation read, `col` NULL for a table it asked for at all.
const READS: &str = "_closure_reads";

fn key(half: &str) -> String {
    format!("closure:{half}")
}

/// WHAT THE CLOSURE IS A FUNCTION OF BESIDES ROWS AND TEXT, hashed: the setup (compilers, config, the extractor's
/// and this build's version), the feature API it was told and the options the extraction was configured with.
pub fn fingerprint(parts: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// ...and the spec's word on how a cell it read is decoded: which of those columns hold JSON text and which a
/// boolean (`Store::read`). Only the columns read: node publishes the spec over the rows it HELD, and a partial run
/// holds a different set than a full one, so the whole spec differs between them where no pass looks.
fn decoded_as(base: &str, spec: Option<&serde_json::Value>, reads: &BTreeMap<String, BTreeSet<String>>) -> String {
    let mut parts: Vec<String> = vec![base.to_string()];
    for key in ["json_columns", "bool_columns"] {
        for (table, columns) in reads {
            let listed: BTreeSet<&str> = spec
                .and_then(|s| s.get(key))
                .and_then(|k| k.get(table))
                .and_then(|l| l.as_array())
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_str())
                .filter(|c| columns.contains(*c))
                .collect();
            parts.push(format!("{key}\u{0}{table}\u{0}{}", listed.into_iter().collect::<Vec<_>>().join("\u{0}")));
        }
    }
    fingerprint(&parts.iter().map(String::as_str).collect::<Vec<_>>())
}

/// No record any more: called before anything is written, so a run that stops half way never vouches for a closure.
pub fn forget(db: &Connection, half: &str) -> Result<()> {
    if cells::meta::exists(db) {
        cells::meta::delete(db, &key(half))?;
    }
    Ok(())
}

/// What a derivation read, and what it was derived under - written after its closure is stored. A table read whole
/// can never be compared by its own half's rows, so a closure that read one is not recorded at all.
pub fn record(db: &Connection, half: &str, reads: &Reads, fingerprint: &str, spec: Option<&serde_json::Value>) -> Result<()> {
    // ONE COMMIT: a row each, synced one at a time, was seconds of a partial run.
    db.execute_batch("PRAGMA synchronous = OFF")?;
    let tx = db.unchecked_transaction()?;
    forget(db, half)?;
    db.execute_batch(&format!("DROP TABLE IF EXISTS {READS}; CREATE TABLE {READS} (tbl TEXT NOT NULL, col TEXT)"))?;
    if reads.unscoped.is_empty() {
        let mut stmt = db.prepare(&format!("INSERT INTO {READS} (tbl, col) VALUES (?1, ?2)"))?;
        for (table, columns) in &reads.tables {
            stmt.execute((table, None::<String>))?;
            for column in columns {
                stmt.execute((table, Some(column)))?;
            }
        }
        cells::meta::ensure(db)?;
        cells::meta::put(db, &key(half), &decoded_as(fingerprint, spec, &reads.tables))?;
    }
    tx.commit()?;
    Ok(())
}

/// The columns the last derivation read - whatever it was derived under; `Before::take` asks that.
fn stored(db: &Connection) -> Option<BTreeMap<String, BTreeSet<String>>> {
    let mut stmt = db.prepare(&format!("SELECT tbl, col FROM {READS}")).ok()?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))).ok()?;
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (table, column) = row.ok()?;
        let columns = out.entry(table).or_default();
        if let Some(column) = column {
            columns.insert(column);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Which rows of a table a part of the run removes, or writes.
pub struct Scope<'a> {
    /// Every row of the half.
    pub all: bool,
    /// ...or those of these files (`owner_file`), these paths (`files.path`), this `kind`, or above this rowid.
    pub owners: &'a [String],
    pub paths: &'a [String],
    pub kind: Option<&'a str>,
    pub above: Option<i64>,
}

fn encode(hasher: &mut blake3::Hasher, value: &Sql) {
    match value {
        Sql::Null => {
            hasher.update(b"N");
        }
        Sql::Integer(i) => {
            hasher.update(b"I");
            hasher.update(&i.to_le_bytes());
        }
        Sql::Real(f) => {
            hasher.update(b"F");
            hasher.update(&f.to_bits().to_le_bytes());
        }
        Sql::Text(t) => {
            hasher.update(b"T");
            hasher.update(&(t.len() as u64).to_le_bytes());
            hasher.update(t.as_bytes());
        }
        Sql::Blob(b) => {
            hasher.update(b"B");
            hasher.update(&(b.len() as u64).to_le_bytes());
            hasher.update(b);
        }
    }
}

/// THE ROWS OF `scope`, IN ROWID ORDER, AS THE CLOSURE READS THEM: each of `columns` the table holds, and each
/// that is a structure as the node rows it is rebuilt from. A table that is not there hashes as one with no rows.
pub fn digest(db: &Connection, table: &str, columns: &BTreeSet<String>, half: &str, scope: &Scope<'_>) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let existing = schema::existing_columns(db, table)?;
    let has = |c: &str| existing.iter().any(|e| e == c);
    // NO `half` COLUMN, NO ROW OF THE HALF: another half made the table and this one never wrote to it - asked
    // for `t.half`, the run failed (a C# `classes` beside an Angular workspace).
    if !has(HALF_COLUMN) {
        return Ok(hasher.finalize().to_hex().to_string());
    }
    let plain: Vec<&String> = columns.iter().filter(|c| has(c)).collect();
    let nodes: Vec<(String, String)> = cells::columns_of(db, table).into_iter().filter(|(c, _)| columns.contains(c)).collect();

    let mut wheres: Vec<String> = Vec::new();
    let mut params: Vec<&dyn ToSql> = vec![&half];
    if !scope.all {
        let mut any: Vec<String> = Vec::new();
        if !scope.owners.is_empty() && has("owner_file") {
            any.push(format!("t.owner_file IN ({})", vec!["?"; scope.owners.len()].join(",")));
            params.extend(scope.owners.iter().map(|o| o as &dyn ToSql));
        }
        if !scope.paths.is_empty() && table == "files" {
            any.push(format!("t.path IN ({})", vec!["?"; scope.paths.len()].join(",")));
            params.extend(scope.paths.iter().map(|p| p as &dyn ToSql));
        }
        if let Some(kind) = &scope.kind && has("kind") {
            any.push("t.kind = ?".to_string());
            params.push(kind);
        }
        if let Some(above) = &scope.above {
            any.push("t.rowid > ?".to_string());
            params.push(above);
        }
        wheres.push(if any.is_empty() { "0".to_string() } else { format!("({})", any.join(" OR ")) });
    }
    let mut select: Vec<String> = plain.iter().map(|c| format!("t.\"{}\"", schema::escape(c))).collect();
    select.push(cells::owner_expr(db, table, "t"));
    let sql = format!(
        "SELECT {} FROM \"{}\" t WHERE t.\"{HALF_COLUMN}\" = ?{} ORDER BY t.rowid",
        select.join(", "),
        schema::escape(table),
        wheres.iter().map(|w| format!(" AND {w}")).collect::<String>()
    );
    let mut stmt = db.prepare(&sql)?;
    // EVERY READ COLUMN, WHETHER THE TABLE HOLDS IT YET OR NOT: a column no row has filled reads as NULL, and a
    // write that adds one (a batch whose rows all left it empty) has changed nothing a pass sees. Each column is its
    // cell, then its structure's node rows (none where it has no node table) - so the two sides hash alike.
    let mut readers: Vec<Option<rusqlite::Statement<'_>>> = columns
        .iter()
        .map(|c| match nodes.iter().find(|(n, _)| n == c) {
            Some((_, child)) => db
                .prepare(&format!("SELECT n, parent, role, ord, node, value FROM \"{}\" WHERE owner = ?1 ORDER BY n", schema::escape(child)))
                .map(Some),
            None => Ok(None),
        })
        .collect::<rusqlite::Result<_>>()?;
    let mut rows = stmt.query(params.as_slice())?;
    while let Some(row) = rows.next()? {
        hasher.update(b"R");
        let owner = match row.get::<_, Sql>(plain.len())? {
            Sql::Text(t) => cells::owner_value(&t),
            other => other,
        };
        for (column, reader) in columns.iter().zip(readers.iter_mut()) {
            let cell = match plain.iter().position(|p| *p == column) {
                Some(i) => row.get::<_, Sql>(i)?,
                None => Sql::Null,
            };
            encode(&mut hasher, &cell);
            hasher.update(b"C");
            let Some(reader) = reader else { continue };
            let mut nodes = reader.query([&owner])?;
            while let Some(node) = nodes.next()? {
                hasher.update(b"n");
                for i in 0..6 {
                    encode(&mut hasher, &node.get::<_, Sql>(i)?);
                }
            }
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// THE TRACE'S VIEW OF ONE FILE'S TEXT: the dotted names and words its live lines spell, as sets - the trace keeps
/// the first file a name appears in, and a file's own order says nothing about that.
fn traced(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let scanned = sourcetrace::scan(text);
    (scanned.dotted.into_iter().collect(), scanned.words.into_iter().collect())
}

/// What the run is about to remove, hashed - taken BEFORE anything goes.
pub struct Before {
    /// What the stored closure was derived under, which this run would derive under too.
    vouched: String,
    reads: BTreeMap<String, BTreeSet<String>>,
    old: BTreeMap<String, String>,
    /// path -> the trace's view of the text `file_text` held for it (None: it held none).
    texts: BTreeMap<String, Option<(BTreeSet<String>, BTreeSet<String>)>>,
}

/// Which part of the run rewrites a table, and so how its old rows are found and its new ones told apart.
pub struct Rewrites<'a> {
    pub ids: &'a [String],
    pub paths: &'a [String],
    /// Rebuilt whole by node.
    pub whole: &'a BTreeSet<String>,
    /// Rebuilt whole after the write (the stylesheet pass), and the `diagnostics` kind it rewrites.
    pub derived: &'a [&'a str],
    pub derived_kind: &'a str,
}

impl Rewrites<'_> {
    fn removed(&self, table: &str) -> Scope<'_> {
        let all = self.whole.contains(table) || self.derived.contains(&table);
        let kind = (table == "diagnostics").then_some(self.derived_kind);
        Scope { all, owners: self.ids, paths: self.paths, kind, above: None }
    }

    fn written(&self, table: &str, mark: i64) -> Scope<'_> {
        let all = self.derived.contains(&table);
        let kind = (table == "diagnostics").then_some(self.derived_kind);
        Scope { all, owners: &[], paths: &[], kind, above: Some(mark) }
    }
}

impl Before {
    /// None when there is nothing to compare against: no record, or one made under another fingerprint.
    pub fn take(db: &Connection, half: &str, fingerprint: &str, spec: Option<&serde_json::Value>, rewrites: &Rewrites<'_>) -> Result<Option<Before>> {
        let Some(reads) = stored(db) else { return Ok(None) };
        let vouched = decoded_as(fingerprint, spec, &reads);
        if cells::meta::get(db, &key(half)).as_deref() != Some(vouched.as_str()) {
            return Ok(None);
        }
        let mut old = BTreeMap::new();
        for (table, columns) in &reads {
            old.insert(table.clone(), digest(db, table, columns, half, &rewrites.removed(table))?);
        }
        let mut texts: BTreeMap<String, Option<(BTreeSet<String>, BTreeSet<String>)>> =
            rewrites.paths.iter().map(|p| (p.clone(), None)).collect();
        // ONE SCAN OF `file_text` for every path: it is a virtual table, and `path = ?` reads every document in it.
        if !rewrites.paths.is_empty() && schema::existing_columns(db, "file_text").map(|c| !c.is_empty()).unwrap_or(false) {
            let marks = vec!["?"; rewrites.paths.len()].join(",");
            let mut stmt = db.prepare(&format!("SELECT path, content FROM file_text WHERE path IN ({marks})"))?;
            let params: Vec<&dyn ToSql> = rewrites.paths.iter().map(|p| p as &dyn ToSql).collect();
            let mut rows = stmt.query(params.as_slice())?;
            while let Some(row) = rows.next()? {
                let (path, content): (String, String) = (row.get(0)?, row.get(1)?);
                // TWO COPIES OF ONE PATH say nothing certain about either: compared as none at all.
                let fresh = texts.get(&path).is_some_and(Option::is_none);
                texts.insert(path, if fresh { Some(traced(&content)) } else { None });
            }
        }
        Ok(Some(Before { vouched, reads, old, texts }))
    }

    /// Kept as it was: the closure stays, and so does the record of what it read.
    pub fn vouch(&self, db: &Connection, half: &str) -> Result<()> {
        cells::meta::put(db, &key(half), &self.vouched)
    }

    /// The highest rowid of each table read, AFTER the removal: every row written later lands above it.
    pub fn marks(&self, db: &Connection) -> BTreeMap<String, i64> {
        self.reads
            .keys()
            .map(|t| {
                let top = db
                    .query_row(&format!("SELECT coalesce(max(rowid), 0) FROM \"{}\"", schema::escape(t)), [], |r| r.get::<_, i64>(0))
                    .unwrap_or(0);
                (t.clone(), top)
            })
            .collect()
    }

    /// Why the stored closure no longer holds, or None when it does - after every row is written and the stylesheets
    /// are read. `workspace` is where the trace reads the text the rows were made from.
    pub fn moved(&self, db: &Connection, half: &str, marks: &BTreeMap<String, i64>, rewrites: &Rewrites<'_>, workspace: &Path) -> Result<Option<String>> {
        for (table, columns) in &self.reads {
            let mark = marks.get(table).copied().unwrap_or(0);
            let new = digest(db, table, columns, half, &rewrites.written(table, mark))?;
            if self.old.get(table) != Some(&new) {
                return Ok(Some(format!("`{table}` moved")));
            }
        }
        for (path, old) in &self.texts {
            let Some(old) = old else { return Ok(Some(format!("no text was held for {path}"))) };
            let full = workspace.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let Ok(bytes) = std::fs::read(&full) else { return Ok(Some(format!("{path} could not be read"))) };
            if traced(&String::from_utf8_lossy(&bytes)) != *old {
                return Ok(Some(format!("the text of {path} moved")));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "unchanged_tests.rs"]
mod tests;
