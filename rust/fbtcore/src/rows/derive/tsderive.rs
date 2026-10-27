//! WHAT RUST DERIVES FROM THE ANGULAR HALF'S STORED ROWS: the stylesheet tables (`cssmap`: `stylesheets`,
//! `style_rules`, `class_hides`) and, over them, the closure (`render_path`, `key_reach`, `gate_values`,
//! `gate_features`). Derived after every write of node's rows (`tsapply`), and DERIVED AGAIN WITHOUT NODE
//! (`rederive`) when the rows stand and only the build moved.
//!
//! A RELEASE THAT CHANGES ONLY WHAT RUST DERIVES must not read the tree again: the rows node writes are keyed by what
//! produces them (`embedded::node_rows`: the scripts and the rust their payload passes through), and the derivation by
//! the running build, kept in `_meta` (`RECORD`) after every derivation with what a derivation needs that no row holds -
//! the feature API node read off the config, the workspace folder and what the closure was derived under. Everything
//! else it reads is the database, as it is on a full run: `Store::from_db` reads every row back, and the stylesheets are
//! read off the disk, which the key says has not moved. So a rederive leaves the database a full run of this build
//! would have written - the derived tables dropped and made again, as a full run makes them.

use super::half;
use super::schema;
use super::store::{self, WriteRequest};
use super::ts::keyreach;
use super::ts::store as tsstore;
use super::tsapply::{workspace_of, Declared, TsReceipt, LANG, LOOKUP_COLUMNS};
use super::unchanged;
use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// THE TABLES THE CLOSURE DERIVES, stated here because this side writes them and node never sees one.
pub const CLOSURE_TABLES: &[&str] = &["render_path", "gate_features", "gate_values", "key_reach", "key_reach_ways", "key_reach_chains", "control_options", "key_builds", "key_node_ways"];

/// The `_meta` key a derivation is recorded under: the build that derived, and what it needs that no row holds.
const RECORD: &str = "rederive:typescript";

/// THE CLOSURE, DERIVED AND STORED, and what it read recorded beside it (`unchanged`) - the one way every run makes it.
/// A full run empties its tables first; a partial run (`delta`) keeps them and writes only the rows that moved.
#[allow(clippy::too_many_arguments)]
pub fn closure(receipt: &mut TsReceipt, db_path: &Path, root: &str, workspace: &Path, declared: &Declared, spec: Option<&Value>,
    fingerprint: &str, delta: bool) -> Result<BTreeMap<String, i64>> {
    if !delta {
        let db = Connection::open(db_path)?;
        db.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; BEGIN")?;
        for table in CLOSURE_TABLES {
            if !schema::existing_columns(&db, table)?.is_empty() {
                half::delete_half_rows(&db, table, LANG)?;
            }
        }
        db.execute_batch("COMMIT")?;
    }
    let closure_started = std::time::Instant::now();
    let (emitted, reads) = {
        let db = Connection::open(db_path)?;
        let mut held = tsstore::Store::from_db(&db, LANG)?;
        let (stats, _) = keyreach::build_closure(&mut held, &declared.checks, &declared.enum_name, workspace, &mut |note| {
            receipt.notes.push(format!("the typescript half {note}"))
        });
        record(receipt, &stats);
        let reads = held.reads();
        let mut out = Map::new();
        for (name, rows) in held.emitted {
            out.insert(name, Value::Array(rows.into_iter().map(|r| Value::Object(r.into_map())).collect()));
        }
        (out, reads)
    };
    // THE STORE AND WHAT THE PASSES DERIVED ARE GONE: their pages back to the system before the write.
    crate::give_back();
    receipt.phases.insert("derive the closure".to_string(), closure_started.elapsed().as_millis() as u64);
    // TIMED HERE TOO: the partial branch always said what writing the closure cost, and a full run's was part of
    // the minute and more of `deep: typescript` no span named.
    let closure_written = std::time::Instant::now();
    let emitted = if delta { changed_rows(receipt, db_path, emitted)? } else { emitted };
    let written = store::write(db_path, &WriteRequest {
        tables: &emitted,
        stream: None,
        texts: &store::Texts::Held(&BTreeMap::new()),
        root,
        shas: &Map::new(),
        gone: &BTreeSet::new(),
        full: false,
        counters: &Map::new(),
        lang: LANG,
        half: Some(LANG),
        setup: None,
        extra: &BTreeMap::new(),
        keep_existing: true,
        counts: true,
    })?
    .written;
    drop(emitted);
    crate::give_back();
    unchanged::record(&Connection::open(db_path)?, LANG, &reads, fingerprint, spec)?;
    receipt.phases.insert("write the closure".to_string(), closure_written.elapsed().as_millis() as u64);
    Ok(written)
}

