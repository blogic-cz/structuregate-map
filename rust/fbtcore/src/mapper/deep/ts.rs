//! THE TYPESCRIPT (ANGULAR) HALF OF THE DEEP MAP: node parses with the workspace's own `typescript` and
//! `@angular/compiler`, and the store in this library writes. The rows travel as a FILE - hundreds of
//! megabytes over a real workspace, more than a pipe or a command line carries.
//!
//! THE ORDER IS STATE, PLAN, PARSE, STORE. The ids are asked for first, because a run that started numbering
//! at 1 again would hand a second row the id a joined row already points at. The plan costs a hash walk and
//! no type checker, so a tree that has not moved stops here.
//!
//! A TREE WITH NO ANGULAR WORKSPACE IN IT IS NOT A FAILURE - it is the plain half's - and a workspace whose
//! dependencies are not installed is: a map silently missing every TypeScript row looks exactly like a
//! repository with no TypeScript in it.

use super::super::protocol::Collector;
use super::Deep;
use crate::hosts;
use crate::rows::calls;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(super) const LANGUAGE: &str = "typescript rows";
/// What the `files` rows of this half are stamped with; `LANGUAGE` names the half in a report.
pub(super) const LANG: &str = "typescript";

/// What the plan came to. `Absent` is a tree with no Angular workspace - a stop, and not this half's tree.
#[derive(PartialEq)]
pub(super) enum Planned {
    Full,
    Partial,
    Stop,
    Absent,
}

/// WHETHER THE HALF RUNS. A tree that holds NO Angular workspace is the one outcome after which the plain
/// TypeScript half answers for the tree instead.
///
/// NOT LAUNCHED AT ALL when `key` - the whole tree, the scripts, node, the borrowed compilers and the config -
/// is the key recorded after its last clean run: node would hash the whole workspace only to say it has not
/// moved. The outcome is kept with the key, so a tree with no workspace is still handed to the plain half.
///
/// `key` is asked with the WORKSPACE folder (relative to the root) the last run reported, and keys by that
/// folder alone; asked with none, by the whole tree. It is asked again after a run, with what that run reported.
///
/// Ok with whether the tree has no workspace, when the half is not launched; Err with the workspace the last run
/// reported, when it has to be - beside the halves after it (`flight.rs`) or in its turn (`grounded`).
pub(super) fn skipped(into: &mut Collector, deep: &Deep, db: &str, root: &str, key: &dyn Fn(Option<&str>) -> Option<String>) -> Result<bool, Option<String>> {
    // NO WORKSPACE IS ANSWERED HERE, before the key - which asks node its version - and before node: every `.ts`
    // edit in a tree with no Angular in it started node twice to hear the same "not here" again.
    if !has_workspace(Path::new(root)) {
        into.notes.push(format!("the typescript half did not run: no Angular workspace (angular.json / nx.json / workspace.json) at or under {root}"));
        return Ok(true);
    }
    // EVERY STEP OF THIS HALF IS A SPAN, as the C# half's are: over a minute of a full run was in none.
    let keying = crate::trace::stage("typescript: key the half");
    let kept = super::payload::recorded(db, LANG).and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let fe_kept = kept.as_ref().and_then(|k| k["fe"].as_str().map(String::from));
    let asked = key(fe_kept.as_deref());
    drop(keying);
    let forced = deep.reread.iter().any(|h| h == "typescript");
    if let Some(key) = asked.filter(|_| !forced)
        && let Some(kept) = &kept
        && kept["key"].as_str() == Some(key.as_str())
    {
        into.notes.push("the typescript half had nothing to do: no file under its roots moved since its rows were written".into());
        into.notes.extend(kept["notes"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(String::from)));
        return Ok(kept["absent"] == Value::Bool(true));
    }
    Err(fe_kept)
}

/// The half launched here and now, after whatever ran before it - and its key recorded after a clean run.
pub(super) fn grounded(into: &mut Collector, deep: &Deep, db: &str, root: &str, key: &dyn Fn(Option<&str>) -> Option<String>, fe_kept: Option<String>) -> bool {
    let (errors, notes) = (into.errors.len(), into.notes.len());
    let mut fe = None;
    let absent = launched(into, deep, db, root, &mut fe, &[false, true]);
    let absence = absence(&into.notes[notes..]);
    settle(db, key, absent, fe, fe_kept, into.errors.len() == errors, absence);
    absent
}

