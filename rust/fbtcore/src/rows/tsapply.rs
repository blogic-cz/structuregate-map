//! THE STORING END OF THE DEEP TYPESCRIPT MAP.
//!
//! The rows are parsed by node, because `@angular/compiler` and `typescript` can only be borrowed
//! from the workspace being mapped. They are stored here, because the SQLite that writes this
//! database is linked into the exe and a consumer still receives two files.
//!
//! THE ROWS OF THIS HALF ARE REPLACED WHOLE, and that is a deliberate difference from the C#
//! half's incremental path. A file's rows cannot be dropped by a `file` column, because more than
//! half of the tables this half writes have none — a binding hangs off a template node, a call off
//! a member — so dropping "the rows of the files that moved" would leave the rest of a re-parsed
//! file's rows behind, joined to nothing. Every row carries `half` instead, and every run deletes
//! exactly those. A PARTIAL run is the exception, and it is `owner_file` that makes it possible.
//!
//! THE RENDER CLOSURE RUNS ON THIS SIDE, not in node. It is derived from EVERY other table, so it needs
//! every row — which node only has while it still holds the whole store. Handing them back for a
//! partial run would cost about as much JSON as a `JSON.parse` can hold at all, so the
//! pass lives on the side that can read the database instead, after the rows are written: what it and the
//! stylesheets derive is `tsderive.rs`, which can derive it again from the stored rows alone.
//!
//! THIS FILE SHAPES THE STORED ROWS, and is listed in `build.rs`: a change here reads every consumer's tree again.

use super::deps;
use super::digest::Digest;
use super::half::drop_half_tables;
use super::pyjson;
use super::schema;
use super::store::{self, WriteRequest};
use super::tsderive::{self, CLOSURE_TABLES};
use super::unchanged;
use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What the `files` rows of this half are stamped with, so the other halves' files stay out of
/// every question this one asks about "what is already here".
pub const LANG: &str = "typescript";

/// THE COLUMNS A READER SELECTS BY that no join names: each closure table by what it is about (a key's reach and
/// route, a component's paths, a gate's values and features, #72), and the plain filters a consumer narrows a large
/// table with (#73) - a template's root nodes, one syntactic position, one locale, one declaration, one method.
pub(super) const LOOKUP_COLUMNS: &[(&str, &str)] = &[
    ("key_reach", "key"), ("key_reach", "route"), ("render_path", "component"), ("gate_values", "gate"),
    ("gate_values", "enum_name"), ("gate_features", "gate"), ("template_nodes", "depth"), ("string_literals", "context"),
    ("translations", "locale"), ("comments", "context"), ("calls", "method"), ("control_options", "key"), ("control_options", "node"), ("key_builds", "key"), ("key_builds", "literal"), ("key_node_ways", "key"), ("key_node_ways", "node"),
];

/// WHICH FILES ARE CARRIED AS TEXT. The same set node carries, stated in both places because
/// neither can see the other's.
const TEXT_EXTS: &[&str] = &[
    "ts", "html", "scss", "js", "json", "css", "md", "yaml", "xml", "properties", "config", "sh",
];

/// The payload node writes.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TsPayload {
    /// Whether this run sends every file of the tree.
    pub all: bool,
    /// ...or a named set of them, with the rest handed back.
    pub partial: bool,
    pub affected: Vec<String>,
    /// The tables this run rebuilt ENTIRELY, so their old rows go rather than the affected
    /// files'. node measures it; see its `Store.rollupTables`.
    pub whole: Vec<String>,
    /// Whether this partial run stopped short of every hop - then a kept row's structures are asked too
    /// (`dangling`, `nested`), as node once asked every row it held.
    pub cut: bool,
    pub shas: Map<String, Value>,
    pub counters: Map<String, Value>,
    /// Rows HELD, for a fixture. A run never parses them into here: they are streamed past twice (`apply_streamed`),
    /// so the field is skipped when a payload is read.
    #[serde(skip_deserializing)]
    pub tables: Map<String, Value>,
    /// The feature API the closure needs, which only node has read off the config.
    pub declared: Declared,
    /// The join spec and the id scheme, published under this half's own `_meta` key.
    pub spec: Option<Value>,
    /// What this extraction WAS, for a consumer's provenance block.
    pub provenance: Map<String, Value>,
    pub setup: Option<String>,
    /// The workspace folder node mapped, relative to the root - what the Angular half's skip is keyed by.
    pub fe: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Declared {
    pub checks: Vec<String>,
    #[serde(rename = "enum")]
    pub enum_name: String,
}

