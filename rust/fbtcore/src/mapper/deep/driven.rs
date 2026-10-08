//! THE TWO HALVES ONLY .NET CAN PARSE - C# (Roslyn) and T-SQL (ScriptDom) - DRIVEN FROM HERE. Their
//! conversation with the store is this file's: which files moved, the batches, the receipts, and the retry
//! when the store catches its cache lying. The caller is asked only what it alone can answer: which project
//! compiles a file, the order files are sent in (one compilation alive at a time), and the rows of a batch.
//!
//! HASH FIRST, DECIDE SECOND, and hash without reading: a file's content hash is the tree map's (`hashes.rs`),
//! which re-reads only what moved; a file it does not name is read and hashed here. What is recorded is that
//! hash folded with the project's inputs and the rows version, SHA-256 cut to 16 hex digits.
//!
//! NOTHING IS HELD THAT THE NEXT FILE DOES NOT NEED. A solution of many thousands of C# files answered `Out of memory`
//! when every row was built before any was sent; the rows leave in batches bounded by ROWS, not files - one
//! long controller carries more rows than dozens of DTOs.

use super::super::protocol::Collector;
use super::hashes::Hashes;
use super::reasons::{self, Parts};
use crate::rows::calls;
use serde_json::{json, Value};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// WHAT A C# ROW MEANS, folded into every file's sha. BUMP IT WITH EVERY CHANGE TO WHAT THE C# EXTRACTOR
/// WRITES: a file whose source and references have not moved is not re-read, so without a bump an upgraded
/// gate kept the old gate's rows - one large solution kept its old, lower bound share after the fixes that raise it.
// 20: `members`, `locals` and `comments` for C#.
// 21: `arguments.template`, and `string.Format` folded.
// 22: a hole naming a field carries its symbol (a referenced project's readonly folds through `consts`); a
// `foreach` over a collection initializer is its strings.
pub(super) const CS_ROWS_VERSION: &str = "22";
/// The same for the SQL extractor. 7: `sql_dynamic`, an object created by dynamic SQL.
const SQL_ROWS_VERSION: &str = "7";
/// Rows held before a batch is sent - both ends pay for it, so it decides whether a large solution fits.
const ROWS_PER_BATCH: i64 = 100_000;

pub type Ask<'a> = dyn FnMut(&Value) -> String + 'a;