/// What is kept of a run's notes with its key: the one that says there is no workspace - the rest describe the work,
/// and a skipped run did none.
pub(super) fn absence(notes: &[String]) -> Vec<String> {
    notes.iter().filter(|n| n.starts_with("the typescript half did not run")).cloned().collect()
}

/// RECORDED ONLY AFTER A CLEAN RUN, so an error is said again next time. A run that stored nothing (the plan found the
/// tree unchanged) keeps the workspace it was told before.
pub(super) fn settle(db: &str, key: &dyn Fn(Option<&str>) -> Option<String>, absent: bool, fe: Option<String>, fe_kept: Option<String>,
    clean: bool, absence: Vec<String>) {
    let fe = if absent { None } else { fe.or(fe_kept) };
    let keying = crate::trace::stage("typescript: key the half");
    let asked = key(fe.as_deref());
    drop(keying);
    if let Some(key) = asked
        && clean
    {
        super::payload::record(db, LANG, &json!({ "key": key, "absent": absent, "notes": absence, "fe": fe }).to_string());
    }
}

/// WHETHER NODE WOULD FIND A WORKSPACE - `findWorkspaceRoot` in `TsProjects.mjs`, walked the same way: a marker at
/// the root, or in a folder up to four below it, never inside `node_modules` or a dot folder, and a link to a folder
/// not followed. Rust never passes `--fe`, so this walk is the whole of node's answer; `pub(super)` for the early
/// plain half, which starts only where there is no workspace.
pub(super) fn has_workspace(root: &Path) -> bool {
    const MARKS: [&str; 3] = ["angular.json", "nx.json", "workspace.json"];
    fn marked(dir: &Path) -> bool {
        MARKS.iter().any(|mark| dir.join(mark).exists())
    }
    fn walk(dir: &Path, depth: i32) -> bool {
        if depth < 0 {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !entry.file_type().is_ok_and(|t| t.is_dir()) || name == "node_modules" || name.starts_with('.') {
                return false;
            }
            let path = entry.path();
            marked(&path) || walk(&path, depth - 1)
        })
    }
    marked(root) || walk(root, 3)
}

/// State, then plan, parse and store - over `hops`: a run that stopped short of every hop and found it could not
/// (`MAP-RETRY`, or a kept row that would point at nothing) wrote nothing, and is asked once more over every hop - see
/// `rows/partial/reads.rs`. `[true]` is that second ask alone.
pub(super) fn launched(into: &mut Collector, deep: &Deep, db: &str, root: &str, fe: &mut Option<String>, hops: &[bool]) -> bool {
    let Some(parse) = staged(into, deep) else { return false };
    let handing = crate::trace::stage("typescript: read and hand over the state");
    let Some(state) = state(into, deep, db) else { return false };
    let work = Work::new();
    let written = std::fs::write(&work.state, &state);
    drop(handing);
    let mut absent = false;
    match written {
        Err(e) => into.errors.push(format!("HALF      {LANGUAGE}: the payload could not be written ({e})")),
        Ok(()) => {
            for &every_hop in hops {
                let mut carried = None;
                let planned = plan(into, deep, &parse, root, db, &work, every_hop, &mut carried);
                absent = planned == Planned::Absent;
                if planned != Planned::Full && planned != Planned::Partial {
                    break;
                }
                let carry = (planned == Planned::Partial).then_some(&work.carry);
                let retry = match parse_tree(into, deep, &parse, root, db, &work, carry, every_hop, &|| {}) {
                    Parsed::Rows => store(into, db, root, &work.rows, carried, fe),
                    Parsed::Retry => true,
                    Parsed::Nothing => false,
                };
                if !retry || every_hop {
                    break;
                }
            }
        }
    }
    work.clean();
    absent
}

