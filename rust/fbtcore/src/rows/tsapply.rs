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
//! THE RENDER CLOSURE RUNS HERE, not in node. It is derived from EVERY other table, so it needs
//! every row — which node only has while it still holds the whole store. Handing them back for a
//! partial run would cost about as much JSON as a `JSON.parse` can hold at all, so the
//! pass lives on the side that can read the database instead. On a full run the rows are already
//! in the payload and are served straight out of it; nothing is read back.

use super::deps;
use super::half::{drop_half, tables_with_half, HALF_COLUMN};
use super::pyjson;
use super::schema;
use super::store::{self, WriteRequest};
use super::ts::keyreach;
use super::ts::store as tsstore;
use anyhow::{bail, Context, Result};
use indexmap::IndexSet;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What the `files` rows of this half are stamped with, so the other halves' files stay out of
/// every question this one asks about "what is already here".
pub const LANG: &str = "typescript";

/// THE FOUR TABLES THE CLOSURE DERIVES, stated here because this side writes them and node never
/// sees one.
const CLOSURE_TABLES: &[&str] = &["render_path", "gate_features", "gate_values", "key_reach"];

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
    pub shas: Map<String, Value>,
    pub counters: Map<String, Value>,
    /// Its list and object cells kept as the text node sent - see `rawcells`.
    #[serde(deserialize_with = "super::rawcells::tables")]
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

/// MAKE ROOM FOR A PARTIAL PAYLOAD: this half's rows for the files being re-extracted, and nothing
/// else.
///
/// TWO KINDS OF TABLE, and node says which is which rather than this side keeping a second copy of
/// the knowledge. A table node rebuilt WHOLE has its rows for this half removed outright — they are
/// derived from the tree and a partial tree cannot produce them correctly. Every other table is per
/// FILE, and only the affected files' rows go.
///
/// BY `owner_file`, which is why that column exists: 21 of the 53 tables carry no `file` of their
/// own, and the parent chain that would stand in for one cannot attribute the module-scope `calls`
/// at all.
///
/// THE OTHER HALVES ARE NEVER TOUCHED — every statement is scoped by `half`.
fn replace_affected(db: &Connection, affected: &[String], whole: &[String]) -> Result<usize> {
    let mut ids: Vec<String> = Vec::new();
    if !affected.is_empty() {
        let marks = vec!["?"; affected.len()].join(",");
        let sql = format!("SELECT id FROM files WHERE half = ? AND path IN ({marks})");
        let mut stmt = db.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(affected.len() + 1);
        let lang = LANG;
        params.push(&lang);
        for path in affected {
            params.push(path);
        }
        for id in stmt.query_map(params.as_slice(), |r| r.get::<_, String>(0))?.flatten() {
            ids.push(id);
        }
    }

    let wholly: IndexSet<&str> = whole.iter().map(String::as_str).collect();
    let mut removed = 0usize;
    for table in tables_with_half(db)? {
        let columns = schema::existing_columns(db, &table)?;
        let escaped = schema::escape(&table);
        let count = if wholly.contains(table.as_str()) {
            db.execute(
                &format!("DELETE FROM \"{escaped}\" WHERE \"{HALF_COLUMN}\" = ?1"),
                [LANG],
            )?
        } else if columns.iter().any(|c| c == "owner_file") && !ids.is_empty() {
            let marks = vec!["?"; ids.len()].join(",");
            let sql = format!(
                "DELETE FROM \"{escaped}\" WHERE \"{HALF_COLUMN}\" = ? AND owner_file IN ({marks})"
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() + 1);
            let lang = LANG;
            params.push(&lang);
            for id in &ids {
                params.push(id);
            }
            db.execute(&sql, params.as_slice())?
        } else {
            continue
        };
        removed += count;
    }

    // THE `files` ROWS OF THE AFFECTED FILES TOO, and last: the lookup above reads them. The write
    // re-creates each one from the payload.
    if !affected.is_empty() {
        let marks = vec!["?"; affected.len()].join(",");
        let sql = format!("DELETE FROM files WHERE half = ? AND path IN ({marks})");
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(affected.len() + 1);
        let lang = LANG;
        params.push(&lang);
        for path in affected {
            params.push(path);
        }
        db.execute(&sql, params.as_slice())?;
    }
    Ok(removed)
}