/// The C# half, over every C# file in the tree - or none: a database holding C# rows holds them for files
/// that have just left, so the half runs with nothing in it and the store drops them.
/// `roots` and `skip`: a project referenced from the map but under a `--skip` folder is bound through its build's dll.
/// `forced`: `--map-reread csharp` - every file is read again, whatever the database recorded.
pub fn csharp(into: &mut Collector, db: &str, root: &str, files: &[(String, String)], hashes: &Hashes, exclude: &[String], roots: &[String], skip: &[String], forced: bool, ask: &mut Ask) {
    const LANGUAGE: &str = "csharp rows";
    // EVERY STEP OF THIS HALF IS A SPAN, so whatever the trace cannot place is a step's own `(untraced)`.
    let hashing = crate::trace::stage("csharp: hash the files and projects");
    let inputs = super::inputs::Inputs::open(db, hashes);
    // EVERY FILE AT ONCE: a file's hash, its markup inputs and its project are independent of every other's.
    let hashed: Vec<(String, String, String, Option<String>)> = files
        .par_iter()
        .filter_map(|(rel, abs)| {
            // A MARKUP FILE'S SHA folds in what it is compiled with: its `_Imports`, `_ViewImports`, `Web.config`.
            let lower = abs.to_ascii_lowercase();
            let sha = if lower.ends_with(".razor") || lower.ends_with(".cshtml") {
                inputs.content(abs).map(|own| {
                    let parts: Vec<String> = std::iter::once(own).chain(crate::csproj::inputs(abs).iter().map(|i| inputs.hash(i))).collect();
                    folded(&parts.join("+"))
                })
            } else {
                inputs.content(abs)
            };
            // A file that cannot be read is already named by the file-level pass; it is dropped here.
            Some((rel.clone(), abs.clone(), sha.ok()?, crate::csproj::owner(abs)))
        })
        .collect();
    // ONE FINGERPRINT PER PROJECT, each taken once - in its parts, so a re-read can say which moved (`reasons.rs`).
    let mut owners: Vec<&String> = hashed.iter().filter_map(|(_, _, _, p)| p.as_ref()).collect();
    owners.sort_by_key(|p| p.to_lowercase());
    owners.dedup_by_key(|p| p.to_lowercase());
    let parts: Parts = owners.par_iter().map(|p| (p.to_string(), inputs.fingerprint_parts(p))).collect();
    let spelled: HashMap<String, String> = owners.iter().map(|p| (p.to_lowercase(), p.to_string())).collect();
    let fingerprints: HashMap<&String, String> = parts.iter().map(|(p, v)| (p, reasons::joined(v))).collect();
    let owned: Vec<String> = owners.iter().map(|p| p.to_string()).collect();
    let before = reasons::before(db);
    let mut shas = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut facts = HashMap::new();
    // AN EXCLUDED FILE'S SHA SAYS SO: adding a pattern re-reads the files it now matches - to drop their rows -
    // and removing one re-reads them to write the rows back, and no other file moves either way.
    let mut excluded = Vec::new();
    for (rel, abs, sha, project) in hashed {
        let project = project.map(|p| spelled[&p.to_lowercase()].clone());
        let fingerprint = project.as_ref().and_then(|p| fingerprints.get(p));
        let matched = crate::sources::glob::excluded(exclude, &rel);
        if matched {
            excluded.push(rel.clone());
        }
        shas.insert(rel.clone(), reasons::salted(&sha, fingerprint.map(String::as_str), CS_ROWS_VERSION, matched));
        facts.insert(rel.clone(), reasons::File { content: sha, project, excluded: matched });
        paths.insert(rel, abs);
    }
    drop(hashing);
    let errors_before = into.errors.len();
    let reading_state = crate::trace::stage("csharp: read what the database recorded");
    let Some(mut state) = state(into, db, "csharp", LANGUAGE) else { return };
    forget(into, &mut state, forced, "C#", "csharp");
    into.halves.insert(format!("{LANGUAGE} via fbtcore"));
    drop(reading_state);
    // NOTHING MOVED AND NOTHING LEFT: no session, no compilation. The half used to open one over every file - a good part of
    // a run that changed nothing - and compile a project for a file it was about to find unchanged.
    let moved = stale(&shas, &state);
    let gone = state.shas.keys().filter(|rel| !shas.contains_key(*rel)).count();
    if moved.is_empty() && gone == 0 {
        // A TREE WITH NO C# runs the half only so the store can drop rows of files that left; it has nothing to say.
        if !shas.is_empty() {
            into.notes.push(format!("the deep C# half had nothing to do: none of its {} file(s) moved", shas.len()));
        }
        // IT RAN, AND RE-READ NONE: "0 file(s) re-read", never the silence of a run with no deep half.
        into.reread = into.reread.max(0);
        into.refresh.insert("csharp".into(), json!({ "reread": 0, "dropped": 0, "files": shas.len() }));
        unrestored(into, db, root, &owned);
        reasons::keep(db, CS_ROWS_VERSION, &parts, before.as_ref());
        return;
    }
    // WHICH FILES, AND WHY - by cause, and by the project that forced them: "thousands of file(s) re-read" after a pull that
    // moved a few hundred could not be told from a cache that lies.
    let explained = reasons::explain(&moved, &facts, &state.shas, state.rebuild, before.as_ref(), &parts, gone, root);
    crate::trace::set("structuregate.csharp.moved", explained.facts["examples"].to_string());
    crate::trace::set("structuregate.csharp.causes", explained.facts["causes"].to_string());
    into.notes.extend(explained.notes);
    into.refresh.insert("csharp".into(), explained.facts);
    drop(facts);
    // WHICH PROJECT COMPILES WHICH FILE, worked out once by the caller - and the order files are sent in.
    let opening = crate::trace::stage("csharp: open the session");
    ask(&json!({ "mode": "csharp-open", "paths": paths, "shas": shas, "counters": state.counters, "excluded": excluded,
        "roots": roots, "skip": skip }));
    drop(opening);
    let order = |ask: &mut Ask, rels: Vec<&String>| -> Vec<String> {
        let _ordering = crate::trace::stage("csharp: order the files by project");
        strings(&answer(ask(&json!({ "mode": "csharp-order", "rels": rels })))["order"])
    };
    let stale = order(ask, stale(&shas, &state));
    let all = stale.len() == shas.len();
    let bound = std::env::var("STRUCTUREGATE_MAP_BATCH").ok().and_then(|n| n.parse::<i64>().ok()).filter(|n| *n > 0).unwrap_or(ROWS_PER_BATCH);
    // WHERE THE TIME WENT: [compile, rows, payload] from the caller, the wait for the store, the caller's file reads.
    let spent = std::cell::RefCell::new([0u64; 5]);
    // THE STORE'S OWN TIME, on its thread - beside the caller's, so it is a fact of the batches and not a child of them.
    let stored_ms = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let detail = crate::trace::detail();
    let batch_no = std::cell::Cell::new(0usize);
    // AND WHERE "COMPILE" WENT, project by project: [load references, parse sources, razor, declarations, files,
    // assemblies opened, assemblies already open].
    let phases: std::cell::RefCell<BTreeMap<String, [u64; 7]>> = std::cell::RefCell::new(BTreeMap::new());
    let send = |into: &mut Collector, ask: &mut Ask, stale: &[String], all: bool, reset: bool| -> Option<bool> {
        let mut sent = 0;
        let mut retry = false;
        let mut explained = 0i64;
        // THE STORE OF ONE BATCH OVERLAPS THE EXTRACTION OF THE NEXT: batch N is written on a worker while the
        // caller binds batch N+1. Still one writer, in order - N is joined before N+1's write starts.
        let mut writing: Option<(std::thread::JoinHandle<Result<String, String>>, Value)> = None;
        let finish = |into: &mut Collector, ask: &mut Ask, writing: Option<(std::thread::JoinHandle<Result<String, String>>, Value)>| -> Option<Receipt> {
            let (worker, header) = writing?;
            let waited = std::time::Instant::now();
            let result = worker.join().unwrap_or_else(|_| Err("the store panicked".into()));
            let ms = waited.elapsed().as_millis() as u64;
            spent.borrow_mut()[3] += ms;
            if detail {
                crate::trace::reported("csharp: wait for the store", ms);
            }
            release(&header, ask);
            received(into, result, LANGUAGE)
        };
        loop {
            // A DETAILED TRACE GIVES EACH BATCH ITS SPAN: what the caller says it spent in it, and the rest of the round
            // trip - the payload crossing, the collector - as the batch's own `(untraced)`.
            batch_no.set(batch_no.get() + 1);
            let batch = detail.then(|| crate::trace::stage("csharp batch"));
            let question = json!({ "mode": "csharp-batch", "rels": &stale[sent..], "bound": bound, "all": all,
                "first": sent == 0, "reset": reset && sent == 0 });
            let header = answer(ask(&question));
            if let Some(batch) = &batch {
                batch.set("structuregate.batch", batch_no.get() as i64);
                batch.set("structuregate.files", header["taken"].as_i64().unwrap_or(0));
                for (i, name) in ["compile", "rows", "payload", "read the sources"].iter().enumerate() {
                    crate::trace::reported(&format!("csharp: {name}"), header["spent"][i].as_u64().unwrap_or(0));
                }
            }
            // A FILE ALREADY NAMED UNPARSED is accounted for: the store's "stored N of M" below counts it again otherwise.
            explained += strings(&header["errors"]).iter().filter(|e| e.starts_with("UNPARSED  ") && !e.starts_with("UNPARSED  the ")).count() as i64;
            // A MOVED FILE THAT WILL NOT OPEN is dropped, as it always was - its rows could not be read, and a sha
            // recorded without rows would fail the store's invariant. The caller says which as it reads them; they
            // were opened here first, one by one, and on Windows that was each file's antivirus scan: a large share of a
            // long run. The file-level pass names them.
            explained += strings(&header["unreadable"]).len() as i64;
            into.errors.extend(strings(&header["errors"]));
            into.notes.extend(strings(&header["notes"]));
            sent += header["taken"].as_u64().unwrap_or(0) as usize;
            for (i, ms) in header["spent"].as_array().into_iter().flatten().enumerate().take(4) {
                // [compile, rows, payload, read] from the caller; the read lands after the store's slot.
                spent.borrow_mut()[if i == 3 { 4 } else { i }] += ms.as_u64().unwrap_or(0);
            }
            for project in header["projects"].as_array().into_iter().flatten() {
                let name = project[0].as_str().unwrap_or("").to_string();
                let mut held = phases.borrow_mut();
                let slot = held.entry(name).or_insert([0u64; 7]);
                for (i, value) in slot.iter_mut().enumerate() {
                    *value += project[i + 1].as_u64().unwrap_or(0);
                }
            }
            // The batch before this one lands first; a store that failed stops the half.
            if writing.is_some() {
                let Some(receipt) = finish(into, ask, writing.take()) else {
                    release(&header, ask);
                    return None;
                };
                retry |= receipt.retry;
            }
            // A REPLY WITH NO PAYLOAD is a caller that failed: its error is already said, and storing empty bytes
            // only added a second, misleading "EOF while parsing" one.
            if header.get("payload").is_none_or(Value::is_null) {
                return None;
            }
            let final_batch = header["final"] == Value::Bool(true);
            let (address, length) = (header["payload"]["address"].as_i64().unwrap_or(0) as usize, header["payload"]["length"].as_u64().unwrap_or(0) as usize);
            let (db_path, root_path) = (db.to_string(), root.to_string());
            let stored_ms = std::sync::Arc::clone(&stored_ms);
            let worker = std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let bytes: &[u8] = if address == 0 || length == 0 {
                    &[]
                } else {
                    // SAFETY: the caller pinned `length` bytes at `address`; they are released only after this
                    // thread is joined.
                    unsafe { std::slice::from_raw_parts(address as *const u8, length) }
                };
                let applied = calls::apply(Path::new(&db_path), &root_path, bytes);
                stored_ms.fetch_add(started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
                applied
            });
            writing = Some((worker, header));
            drop(batch);
            if final_batch {
                let receipt = finish(into, ask, writing.take())?;
                retry |= receipt.retry;
                // ONLY THE LAST BATCH IS A FULL ACCOUNT of the tree, so only it is held to the tree's own count.
                if receipt.stored + explained < shas.len() as i64 {
                    into.errors.push(format!("UNPARSED  the {LANGUAGE} half stored {} of {} file(s) and said nothing about the rest", receipt.stored, shas.len()));
                }
                return Some(retry);
            }
        }
    };
    let batches = crate::trace::stage("csharp: batches");
    if send(into, ask, &stale, all, false) == Some(true) {
        // THE INVARIANT IS CHECKED, NOT ASSUMED: only the caller can parse C#, so every file is parsed again.
        into.notes.push("the deep C# map disagreed with the tree and was written again in full".into());
        let everything = order(ask, shas.keys().collect());
        if send(into, ask, &everything, true, true) == Some(true) {
            into.errors.push(format!("HALF      {LANGUAGE}: the database still disagrees with the tree after a full rewrite — the deep C# rows in it cannot be trusted"));
        }
    }
    // THE TOTALS ADD UP THE BATCHES - unless each batch carried its own, when they only summarise them.
    const SPENT: [&str; 5] = ["compile", "rows", "payload", "wait for the store", "read the sources"];
    for (name, ms) in SPENT.iter().zip(spent.borrow().iter()) {
        let name = format!("csharp: {name}");
        if detail { crate::trace::breakdown_with(&name, *ms, Vec::new()) } else { crate::trace::reported(&name, *ms) }
    }
    batches.set("structuregate.batches", batch_no.get() as i64);
    batches.set("structuregate.csharp.store_thread_ms", stored_ms.load(std::sync::atomic::Ordering::Relaxed) as i64);
    compile_phases(root, &phases.into_inner());
    drop(batches);
    {
        let _closing = crate::trace::stage("csharp: close the session");
        ask(&json!({ "mode": "csharp-close" }));
    }
    {
        let _checking = crate::trace::stage("csharp: find unrestored projects");
        unrestored(into, db, root, &owned);
    }
    let spent_parts: Vec<String> = ["compile", "rows", "payload", "store"].iter().zip(spent.borrow().iter())
        .filter(|(_, ms)| **ms >= 1000).map(|(name, ms)| format!("{name} {} s", (ms + 500) / 1000)).collect();
    if !spent_parts.is_empty() {
        into.notes.push(format!("the C# half spent: {}", spent_parts.join(", ")));
    }
    // THE FINGERPRINTS THE NEXT RUN EXPLAINS ITS RE-READS AGAINST - only after a half that added no error.
    if into.errors.len() == errors_before {
        reasons::keep(db, CS_ROWS_VERSION, &parts, before.as_ref());
    }
}

