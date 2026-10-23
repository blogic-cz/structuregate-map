//! `--map-no-text`: WHETHER THE MAP KEEPS THE SOURCE OF ITS FILES (`file_text`, the FTS5 index over it).
//!
//! The text is most of a map's bytes on a code-heavy tree and no row is derived from it that the disk cannot give
//! again, so a consumer that wants only the rows asks not to store it. EVERY WRITER of `file_text` is `store::search`
//! and asks `keep()` there; every reader has its own answer without text (`query/cat.rs` reads the disk, `--text`
//! says it was not built, `unchanged.rs` derives again, `keyreach.rs` and `cssmap` read the disk anyway).
//!
//! THE SETTING LIVES IN `_meta` (`options:text`, 1 or 0; a map older than the key held its text), and a run that
//! asks for the other value CHANGES THE MAP BEFORE ANY HALF READS IT (`reconcile`). Left to the halves, an
//! unchanged tree is kept as it is and neither of the two would happen: text switched off stayed stored for ever,
//! text switched on stayed missing for every file that did not move.

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

/// The `_meta` key.
pub const KEY: &str = "options:text";

/// What THIS RUN stores, set once by `reconcile` before a half writes: 0 not said, 1 keep, 2 leave out. A process-wide
/// flag, because the writers are reached from five doors (python, C#, rust, go and typescript payloads, and the
/// derived passes) that share nothing but the process.
static KEEP: AtomicU8 = AtomicU8::new(0);

/// Whether the store writes `file_text` into `db`: what the run said, and where it said nothing (a door that is
/// not a map run) what the database itself recorded - so no door adds text to a map built without it.
pub fn effective(db: &Connection) -> bool {
    match KEEP.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => held(db),
    }
}

pub fn set(keep: bool) {
    KEEP.store(if keep { 1 } else { 2 }, Ordering::Relaxed);
}

/// Whether the map in `db` holds source text: everything but a map recorded as built without it.
pub fn held(db: &Connection) -> bool {
    super::meta::get(db, KEY).as_deref() != Some("0")
}

/// THE RUN'S SETTING, MADE TRUE OF THE DATABASE before any half looks at it. None when nothing had to change, else a
/// note for the run.
///
/// - Switched OFF: `file_text` and its shadows are dropped and the file is compacted. The rows do not depend on the
///   text, so nothing is read again.
/// - Switched ON: the database is removed, so every half reads every file again and stores it. The halves skip a
///   file whose bytes did not move, which is a file whose text was never stored.
pub fn reconcile(db_path: &Path, keep: bool) -> Option<String> {
    set(keep);
    if !db_path.is_file() || !crate::rows::store::usable(db_path) {
        return None;
    }
    let held_now = {
        let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        held(&db)
    };
    if held_now == keep {
        return None;
    }
    if keep {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", db_path.display()));
        }
        let _ = std::fs::remove_file(db_path.with_extension("carry.json"));
        return Some("the map held no source text and this run keeps it: rebuilt from scratch (--map-no-text was dropped)".to_string());
    }
    match drop_text(db_path) {
        Ok(()) => Some("the map held source text and this run does not keep it: file_text dropped (--map-no-text)".to_string()),
        Err(why) => Some(format!("--map-no-text could not drop the stored source text: {why:#}")),
    }
}

fn drop_text(db_path: &Path) -> Result<()> {
    let db = Connection::open(db_path)?;
    // THE FTS5 TABLE'S DROP TAKES ITS SHADOWS (`file_text_content`, `_data`, `_idx`, `_docsize`, `_config`) with it.
    db.execute_batch("DROP TABLE IF EXISTS file_text")?;
    super::meta::put(&db, KEY, "0")?;
    db.execute_batch("VACUUM")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_with_no_record_holds_its_text() {
        let db = Connection::open_in_memory().unwrap();
        super::super::meta::ensure(&db).unwrap();
        assert!(held(&db));
        super::super::meta::put(&db, KEY, "0").unwrap();
        assert!(!held(&db));
        super::super::meta::put(&db, KEY, "1").unwrap();
        assert!(held(&db));
    }
}
