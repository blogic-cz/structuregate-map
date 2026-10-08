//! THE PASSES OVER THE FINISHED DATABASE - the SQL links, the seed walk, the row search index, the atlas - ARE
//! NOT RUN AGAIN OVER THE SAME ROWS. They read nothing but what the halves stored, so when no half changed a
//! row since the last run that ran them cleanly, their answer is the one they gave then: a per-turn run over an
//! unchanged tree paid seconds for it, every turn.
//!
//! THE KEY IS WHAT THEY READ: every `files` row's language, path and sha, the store's id counters (they move
//! with any rewrite) and the TypeScript half's setup, the SQL config, which of the optional
//! outputs were asked for, and the BUILD of this tool - a new build may derive differently from the same rows.
//! What they said - their notes, the tables they wrote, the halves they name - is kept beside the key and said
//! again, so a skipped pass prints exactly what the run that did the work printed.
//!
//! Kept only after a run whose derived passes added no error, so a failure is said again every run.

use super::super::protocol::Collector;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

const KEPT: &str = "derived";

/// What the derived passes read: one hash, and its PARTS - the run, each language's files, each counter and setup -
/// kept beside it so a run that has to do the work again can say which part moved (a large tree re-ran them, most of
/// a run that changed nothing, and nothing said why). None when the database cannot be read; they then run.
pub struct Key {
    pub all: String,
    parts: BTreeMap<String, String>,
}

/// A NULL CELL IS AN EMPTY ONE: the TypeScript half stores a `files` row with no sha (a file it lists and does not
/// hash), and read as a String it failed the whole key - so on a large tree the passes never replayed and never said why,
/// most of a run that changed nothing. Err says why there is no key; the passes then run.
pub fn key(db: &str, config_stamp: &str, facts_stamp: &str, build: &str, row_fts: bool, atlas: Option<&str>) -> Result<Key, String> {
    keyed(db, config_stamp, facts_stamp, build, row_fts, atlas).map_err(|e| e.to_string())
}

fn keyed(db: &str, config_stamp: &str, facts_stamp: &str, build: &str, row_fts: bool, atlas: Option<&str>) -> rusqlite::Result<Key> {
    let text = |r: &rusqlite::Row, i: usize| -> rusqlite::Result<String> { Ok(r.get::<_, Option<String>>(i)?.unwrap_or_default()) };
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut parts = BTreeMap::new();
    parts.insert("the run's options or this build".to_string(), format!("{config_stamp}|{build}|{row_fts}|{}", atlas.unwrap_or("")));
    // ITS OWN PART, so a run says it was a document that moved, not the options.
    parts.insert("the facts config or its snapshots".to_string(), facts_stamp.to_string());
    let mut files = conn.prepare("SELECT lang, path, sha FROM files ORDER BY lang, path")?;
    let rows = files.query_map([], |r| Ok((text(r, 0)?, format!("{}\t{}\n", text(r, 1)?, text(r, 2)?))))?;
    let mut by_lang: BTreeMap<String, blake3::Hasher> = BTreeMap::new();
    for row in rows {
        let (lang, line) = row?;
        by_lang.entry(lang).or_default().update(line.as_bytes());
    }
    for (lang, hash) in by_lang {
        parts.insert(format!("the {lang} files"), hash.finalize().to_hex()[..16].to_string());
    }
    // WHAT DESCRIBES THE ROWS, not the tallies: `rows` counts the derived tables too, so it moves because these
    // passes ran, and a key holding it would never match.
    let mut meta = conn.prepare("SELECT key, value FROM _meta WHERE key LIKE 'counter:%' OR key LIKE 'setup:%' OR key = 'schema' ORDER BY key")?;
    let rows = meta.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))?;
    for row in rows {
        let (name, value) = row?;
        parts.insert(name, blake3::hash(value.as_bytes()).to_hex()[..16].to_string());
    }
    let mut hash = blake3::Hasher::new();
    for (name, value) in &parts {
        hash.update(format!("{name}\t{value}\n").as_bytes());
    }
    Ok(Key { all: hash.finalize().to_hex()[..32].to_string(), parts })
}

