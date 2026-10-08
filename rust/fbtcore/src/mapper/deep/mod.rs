//! THE DEEP MAP (`--map-sqlite`), EVERY HALF OF IT, and the order they run in - all in this library except
//! the two halves only .NET can parse: C# (Roslyn) and T-SQL (ScriptDom), which are asked for through the
//! caller's callback and answer with what they added.
//!
//! ONE PASS OVER EVERY ROOT: this writes a DATABASE, rebuilt rather than appended to, so a half run per root
//! left only the last root's rows behind. The paths already carry their root prefix.
//!
//! THE ORDER IS THE CONTRACT. TypeScript first, because it replaces its own rows whole; then python and rust,
//! then C#, which asks the database what it holds before it parses and needs the answer to still be true when it
//! writes; then SQL, and the links after both, since a link is a fact about two halves' rows; then what is
//! derived from the finished database - the seeds, the search index, the atlas.

mod derived;
mod driven;
mod hashes;
mod inputs;
mod payload;
pub use payload::stored_nothing;
pub use driven::UNRESTORED;
mod reasons;
mod refresh;
mod rust;
pub(crate) mod sqlconfig;
mod ts;

use super::halves;
use super::protocol::Collector;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// What the deep halves are launched with - the options that reach them, and the scripts the caller staged.
pub struct Deep {
    pub db: String,
    pub root: String,
    /// Every root with the prefix its paths carry, for the python half's roots file.
    pub roots: Vec<(String, String)>,
    pub ts_host: String,
    pub py_host: String,
    pub skip: Vec<String>,
    pub ts_node_modules: Vec<String>,
    pub ts_config: Option<String>,
    pub ts_html: Option<String>,
    pub row_fts: bool,
    pub atlas_dir: Option<String>,
    /// `--map-exclude`: globs over a C# file's map path. A match is listed and not walked - see `driven.rs`.
    pub exclude: Vec<String>,
    /// `--map-reread`: the halves that read every file again this run.
    pub reread: Vec<String>,
    /// `--sql-config`, for the caller's SQL half; beside the exe when it is not given.
    pub sql_config: Option<String>,
    /// `--facts-config`, for the facts pass; `structuregate.facts.json` beside the exe when it is not given.
    pub facts_config: Option<String>,
    /// Where the exe is: `structuregate.sql.json` is looked for beside it when `--sql-config` is not given.
    pub base_dir: String,
    /// The file-level map, when this run writes one: it changes because of the run, so the tree map prunes it.
    pub map_out: Option<String>,
    /// Which build of this tool is running - a new build may derive differently from the same rows.
    pub build: String,
    pub scripts: HashMap<String, String>,
    pub stage_errors: HashMap<String, String>,
}

/// Every file the deep map is handed, each named with its root prefix: `(rel, abs)`.
pub struct Files {
    pub python: Vec<(String, String)>,
    /// `.cs`, `.razor` and `.cshtml` - all compiled by Roslyn.
    pub sharp: Vec<(String, String)>,
    pub script: Vec<(String, String)>,
    pub sql: Vec<(String, String)>,
    pub rust: Vec<(String, String)>,
}

impl Deep {
    fn staged(&self, name: &str) -> Result<&str, &str> {
        match self.scripts.get(name) {
            Some(path) => Ok(path.as_str()),
            None => Err(self.stage_errors.get(name).map_or("not staged", String::as_str)),
        }
    }
}

/// WHAT THE DEEP MAP STARTS BEFORE ITS TURN, while the file map runs: the tree's hashes, and the python half's host
/// (`payload::Early`), which needs nothing the file map makes and was the longest wait of a `.py` edit.
pub struct Early {
    hashes: Arc<hashes::Hashes>,
    /// How long the tree's hashes took, for the run's account (`refresh.rs`).
    hashed_ms: u64,
    python: Option<payload::Early>,
    /// The plain TypeScript half, started only where there is no Angular workspace - the one tree it answers for.
    plain: Option<payload::Early>,
}