/// What the run did, rendered by the caller into the lines every half speaks.
#[derive(Debug, Default, Serialize)]
pub struct TsReceipt {
    pub written: BTreeMap<String, i64>,
    /// Read back OUT OF THE DATABASE, never counted off what was sent.
    pub stored: usize,
    pub partial: bool,
    pub replaced: usize,
    pub path_rows: usize,
    pub components: usize,
    pub keys: usize,
    pub gate_features: usize,
    pub gate_values: usize,
    /// HOW LONG EACH PART TOOK, in milliseconds.
    ///
    /// Here rather than in a log because this half is one call from a build: nobody watches it,
    /// and "the map took minutes" is not a finding anybody can act on. The parse is timed by
    /// the caller, which is the only side that knows when the payload started being read.
    pub phases: BTreeMap<String, u64>,
    /// The steps of the first `write the rows`, in order: what that phase is made of.
    pub write_steps: Vec<(String, u64)>,
    pub notes: Vec<String>,
    /// The payload's `fe`, handed back to the caller that keys the next run's skip by it.
    pub fe: Option<String>,
}

/// `{path -> the file's source}`, read HERE and not carried in the payload.
///
/// node already holds every row; adding tens of MB of source text to a payload it writes and this reads
/// would double the cost of the hand-off to move bytes this side can take straight off the disk. A
/// file that cannot be read contributes NOTHING rather than an empty string: "" is a real answer
/// for an empty file and must not also mean "unreadable".
/// WHICH FILES ARE INDEXED - their paths only.
///
/// The text itself is read by the store, one file at a time, as it indexes each: see `Texts`.
/// Holding the whole workspace here to hand it over cost most of a GB of the peak.
fn text_paths_of(root: &str, files: Option<&Value>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(Value::Array(rows)) = files else { return out };
    for row in rows.iter().filter_map(|r| r.as_object()) {
        let Some(Value::String(rel)) = row.get("path") else { continue };
        let ext = match row.get("ext") {
            Some(Value::String(e)) => e.trim_start_matches('.').to_lowercase(),
            _ => String::new(),
        };
        if rel.is_empty() || !TEXT_EXTS.contains(&ext.as_str()) {
            continue;
        }
        // A FILE OUTSIDE THE WORKSPACE IS NOT THIS APPLICATION'S SOURCE. The compiler's own library
        // is a program file, and therefore a `files` row, whose path escapes the root through the
        // borrowed node_modules. Carrying it would put the TypeScript standard library in here.
        if rel.starts_with("..") {
            continue;
        }
        out.insert(rel.clone());
    }
    let _ = root;
    out
}