/// A PARTIAL RUN'S CLOSURE, AS WHAT CHANGED (`delta`): only the emitted rows nothing stored matched come back to be written.
fn changed_rows(receipt: &mut TsReceipt, db_path: &Path, emitted: Map<String, Value>) -> Result<Map<String, Value>> {
    let (fresh, total) = super::delta::reconcile_tables(&Connection::open(db_path)?, CLOSURE_TABLES, emitted, LANG)?;
    receipt.notes.push(format!(
        "the typescript half rewrote {} closure row(s) and deleted {}, keeping {} that did not move",
        total.inserted, total.deleted, total.kept
    ));
    Ok(fresh)
}

/// WHAT THE CLOSURE IS DERIVED UNDER THAT THE ROWS DO NOT SAY: node's setup, the feature API, the options and the
/// workspace - one hash, kept in the record so a rederive is under the same as the run that wrote the rows.
pub fn base(setup: Option<&str>, declared: &Declared, options: Option<&str>, workspace: &Path) -> String {
    let checks = declared.checks.join("\u{1}");
    let workspace = workspace.to_string_lossy();
    unchanged::fingerprint(&[setup.unwrap_or_default(), &declared.enum_name, &checks, options.unwrap_or_default(), &workspace])
}

/// ...and the BUILD that derives it: a partial run keeps the closure only when the same build derived it
/// (`unchanged`) - node's setup no longer moves with the build.
pub fn fingerprint(base: &str, build: &str) -> String {
    unchanged::fingerprint(&[base, build, env!("CARGO_PKG_VERSION")])
}

/// THE STYLESHEETS, AS RULES (`cssmap`) - derived after every other row is in, because `class_hides` joins node's gates
/// to them; whole on every run, so a partial one needs nothing of its own.
pub fn styles(receipt: &mut TsReceipt, written: &mut BTreeMap<String, i64>, db_path: &Path, root: &str, workspace: &Path) -> Result<()> {
    let started = std::time::Instant::now();
    written.extend(crate::cssmap::store(db_path, root, workspace, LANG, &mut receipt.notes)?);
    // THE TALLY OF EVERY ROW, as the write that counts every table states it - over the stylesheets' new counts.
    super::meta::put_moved(&Connection::open(db_path)?, "rows", &written.values().filter(|n| **n > 0).sum::<i64>().to_string())?;
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
    if stats.build_rows > 0 {
        receipt.notes.push(format!("the typescript half related {} key hole(s) to the template literal that builds them (key_builds)", stats.build_rows));
    }
    if stats.option_rows > 0 {
        receipt.notes.push(format!("the typescript half found {} option key(s) offered to form controls", stats.option_rows));
    }
}

/// No record: called before a run deletes a row, so one that stops half way vouches for no derivation.
pub fn forget(db: &Connection) -> Result<()> {
    if super::meta::exists(db) {
        super::meta::delete(db, RECORD)?;
    }
    Ok(())
}

/// The derivation just stored, and what a rederive needs that no row holds.
pub fn remember(db: &Connection, build: &str, base: &str, declared: &Declared, fe: Option<&str>) -> Result<()> {
    let record = json!({ "build": build, "base": base, "checks": declared.checks, "enum": declared.enum_name, "fe": fe.unwrap_or_default() });
    super::meta::ensure(db)?;
    super::meta::put(db, RECORD, &record.to_string())
}

/// The build that derived what the database holds; None when no derivation is recorded.
pub fn derived_by(db_path: &Path) -> Option<String> {
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let record: Value = serde_json::from_str(&super::meta::get(&db, RECORD)?).ok()?;
    record["build"].as_str().map(String::from)
}