pub fn early(deep: &Deep, files: &Files) -> Early {
    let hashing = std::time::Instant::now();
    let hashes = Arc::new(tree_hashes(deep, files));
    let hashed_ms = hashing.elapsed().as_millis() as u64;
    let python = (!files.python.is_empty()).then(|| {
        let (shared, host, arguments, given) = (Arc::clone(&hashes), deep.py_host.clone(), vec![roots_key(deep)], files.python.clone());
        payload::early(&python_half(deep, &hashes, files), &files.python, &deep.db, &deep.root,
            move || key(&shared, &crate::embedded::PYROWS, &host, &arguments, &given, "", None))
    }).flatten();
    let plain = (!files.script.is_empty() && !ts::has_workspace(Path::new(&deep.root))).then(|| {
        let half = plain_half(deep, &hashes, files);
        let (shared, host, arguments, given, outside) = (Arc::clone(&hashes), deep.ts_host.clone(), half.extra.clone(), files.script.clone(), typescript(deep, &files.script));
        payload::early(&half, &files.script, &deep.db, &deep.root,
            move || key(&shared, &crate::embedded::TSPLAIN, &host, &arguments, &given, &outside, None))
    }).flatten();
    Early { hashes, hashed_ms, python, plain }
}

/// The plain TypeScript half, as it is launched - early or in its turn.
fn plain_half<'a>(deep: &'a Deep, hashes: &hashes::Hashes, files: &Files) -> payload::Half<'a> {
    payload::Half {
        language: "plain-ts rows",
        lang: "ts",
        folder: "tsplain",
        script: deep.staged("plain-ts rows"),
        host: &deep.ts_host,
        flag: "--ts-host",
        extra: deep.ts_node_modules.iter().flat_map(|m| ["--node-modules".to_string(), m.clone()]).chain(["--hashes".into(), "hashes.json".into()]).collect(),
        beside: vec![("hashes.json", handed(hashes, &files.script))],
        named: "the plain typescript half",
        key: None,
    }
}

/// WHAT EVERY FILE HOLDS, AND WHAT THE WHOLE TREE IS, from the tree map - taken once for every half.
fn tree_hashes(deep: &Deep, files: &Files) -> hashes::Hashes {
    let db = deep.db.as_str();
    let any = !files.sharp.is_empty() || !files.sql.is_empty() || !files.script.is_empty() || !files.python.is_empty()
        || !files.rust.is_empty() || Path::new(db).is_file();
    let roots: Vec<String> = deep.roots.iter().map(|(_, root)| root.clone()).collect();
    let mut avoid = vec![db];
    avoid.extend(deep.map_out.as_deref());
    let _hashing = crate::trace::stage("deep: hash the tree");
    if any { hashes::Hashes::of(&roots, &deep.skip, &avoid) } else { hashes::Hashes::of(&[], &[], &avoid) }
}

/// The python half, as it is launched - early or in its turn.
fn python_half<'a>(deep: &'a Deep, hashes: &hashes::Hashes, files: &Files) -> payload::Half<'a> {
    // EVERY ROOT, WITH ITS PREFIX: resolved against the first root only, an import from `scripts/`
    // bound to nothing, or to a path no `files` row carries.
    let roots = serde_json::Value::from(deep.roots.iter().map(|(prefix, root)| vec![prefix.clone(), root.clone()]).collect::<Vec<_>>()).to_string();
    payload::Half {
        language: "python rows",
        lang: "python",
        folder: "pyrows",
        script: deep.staged("python rows"),
        host: &deep.py_host,
        flag: "--py-host",
        extra: vec!["--roots-file".into(), "roots.json".into(), "--hashes".into(), "hashes.json".into()],
        beside: vec![("roots.json", roots), ("hashes.json", handed(hashes, &files.python))],
        named: "the python half",
        key: None,
    }
}

/// The deep map, and EVERY RUN'S ACCOUNT OF IT after - how long each step took and why each half ran (`refresh.rs`).
pub fn run(into: &mut Collector, deep: &Deep, files: &Files, ask: &mut driven::Ask, early: Option<Early>) {
    let mut clock = refresh::Clock::start();
    if let Some(replayed) = steps(into, deep, files, ask, early, &mut clock) {
        refresh::said(into, &deep.db, clock, replayed);
    }
}