/// WHERE A RUN'S COMPILE TIME WENT, for the trace: each phase's total across projects, and the slowest projects
/// with their phases and how many files each parsed. Only a traced run pays for this.
fn compile_phases(root: &str, phases: &BTreeMap<String, [u64; 7]>) {
    const NAMES: [&str; 4] = ["load references", "parse sources", "razor", "declarations"];
    for (i, name) in NAMES.iter().enumerate() {
        let total: u64 = phases.values().map(|p| p[i]).sum();
        crate::trace::breakdown_with(&format!("csharp: {name}"), total, Vec::new());
    }
    let mut slowest: Vec<(&String, &[u64; 7])> = phases.iter().collect();
    slowest.sort_by_key(|(_, p)| std::cmp::Reverse(p[..4].iter().sum::<u64>()));
    // EVERY PROJECT in a detailed trace; the 20 slowest otherwise.
    let shown = if crate::trace::detail() { slowest.len() } else { 20 };
    for (project, p) in slowest.into_iter().take(shown) {
        let named = Path::new(project).strip_prefix(root).map_or_else(|_| project.clone(), |r| r.to_string_lossy().replace('\\', "/"));
        let mut facts: Vec<(String, Value)> = NAMES.iter().zip(p).map(|(n, ms)| (format!("structuregate.csharp.{}", n.replace(' ', "_")), Value::from(*ms))).collect();
        facts.push(("structuregate.files".into(), Value::from(p[4])));
        // HOW MANY REFERENCES IT OPENED, and how many a project before it had already opened - the same assembly
        // copied into another project's `bin` is one of the latter.
        facts.push(("structuregate.csharp.assemblies_opened".into(), Value::from(p[5])));
        facts.push(("structuregate.csharp.assemblies_shared".into(), Value::from(p[6])));
        crate::trace::breakdown_with(&format!("csharp project: {named}"), p[..4].iter().sum(), facts);
    }
}