/// What a clean run of the passes said, before they ran now.
pub struct Before {
    notes: usize,
    errors: usize,
    database: BTreeMap<String, i64>,
    halves: Vec<String>,
}

pub fn before(into: &Collector) -> Before {
    Before { notes: into.notes.len(), errors: into.errors.len(), database: into.database.clone(), halves: into.halves.iter().cloned().collect() }
}

/// Said again from the last clean run, when nothing they read has moved. False when they have to run - and then the
/// run says what moved since the last one kept its key.
pub fn replay(into: &mut Collector, db: &str, key: &Key, atlas: Option<&str>) -> bool {
    // AN ATLAS SOMEBODY DELETED is written again, whatever the rows say.
    if atlas.is_some_and(|dir| !Path::new(dir).join("atlas.json").is_file()) {
        return false;
    }
    let Some(kept) = recorded(db) else { return false };
    if kept["key"].as_str() != Some(key.all.as_str()) {
        let was = kept["parts"].as_object();
        let moved: Vec<&String> = key.parts.iter()
            .filter(|(name, value)| was.and_then(|w| w.get(*name)).and_then(Value::as_str) != Some(value.as_str()))
            .map(|(name, _)| name)
            .chain(was.into_iter().flatten().map(|(name, _)| name).filter(|name| !key.parts.contains_key(*name)))
            .collect();
        let said = if was.is_none() { "the key kept last time predates its parts".to_string() }
            else { moved.iter().take(6).map(|m| m.as_str()).collect::<Vec<_>>().join(", ") };
        into.notes.push(format!("the passes over the finished rows run again: {said} moved since they last ran"));
        crate::trace::set("structuregate.deep.derived_moved", said);
        return false;
    }
    into.notes.extend(strings(&kept["notes"]));
    into.halves.extend(strings(&kept["halves"]));
    for (table, rows) in kept["database"].as_object().into_iter().flatten() {
        into.database.insert(table.clone(), rows.as_i64().unwrap_or(0));
    }
    // EVERY TABLE AS IT WAS COUNTED THEN: no row moved, and a half that had nothing to do brought no receipt - the
    // `database` line otherwise shrank on the run that changed nothing.
    for (table, rows) in kept["tables"].as_object().into_iter().flatten() {
        into.database.entry(table.clone()).or_insert(rows.as_i64().unwrap_or(0));
    }
    true
}

/// Keep what the passes said under the key - only when they added no error.
pub fn remember(into: &mut Collector, db: &str, key: &Key, before: Before) {
    if into.errors.len() != before.errors {
        return;
    }
    let database: BTreeMap<&String, &i64> = into.database.iter().filter(|(t, n)| before.database.get(*t) != Some(n)).collect();
    let halves: Vec<&String> = into.halves.iter().filter(|h| !before.halves.contains(h)).collect();
    // THE RUN'S OWN NOTE about why the passes ran is not theirs to say again.
    let notes: Vec<&String> = into.notes[before.notes..].iter().filter(|n| !n.starts_with("the passes over the finished rows run again")).collect();
    let kept = json!({ "key": key.all, "parts": key.parts, "notes": notes, "database": database, "halves": halves,
                       "tables": into.database });
    // A KEY THAT COULD NOT BE KEPT IS SAID: dropped in silence - a database busy on Windows - the passes ran on every
    // run and nothing said why.
    let written = rusqlite::Connection::open(db).and_then(|conn| {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute("DELETE FROM _meta WHERE key = ?1", [KEPT])?;
        conn.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [KEPT, &kept.to_string()])
    });
    if let Err(why) = written {
        into.notes.push(format!("the passes over the finished rows could not keep their key, so they run again next time - {why}"));
    }
}

fn recorded(db: &str) -> Option<Value> {
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let text: String = conn.query_row("SELECT value FROM _meta WHERE key = ?1", [KEPT], |r| r.get(0)).ok()?;
    serde_json::from_str(&text).ok()
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect()
}
