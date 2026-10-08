//! THE HALVES WHOSE EXTRACTOR WRITES A PAYLOAD FILE - python (with its own `ast`) and plain TypeScript (with
//! the tree's own `typescript`) - both shaped the same: ask the database what it holds, hand the host that
//! state, store what it wrote, and CHECK THE INVARIANT: a file row that disagrees with the tree after an
//! incremental pass means the cache is lying, and a lying cache is worse than a slow one, so every file of
//! the half is read once more - never the database replaced, because the other halves' rows are in it.

use super::super::halves::{self, Script};
use super::super::protocol::Collector;
use crate::hosts;
use crate::rows::calls;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

/// One payload half: how it is named in a report, how its rows are stamped, and how it is launched.
pub struct Half<'a> {
    pub language: &'a str,
    pub lang: &'a str,
    pub folder: &'a str,
    pub script: Result<&'a str, &'a str>,
    pub host: &'a str,
    pub flag: &'a str,
    /// The arguments after the state, rows and reset flags - node modules, the roots file.
    pub extra: Vec<String>,
    /// Written beside the state before the launch: `(file name, body)`.
    pub beside: Vec<(&'a str, String)>,
    /// Said when the incremental pass disagreed: "the python half", "the plain typescript half".
    pub named: &'a str,
    /// WHAT THE ROWS WERE WRITTEN AGAINST: the whole tree's merkle hash, the scripts, the host's version and
    /// the arguments. Equal to the key recorded after the last clean run, nothing this half reads has moved,
    /// and it is not launched at all. None when the tree map could not answer - the half then always runs.
    pub key: Option<String>,
}

/// The key recorded after this half's last clean run.
pub(super) fn recorded(db: &str, lang: &str) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.query_row("SELECT value FROM _meta WHERE key = ?1", [format!("skip:{lang}")], |r| r.get(0)).ok()
}

/// WHY THE LAST CLEAN RUN STORED NOTHING travels with its key, one line after it: a half with no compiler to borrow
/// stores an empty payload cleanly, and every run after it said only "had nothing to do".
fn recorded_key(db: &str, lang: &str) -> Option<(String, Vec<String>)> {
    let text = recorded(db, lang)?;
    let mut lines = text.split('\n');
    let key = lines.next()?.to_string();
    Some((key, lines.map(String::from).collect()))
}

/// A note that says a half stored nothing, and why - kept with the key and said in the map's summary.
pub fn stored_nothing(note: &str) -> bool {
    note.contains(" half did not run: ") && !note.contains("no Angular workspace")
}

pub(super) fn record(db: &str, lang: &str, key: &str) {
    let Ok(conn) = rusqlite::Connection::open(db) else { return };
    let name = format!("skip:{lang}");
    // THE FIRST HALF OF A COLD RUN WRITES BEFORE ANY STORE HAS: with no `_meta` the key was dropped without a word,
    // and every first turn after a cold run started node again to hear "nothing to do" (`store::write` carries it).
    let _ = conn.execute_batch("CREATE TABLE IF NOT EXISTS _meta (key TEXT, value TEXT)");
    let _ = conn.execute("DELETE FROM _meta WHERE key = ?1", [&name]);
    let _ = conn.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [&name, &key.to_string()]);
}

/// A PAYLOAD HALF STARTED BEFORE ITS TURN, on its own thread, while the file map runs - the python file map and the
/// deep python rows are two hosts that need nothing of each other, and one waited for the other on every `.py` edit.
///
/// THE HOST'S ANSWER IS A FUNCTION OF THE STATE IT WAS HANDED, the files and what is written beside them, all fixed
/// before it starts. So it is used only when the state at the half's turn is still the one it was started from: the
/// halves that store before it (TypeScript) and the id counters every half shares can move it, and an answer
/// computed against another state is thrown away and asked again, in turn - never stored.
pub struct Early {
    thread: Option<JoinHandle<Started>>,
    work: PathBuf,
}

/// What a half started early came back with: its key, and - unless the key said there was nothing to do or the
/// state could not be read - the state it was handed and what its host said.
pub struct Started {
    pub key: Option<String>,
    ran: Option<(String, Result<hosts::Ran, String>)>,
}

impl Early {
    pub fn wait(mut self) -> Option<Started> {
        self.thread.take()?.join().ok()
    }
}

/// A HALF STARTED AND NEVER WAITED FOR - the run ended before the deep map's turn - is let finish, and its folder goes.
impl Drop for Early {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
            let _ = std::fs::remove_dir_all(&self.work);
        }
    }
}