/// Where node's parse script was staged; None, said, when it was not.
pub(super) fn staged(into: &mut Collector, deep: &Deep) -> Option<String> {
    match deep.scripts.get("tsrows-node") {
        Some(path) => Some(path.clone()),
        None => {
            let why = deep.stage_errors.get("tsrows-node").map_or("not staged", String::as_str);
            into.errors.push(format!("HALF      {LANGUAGE}: could not be staged ({why})"));
            None
        }
    }
}

/// WHAT NODE HAS TO KNOW BEFORE IT PARSES, read from the database; None, said, when it cannot be.
pub(super) fn state(into: &mut Collector, deep: &Deep, db: &str) -> Option<String> {
    match calls::ts_state(Path::new(db), LANG) {
        // `--map-reread typescript`: node compares the setup it would write with the one recorded, and a setup that
        // differs makes it read every file - so the recorded one is handed over as one no run ever wrote.
        Ok(state) if deep.reread.iter().any(|h| h == "typescript") => {
            into.notes.push("the typescript half re-reads every file: --map-reread typescript".into());
            let mut held: Value = serde_json::from_str(&state).unwrap_or_default();
            held["setup"] = Value::from("--map-reread");
            Some(held.to_string())
        }
        Ok(state) => Some(state),
        // A FAILURE HERE STOPS THE HALF: guessing an empty database would hand out ids already given away.
        Err(error) => {
            into.errors.push(format!("HALF      {LANGUAGE}: the database state could not be read — {error}"));
            None
        }
    }
}

pub(super) struct Work {
    dir: PathBuf,
    pub(super) state: PathBuf,
    pub(super) rows: PathBuf,
    plan: PathBuf,
    pub(super) carry: PathBuf,
    lines: PathBuf,
}