/// What a note about projects that were never restored starts with - the map's summary says it as a WARNING.
pub const UNRESTORED: &str = "C# project(s) never restored";

/// A TREE WHOSE PROJECTS WERE NEVER RESTORED IS SAID TO BE ONE. Roslyn still binds it - without the NuGet
/// packages, the referenced assets or the generators - so every count looked right while hundreds of files carried
/// compile errors and every resolved column was empty. An SDK project with no `obj/project.assets.json` is one;
/// the list is kept in `_meta` as `unrestored:csharp`, so a reader of the database can tell too.
fn unrestored(into: &mut Collector, db: &str, root: &str, projects: &[String]) {
    let missing: Vec<&String> = projects.iter()
        .filter(|csproj| crate::csproj::sdk_style(csproj))
        .filter(|csproj| crate::csproj::parent(csproj).is_none_or(|folder| !Path::new(&folder).join("obj").join("project.assets.json").is_file()))
        .collect();
    let named: Vec<String> = missing.iter()
        .map(|p| Path::new(p).strip_prefix(root).map_or_else(|_| p.to_string(), |r| r.to_string_lossy().replace('\\', "/")))
        .collect();
    let Ok(conn) = rusqlite::Connection::open(db) else { return };
    let _ = conn.execute_batch("CREATE TABLE IF NOT EXISTS _meta (key TEXT, value TEXT)");
    // WRITTEN ONLY WHEN IT CHANGES: a run that changed nothing writes nothing.
    let listed = serde_json::Value::from(named.clone()).to_string();
    let kept: Option<String> = conn.query_row("SELECT value FROM _meta WHERE key = 'unrestored:csharp'", [], |r| r.get(0)).ok();
    if kept.as_deref() != Some(listed.as_str()) {
        let _ = conn.execute("DELETE FROM _meta WHERE key = 'unrestored:csharp'", []);
        let _ = conn.execute("INSERT INTO _meta (key, value) VALUES ('unrestored:csharp', ?1)", [&listed]);
    }
    if named.is_empty() {
        return;
    }
    let broken: i64 = conn.query_row("SELECT count(*) FROM files WHERE lang = 'csharp' AND errors > 0", [], |r| r.get(0)).unwrap_or(0);
    let shown: Vec<&str> = named.iter().take(3).map(String::as_str).collect();
    into.notes.push(format!(
        "{} {UNRESTORED} (no obj/project.assets.json: {}{}) - Roslyn bound them without their packages, project assets or \
         generators, and {broken} C# file(s) have compile errors; run `dotnet restore`",
        named.len(), shown.join(", "), if named.len() > 3 { ", …" } else { "" }
    ));
}

