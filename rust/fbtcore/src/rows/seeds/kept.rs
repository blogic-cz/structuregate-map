//! WHETHER THE SEEDS PASS HAS TO RUN AT ALL (#145). It was run with every pass over the finished rows, so a one-line edit
//! of an unrelated C# file rebuilt every seed row and typed view - most of a run on a tree with large seed scripts.
//!
//! The pass reads only the SQL half's rows (`files` of `lang = 'sql'`, `sql_steps`, `sql_refs`, `sql_objects`,
//! `sql_columns`), and a SQL file's rows move only with its `sha`, which folds its content, its project, the SQL config
//! and the rows version (`mapper/deep/driven.rs`). So the key is the running build, every SQL file's id, path and sha,
//! and the ids the seed rows point at - a forced re-read writes the same shas under new ids.

use rusqlite::Connection;
use serde_json::{json, Value};

/// The `_meta` key the pass keeps what it was derived under, and what it answered then.
const KEPT: &str = "seeds";

/// The ids a seed row or view points at, each set in order.
const POINTED: [&str; 4] = [
    "SELECT id FROM files WHERE lang = 'sql' ORDER BY id",
    "SELECT id FROM sql_steps ORDER BY id",
    "SELECT id FROM sql_objects ORDER BY id",
    "SELECT id FROM sql_refs WHERE lang = 'sql' ORDER BY id",
];

/// WHAT THE PASS IS A FUNCTION OF. None when a table it reads cannot be - the pass then runs, never a wrong replay.
pub fn key(db: &Connection, build: &str) -> Option<String> {
    let mut hash = blake3::Hasher::new();
    hash.update(format!("{build}\u{0}").as_bytes());
    let mut stmt = db.prepare("SELECT id, path, sha FROM files WHERE lang = 'sql' ORDER BY path").ok()?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?))).ok()?;
    for row in rows {
        let (id, path, sha) = row.ok()?;
        hash.update(format!("{}\t{}\t{}\n", id.unwrap_or_default(), path.unwrap_or_default(), sha.unwrap_or_default()).as_bytes());
    }
    for sql in POINTED {
        hash.update(b"\x01");
        // A TABLE NOT THERE YET holds no id: the SQL half has not written one, and nothing points at it.
        let Ok(mut stmt) = db.prepare(sql) else { continue };
        let ids = stmt.query_map([], |r| r.get::<_, Option<String>>(0)).ok()?;
        for id in ids {
            hash.update(id.ok()?.unwrap_or_default().as_bytes()).update(b"\n");
        }
    }
    Some(hash.finalize().to_hex()[..32].to_string())
}

/// What the last run answered, when it was derived under `key` and what it made is still there.
pub fn replayed(db: &Connection, key: &str) -> Option<Value> {
    let kept: Value = serde_json::from_str(&crate::rows::meta::get(db, KEPT)?).ok()?;
    if kept["key"].as_str() != Some(key) || !crate::rows::dups::has_columns(db, "sql_seeds", &[]) {
        return None;
    }
    let views: i64 = db
        .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'view' AND substr(name, 1, length(?1)) = ?1", [super::views::PREFIX], |r| r.get(0))
        .ok()?;
    let answer = kept["answer"].clone();
    (answer["views"].as_i64() == Some(views)).then_some(answer)
}

/// Kept after a clean run, under the key it read.
pub fn remember(db: &Connection, key: &str, answer: &Value) -> anyhow::Result<()> {
    crate::rows::meta::ensure(db)?;
    crate::rows::meta::put(db, KEPT, &json!({ "key": key, "answer": answer }).to_string())
}