/// The graph, derived from the rows just written and kept in `_meta` for the next run to read.
///
/// `replaced` names the files a PARTIAL run re-extracted: the stored graph is repaired for those
/// and left alone everywhere else, because this run never saw the other rows.
///
/// DELETED AND THEN INSERTED, never `INSERT OR REPLACE`: `_meta` is `(key TEXT, value TEXT)` with
/// NO unique key, so REPLACE has nothing to replace ON and simply adds a second row. Every reader
/// takes the first one, so each run stored a graph nobody read and answered with the PREVIOUS run's.
fn remember_deps(db: &Connection, tables: &Map<String, Value>, replaced: Option<&[String]>) {
    let read = |key: &str| -> Option<String> {
        db.query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| r.get::<_, String>(0))
            .ok()
    };
    let deps_key = format!("deps:{LANG}");
    let scope_key = format!("scope:{LANG}");

    let (edges, scope) = match replaced {
        None => (deps::graph(tables), deps::scope_files(tables)),
        Some(files) => {
            // READ BACK AS JSON AND NOT AS A MAP TYPE: this crate's ordered map has no serde of its
            // own, and the stored graph is a plain object of string lists either way.
            let mut previous: indexmap::IndexMap<String, Vec<String>> = indexmap::IndexMap::new();
            if let Some(Value::Object(stored)) = read(&deps_key).and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
                for (target, dependents) in stored {
                    if let Value::Array(list) = dependents {
                        previous.insert(
                            target,
                            list.iter().filter_map(|d| d.as_str().map(String::from)).collect(),
                        );
                    }
                }
            }
            let held: Vec<String> = read(&scope_key)
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            (deps::merge(&previous, tables, files), deps::merge_scope(&held, tables, files))
        }
    };

    // A graph that could not be stored costs the next run its incremental parse and nothing else,
    // so it is not worth failing a map that is otherwise complete.
    let mut as_object = Map::new();
    for (target, dependents) in edges {
        as_object.insert(target, Value::from(dependents));
    }
    let compact = |v: &Value| serde_json::to_string(v).unwrap_or_else(|_| "null".into());
    let edges_json = compact(&Value::Object(as_object));
    let scope_json = compact(&Value::from(scope));
    for (key, value) in [(deps_key, edges_json), (scope_key, scope_json)] {
        let _ = db.execute("DELETE FROM _meta WHERE key = ?1", [key.as_str()]);
        let _ = db.execute(
            "INSERT INTO _meta (key, value) VALUES (?1, ?2)",
            [key.as_str(), value.as_str()],
        );
    }
}

/// THE FOLDER A `files.path` OF THIS HALF IS RELATIVE TO: the workspace node mapped (`fe`), not the root.
///
/// The two are one folder only when the workspace sits AT the root. On a consumer it is often a folder
/// below it, and every source read off `root/src/app/...` missed: `file_text` held no `.ts` or `.html`
/// at all, and the dead-key trace scanned an empty workspace. A read that misses is skipped by design,
/// so nothing said so.
fn workspace_of(root: &str, fe: Option<&str>) -> PathBuf {
    match fe {
        Some(fe) if !fe.is_empty() && fe != "." => Path::new(root).join(fe.replace('/', std::path::MAIN_SEPARATOR_STR)),
        _ => PathBuf::from(root),
    }
}