/// The SQL half: one batch, since a `.sql` file carries far fewer rows than a C# one.
pub fn sql(into: &mut Collector, db: &str, root: &str, files: &[(String, String)], stamp: &str, hashes: &Hashes, forced: bool, ask: &mut Ask) {
    let inputs = super::inputs::Inputs::open(db, hashes);
    let plan = answer(ask(&json!({ "mode": "sql-plan", "files": files })));
    let entries: Vec<&Value> = plan["files"].as_array().into_iter().flatten().collect();
    let hashed: Vec<(String, String, String)> = entries
        .par_iter()
        .zip(files.par_iter())
        .filter_map(|(entry, (rel, abs))| {
            let project = match entry["project"].as_str() {
                None => Ok("-".to_string()),
                Some(project) => inputs.content(project),
            };
            let (Ok(own), Ok(project)) = (inputs.content(abs), project) else { return None };
            Some((rel.clone(), abs.clone(), folded(&format!("{own}|{project}|{stamp}|{SQL_ROWS_VERSION}"))))
        })
        .collect();
    let mut shas = BTreeMap::new();
    let mut paths = BTreeMap::new();
    for (rel, abs, sha) in hashed {
        shas.insert(rel.clone(), sha);
        paths.insert(rel, abs);
    }
    let Some(mut state) = state(into, db, "sql", "sql") else { return };
    forget(into, &mut state, forced, "SQL", "sql");
    into.halves.insert("sql in-process (ScriptDom)".into());
    unopenable(&mut shas, &mut paths, &state);
    // NOTHING MOVED AND NOTHING LEFT, as for C#: no batch for the store to write back unchanged - seconds of a cold run.
    if stale(&shas, &state).is_empty() && state.shas.keys().all(|rel| shas.contains_key(rel)) {
        if !shas.is_empty() {
            into.notes.push(format!("the deep SQL half had nothing to do: none of its {} file(s) moved", shas.len()));
        }
        return;
    }
    let send = |into: &mut Collector, ask: &mut Ask, stale: Vec<&String>, reset: bool| -> Option<bool> {
        let question = json!({ "mode": "sql-batch", "rels": stale, "paths": paths, "shas": shas,
            "counters": state.counters, "all": stale.len() == shas.len(), "reset": reset });
        let header = answer(ask(&question));
        into.errors.extend(strings(&header["errors"]));
        pinned(&header, ask, |payload| store(into, db, root, payload, "sql")).map(|receipt| receipt.retry)
    };
    if send(into, ask, stale(&shas, &state), false) == Some(true) {
        into.notes.push("the deep SQL map disagreed with the tree and was written again in full".into());
        if send(into, ask, shas.keys().collect(), true) == Some(true) {
            into.errors.push("HALF      sql: the database still disagrees with the tree after a full rewrite".into());
        }
    }
}

