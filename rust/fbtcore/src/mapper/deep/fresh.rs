//! A FRESH DATABASE WHEN EVERY ROW IS ABOUT TO BE WRITTEN AGAIN. An upgrade that changes what the big halves extract
//! made each of them delete every row it held before writing them back - the TypeScript half table by table, the C#
//! half file by file, every index kept up row by row - and on a large map that took longer than a first build into an
//! empty file. When the TypeScript half's extractor changed (it reads every file again) and nearly every C# file moved,
//! the database is removed before any half runs and every half writes into a new one, as on a first build.
//!
//! Only then: a run that keeps most rows of either half keeps the database. The store is a cache - removing it loses
//! nothing a full run does not write back.

use std::path::Path;

/// The share of C# files that must move, in tenths, for its rows to count as all written again.
const MOVED_TENTHS: usize = 9;
/// The files the halves writing everything again must hold together: a small map's rows go in seconds either way, and
/// one edit in a tree of three files is no upgrade.
const AT_LEAST: usize = 50;

/// Why the database should start empty, or None: it is no usable database already, or a half would keep rows.
pub(super) fn reason(db: &str, typescript_files: usize, csharp_moving: usize, csharp_files: usize) -> Option<String> {
    let path = Path::new(db);
    if !crate::rows::store::usable(path) {
        return None;
    }
    // THE TYPESCRIPT HALF reads every file when the extractor it records (`build` beside its key) is not this one.
    let recorded = super::payload::recorded(db, super::ts::LANG).and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let built = recorded.as_ref().and_then(|r| r["build"].as_str().map(str::to_string));
    // A WORKSPACE THE LAST RUN FOUND NONE OF (`absent`) has no rows to write again.
    let typescript = typescript_files > 0 && recorded.as_ref().is_some_and(|r| r["absent"] != true);
    let typescript_all = typescript && built.is_some_and(|b| b != crate::embedded::node_rows::extractor());
    let csharp_all = csharp_files > 0 && csharp_moving * 10 >= csharp_files * MOVED_TENTHS;
    // EVERY BIG HALF PRESENT MUST REWRITE ALL, and at least one must be here: a half that keeps its rows would pay a
    // full read for the other's sake.
    if (typescript && !typescript_all) || (csharp_files > 0 && !csharp_all) || !(typescript_all || csharp_all) {
        return None;
    }
    if usize::from(typescript_all) * typescript_files + usize::from(csharp_all) * csharp_files < AT_LEAST {
        return None;
    }
    let mut why = Vec::new();
    if typescript_all {
        why.push("the typescript extractor changed".to_string());
    }
    if csharp_all {
        why.push(format!("{csharp_moving} of {csharp_files} C# file(s) moved"));
    }
    Some(why.join(", "))
}

/// Whether this run starts on an empty database - removed here, and said so.
pub(super) fn begin(into: &mut crate::mapper::Collector, db: &str, typescript_files: usize, csharp_moving: usize, csharp_files: usize) -> bool {
    let Some(why) = reason(db, typescript_files, csharp_moving, csharp_files) else { return false };
    if !start(db) {
        return false;
    }
    into.notes.push(format!("the deep map starts on an empty database: {why}, and writing every row into a new file is faster than deleting the old ones first"));
    true
}

/// The database and its journal files removed - the journals first, so a database that will not go (a reader holds it,
/// as Windows refuses) is left whole, and the run goes on over it as before.
pub(super) fn start(db: &str) -> bool {
    for suffix in ["-wal", "-shm", "-journal", ""] {
        let path = format!("{db}{suffix}");
        if Path::new(&path).exists() && std::fs::remove_file(&path).is_err() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_database_starts_empty_only_when_every_big_half_would_write_all_its_rows_again() {
        let dir = std::env::temp_dir().join(format!("fbt-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("map.sqlite");
        let db = db.to_str().unwrap();
        {
            let conn = rusqlite::Connection::open(db).unwrap();
            crate::rows::meta::put(&conn, "schema", crate::rows::store::SCHEMA_VERSION).unwrap();
        }
        let typescript = |build: &str| super::super::payload::record(db, super::super::ts::LANG, &serde_json::json!({ "key": "k", "absent": false, "build": build }).to_string());
        let now = crate::embedded::node_rows::extractor();
        typescript("an older extractor");
        assert!(reason(db, 10, 95, 100).is_some(), "the extractor changed and most C# moved");
        assert!(reason(db, 10, 50, 100).is_none(), "half the C# rows are kept");
        assert!(reason(db, 60, 0, 0).is_some(), "no C# at all");
        assert!(reason(db, 10, 3, 3).is_none(), "a small map");
        typescript(&now);
        assert!(reason(db, 10, 100, 100).is_none(), "the typescript rows are kept");
        assert!(reason(db, 0, 100, 100).is_some(), "no typescript in the tree");
        assert!(start(db) && !Path::new(db).exists());
        assert!(reason(db, 10, 100, 100).is_none(), "no usable database is already a first build");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