impl Work {
    pub(super) fn new() -> Work {
        let dir = crate::hosts::temp_dir().join(format!("structuregate-tsrows-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        Work { state: dir.join("state.json"), rows: dir.join("rows.json"), plan: dir.join("plan.json"), carry: dir.join("carry.json"),
            lines: dir.join("lines.json"), dir }
    }

    pub(super) fn clean(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `{path: [sha, lines]}` of every file this half recorded with a line count.
fn counted(db: &str) -> String {
    let mut out = serde_json::Map::new();
    if let Ok(conn) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        && let Ok(mut rows) = conn.prepare("SELECT path, sha, lines FROM files WHERE lang = ?1 AND sha IS NOT NULL AND lines IS NOT NULL")
    {
        let read = rows.query_map([LANG], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)));
        for (path, sha, lines) in read.into_iter().flatten().flatten() {
            out.insert(path, json!([sha, lines]));
        }
    }
    Value::Object(out).to_string()
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The arguments both launches share after their own: node modules, the gate's `--skip` (on a large Angular workspace the
/// `.angular` cache, `dist` and `.nx` were most of the "source" files) and the config.
fn shared(deep: &Deep, arguments: &mut Vec<String>) {
    for modules in &deep.ts_node_modules {
        arguments.extend(["--node-modules".into(), modules.clone()]);
    }
    // THE TREE MAP'S OWN STATE IS NOT THE WORKSPACE: `.fbt/` changes because the gate and the deep map ran,
    // and a workspace fingerprint that read it would call every tree moved and rebuild the half in full.
    let mut skip = deep.skip.clone();
    if !skip.iter().any(|s| s.eq_ignore_ascii_case(".fbt")) {
        skip.push(".fbt".into());
    }
    skip.sort();
    for dir in skip {
        arguments.extend(["--skip-dir".into(), dir]);
    }
    if let Some(config) = &deep.ts_config {
        arguments.extend(["--config".into(), config.clone()]);
    }
}

/// ASK WHAT WOULD HAVE TO BE READ, then fetch the rows that would not be. A PLAN THAT CANNOT BE READ IS A
/// FULL RUN, never a partial one: a fraction read because the answer was unavailable would be written over
/// a whole map. How long the carry took goes to `carried`, for the note `store` writes.
#[allow(clippy::too_many_arguments)]
pub(super) fn plan(into: &mut Collector, deep: &Deep, parse: &str, root: &str, db: &str, work: &Work, every_hop: bool, carried: &mut Option<u64>) -> Planned {
    let mut arguments = vec![parse.to_string(), "--root".into(), root.into(), "--rows".into(), text(&work.rows),
        "--state".into(), text(&work.state), "--plan".into(), text(&work.plan), "--db".into(), db.into()];
    if every_hop {
        arguments.push("--every-hop".into());
    }
    shared(deep, &mut arguments);
    // NOTHING LEFT BY AN EARLIER PROCESS OF THE SAME ID: the parse run takes its hashes from this file.
    let _ = std::fs::remove_file(&work.plan);
    let launch = crate::trace::stage("typescript: plan (node)");
    let ran = hosts::process(&deep.ts_host, &arguments, root, &[]);
    drop(launch);
    let ran = match ran {
        Ok(ran) => ran,
        Err(why) => {
            into.errors.push(format!("HALF      {LANGUAGE}: `{}` did not run ({why}) — pass --ts-host with a node that exists, or drop --map-sqlite", deep.ts_host));
            return Planned::Stop;
        }
    };
    if let Some(skipped) = records(&ran.stdout, "MAP-SKIP|").into_iter().next() {
        into.notes.push(format!("the typescript half did not run: {skipped}"));
        return Planned::Absent;
    }
    if !work.plan.is_file() {
        into.notes.push(format!("the typescript half is reading everything: no plan was written (exit {})", ran.exit));
        return Planned::Full;
    }
    let read = std::fs::read_to_string(&work.plan).map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()));
    let plan = match read {
        Ok(plan) => plan,
        Err(why) => {
            into.notes.push(format!("the typescript half is reading everything: the plan could not be read ({why})"));
            return Planned::Full;
        }
    };
    // NOTHING CHANGED IS AN ANSWER: this half is a whole-workspace rebuild.
    if plan["unchanged"] == Value::Bool(true) {
        into.notes.push("the typescript half had nothing to do: the tree has not moved since the map was written".into());
        return Planned::Stop;
    }
    if plan["full"] == Value::Bool(true) {
        why(into, &plan["why"], "everything");
        return Planned::Full;
    }
    why(into, &plan["why"], "what moved");
    // A PARTIAL RUN WITHOUT THE REST OF THE ROWS IS NOT A PARTIAL RUN: it reads everything instead.
    let carry_started = std::time::Instant::now();
    let fetched = calls::carry(Path::new(db), LANG, &text(&work.plan), &text(&work.carry));
    *carried = Some(carry_started.elapsed().as_millis() as u64);
    let receipt = match fetched {
        Ok(receipt) if work.carry.is_file() => receipt,
        Ok(_) => {
            into.notes.push("the typescript half is reading everything: the rows it would not have re-extracted could not be fetched ()".into());
            return Planned::Full;
        }
        Err(why) => {
            into.notes.push(format!("the typescript half is reading everything: the rows it would not have re-extracted could not be fetched ({why})"));
            return Planned::Full;
        }
    };
    match serde_json::from_str::<Value>(&receipt) {
        Ok(receipt) => into.notes.push(format!(
            "the typescript half kept {} row(s) it does not have to re-extract, across {} table(s)",
            receipt["rows"].as_i64().unwrap_or(0),
            receipt["tables"].as_i64().unwrap_or(0)
        )),
        Err(e) => into.notes.push(format!("the typescript half kept rows it does not have to re-extract, and could not say how many ({e})")),
    }
    Planned::Partial
}