/// `--map-reread <half>`: the half forgets what it recorded for this run, so every file is read again - a version that
/// changed what a half derives without moving a file, measured on a whole tree without deleting its database.
/// The rows are replaced file by file as on any run; nothing is deleted first.
fn forget(into: &mut Collector, state: &mut State, forced: bool, said: &str, flag: &str) {
    if forced {
        state.shas.clear();
        into.notes.push(format!("the deep {said} half re-reads every file: --map-reread {flag}"));
    }
}

struct State {
    rebuild: bool,
    shas: HashMap<String, String>,
    counters: Value,
}

/// What the database already holds. A store that cannot be asked stops the half: guessing an empty database
/// would rewrite rows that were fine.
fn state(into: &mut Collector, db: &str, lang: &str, language: &str) -> Option<State> {
    let parsed = calls::state(Path::new(db), lang, false)
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| format!("the database state could not be read — {e}")));
    match parsed {
        Err(error) => {
            into.errors.push(format!("HALF      {language}: the database state could not be read — {error}"));
            None
        }
        Ok(state) => Some(State {
            rebuild: state["rebuild"] == Value::Bool(true),
            shas: state["shas"].as_object().into_iter().flatten().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect(),
            counters: state["counters"].clone(),
        }),
    }
}

/// A MOVED SQL FILE THAT WILL NOT OPEN is dropped: a sha recorded without rows would fail the store's invariant on every
/// run. On every core - on Windows a first open is the file's antivirus scan. (The C# half no longer asks: its caller
/// says which files it could not read, as it reads them.)
fn unopenable(shas: &mut BTreeMap<String, String>, paths: &mut BTreeMap<String, String>, state: &State) {
    let moved = stale(shas, state);
    let shut: Vec<String> = moved.par_iter()
        .filter(|rel| paths.get(**rel).is_none_or(|abs| std::fs::File::open(abs).is_err()))
        .map(|rel| (*rel).clone())
        .collect();
    for rel in shut {
        shas.remove(&rel);
        paths.remove(&rel);
    }
}