/// THE PAYLOAD IS TAKEN BY VALUE, because storing it is the last thing anybody does with it.
/// A full run's `tables` is some gigabytes of parsed rows, and the full path below MOVES them
/// into the store rather than cloning them; a caller that still needed its payload afterwards
/// would have forced the copy back.
pub fn apply(db_path: &Path, root: &str, mut payload: TsPayload) -> Result<TsReceipt> {
    if !payload.all && !payload.partial {
        bail!("a payload must say whether it is the whole half or a named set of files");
    }
    let mut receipt = TsReceipt { partial: payload.partial, fe: payload.fe.take(), ..Default::default() };
    let workspace = workspace_of(root, receipt.fe.as_deref());

    // A FULL RUN REPLACES THE FILE, and it is decided by the DATABASE: with three extractors
    // writing here, a half that rebuilt on its own row count would take the other two halves' rows
    // with it.
    let full = !store::usable(db_path) && !payload.partial;
    if payload.partial {
        let db = Connection::open(db_path)
            .with_context(|| format!("the database could not be opened: {}", db_path.display()))?;
        // THE CLOSURE'S OWN TABLES ARE WHOLE TOO, and node cannot say so: it does not produce them.
        let mut whole = payload.whole.clone();
        whole.extend(CLOSURE_TABLES.iter().map(|t| t.to_string()));
        let replace_started = std::time::Instant::now();
        receipt.replaced = replace_affected(&db, &payload.affected, &whole)?;
        receipt.phases.insert("replace the affected rows".to_string(), replace_started.elapsed().as_millis() as u64);
        receipt.notes.push(format!(
            "the typescript half is replacing {} file(s), not all {}",
            payload.affected.len(),
            payload.shas.len()
        ));
    } else if !full && db_path.exists() {
        let db = Connection::open(db_path)?;
        drop_half(&db, LANG)?;
    }

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

    let named = text_paths_of(root, payload.tables.get("files"));
    let texts = store::Texts::OnDisk { root: &workspace, rels: &named };
    let gone: BTreeSet<String> = BTreeSet::new();

    let applied = if payload.partial {
        // NOTHING KEPT MAY POINT AT A ROW THIS RUN REPLACES - see `reads::dangling`. Checked before anything is
        // written, so a run that would break a join leaves the map as it was and says which.
        let check_started = std::time::Instant::now();
        {
            let db = Connection::open(db_path)?;
            let deps = super::half::meta(db_path, &format!("deps:{LANG}"));
            let deps: Map<String, Value> = serde_json::from_str(&deps).unwrap_or_default();
            if let Some((count, first)) = super::reads::dangling(&db, LANG, &payload.affected, &payload.tables, &deps)? {
                bail!("{} would leave {count} reference(s) to rows it replaced, first {first}", super::reads::STOPPED_SHORT);
            }
        }
        receipt.phases.insert("check the kept rows".to_string(), check_started.elapsed().as_millis() as u64);
        // THE ROWS GO IN FIRST, and only then is the closure asked — it is derived from EVERY table,
        // and on a partial run the payload holds a fraction of them. The database is the only place
        // the whole map exists, which is the reason this pass lives on this side at all.
        let write_started = std::time::Instant::now();
        let first = store::write(db_path, &WriteRequest {
            tables: &payload.tables,
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
        let rebuilt: Vec<String> = payload.spec.as_ref().map(|s| deps::list(s.get("rebuilt"))).unwrap_or_default();
        remember_graph(&mut receipt, db_path, &payload.tables, Some(payload.affected.as_slice()), &rebuilt);
        payload.tables = Map::new();

        let closure_started = std::time::Instant::now();
        let emitted = {
            let db = Connection::open(db_path)?;
            let mut held = tsstore::Store::from_db(&db, LANG)?;
            let (stats, _) = keyreach::build_closure(
                &mut held,
                &payload.declared.checks,
                &payload.declared.enum_name,
                &workspace,
                &mut |note| receipt.notes.push(format!("the typescript half {note}")),
            );
            record(&mut receipt, &stats);
            let mut out = Map::new();
            for (name, rows) in held.emitted {
                out.insert(name, Value::Array(rows.into_iter().map(|r| Value::Object(r.into_map())).collect()));
            }
            out
        };
        receipt.phases.insert("derive the closure".to_string(), closure_started.elapsed().as_millis() as u64);
        // THE COUNTS ARE THE FIRST WRITE'S. The second puts the closure's own four tables in, and
        // reporting its tally would say this run wrote four tables and nothing else.
        let closure_written = std::time::Instant::now();
        store::write(db_path, &WriteRequest {
            tables: &emitted,
            texts: &store::Texts::Held(&BTreeMap::new()),
            root,
            shas: &Map::new(),
            gone: &gone,
            full: false,
            counters: &Map::new(),
            lang: LANG,
            half: Some(LANG),
            setup: None,
            extra: &BTreeMap::new(),
            keep_existing: true,
            counts: true,
        })?;
        receipt.phases.insert("write the closure".to_string(), closure_written.elapsed().as_millis() as u64);
        let mut first = first;
        styles(&mut receipt, &mut first.written, db_path, root, &workspace)?;
        first
    } else {
        // THE ROWS GO IN FIRST HERE TOO, and the closure reads them back - what a partial run does. Holding the
        // payload in the store while the closure ran put a full run's exe at several GB on a large Angular workspace; written and
        // dropped first, the closure works from rows that are decoded only where it reads them.
        let write_started = std::time::Instant::now();
        let mut applied = store::write(db_path, &WriteRequest {
            tables: &payload.tables,
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
        let rebuilt: Vec<String> = payload.spec.as_ref().map(|s| deps::list(s.get("rebuilt"))).unwrap_or_default();
        remember_graph(&mut receipt, db_path, &payload.tables, None, &rebuilt);
        payload.tables = Map::new();

        let closure_started = std::time::Instant::now();
        let emitted = {
            let db = Connection::open(db_path)?;
            let mut held = tsstore::Store::from_db(&db, LANG)?;
            let (stats, _) = keyreach::build_closure(
                &mut held,
                &payload.declared.checks,
                &payload.declared.enum_name,
                &workspace,
                &mut |note| receipt.notes.push(format!("the typescript half {note}")),
            );
            record(&mut receipt, &stats);
            let mut out = Map::new();
            for (name, rows) in held.emitted {
                out.insert(name, Value::Array(rows.into_iter().map(|r| Value::Object(r.into_map())).collect()));
            }
            out
        };
        receipt
            .phases
            .insert("derive the closure".to_string(), closure_started.elapsed().as_millis() as u64);
        // TIMED HERE TOO: the partial branch always said what writing the closure cost, and a full run's was part of
        // the minute and more of `deep: typescript` no span named.
        let closure_written = std::time::Instant::now();
        let closure = store::write(db_path, &WriteRequest {
            tables: &emitted,
            texts: &store::Texts::Held(&BTreeMap::new()),
            root,
            shas: &Map::new(),
            gone: &gone,
            full: false,
            counters: &Map::new(),
            lang: LANG,
            half: Some(LANG),
            setup: None,
            extra: &BTreeMap::new(),
            keep_existing: true,
            counts: true,
        })?;
        receipt.phases.insert("write the closure".to_string(), closure_written.elapsed().as_millis() as u64);
        // A FULL RUN'S TALLY HAS ALWAYS NAMED THE CLOSURE'S FOUR TABLES beside the rest.
        applied.written.extend(closure.written);
        styles(&mut receipt, &mut applied.written, db_path, root, &workspace)?;
        applied
    };
    receipt.written = applied.written;

    receipt.stored = store::recorded(db_path, LANG).len();
    Ok(receipt)
}

/// WHICH FILES HAVE TO BE RE-EXTRACTED WHEN ONE CHANGES, stored beside the rows it is derived from so the
/// next run reads it instead of deriving it. A partial run REPAIRS it (`replaced`) rather than deriving
/// it: it holds a fraction of the rows, and reading the rest back costs close to a minute.
fn remember_graph(receipt: &mut TsReceipt, db_path: &Path, seen: &Map<String, Value>, replaced: Option<&[String]>, rebuilt: &[String]) {
    let deps_started = std::time::Instant::now();
    if let Ok(db) = Connection::open(db_path) {
        remember_deps(&db, seen, replaced);
        // THE RECORDED READS ARE PROVED against every row written - see `reads::unread` - and only proved reads
        // let the next run stop short of every hop of dependents. A full run proves them; a partial one holds
        // only its own rows, so it can find them wrong and never right.
        let missed = super::reads::unread(seen, rebuilt);
        let verdict = match missed.first() {
            Some((owner, named)) => {
                receipt.notes.push(format!(
                    "the typescript half's recorded reads miss {} file(s) its rows name, first {owner} -> {named} -                      the next run reads every dependent again",
                    missed.len()
                ));
                Some(format!("unproven: {owner} -> {named}"))
            }
            None => replaced.is_none().then(|| "proven".to_string()),
        };
        if let Some(verdict) = verdict {
            let key = format!("reads:{LANG}");
            let _ = db.execute("DELETE FROM _meta WHERE key = ?1", [key.as_str()]);
            let _ = db.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [key.as_str(), verdict.as_str()]);
        }
    }
    receipt
        .phases
        .insert("remember the graph".to_string(), deps_started.elapsed().as_millis() as u64);
}

/// THE STYLESHEETS, AS RULES (`cssmap`) - derived after every other row is in, because `class_hides`
/// joins node's gates to them; whole on every run, so a partial one needs nothing of its own.
fn styles(receipt: &mut TsReceipt, written: &mut BTreeMap<String, i64>, db_path: &Path, root: &str, workspace: &Path) -> Result<()> {
    let started = std::time::Instant::now();
    written.extend(crate::cssmap::store(db_path, root, workspace, LANG, &mut receipt.notes)?);
    receipt.phases.insert("read the stylesheets".to_string(), started.elapsed().as_millis() as u64);
    Ok(())
}

fn record(receipt: &mut TsReceipt, stats: &keyreach::Closure) {
    receipt.path_rows = stats.path_rows;
    receipt.components = stats.components;
    receipt.keys = stats.keys;
    receipt.gate_features = stats.gate_feature_rows;
    receipt.gate_values = stats.gate_value_rows;
    receipt.notes.push(format!(
        "the typescript half walked {} render path(s) into {} component(s) and placed {} translation key(s)",
        stats.path_rows, stats.components, stats.keys
    ));
    receipt.notes.push(format!(
        "the typescript half resolved {} gate feature(s) and {} gate value restriction(s)",
        stats.gate_feature_rows, stats.gate_value_rows
    ));
}

#[cfg(test)]
#[path = "tsapply_tests.rs"]
mod tests;