/// The same value with every object's keys in order.
///
/// `json.dumps(..., sort_keys=True)` is what wrote the `spec` cell, and this crate keeps a JSON
/// object in the order it was built. A cell that holds the same facts in a different order is a
/// cell every consumer's diff reports as changed.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<&String, &Value> = map.iter().collect();
            Value::Object(ordered.into_iter().map(|(k, v)| (k.clone(), sorted(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// The `files` ids of the affected paths - what their rows' `owner_file` holds.
fn affected_ids(db: &Connection, affected: &[String]) -> Result<Vec<String>> {
    if affected.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; affected.len()].join(",");
    let mut stmt = db.prepare(&format!("SELECT id FROM files WHERE half = ? AND path IN ({marks})"))?;
    let lang = LANG;
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&lang];
    params.extend(affected.iter().map(|p| p as &dyn rusqlite::ToSql));
    Ok(stmt.query_map(params.as_slice(), |r| r.get::<_, String>(0))?.flatten().collect())
}

/// THE FOLDER A `files.path` OF THIS HALF IS RELATIVE TO: the workspace node mapped (`fe`), not the root.
///
/// The two are one folder only when the workspace sits AT the root. On a consumer it is often a folder
/// below it, and every source read off `root/src/app/...` missed: `file_text` held no `.ts` or `.html`
/// at all, and the dead-key trace scanned an empty workspace. A read that misses is skipped by design,
/// so nothing said so.
pub(super) fn workspace_of(root: &str, fe: Option<&str>) -> PathBuf {
    match fe {
        Some(fe) if !fe.is_empty() && fe != "." => Path::new(root).join(fe.replace('/', std::path::MAIN_SEPARATOR_STR)),
        _ => PathBuf::from(root),
    }
}

/// THE PAYLOAD IS TAKEN BY VALUE, because storing it is the last thing anybody does with it.
/// A full run's `tables` is some gigabytes of parsed rows, and the full path below MOVES them
/// into the store rather than cloning them; a caller that still needed its payload afterwards
/// would have forced the copy back.
/// Rows HELD - a fixture's door; a run streams them (`apply_streamed`).
#[cfg(test)]
pub fn apply(db_path: &Path, root: &str, payload: TsPayload) -> Result<TsReceipt> {
    let rebuilt = rebuilt_of(&payload);
    let digest = Digest::over(&payload.tables, &rebuilt);
    apply_with(db_path, root, "", payload, digest, None)
}

/// A payload STREAMED: `head` is the payload without its rows, `bytes` the whole file (mapped). The rows are walked
/// twice and never held - once for what they say about each other (`Digest`), once while they are written, in
/// batches - so the peak is a batch and the digest, not every row of the workspace.
/// `build`: the running build, recorded as the one that derived the closure (`tsderive`).
pub fn apply_streamed(db_path: &Path, root: &str, build: &str, head: TsPayload, bytes: &[u8]) -> Result<TsReceipt> {
    // A RUN THAT STOPPED SHORT checks the rows of every whole table it sends for an id that is gone (`dangling`).
    let naming: &[String] = if head.partial && head.cut { &head.whole } else { &[] };
    let mut digest = Digest::new(&rebuilt_of(&head)).naming(naming);
    super::rawcells::each_row(bytes, &mut |table, row| {
        digest.first(table, &row);
        Ok(())
    })?;
    apply_with(db_path, root, build, head, digest, Some(bytes))
}

/// The tables a rollup rebuilds whole, as the payload's spec names them.
fn rebuilt_of(payload: &TsPayload) -> Vec<String> {
    payload.spec.as_ref().map(|s| deps::list(s.get("rebuilt"))).unwrap_or_default()
}

/// How many rows of one table go into the store at a time when they are streamed.
const BATCH: usize = 5_000;

fn apply_with(db_path: &Path, root: &str, build: &str, mut payload: TsPayload, digest: Digest, bytes: Option<&[u8]>) -> Result<TsReceipt> {
    if !payload.all && !payload.partial {
        bail!("a payload must say whether it is the whole half or a named set of files");
    }
    // A TEST'S DOOR, NOTHING ELSE: a table named here is deleted WHOLE by a partial run while node sends only the rows it
    // made - a table wrongly rebuilt, the shape of the v2.6.0 defect - so a black-box case proves what `dangling` does
    // with the ids that vanish under kept rows. No defect of that shape is left to reproduce it by.
    if payload.partial
        && let Ok(names) = std::env::var("STRUCTUREGATE_TEST_WHOLE")
    {
        payload.whole.extend(names.split(',').filter(|n| !n.is_empty()).map(String::from));
    }
    let mut receipt = TsReceipt { partial: payload.partial, fe: payload.fe.take(), ..Default::default() };
    let workspace = workspace_of(root, receipt.fe.as_deref());

    // A FULL RUN REPLACES THE FILE, and it is decided by the DATABASE: with three extractors
    // writing here, a half that rebuilt on its own row count would take the other two halves' rows
    // with it.
    let full = !store::usable(db_path) && !payload.partial;

    // THE MANIFEST, stored beside the rows it describes. SQLite records none of it, so the half
    // that derived it publishes it — under one key of its own, because the other halves have their
    // own answers and neither may overwrite the other.
    let mut extra: BTreeMap<String, String> = BTreeMap::new();
    if let Some(spec) = &payload.spec {
        extra.insert(format!("spec:{LANG}"), pyjson::dumps(&sorted(spec)));
    }
    for (key, value) in &payload.provenance {
        let text = match value {
            Value::String(s) => s.clone(),
            other => pyjson::dumps(&sorted(other)),
        };
        extra.insert(format!("{key}:{LANG}"), text);
    }
    let base = tsderive::base(payload.setup.as_deref(), &payload.declared, extra.get(&format!("options:{LANG}")).map(String::as_str), &workspace);
    let fingerprint = tsderive::fingerprint(&base, build);
    // WHAT A PARTIAL RUN REMOVES AND WRITES, for the comparison that may keep the closure (`unchanged`).
    let mut ids: Vec<String> = Vec::new();
    let node_whole: BTreeSet<String> = payload.whole.iter().cloned().collect();
    let mut kept: Option<(unchanged::Before, BTreeMap<String, i64>)> = None;
    let mut http_before: Option<super::http::inputs::Before> = None;
    if payload.partial {
        let db = Connection::open(db_path)
            .with_context(|| format!("the database could not be opened: {}", db_path.display()))?;
        ids = affected_ids(&db, &payload.affected)?;
        // NOTHING KEPT MAY POINT AT A ROW THIS RUN REPLACES - see `dangling`. Checked before anything is deleted, while
        // the rows about to go still say which ids they held, so a run that would break a join leaves the map as it was.
        let check_started = std::time::Instant::now();
        let joins: Vec<String> = payload.spec.as_ref().and_then(|s| s.get("joins")).and_then(Value::as_object).map(|j| j.keys().cloned().collect()).unwrap_or_default();
        let plan = super::dangling::Replace { half: LANG, replaced: &payload.affected, whole: &payload.whole, kept: &digest.ids, named: &digest.named, joins: &joins };
        super::dangling::refuse(&db, &plan, payload.cut)?;
        receipt.phases.insert("check the kept rows".to_string(), check_started.elapsed().as_millis() as u64);
        let compare_started = std::time::Instant::now();
        let before = unchanged::Before::take(&db, LANG, &fingerprint, payload.spec.as_ref(), &rewrites(&ids, &payload.affected, &node_whole))?;
        http_before = Some(super::http::inputs::Before::take(&db, LANG, &rewrites(&ids, &payload.affected, &node_whole))?);
        receipt.phases.insert("compare the closure's reads".to_string(), compare_started.elapsed().as_millis() as u64);
        // ONE TRANSACTION, UNJOURNALLED as the store writes: each delete was a commit of its own, synced.
        db.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; BEGIN")?;
        // THE RECORD GOES BEFORE ANY ROW DOES: a run that stops between here and its closure vouches for nothing.
        unchanged::forget(&db, LANG)?;
        tsderive::forget(&db)?;
        super::http::inputs::forget(&db)?;
        // THE CLOSURE'S OWN TABLES ARE NOT REPLACED HERE: they stay until the closure is known to have moved.
        let replace_started = std::time::Instant::now();
        receipt.replaced = super::dangling::replace_affected(&db, LANG, &payload.affected, &ids, &payload.whole)?;
        receipt.phases.insert("replace the affected rows".to_string(), replace_started.elapsed().as_millis() as u64);
        http_before.iter_mut().for_each(|h| h.mark(&db));
        kept = before.map(|b| {
            let marks = b.marks(&db);
            (b, marks)
        });
        db.execute_batch("COMMIT")?;
        receipt.notes.push(format!(
            "the typescript half is replacing {} file(s), not all {}",
            payload.affected.len(),
            payload.shas.len()
        ));
    } else if !full && db_path.exists() {
        // ONE TRANSACTION, UNJOURNALLED as the store writes: a statement per table, each its own journalled commit,
        // over a database of gigabytes - and in no phase, so its time was the store's own `(untraced)`.
        let dropping = std::time::Instant::now();
        let db = Connection::open(db_path)?;
        db.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; BEGIN")?;
        unchanged::forget(&db, LANG)?;
        tsderive::forget(&db)?;
        super::http::inputs::forget(&db)?;
        drop_half_tables(&db, LANG)?;
        db.execute_batch("COMMIT")?;
        receipt.phases.insert("drop the old rows".to_string(), dropping.elapsed().as_millis() as u64);
    }

    let mut named = text_paths_of(root, Some(&Value::Array(digest.files.clone())));
    // A PARTIAL RUN'S TEXTS ARE ITS FILES': `files` comes back whole, and every other text was deleted and written again.
    if payload.partial && Connection::open(db_path)?.prepare("SELECT 1 FROM file_text LIMIT 1").and_then(|mut s| s.exists([])).unwrap_or(false) {
        named.retain(|p| payload.affected.contains(p));
    }
    // THE SECOND WALK, run by the store as it writes: each row past the digest, then into a batch of its table.
    let digest = std::cell::RefCell::new(digest);
    let feed = |sink: &mut dyn FnMut(&str, &[Value]) -> Result<()>| -> Result<()> {
        let Some(bytes) = bytes else { return Ok(()) };
        let mut batch: Vec<Value> = Vec::with_capacity(BATCH);
        let mut current = String::new();
        super::rawcells::each_row(bytes, &mut |table, row| {
            digest.borrow_mut().second(table, &row);
            if table != current {
                if !batch.is_empty() {
                    sink(&current, &batch)?;
                    batch.clear();
                }
                current = table.to_string();
            }
            batch.push(Value::Object(row));
            if batch.len() >= BATCH {
                sink(&current, &batch)?;
                batch.clear();
            }
            Ok(())
        })?;
        if !batch.is_empty() {
            sink(&current, &batch)?;
        }
        Ok(())
    };
    let stream: Option<&store::Stream<'_>> = bytes.map(|_| &feed as &store::Stream<'_>);
    let texts = store::Texts::OnDisk { root: &workspace, rels: &named };
    let gone: BTreeSet<String> = BTreeSet::new();

    let applied = if payload.partial {
        // THE ROWS GO IN FIRST, and only then is the closure asked — it is derived from EVERY table,
        // and on a partial run the payload holds a fraction of them. The database is the only place
        // the whole map exists, which is the reason this pass lives on this side at all.
        let write_started = std::time::Instant::now();
        let first = store::write(db_path, &WriteRequest {
            tables: &payload.tables,
            stream,
            texts: &texts,
            root,
            shas: &payload.shas,
            gone: &gone,
            full: false,
            counters: &payload.counters,
            lang: LANG,
            half: Some(LANG),
            setup: payload.setup.as_deref(),
            extra: &extra,
            keep_existing: true,
            counts: true,
        })?;
        receipt.phases.insert("write the rows".to_string(), write_started.elapsed().as_millis() as u64);
        receipt.write_steps = first.steps.iter().map(|(n, ms)| (n.to_string(), *ms)).collect();
        // THE GRAPH NOW, AND THEN THE PAYLOAD GOES. The graph is repaired from the payload's own tables;
        // the closure below reads EVERY row back from the database, and holding the payload beside them
        // put the exe at several GB on a large consumer, much of it rows already written.
        if let Ok(conn) = Connection::open(db_path) {
            digest.borrow_mut().resolve_kept(&conn);
        }
        super::remember::remember_graph(&mut receipt, db_path, &digest.borrow(), Some(payload.affected.as_slice()));
        payload.tables = Map::new();
        crate::give_back();
        // THE HTTP PASS'S INPUTS: their token stays when the rows it reads came back as they went (`http::inputs`).
        if let Some(h) = &http_before { h.settle(&Connection::open(db_path)?, LANG, &rewrites(&ids, &payload.affected, &node_whole))?; }

        // THE STYLESHEETS BEFORE THE CLOSURE: `class_hides` says which class gates hide their element, and the
        // closure reads a class gate on the false side only then. Derived after it, every class gate read as a plain condition.
        let mut first = first;
        tsderive::styles(&mut receipt, &mut first.written, db_path, root, &workspace)?;
        // THE CLOSURE STAYS WHEN NOTHING IT READ MOVED (`unchanged`): the comparison is of what the run removed with
        // what it wrote in its place, cell for cell of what the last derivation read.
        let compare_started = std::time::Instant::now();
        let moved = match &kept {
            Some((before, marks)) => before.moved(&Connection::open(db_path)?, LANG, marks, &rewrites(&ids, &payload.affected, &node_whole), &workspace)?,
            None => Some("no closure was recorded under this setup".to_string()),
        };
        *receipt.phases.entry("compare the closure's reads".to_string()).or_default() += compare_started.elapsed().as_millis() as u64;
        match moved {
            None => {
                receipt.notes.push("the typescript half kept the closure it stored: nothing it reads moved".to_string());
                if let Some((before, _)) = &kept {
                    before.vouch(&Connection::open(db_path)?, LANG)?;
                }
            }
            Some(why) => {
                receipt.notes.push(format!("the typescript half derives its closure again: {why}"));
                // THE COUNTS ARE THE FIRST WRITE'S, but for the closure's own four tables, which only the second knows.
                let written = tsderive::closure(&mut receipt, db_path, root, &workspace, &payload.declared, payload.spec.as_ref(), &fingerprint, true)?;
                first.written.extend(written.into_iter().filter(|(t, _)| CLOSURE_TABLES.contains(&t.as_str())));
            }
        }
        first
    } else {
        // THE ROWS GO IN FIRST HERE TOO, and the closure reads them back - what a partial run does. Holding the
        // payload in the store while the closure ran put a full run's exe at several GB on a large Angular workspace; written and
        // dropped first, the closure works from rows that are decoded only where it reads them.
        let write_started = std::time::Instant::now();
        let mut applied = store::write(db_path, &WriteRequest {
            tables: &payload.tables,
            stream,
            texts: &texts,
            root,
            shas: &payload.shas,
            gone: &gone,
            full,
            counters: &payload.counters,
            lang: LANG,
            half: Some(LANG),
            setup: payload.setup.as_deref(),
            extra: &extra,
            keep_existing: false,
            counts: true,
        })?;
        receipt
            .phases
            .insert("write the rows".to_string(), write_started.elapsed().as_millis() as u64);
        receipt.write_steps = applied.steps.iter().map(|(n, ms)| (n.to_string(), *ms)).collect();
        super::remember::remember_graph(&mut receipt, db_path, &digest.borrow(), None);
        super::http::inputs::fresh(&Connection::open(db_path)?)?;
        let freeing = std::time::Instant::now();
        payload.tables = Map::new();
        crate::give_back();
        receipt.phases.insert("free the payload".to_string(), freeing.elapsed().as_millis() as u64);

        // THE STYLESHEETS BEFORE THE CLOSURE: `class_hides` says which class gates hide their element, and the
        // closure reads a class gate on the false side only then. Derived after it, every class gate read as a plain condition.
        tsderive::styles(&mut receipt, &mut applied.written, db_path, root, &workspace)?;
        let written = tsderive::closure(&mut receipt, db_path, root, &workspace, &payload.declared, payload.spec.as_ref(), &fingerprint, false)?;
        // A FULL RUN'S TALLY HAS ALWAYS NAMED THE CLOSURE'S FOUR TABLES beside the rest.
        applied.written.extend(written);
        applied
    };
    receipt.written = applied.written;

    // EVERY JOIN THE SPEC DECLARES IS INDEXED, and every column a reader selects by (`LOOKUP_COLUMNS`) - built LAST,
    // once over the finished rows, not kept up insert by insert.
    let indexing = std::time::Instant::now();
    let db = Connection::open(db_path)?;
    schema::index_declared(&db, LOOKUP_COLUMNS)?;
    receipt.phases.insert("index the joins".to_string(), indexing.elapsed().as_millis() as u64);
    // WHO DERIVED WHAT IS NOW STORED, and what deriving it again needs that no row holds (`tsderive::rederive`).
    tsderive::remember(&db, build, &base, &payload.declared, receipt.fe.as_deref())?;

    receipt.stored = store::recorded(db_path, LANG).len();
    Ok(receipt)
}


/// Which part of a partial run removes and writes which rows - see `unchanged::Rewrites`.
fn rewrites<'a>(ids: &'a [String], affected: &'a [String], whole: &'a BTreeSet<String>) -> unchanged::Rewrites<'a> {
    unchanged::Rewrites { ids, paths: affected, whole, derived: crate::cssmap::TABLES, derived_kind: crate::cssmap::UNPARSED }
}

#[cfg(test)]
#[path = "tsapply_tests.rs"]
mod tests;