/// The files that moved - every one when the database cannot be built on.
fn stale<'a>(shas: &'a BTreeMap<String, String>, state: &State) -> Vec<&'a String> {
    shas.iter().filter(|(rel, sha)| state.rebuild || state.shas.get(*rel) != Some(*sha)).map(|(rel, _)| rel).collect()
}

struct Receipt {
    retry: bool,
    stored: i64,
}

/// One batch stored. MAP-DONE'S COUNT IS READ BACK OUT OF THE DATABASE by the store, never counted off what
/// was sent: a half that says "I was given 101 files" proves nothing about what landed.
fn store(into: &mut Collector, db: &str, root: &str, payload: &[u8], language: &str) -> Option<Receipt> {
    received(into, calls::apply(Path::new(db), root, payload), language)
}

/// A store's receipt, into the collector: the tables written and how many files were read again.
fn received(into: &mut Collector, result: Result<String, String>, language: &str) -> Option<Receipt> {
    let receipt = match result.and_then(|r| serde_json::from_str::<Value>(&r).map_err(|e| e.to_string())) {
        Ok(receipt) => receipt,
        Err(error) => {
            into.errors.push(format!("HALF      {language}: {error}"));
            return None;
        }
    };
    for (table, rows) in receipt["written"].as_object().into_iter().flatten() {
        into.database.insert(table.clone(), rows.as_i64().unwrap_or(0));
    }
    // SUMMED: a mixed tree has several deep halves writing one database.
    into.reread = into.reread.max(0) + receipt["read"].as_i64().unwrap_or(0);
    Some(Receipt { retry: receipt["retry"].as_i64().unwrap_or(0) > 0, stored: receipt["stored"].as_i64().unwrap_or(0) })
}

/// The caller's pin on a batch, let go once the store has read it.
fn release(header: &Value, ask: &mut Ask) {
    if let Some(handle) = header["payload"]["handle"].as_i64() {
        ask(&json!({ "mode": "release", "handle": handle }));
    }
}

/// A batch's payload READ WHERE THE CALLER PINNED IT - no copy - and released once the store is done. A
/// batch is tens of megabytes; copied into native memory and again into a rust string it was held three times.
fn pinned<T>(header: &Value, ask: &mut Ask, work: impl FnOnce(&[u8]) -> T) -> T {
    let payload = &header["payload"];
    let address = payload["address"].as_i64().unwrap_or(0);
    let length = payload["length"].as_u64().unwrap_or(0) as usize;
    let bytes: &[u8] = if address == 0 || length == 0 {
        &[]
    } else {
        // SAFETY: the caller pinned `length` bytes at `address` and keeps them pinned until `release` below.
        unsafe { std::slice::from_raw_parts(address as usize as *const u8, length) }
    };
    let out = work(bytes);
    if let Some(handle) = payload["handle"].as_i64() {
        ask(&json!({ "mode": "release", "handle": handle }));
    }
    out
}

fn answer(reply: String) -> Value {
    serde_json::from_str(&reply).unwrap_or_default()
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect()
}

/// SHA-256 of a text, as 16 lowercase hex digits.
pub(super) fn folded(text: &str) -> String {
    hex16(&Sha256::digest(text.as_bytes()))
}

pub(super) fn hex16(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_folds_to_the_first_sixteen_hex_digits_of_its_sha256() {
        assert_eq!(folded("abc"), "ba7816bf8f01cfea");
    }
}