/// WHY THE HALF RUNS, said on every run that does and kept for `_meta.last_refresh`: a pull that moved a hundred-odd `.ts`
/// and `.html` files cost minutes here, and the log said only how many files were re-read across every half.
fn why(into: &mut Collector, why: &Value, reads: &str) {
    // Said ONCE: a run asked again over every hop plans again over the same tree.
    if !why.is_object() || into.refresh.contains_key("typescript") {
        return;
    }
    let mut exts: Vec<(&String, i64)> = why["exts"].as_object().into_iter().flatten().map(|(e, n)| (e, n.as_i64().unwrap_or(0))).collect();
    exts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let shown: Vec<String> = exts.iter().take(6).map(|(e, n)| format!("{e} {n}")).collect();
    let mut causes = vec![format!("{} file(s) moved since its rows were written{}", why["changed"].as_i64().unwrap_or(0),
        if shown.is_empty() { String::new() } else { format!(" ({})", shown.join(", ")) })];
    if why["recorded"].as_i64() == Some(0) {
        causes.push("no rows of it were recorded before".into());
    }
    if why["setup_moved"] == Value::Bool(true) {
        causes.push("its setup moved (typescript, angular, node, its config or its rows version)".into());
    }
    if why["template"].as_i64().unwrap_or(0) > 0 {
        causes.push(format!("{} of them are new or can move what a template resolves", why["template"]));
    }
    into.notes.push(format!("the typescript half reads {reads}: {}", causes.join("; ")));
    let mut kept = why.as_object().cloned().unwrap_or_default();
    kept.insert("reads".into(), Value::from(if reads == "everything" { "all" } else { "partial" }));
    into.refresh.insert("typescript".into(), Value::Object(kept));
}

/// What a parse run left: a payload to store, nothing, or a run that stopped short and has to be asked again.
pub(super) enum Parsed {
    Rows,
    Nothing,
    Retry,
}

/// node, with the workspace's own compiler. `read` is told once the last read of the database is behind it, before node starts.
#[allow(clippy::too_many_arguments)]
pub(super) fn parse_tree(into: &mut Collector, deep: &Deep, parse: &str, root: &str, db: &str, work: &Work, carry: Option<&PathBuf>, every_hop: bool,
    read: &dyn Fn()) -> Parsed {
    // `--db`: WHERE ITS OWN OUTPUT GOES, so a database inside the tree is not read as a file of it.
    let mut arguments = vec![parse.to_string(), "--root".into(), root.into(), "--rows".into(), text(&work.rows),
        "--state".into(), text(&work.state), "--db".into(), db.into()];
    if let Some(carry) = carry {
        arguments.extend(["--carry".into(), text(carry)]);
    }
    if every_hop {
        arguments.push("--every-hop".into());
    }
    // The plan's own hashes: node does not walk the tree a second time to reach the answer it just gave.
    if work.plan.is_file() {
        arguments.extend(["--hashes".into(), text(&work.plan)]);
    }
    // WHAT EVERY FILE HELD AND HOW MANY LINES IT HAD: the inventory counts again only a file whose hash moved.
    if std::fs::write(&work.lines, counted(db)).is_ok() {
        arguments.extend(["--lines".into(), text(&work.lines)]);
    }
    read();
    shared(deep, &mut arguments);
    if let Some(html) = &deep.ts_html {
        arguments.extend(["--html".into(), html.clone()]);
    }
    // ROOM FOR THE PAYLOAD: hundreds of MB of JSON on a workspace of thousands of files, and node's default ceiling killed it
    // (`exit 134`) after minutes of parsing. Through the environment, since only node reads it.
    let environment = [("NODE_OPTIONS".to_string(), "--max-old-space-size=8192".to_string())];
    let launch = crate::trace::stage("typescript: parse (node)");
    let ran = hosts::process(&deep.ts_host, &arguments, root, &environment);
    drop(launch);
    let ran = match ran {
        Ok(ran) => ran,
        Err(why) => {
            into.errors.push(format!("HALF      {LANGUAGE}: `{}` did not run ({why}) — pass --ts-host with a node that exists, or drop --map-sqlite", deep.ts_host));
            return Parsed::Nothing;
        }
    };
    if let Some(why) = records(&ran.stdout, "MAP-RETRY|").into_iter().next() {
        into.notes.push(format!("the typescript half is reading every hop of dependents again: {why}"));
        return Parsed::Retry;
    }
    into.halves.insert(format!("{LANGUAGE} via {}", deep.ts_host));
    into.notes.extend(records(&ran.stdout, "MAP-NOTE|"));
    for toolchain in records(&ran.stdout, "MAP-TOOLCHAIN|") {
        into.notes.push(format!("the typescript half parsed with {}", toolchain.replace('|', ", ")));
    }
    if let Some(skipped) = records(&ran.stdout, "MAP-SKIP|").into_iter().next() {
        into.notes.push(format!("the typescript half did not run: {skipped}"));
        return Parsed::Nothing;
    }
    if let Some(unchanged) = records(&ran.stdout, "MAP-UNCHANGED|").into_iter().next() {
        into.notes.push(format!("the typescript half had nothing to do: {unchanged} file(s) unchanged since the map was written"));
        return Parsed::Nothing;
    }
    if let Some(fatal) = records(&ran.stdout, "MAP-FATAL|").into_iter().next() {
        into.errors.push(format!("HALF      {LANGUAGE}: {fatal}"));
        return Parsed::Nothing;
    }
    if !ran.stdout.contains("MAP-DONE|") {
        into.errors.push(format!("HALF      {LANGUAGE}: node died half way (exit {}) — {}", ran.exit, hosts::tail(&(ran.stderr.clone() + &ran.stdout))));
        return Parsed::Nothing;
    }
    if work.rows.is_file() { Parsed::Rows } else { Parsed::Nothing }
}

