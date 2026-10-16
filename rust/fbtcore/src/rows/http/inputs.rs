//! WHAT THE HTTP PASS READS OF THE TYPESCRIPT HALF, and whether a run moved it - so the pass is not run again over the
//! same rows (`mapper/deep/passes.rs::http`).
//!
//! The pass reads `files`, `calls`, `members`, `functions`, `locals` and `consts` - the columns `TS_INPUTS` names, the
//! same lists its loaders ask for (`calls.rs`) - and those rows change only when the half stores. So the half keeps a
//! TOKEN in `_meta` (`TOKEN`) that moves whenever they may have: a fresh one on a full run, and on a partial one unless
//! the rows it removed and the rows it wrote in their place hash alike, every listed column with its node rows
//! (`unchanged::digest` - the comparison the closure is kept by). A run that stops half way leaves no token (`forget`
//! runs before the first delete), and no token is a key that matches nothing.

use crate::rows::unchanged::{digest, Rewrites};
use anyhow::Result;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

/// Every TypeScript table and column the http pass reads (`calls.rs`).
pub const TS_INPUTS: &[(&str, &[&str])] = &[
    ("files", &["id", "path", "lang"]),
    ("calls", &["id", "file", "line", "member", "target_id", "target", "args", "arg_params"]),
    ("members", &["id", "file", "class", "name", "params", "value"]),
    ("functions", &["id", "file", "params"]),
    ("locals", &["file", "member", "name", "line", "value"]),
    ("consts", &["file", "name", "value"]),
];

/// The columns of `table` the pass reads, as `TS_INPUTS` lists them - what each loader asks the table for.
pub fn read_of(table: &str) -> &'static [&'static str] {
    TS_INPUTS.iter().find(|(t, _)| *t == table).map_or(&[], |(_, c)| c)
}

/// The `_meta` key the token is kept under.
pub const TOKEN: &str = "http-inputs:typescript";

fn columns(listed: &[&str]) -> BTreeSet<String> {
    listed.iter().map(|c| c.to_string()).collect()
}

/// No token: called before a run deletes a row, so one that stops half way vouches for nothing.
pub fn forget(db: &Connection) -> Result<()> {
    if crate::rows::meta::exists(db) {
        crate::rows::meta::delete(db, TOKEN)?;
    }
    Ok(())
}

/// A token no run has had - a full run rewrote every row the pass reads.
pub fn fresh(db: &Connection) -> Result<()> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let token = blake3::hash(format!("{now}|{}", std::process::id()).as_bytes()).to_hex()[..32].to_string();
    crate::rows::meta::ensure(db)?;
    crate::rows::meta::put(db, TOKEN, &token)
}

/// What a partial run is about to remove, hashed before it goes, and the token the rows held then.
pub struct Before {
    token: Option<String>,
    old: BTreeMap<String, String>,
    marks: BTreeMap<String, i64>,
}

impl Before {
    pub fn take(db: &Connection, half: &str, rewrites: &Rewrites<'_>) -> Result<Before> {
        let token = crate::rows::meta::get(db, TOKEN);
        let mut old = BTreeMap::new();
        if token.is_some() {
            for (table, listed) in TS_INPUTS {
                old.insert(table.to_string(), digest(db, table, &columns(listed), half, &rewrites.removed(table))?);
            }
        }
        Ok(Before { token, old, marks: BTreeMap::new() })
    }

    /// The highest rowid of each table, AFTER the removal: every row written later lands above it.
    pub fn mark(&mut self, db: &Connection) {
        for (table, _) in TS_INPUTS {
            let top = db
                .query_row(&format!("SELECT coalesce(max(rowid), 0) FROM \"{table}\""), [], |r| r.get::<_, i64>(0))
                .unwrap_or(0);
            self.marks.insert(table.to_string(), top);
        }
    }

    /// After the write: the token the rows held when nothing the pass reads moved, else a fresh one.
    pub fn settle(&self, db: &Connection, half: &str, rewrites: &Rewrites<'_>) -> Result<()> {
        if let Some(token) = &self.token {
            let mut same = true;
            for (table, listed) in TS_INPUTS {
                let mark = self.marks.get(*table).copied().unwrap_or(0);
                let new = digest(db, table, &columns(listed), half, &rewrites.written(table, mark))?;
                same &= self.old.get(*table) == Some(&new);
            }
            if same {
                return crate::rows::meta::put(db, TOKEN, token);
            }
        }
        fresh(db)
    }
}

/// The `_meta` key the pass keeps what it was derived under, and what it counted then.
const KEPT: &str = "http";

/// WHAT THE PASS IS A FUNCTION OF: the running build, the TypeScript rows' token and every C# file's path and sha (a C#
/// row moves only with its file's salted sha - `mapper/deep/reasons.rs`). None without a token - unless no TypeScript
/// file is there at all, when the pass reads nothing of that half: then it runs.
pub fn key(db: &Connection, build: &str) -> Option<String> {
    let none = || db.prepare("SELECT 1 FROM files WHERE lang = 'typescript' LIMIT 1").and_then(|mut s| s.exists([])).ok() == Some(false);
    let token = crate::rows::meta::get(db, TOKEN).or_else(|| none().then(|| "no typescript".to_string()))?;
    let mut hash = blake3::Hasher::new();
    hash.update(format!("{build}\u{0}{token}\u{0}").as_bytes());
    let mut stmt = db.prepare("SELECT path, sha FROM files WHERE lang = 'csharp' ORDER BY path").ok()?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))).ok()?;
    for row in rows {
        let (path, sha) = row.ok()?;
        hash.update(format!("{}\t{}\n", path.unwrap_or_default(), sha.unwrap_or_default()).as_bytes());
    }
    Some(hash.finalize().to_hex()[..32].to_string())
}

/// What the last run counted, when it was derived under `key` and its tables are still there.
pub fn replayed(db: &Connection, key: &str) -> Option<super::Counts> {
    let kept: serde_json::Value = serde_json::from_str(&crate::rows::meta::get(db, KEPT)?).ok()?;
    let there = ["http_endpoints", "http_calls", "http_links"].iter().all(|t| crate::rows::dups::has_columns(db, t, &[]));
    if kept["key"].as_str() != Some(key) || !there {
        return None;
    }
    let n = |name: &str| kept[name].as_u64().map(|n| n as usize);
    Some(super::Counts { endpoints: n("endpoints")?, calls: n("calls")?, linked: n("linked")?, unmatched: n("unmatched")?, ambiguous: n("ambiguous")? })
}

/// Kept after a clean run, under the key it read.
pub fn remember(db: &Connection, key: &str, c: &super::Counts) -> Result<()> {
    let kept = serde_json::json!({ "key": key, "endpoints": c.endpoints, "calls": c.calls, "linked": c.linked, "unmatched": c.unmatched, "ambiguous": c.ambiguous });
    crate::rows::meta::ensure(db)?;
    crate::rows::meta::put(db, KEPT, &kept.to_string())
}