/// Start `half` now. `key` is computed on the thread too: it asks the host for its version, which is a launch.
pub fn early(half: &Half, files: &[(String, String)], db: &str, root: &str, key: impl FnOnce() -> Option<String> + Send + 'static) -> Option<Early> {
    let script = half.script.ok()?;
    let work = work_dir(half);
    let launch = hosts::Launch {
        host: half.host.into(),
        before: Vec::new(),
        script: script.into(),
        after: arguments(half, &work, false),
        list_flag: "--list-file".into(),
        root_flag: "--root".into(),
        cwd: root.into(),
        files: files.to_vec(),
        tag: half.language.into(),
    };
    let (db, lang, at) = (db.to_string(), half.lang.to_string(), work.clone());
    let beside: Vec<(String, String)> = half.beside.iter().map(|(name, body)| (name.to_string(), body.clone())).collect();
    let thread = std::thread::spawn(move || {
        let key = key();
        if key.is_some() && recorded_key(&db, &lang).map(|(was, _)| was) == key {
            return Started { key, ran: None };
        }
        let Ok(state) = calls::state(Path::new(&db), &lang, false) else {
            return Started { key, ran: None };
        };
        let ran = prepare(&at, &state, &beside).map_err(|e| e.to_string()).and_then(|()| hosts::run(&launch));
        Started { key, ran: Some((state, ran)) }
    });
    Some(Early { thread: Some(thread), work })
}

fn work_dir(half: &Half) -> PathBuf {
    crate::hosts::temp_dir().join(format!("structuregate-{}-{}", half.folder, std::process::id()))
}

/// The state and what goes beside it, written where the host reads them.
fn prepare(work: &Path, state: &str, beside: &[(String, String)]) -> std::io::Result<()> {
    std::fs::create_dir_all(work)?;
    std::fs::write(work.join("state.json"), state)?;
    for (name, body) in beside {
        std::fs::write(work.join(name), body)?;
    }
    // A PAYLOAD LEFT BY AN EARLIER START is not this run's answer: a host that dies now must leave none.
    let _ = std::fs::remove_file(work.join("rows.json"));
    Ok(())
}

