//! HAS THIS TREE ALREADY PASSED? The gate runs before `CoreCompile` on every build of every
//! consuming project, and it walks the whole tree to answer a question that is a PURE FUNCTION of
//! that tree: do the structure rules hold. A tree byte-identical to one that passed still passes.
//!
//! So the answer is cached under the tree's own merkle root, which `fbt` computes - from the NTFS
//! journal where it can, from a full walk where it cannot. The key is taken from disk and never
//! from the cache, which is what keeps this honest: nothing here compares the database against
//! itself the way a recorded-sha check would.
//!
//! WHAT A MISS COSTS, stated because every cache has to answer it. If the journal ever misses a
//! change, one build skips its gate and the next run that does see a change catches it. That is a
//! bounded, self-correcting failure - unlike a map that goes stale and keeps looking real.
//!
//! THE DATABASE LIVES IN THE TREE IT WATCHES, under `.fbt/`. `Engine::open` prunes it from the
//! scan by path, so the map never describes itself and never reports itself as modified.

use anyhow::{Context, Result};
use fbt::pipeline::Engine;
use fbt::scan::ScanOptions;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// What the last run of this tree was, and what it is now.
pub struct Verdict {
    /// The tree's merkle root, as hex. Empty when it could not be taken.
    pub hash: String,
    /// How it was reached: `first scan`, `full scan`, or `usn journal`.
    pub method: String,

}

fn store_of(root: &Path) -> PathBuf {
    root.join(".fbt").join("gate.sqlite")
}

/// `.fbt/` IGNORES ITSELF: a `.gitignore` of `*` inside it, written once. It is state derived from the tree,
/// and a consumer whose own `.gitignore` does not name it would otherwise see it as untracked on every
/// status - in a repository this tool has no business editing the ignore file of.
pub fn ignored(root: &Path) {
    let marker = root.join(".fbt").join(".gitignore");
    if !marker.exists() {
        let _ = std::fs::write(marker, "*\n");
    }
}

/// THE TREE AS THE GATE ITSELF SEES IT, which is the only tree this may be keyed on.
///
/// A PATH THE GATE NEVER OPENS CANNOT CHANGE ITS VERDICT, and hashing it anyway is not merely
/// wasted work - it is most of the work. In one repository the gate judges a few hundred files while
/// this walked the whole checkout: thousands of files of build cache, `.venv`, `node_modules`, `dist`,
/// `logs`, and hundreds of MB of derived map databases. That is a store of hundreds of MB, and most of a second on a tree where
/// nothing had moved - a cache costing what the walk it replaces costs.
///
/// SO THE SCAN IS GIVEN THE SAME TWO BOUNDS THE GATE HAS. `skip` is the gate's own `--skip`,
/// matched on any path component exactly as `Sources.Skipped` matches it. `tracked` says the gate
/// took its file set from git, and there the gitignored half is not merely uninteresting - it is
/// not in the set being judged at all, so `fbt`'s own gitignore filter is the right one. Off that
/// mode nothing is assumed: a rule about file size holds for a generated file too, so the walk
/// keeps everything the skip list did not name.
///
/// SOUND BECAUSE BOTH BOUNDS ARE IN THE KEY. `GateCache` hashes the arguments verbatim, so a run
/// with a different `--skip` - or without `--tracked` - cannot read this run's entry. The tree a
/// hash describes and the tree the rules judged are the same tree by construction.
///
/// THE ONE EDGE, named rather than hidden: in tracked mode a doc may link to a gitignored file,
/// and deleting that file would newly fail a gate this reads as unchanged. It is the bounded,
/// self-correcting miss this module's header already accepts - the next change anywhere in the
/// tracked set walks again and catches it.
fn as_the_gate_sees_it(skip: &[String], tracked: bool) -> ScanOptions {
    ScanOptions {
        use_gitignore: tracked,
        no_default_skip: true,
        extra_skip: skip.to_vec(),
        ..ScanOptions::default()
    }
}

/// The tree's root hash now, and whether a run over that exact tree has already passed.
pub fn look(root: &Path, skip: &[String], tracked: bool) -> Result<Verdict> {
    let store = store_of(root);
    let mut engine = Engine::open(&store, root, as_the_gate_sees_it(skip, tracked))
        .with_context(|| format!("the tree map at {} could not be opened", store.display()))?;
    ignored(root);
    // STORED, so the next build has a snapshot to compare against and can take the journal path.
    let refreshed = engine.refresh(true, true, true)?;
    let hash = refreshed.root_hash.as_ref().map(fbt::hash::hex).unwrap_or_default();
    Ok(Verdict {
        hash,
        method: refreshed.method.label().to_string(),
    })
}

/// What every file of the last snapshot `look` stored holds - `(the store, relative path key -> content hash)`.
/// Read, never refreshed: it answers for the tree `look` has just seen.
pub fn contents(root: &Path, skip: &[String], tracked: bool) -> Option<(PathBuf, std::collections::HashMap<String, String>)> {
    let store = store_of(root);
    let engine = Engine::open(&store, root, as_the_gate_sees_it(skip, tracked)).ok()?;
    let latest = engine.latest().ok()??;
    let entries = fbt::db::load_entries(&engine.conn, latest.id).ok()?;
    let files = entries
        .into_iter()
        .filter(|(_, e)| e.kind == fbt::entry::Kind::File)
        .filter_map(|(key, e)| e.hash.map(|h| (key, fbt::hash::hex(&h))))
        .collect();
    Some((store, files))
}

/// Record that the rules held for this tree.
pub fn remember(root: &Path, hash: &str) -> Result<()> {
    if hash.is_empty() {
        return Ok(());
    }
    let db = Connection::open(store_of(root))?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS gate_pass (hash TEXT PRIMARY KEY)")?;
    db.execute("INSERT OR IGNORE INTO gate_pass (hash) VALUES (?1)", [hash])?;
    Ok(())
}

/// Whether this key - the tree AND the rules judging it - is recorded against a pass.
pub fn passed_before(root: &Path, key: &str) -> Result<bool> {
    let store = store_of(root);
    if !store.exists() || key.is_empty() {
        return Ok(false);
    }
    passed(&Connection::open(store)?, key)
}

fn passed(db: &Connection, hash: &str) -> Result<bool> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS gate_pass (hash TEXT PRIMARY KEY)")?;
    let found: i64 =
        db.query_row("SELECT count(*) FROM gate_pass WHERE hash = ?1", [hash], |r| r.get(0))?;
    Ok(found > 0)
}