/// Every half and pass, in order. Whether the passes over the finished rows were said again rather than run; None
/// when the run stopped before any half.
fn steps(into: &mut Collector, deep: &Deep, files: &Files, ask: &mut driven::Ask, early: Option<Early>, clock: &mut refresh::Clock) -> Option<bool> {
    let db = deep.db.as_str();
    let root = deep.root.as_str();
    // THE DOCUMENTS' FACTS, READ FIRST: a config that is wrong fails the run before a half has spent half a minute.
    // The config and every snapshot go into the passes' key, so a pulled document that moved is checked again
    // on a tree that did not.
    let facts = match crate::facts::config::read(deep.facts_config.as_deref(), &deep.base_dir) {
        Ok(facts) => facts,
        Err(why) => {
            into.errors.push(format!("FACTS     the facts config is wrong - {why}"));
            return None;
        }
    };
    let (hashes, python_early, plain_early) = match early {
        Some(early) => {
            clock.add("hash the tree", early.hashed_ms);
            (Arc::clone(&early.hashes), early.python, early.plain)
        }
        None => (Arc::new(clock.time("hash the tree", || tree_hashes(deep, files))), None, None),
    };
    // A TREE WITH SCRIPT FILES AND NO ANGULAR WORKSPACE is the plain half's; anywhere else its rows are
    // dropped, because they describe a tree it no longer answers for.
    // THE ANGULAR HALF IS KEYED BY ITS WORKSPACE once a run has said where that is (`fe`): node's own plan hashes
    // nothing else, so a C# edit beside the workspace launched it only to hear "unchanged".
    let angular = |fe: Option<&str>| {
        let under = fe.map(|fe| Path::new(root).join(fe));
        key(&hashes, &crate::embedded::TSROWS, &deep.ts_host, &angular_arguments(deep), &files.script, &angular_outside(deep), under.as_deref())
    };
    let typescript_stage = (!files.script.is_empty()).then(|| crate::trace::stage("deep: typescript"));
    let plain_next = !files.script.is_empty() && clock.time("typescript", || ts::run(into, deep, db, root, &angular));
    drop(typescript_stage);
    if plain_next {
        let timed = Instant::now();
        let stage = crate::trace::stage("deep: plain typescript");
        let started = plain_early.and_then(payload::Early::wait);
        stage.set("structuregate.started_early", started.is_some());
        let plain = plain_half(deep, &hashes, files);
        let key = match &started {
            Some(started) => started.key.clone(),
            None => key(&hashes, &crate::embedded::TSPLAIN, &deep.ts_host, &plain.extra, &files.script, &typescript(deep, &files.script), None),
        };
        payload::run(into, &payload::Half { key, ..plain }, &files.script, db, root, started);
        clock.add("plain typescript", since(timed));
    } else if Path::new(db).is_file() {
        payload::forget(into, "plain-ts rows", "ts", db, root);
    }

    if !files.python.is_empty() {
        let timed = Instant::now();
        let stage = crate::trace::stage("deep: python");
        stage.set("structuregate.files", files.python.len() as i64);
        let started = python_early.and_then(payload::Early::wait);
        stage.set("structuregate.started_early", started.is_some());
        let key = match &started {
            Some(started) => started.key.clone(),
            None => key(&hashes, &crate::embedded::PYROWS, &deep.py_host, &[roots_key(deep)], &files.python, "", None),
        };
        let python = payload::Half { key, ..python_half(deep, &hashes, files) };
        payload::run(into, &python, &files.python, db, root, started);
        clock.add("python", since(timed));
    }

    // IN PROCESS, by `syn`: no host to start, and an unchanged file is not parsed again. A tree whose last
    // `.rs` has gone still runs it, so the rows it recorded go too.
    {
        let stage = crate::trace::stage("deep: rust");
        stage.set("structuregate.files", files.rust.len() as i64);
        clock.time("rust", || rust::run(into, db, root, &files.rust));
    }

    // NO C# IN THE TREE IS NOT NOTHING TO DO: a database holding C# rows holds them for files that have just
    // left, so the half runs once with nothing in it and the store drops them.
    if !files.sharp.is_empty() || Path::new(db).is_file() {
        let stage = crate::trace::stage("deep: csharp");
        stage.set("structuregate.files", files.sharp.len() as i64);
        let roots: Vec<String> = deep.roots.iter().map(|(_, dir)| dir.clone()).collect();
        clock.time("csharp", || driven::csharp(into, db, root, &files.sharp, &hashes, &deep.exclude, &roots, &deep.skip, deep.reread.iter().any(|h| h == "csharp"), ask));
    }
    // THE SQL HALF AFTER C#, AND THE LINKS AFTER BOTH: a link is a fact about two halves' rows. A database
    // that held SQL rows keeps being told when the last `.sql` has gone, as the C# half is.
    let timed = Instant::now();
    let config = sqlconfig::read(deep.sql_config.as_deref(), &deep.base_dir);
    ask(&json!({ "mode": "sql-open", "config": config }));
    if !files.sql.is_empty() || Path::new(db).is_file() {
        let stage = crate::trace::stage("deep: sql");
        stage.set("structuregate.files", files.sql.len() as i64);
        driven::sql(into, db, root, &files.sql, config["stamp"].as_str().unwrap_or("none"), &hashes, deep.reread.iter().any(|h| h == "sql"), ask);
    }
    clock.add("sql", since(timed));
    let facts_stamp = facts.as_ref().map_or("none", |f| f.stamp.as_str());
    // THE PASSES OVER THE FINISHED ROWS, not run again over the same rows - see derived.rs.
    let timed = Instant::now();
    let keying = crate::trace::stage("deep: key the passes over the finished rows");
    let key = match derived::key(db, config["stamp"].as_str().unwrap_or("none"), facts_stamp, &deep.build, deep.row_fts, deep.atlas_dir.as_deref()) {
        Ok(key) => Some(key),
        Err(why) => {
            into.notes.push(format!("the passes over the finished rows run every time: they could not be keyed - {why}"));
            None
        }
    };
    let replayed = key.as_ref().is_some_and(|k| derived::replay(into, db, k, deep.atlas_dir.as_deref()));
    drop(keying);
    if replayed {
        crate::trace::set("structuregate.deep.derived_replayed", true);
        clock.add("passes over the finished rows (replayed)", since(timed));
        return Some(true);
    }
    let derived_stage = crate::trace::stage("deep: links, seeds, index");
    let before = derived::before(into);
    // EACH PASS ITS OWN SPAN: their sum was one number, and it grew several times over on a large tree with nothing to say which.
    // THE SEEDS FIRST: dynamic SQL that creates an object per seeded row is linked by those rows.
    pass("pass: sql seeds and typed views", || seeds(into, db));
    {
        let _pass = crate::trace::stage("pass: sql links");
        let answer = serde_json::from_str(&ask(&json!({ "mode": "sql-links", "notes": into.notes, "db": db }))).unwrap_or_default();
        halves::deep(into, &answer);
    }
    pass("pass: duplicate types", || duplicates(into, db));
    if let Some(facts) = &facts {
        pass("pass: document facts", || documented(into, db, facts));
    }
    // AFTER EVERY HALF: an index over whichever half ran last would answer about a fraction of the map.
    if deep.row_fts {
        pass("pass: row search index", || search(into, db));
    }
    if let Some(dir) = &deep.atlas_dir {
        pass("pass: atlas", || atlas(into, db, dir));
    }
    // WHAT THE PASSES WROTE, COUNTED AS THE STORE COUNTS: the store's receipt was taken before they made their
    // tables, so a first run's `database` line left out sql_links and sql_seeds that every later run counts.
    pass("pass: count the tables", || tally(into, db));
    if let Some(key) = key {
        pass("pass: keep the key", || derived::remember(into, db, &key, before));
    }
    drop(derived_stage);
    clock.add("passes over the finished rows", since(timed));
    Some(false)
}