pub fn run(into: &mut Collector, half: &Half, files: &[(String, String)], db: &str, root: &str, started: Option<Started>) {
    // NOTHING THIS HALF READS HAS MOVED: its rows already say what a run would write, so no host is started.
    if let Some(key) = &half.key
        && let Some((was, why)) = recorded_key(db, half.lang)
        && was == *key
    {
        into.notes.push(format!("{} had nothing to do: no file under its roots moved since its rows were written", half.named));
        into.notes.extend(why);
        return;
    }
    let errors_before = into.errors.len();
    let notes_before = into.notes.len();
    let mut clean = false;
    let state = match calls::state(Path::new(db), half.lang, false) {
        Ok(state) => state,
        // A STATE THAT CANNOT BE READ STOPS THE HALF: guessing an empty database would hand out ids it has
        // already given away.
        Err(error) => {
            into.errors.push(format!("HALF      {}: the database state could not be read — {error}", half.language));
            return;
        }
    };
    // STARTED EARLY AGAINST THIS SAME STATE: its answer is the one a launch now would give, and it already wrote the
    // payload where this run reads it.
    let early = started.and_then(|s| s.ran).map(|(was, ran)| {
        crate::trace::set(&format!("structuregate.early.{}", half.lang), if was == state { "used" } else { "stale" });
        (was, ran)
    }).filter(|(was, _)| *was == state).map(|(_, ran)| ran);
    let work = work_dir(half);
    let outcome = (|| -> std::io::Result<()> {
        if early.is_none() {
            let beside: Vec<(String, String)> = half.beside.iter().map(|(name, body)| (name.to_string(), body.clone())).collect();
            prepare(&work, &state, &beside)?;
        }
        if !extract(into, half, files, root, &work, false, early) {
            return Ok(());
        }
        let wrong = store(into, db, root, &work.join("rows.json"), half.language);
        clean = wrong == 0;
        if wrong > 0 {
            into.notes.push(format!(
                "{} is re-reading everything: {wrong} file(s) disagreed with the tree after an incremental pass",
                half.named
            ));
            if !extract(into, half, files, root, &work, true, None) {
                return Ok(());
            }
            if store(into, db, root, &work.join("rows.json"), half.language) > 0 {
                into.errors.push(format!("HALF      {}: file(s) still disagree with the tree after a full rebuild", half.language));
            }
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        into.errors.push(format!("HALF      {}: could not be run ({e})", half.language));
    }
    let _ = std::fs::remove_dir_all(&work);
    // RECORDED ONLY AFTER A CLEAN RUN: a half that reported an error (a file that does not parse, a host
    // that stopped) is run again next time, so the error is said again rather than skipped into silence.
    if clean
        && into.errors.len() == errors_before
        && let Some(key) = &half.key
    {
        let why: String = into.notes[notes_before..].iter().filter(|n| stored_nothing(n)).map(|n| format!("\n{}", n.replace('\n', " "))).collect();
        record(db, half.lang, &format!("{key}{why}"));
    }
}

/// The host, over the files. Returns whether a payload was written.
fn extract(into: &mut Collector, half: &Half, files: &[(String, String)], root: &str, work: &Path, reset: bool, early: Option<Result<hosts::Ran, String>>) -> bool {
    let after = arguments(half, work, reset);
    let script = Script {
        language: half.language,
        host: half.host,
        script: half.script,
        flag: half.flag,
        before: Vec::new(),
        after,
        list_flag: "--list-file",
        root_flag: "--root",
        per_file: false,
        // A DEEP HALF ANSWERS IN TABLES, rebuilt by its own store - nothing here is kept per file.
        known_flag: "",
        whole: None,
        dependents: None,
        early,
    };
    let reason = halves::script(into, &script, files, root, "", &mut None);
    if !reason.is_empty() {
        into.errors.push(format!("HALF      {}: {reason}", half.language));
        return false;
    }
    work.join("rows.json").is_file()
}

/// The host's arguments after the file list: the state, the payload, what is written beside them.
fn arguments(half: &Half, work: &Path, reset: bool) -> Vec<String> {
    let path = |name: &str| work.join(name).to_string_lossy().into_owned();
    let mut after = vec!["--state".to_string(), path("state.json"), "--rows".into(), path("rows.json")];
    for (flag, name) in half.extra.chunks(2).filter_map(|c| Some((c.first()?.clone(), c.get(1)?.clone()))) {
        after.push(flag);
        // A NAME WRITTEN BESIDE THE STATE is handed as its path there.
        after.push(if half.beside.iter().any(|(b, _)| *b == name) { path(&name) } else { name });
    }
    if reset {
        after.push("--reset".into());
    }
    after
}

/// The rows of a payload half, stored. Returns how many files disagree with the tree afterwards - the
/// receipt's `retry`, named for what it asks the caller to do.
pub fn store(into: &mut Collector, db: &str, root: &str, rows: &Path, language: &str) -> i64 {
    let payload = match std::fs::read(rows) {
        Ok(bytes) => bytes,
        Err(e) => {
            into.errors.push(format!("HALF      {language}: the rows were not stored — {e}"));
            return 0;
        }
    };
    let receipt = match calls::apply(Path::new(db), root, &payload) {
        Ok(receipt) => receipt,
        Err(error) => {
            into.errors.push(format!("HALF      {language}: the rows were not stored — {error}"));
            return 0;
        }
    };
    let receipt = match serde_json::from_str::<Value>(&receipt) {
        Ok(receipt) => receipt,
        Err(e) => {
            into.errors.push(format!("HALF      {language}: the receipt could not be read — {e}"));
            return 0;
        }
    };
    for (table, rows) in receipt["written"].as_object().into_iter().flatten() {
        into.database.insert(table.clone(), rows.as_i64().unwrap_or(0));
    }
    receipt["retry"].as_i64().unwrap_or(0)
}

/// THIS HALF'S ROWS, GONE, when it no longer answers for the tree - the last script file left, or an Angular
/// workspace appeared. Nothing is launched: the store is handed an empty tree for the half.
pub fn forget(into: &mut Collector, language: &str, lang: &str, db: &str, root: &str) {
    let Ok(state) = calls::state(Path::new(db), lang, false) else { return };
    let held = serde_json::from_str::<Value>(&state).ok().and_then(|s| s["shas"].as_object().map(|o| !o.is_empty()));
    if held != Some(true) {
        return;
    }
    let empty = serde_json::json!({ "all": true, "first": true, "final": true, "reset": false,
        "shas": {}, "counters": {}, "tables": {}, "read": [], "lang": lang });
    if let Err(failed) = calls::apply(Path::new(db), root, empty.to_string().as_bytes()) {
        into.errors.push(format!("HALF      {language}: its rows could not be dropped — {failed}"));
        return;
    }
    into.notes.push("the plain typescript rows were dropped: no tree here is mapped by that half now".into());
}