/// THE STORED ROWS CANNOT BE DERIVED AGAIN (no record): their setup goes, so node reads every file on its next run.
pub fn unrecorded(db_path: &Path) -> Result<()> {
    let db = Connection::open(db_path)?;
    super::meta::delete(&db, &format!("setup:{LANG}"))
}

/// EVERYTHING RUST DERIVES FROM THE STORED ROWS, DERIVED AGAIN by `build` without node: the derived tables dropped as
/// a full run drops them, the stylesheets read, the closure derived over every row read back, the joins indexed. Err
/// when there is no record to derive under - the caller then has node read the tree.
pub fn rederive(db_path: &Path, root: &str, build: &str) -> Result<TsReceipt> {
    let db = Connection::open(db_path).with_context(|| format!("the database could not be opened: {}", db_path.display()))?;
    let Some(text) = super::meta::get(&db, RECORD) else { bail!("no derivation is recorded") };
    let record: Value = serde_json::from_str(&text).context("the recorded derivation could not be read")?;
    let (Some(base), Some(checks), Some(enum_name), Some(fe)) =
        (record["base"].as_str(), record["checks"].as_array(), record["enum"].as_str(), record["fe"].as_str())
    else {
        bail!("the recorded derivation is not whole")
    };
    let declared = Declared { checks: checks.iter().filter_map(|c| c.as_str().map(String::from)).collect(), enum_name: enum_name.to_string() };
    let fe = (!fe.is_empty()).then(|| fe.to_string());
    let base = base.to_string();
    let spec: Option<Value> = super::meta::get(&db, &format!("spec:{LANG}")).and_then(|s| serde_json::from_str(&s).ok());
    let workspace = workspace_of(root, fe.as_deref());
    let mut receipt = TsReceipt { fe: fe.clone(), ..Default::default() };

    // WHAT A FULL RUN DROPS BEFORE IT WRITES, of the tables derived here: a table only this half holds goes whole - its
    // nodes and indexes with it - and is made again by the write, as a fresh map's; a shared one keeps the others' rows.
    let dropping = std::time::Instant::now();
    db.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; BEGIN")?;
    unchanged::forget(&db, LANG)?;
    forget(&db)?;
    for table in CLOSURE_TABLES.iter().chain(crate::cssmap::TABLES) {
        if schema::existing_columns(&db, table)?.is_empty() {
            continue;
        }
        let escaped = schema::escape(table);
        let shared = db.prepare(&format!("SELECT 1 FROM \"{escaped}\" WHERE \"{}\" IS NOT ?1 LIMIT 1", half::HALF_COLUMN))?.exists([LANG])?;
        if shared {
            half::delete_half_rows(&db, table, LANG)?;
        } else {
            super::cells::drop_children(&db, table)?;
            db.execute_batch(&format!("DROP TABLE \"{escaped}\""))?;
        }
    }
    db.execute_batch("COMMIT")?;
    receipt.phases.insert("drop the derived rows".to_string(), dropping.elapsed().as_millis() as u64);

    // THE COUNTS A FULL RUN'S WRITE STATES, every table but the derived ones, which follow.
    let mut written: BTreeMap<String, i64> = schema::listable_tables(&db)?.into_iter().map(|t| {
        let n = schema::count_of(&db, &t);
        (t, n)
    }).collect();
    if !schema::existing_columns(&db, "file_text")?.is_empty() {
        written.insert("file_text".to_string(), schema::count_of(&db, "file_text"));
    }
    drop(db);
    styles(&mut receipt, &mut written, db_path, root, &workspace)?;
    written.extend(closure(&mut receipt, db_path, root, &workspace, &declared, spec.as_ref(), &fingerprint(&base, build), false)?);
    let indexing = std::time::Instant::now();
    let db = Connection::open(db_path)?;
    schema::index_declared(&db, LOOKUP_COLUMNS)?;
    receipt.phases.insert("index the joins".to_string(), indexing.elapsed().as_millis() as u64);
    remember(&db, build, &base, &declared, fe.as_deref())?;
    receipt.written = written;
    receipt.stored = store::recorded(db_path, LANG).len();
    receipt.notes.push("the typescript half derived again what rust derives from its stored rows: this build did not derive them, and node's rows stand".to_string());
    Ok(receipt)
}