fn since(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

/// One pass over the finished rows, as a span of its own.
fn pass(name: &str, work: impl FnOnce()) {
    let _span = crate::trace::stage(name);
    work();
}

/// Every table the store would list that nothing here has counted yet - and the two the passes REWRITE, whose
/// count from an earlier receipt is the count before they ran.
fn tally(into: &mut Collector, db: &str) {
    let Ok(conn) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { return };
    for table in crate::rows::schema::listable_tables(&conn).unwrap_or_default() {
        if !into.database.contains_key(&table) || ["sql_links", "sql_seeds", "duplicate_types", "doc_facts", "doc_links"].contains(&table.as_str()) {
            let rows = crate::rows::schema::count_of(&conn, &table);
            into.database.insert(table, rows);
        }
    }
}

/// THE DEEP MAP ON ITS OWN - `--map-sqlite` without `--map`, what a per-turn hook runs. A HALF THAT FAILED
/// EXITS NON-ZERO: there is no graph here to report it against.
pub fn only(into: &mut Collector, deep: &Deep, files: &Files, ask: &mut driven::Ask, out: &mut Vec<(&str, String)>) -> i64 {
    run(into, deep, files, ask, None);
    for note in &into.notes {
        out.push(("o", format!("  note      {note}")));
    }
    for error in &into.errors {
        out.push(("e", format!("  error: {error}")));
    }
    let rows: i64 = into.database.values().filter(|n| **n > 0).sum();
    let reread = if into.reread >= 0 { format!(", {} file(s) re-read", into.reread) } else { String::new() };
    out.push(("o", format!("  database     {rows} row(s) in {} table(s){reread} -> {}", into.database.len(), deep.db)));
    i64::from(!into.errors.is_empty())
}

/// WHAT THE DEPLOY SCRIPTS WRITE INTO A TABLE, walked from the SQL half's rows.
fn seeds(into: &mut Collector, db: &str) {
    match crate::rows::seeds::run(db) {
        Err(why) => into.errors.push(format!("SQL SEEDS were not written — {why}")),
        Ok(answer) if answer.get("skipped").is_some() => {}
        Ok(answer) => {
            let counts: Vec<String> = answer["counts"].as_array().into_iter().flatten()
                .map(|c| format!("{} {}", c[1].as_i64().unwrap_or(0), c[0].as_str().unwrap_or("")))
                .collect();
            into.notes.push(format!(
                "the sql seeds: {} ({} row(s) in sql_seeds, {} typed view(s) seed_<schema>_<table>)",
                counts.join(", "),
                answer["total"].as_i64().unwrap_or(0),
                answer["views"].as_i64().unwrap_or(0)
            ));
        }
    }
}

/// A TYPE NAME DECLARED IN MORE THAN ONE PLACE - listed, never merged: see `rows/dups.rs`.
fn duplicates(into: &mut Collector, db: &str) {
    // ONE TRANSACTION: each row its own commit was an fsync per row on a large database - tens of seconds for a few thousand names.
    let written = rusqlite::Connection::open(db).and_then(|mut conn| {
        let tx = conn.transaction()?;
        let n = crate::rows::dups::write(&tx)?;
        tx.commit()?;
        Ok(n)
    });
    match written {
        Ok(0) => {}
        Ok(n) => into.notes.push(format!("{n} type name(s) are declared in more than one place (duplicate_types)")),
        Err(why) => into.errors.push(format!("DUPLICATES were not written — {why}")),
    }
}

/// The facts of the consumer's documents, and how each list stands against the code - see `facts/`.
fn documented(into: &mut Collector, db: &str, facts: &crate::facts::config::Config) {
    match crate::facts::run(db, facts) {
        Err(why) => into.errors.push(format!("FACTS     were not written — {why}")),
        Ok(made) => {
            into.notes.extend(made.notes);
            let [bound, in_doc_only, in_code_only] = made.links;
            into.notes.push(format!(
                "the document facts: {} row(s) in doc_facts; doc_links {bound} bound, {in_doc_only} missing in code, {in_code_only} missing in doc",
                made.facts
            ));
        }
    }
}

/// The search index over every row, on request. A MISSING DATABASE OR A SQLITE WITHOUT FTS5 IS A NOTE: the
/// map is complete either way.
fn search(into: &mut Collector, db: &str) {
    let receipt = match crate::rows::calls::search(Path::new(db), false) {
        Ok(receipt) => receipt,
        Err(error) => return into.errors.push(format!("SEARCH    the row index was not built — {error}")),
    };
    match serde_json::from_str::<Value>(&receipt) {
        Err(e) => into.errors.push(format!("SEARCH    the row index receipt could not be read — {e}")),
        Ok(receipt) => match receipt["skipped"].as_str() {
            Some(why) => into.notes.push(format!("the row search index was not built: {why}")),
            None => into.notes.push(format!("the row search index holds {} row(s)", receipt["rows"].as_i64().unwrap_or(0))),
        },
    }
}

/// The overview, on request. A DATABASE WITH NO ANGULAR IN IT IS A NOTE, NOT AN ERROR.
fn atlas(into: &mut Collector, db: &str, dir: &str) {
    let receipt = match crate::rows::calls::atlas(Path::new(db), dir) {
        Ok(receipt) => receipt,
        Err(error) => return into.errors.push(format!("ATLAS     was not written — {error}")),
    };
    match serde_json::from_str::<Value>(&receipt) {
        Err(e) => into.errors.push(format!("ATLAS     receipt could not be read — {e}")),
        Ok(receipt) => match receipt["skipped"].as_str() {
            Some(why) => into.notes.push(format!("the atlas was not written: {why}")),
            None => into.notes.push(format!(
                "the atlas covers {} project(s), {} route(s) and names {} boundary(ies) -> {dir}",
                receipt["projects"].as_i64().unwrap_or(0),
                receipt["routes"].as_i64().unwrap_or(0),
                receipt["boundaries"].as_i64().unwrap_or(0)
            )),
        },
    }
}

/// `{rel: content hash}` for every file the tree map has hashed, as JSON: the host reads only the files it parses.
fn handed(hashes: &hashes::Hashes, files: &[(String, String)]) -> String {
    let handed: serde_json::Map<String, serde_json::Value> =
        files.iter().filter_map(|(rel, abs)| Some((rel.clone(), hashes.get(abs)?.into()))).collect();
    serde_json::Value::Object(handed).to_string()
}

/// A payload half's key: the whole tree, the files it is GIVEN (`--ext` and `--skip` change those without
/// touching the tree), its scripts, its host's own version, its arguments and anything else it reads from
/// outside the tree. None when the tree map could not answer, and the half then runs. `under` narrows "the whole
/// tree" to one folder of it; a folder the tree map does not hold is the whole tree again.
#[allow(clippy::too_many_arguments)]
fn key(hashes: &hashes::Hashes, set: &crate::embedded::Set, host: &str, arguments: &[String], given: &[(String, String)], outside: &str, under: Option<&Path>) -> Option<String> {
    let scoped = under.and_then(|dir| hashes.under(dir)).map(|files| format!("under:{files}"));
    // NO WORKSPACE KNOWN: what the half reads (`Hashes::scoped`), never the whole tree - see there.
    let read = scoped.is_none().then(|| hashes.scoped(given, project_file)).flatten();
    let tree = match (&scoped, &read) {
        (Some(scoped), _) => scoped.as_str(),
        (None, Some(read)) => read.as_str(),
        (None, None) => hashes.tree()?,
    };
    let mut list = blake3::Hasher::new();
    for (rel, abs) in given {
        list.update(rel.as_bytes()).update(b"	").update(abs.as_bytes()).update(b"
");
    }
    let tree = format!("{tree}|{}", &list.finalize().to_hex()[..16]);
    let version = crate::hosts::process(host, &["--version".to_string()], ".", &[]).ok()
        .filter(|ran| ran.exit == 0)
        .map(|ran| format!("{}{}", ran.stdout.trim(), ran.stderr.trim()))?;
    let text = format!("{tree}|{}|{host}|{version}|{}|{outside}", crate::embedded::digest(set), arguments.join(" "));
    Some(blake3::hash(text.as_bytes()).to_hex()[..32].to_string())
}

/// A file a half reads besides the ones it is given: which compiler, which project, which workspace, which launcher.
fn project_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    (name.starts_with("tsconfig") && name.ends_with(".json"))
        || ["package.json", "package-lock.json", "pnpm-lock.yaml", "yarn.lock", "npm-shrinkwrap.json", "angular.json",
            "workspace.json", "project.json", "nx.json", "structuregate.ts.json", "pyproject.toml"].contains(&name)
        || name.ends_with(".spec")
}

/// The roots the python half resolves imports against, as its key sees them.
fn roots_key(deep: &Deep) -> String {
    deep.roots.iter().map(|(prefix, root)| format!("{prefix}={root}")).collect::<Vec<_>>().join(";")
}

/// WHICH `typescript` THE PLAIN HALF BORROWS, which lives in a `node_modules` the tree map does not walk: the
/// `package.json` of every candidate node resolves from - the root's own and its ancestors', then
/// `--ts-node-modules` - hashed, so a compiler upgrade re-runs the half.
fn typescript(deep: &Deep, files: &[(String, String)]) -> String {
    let mut candidates: Vec<std::path::PathBuf> = Path::new(&deep.root).ancestors().map(|at| at.join("node_modules")).collect();
    candidates.extend(deep.ts_node_modules.iter().map(std::path::PathBuf::from));
    // AND EVERY PACKAGE FOLDER UNDER THE ROOT that a file sits in - where the half also looks for a compiler, so
    // upgrading the one in `web/` runs the half again.
    let mut folders: Vec<std::path::PathBuf> = files.iter()
        .filter_map(|(_, abs)| Path::new(abs).ancestors().skip(1).take_while(|at| at.starts_with(&deep.root) && *at != Path::new(&deep.root))
            .find(|at| at.join("package.json").is_file()).map(Path::to_path_buf))
        .collect();
    folders.sort();
    folders.dedup();
    candidates.extend(folders.into_iter().map(|folder| folder.join("node_modules")));
    candidates.iter()
        .map(|modules| modules.join("typescript").join("package.json"))
        .filter_map(|package| std::fs::read(&package).ok().map(|bytes| blake3::hash(&bytes).to_hex()[..16].to_string()))
        .collect::<Vec<_>>()
        .join("+")
}

/// The Angular half's arguments, as its key sees them.
fn angular_arguments(deep: &Deep) -> Vec<String> {
    let mut arguments = deep.ts_node_modules.clone();
    arguments.extend(deep.skip.iter().cloned());
    arguments.extend(deep.ts_config.iter().cloned());
    arguments.extend(deep.ts_html.iter().cloned());
    arguments
}

/// WHAT THE ANGULAR HALF READS FROM OUTSIDE THE TREE: the borrowed `typescript` and `@angular/compiler` (in a
/// `node_modules` the tree map does not walk) and `structuregate.ts.json`, which lives beside the workspace or
/// above it - each hashed, so a compiler upgrade or a config edit runs the half again.
fn angular_outside(deep: &Deep) -> String {
    let mut modules: Vec<std::path::PathBuf> = Path::new(&deep.root).ancestors().map(|at| at.join("node_modules")).collect();
    modules.extend(deep.ts_node_modules.iter().map(std::path::PathBuf::from));
    let mut candidates: Vec<std::path::PathBuf> = modules.iter()
        .flat_map(|m| [m.join("typescript").join("package.json"), m.join("@angular").join("compiler").join("package.json")])
        .collect();
    match &deep.ts_config {
        Some(config) => candidates.push(std::path::PathBuf::from(config)),
        None => candidates.extend(Path::new(&deep.root).ancestors().map(|at| at.join("structuregate.ts.json"))),
    }
    candidates.iter()
        .filter_map(|file| std::fs::read(file).ok().map(|bytes| format!("{}={}", file.display(), &blake3::hash(&bytes).to_hex()[..16])))
        .collect::<Vec<_>>()
        .join("+")
}