/// THE ROWS, THE CLOSURE DERIVED FROM THEM AND THE GRAPH THE NEXT RUN PLANS WITH, stored in this process.
/// True when nothing was stored because a run that stopped short would have left a kept row pointing at nothing.
pub(super) fn store(into: &mut Collector, db: &str, root: &str, rows: &Path, carried: Option<u64>, fe: &mut Option<String>) -> bool {
    // ONE SPAN FOR THE WHOLE STORE, its phases inside it: what they do not cover is the store's own `(untraced)`.
    let _storing = crate::trace::stage("typescript: store");
    let receipt = match calls::ts_apply(Path::new(db), root, &text(rows)) {
        Ok(receipt) => receipt,
        Err(error) if error.starts_with(crate::rows::reads::STOPPED_SHORT) => {
            into.notes.push(format!("the typescript half is reading every hop of dependents again: {error}"));
            return true;
        }
        Err(error) => {
            into.errors.push(format!("HALF      {LANGUAGE}: the rows were not stored — {error}"));
            return false;
        }
    };
    let mut receipt: Value = match serde_json::from_str(&receipt) {
        Ok(receipt) => receipt,
        Err(e) => {
            into.errors.push(format!("HALF      {LANGUAGE}: the receipt could not be read — {e}"));
            return false;
        }
    };
    into.notes.extend(receipt["notes"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(String::from)));
    *fe = receipt["fe"].as_str().map(String::from);
    // WHERE THE TIME WENT, as one note. The carry was timed here, before node parsed: the store never saw it.
    if let (Some(ms), Some(phases)) = (carried, receipt["phases"].as_object_mut()) {
        phases.insert("carry the rows".into(), json!(ms));
        phases.sort_keys();
    }
    for (phase, ms) in receipt["phases"].as_object().into_iter().flatten() {
        crate::trace::reported(&format!("typescript: {phase}"), ms.as_u64().unwrap_or(0));
    }
    // WHAT `write the rows` IS MADE OF, as a breakdown: it explains that phase and adds nothing to the store.
    for step in receipt["write_steps"].as_array().into_iter().flatten() {
        if let (Some(name), Some(ms)) = (step[0].as_str(), step[1].as_u64()) {
            crate::trace::breakdown_with(&format!("write the rows: {name}"), ms, Vec::new());
        }
    }
    let spent: Vec<String> = receipt["phases"].as_object().into_iter().flatten()
        .filter_map(|(phase, ms)| ms.as_i64().filter(|ms| *ms >= 1000).map(|ms| format!("{phase} {} s", seconds(ms))))
        .collect();
    if !spent.is_empty() {
        into.notes.push(format!("the typescript half spent: {}", spent.join(", ")));
    }
    for (table, rows) in receipt["written"].as_object().into_iter().flatten() {
        into.database.insert(table.clone(), rows.as_i64().unwrap_or(0));
    }
    false
}

/// Milliseconds as whole seconds, rounded half AWAY from zero as .NET's `F0` does.
fn seconds(ms: i64) -> i64 {
    (ms + 500) / 1000
}

fn records(output: &str, prefix: &str) -> Vec<String> {
    output.split('\n').filter_map(|line| line.trim_end_matches('\r').strip_prefix(prefix).map(String::from)).collect()
}
