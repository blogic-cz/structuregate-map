//! The one flow every command shares: take the stored map, bring it up to date
//! by the cheapest method that is safe, report what moved, store if asked.

use crate::db::{self, Snapshot};
use crate::diff::{self, Change};
use crate::entry::{now_unix_ns, Entry, Hash};
use crate::scan::{self, Cache, ScanOptions};
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Journal runs allowed in a row before a full walk is forced.
///
/// NTFS coalesces journal reasons per open file: once a reason has been recorded
/// for a file that some process still holds open, further changes of that same
/// reason add no new record until every handle closes. A log a server appends to
/// all day therefore produces one record, not thousands. Journal results are
/// exact for ordinary edit-save-close work and can lag for a file held open, so
/// the chain of journal runs is bounded and a full walk resettles the map.
pub const MAX_USN_CHAIN: i64 = 20;

/// Snapshots kept per root after a store: the newest, and the one before it for a caller comparing the two.
pub const KEEP: i64 = 2;

/// How the fresh tree state was obtained.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    /// No snapshot existed; everything is new.
    FirstScan,
    /// Full directory walk, with hashes reused for files whose stat matched.
    FullScan,
    /// NTFS journal replay: only the nodes the journal named were read.
    Usn,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::FirstScan => "first scan",
            Method::FullScan => "full scan",
            Method::Usn => "usn journal",
        }
    }
}

#[derive(Debug)]
pub struct Refresh {
    pub method: Method,
    pub changes: Vec<Change>,
    pub root_hash: Option<Hash>,
    pub previous_root_hash: Option<Hash>,
    pub entry_count: usize,
    /// Nodes whose content was read from disk this run.
    pub read_from_disk: usize,
    pub elapsed_ms: u128,
    /// Set when the result was written as a new snapshot.
    pub snapshot_id: Option<i64>,
    /// Why the journal path was not used, when it was not.
    pub usn_note: Option<String>,
}

impl Refresh {
    /// Changed paths, as the target matcher wants them: no duplicates, and a
    /// rename contributes both ends because both sides may feed a build.
    pub fn changed_paths(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::with_capacity(self.changes.len() + 8);
        for c in &self.changes {
            v.push(c.path.clone());
            if let Some(f) = &c.from {
                v.push(f.clone());
            }
        }
        v.sort();
        v.dedup();
        v
    }
}

/// Path of `target` relative to `root_label`, or `None` when it sits outside.
/// The SQLite side files (`-wal`, `-shm`) live beside the database, so the
/// database's own directory is pruned when it has one below the root.
fn relative_to(root_label: &str, target: &Path) -> Option<String> {
    let abs = std::path::absolute(target).ok()?;
    let abs = abs
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");
    let prefix = format!("{}/", root_label.trim_end_matches('/'));
    let rest = abs
        .to_lowercase()
        .strip_prefix(&prefix.to_lowercase())?
        .to_string();
    let start = abs.len() - rest.len();
    let rel = abs[start..].to_string();
    Some(match rel.rfind('/') {
        Some(i) => rel[..i].to_string(),
        None => rel,
    })
}

pub struct Engine {
    pub conn: Connection,
    pub root: PathBuf,
    pub root_label: String,
    pub opts: ScanOptions,
}

impl Engine {
    pub fn open(db_path: &Path, root: &Path, mut opts: ScanOptions) -> Result<Engine> {
        if let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        let conn = db::open(db_path)?;
        let root = std::fs::canonicalize(root)?;
        let root_label = root
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .replace('\\', "/");

        // A database inside the scanned tree would change on every store and
        // report itself as modified on every run. Prune it by path, whatever the
        // caller named it.
        if let Some(rel) = relative_to(&root_label, db_path) {
            opts.skip_paths.push(rel);
        }
        Ok(Engine { conn, root, root_label, opts })
    }

    pub fn latest(&self) -> Result<Option<Snapshot>> {
        db::latest_snapshot(&self.conn, &self.root_label)
    }

    /// Bring the map up to date. `allow_usn` lets the journal path be tried,
    /// `store` writes the result as a new snapshot.
    pub fn refresh(&mut self, allow_usn: bool, store: bool, include_dirs: bool) -> Result<Refresh> {
        let started = std::time::Instant::now();
        // Both marks are taken before anything is read, never after. A file
        // written while the scan runs must look newer than the snapshot, and its
        // journal record must fall after the stored USN. Taking either mark at
        // the end would place that change before the snapshot and lose it for
        // good.
        let created_ns = now_unix_ns();
        let journal_at_start = self.journal_position();

        let prev = self.latest()?;
        let Some(prev) = prev else {
            return self.first_scan(store, started, created_ns, journal_at_start);
        };
        let prev_entries = db::load_entries(&self.conn, prev.id)?;

        let mut usn_note = None;
        let chain_exhausted = prev.usn_chain >= MAX_USN_CHAIN;
        if allow_usn && chain_exhausted {
            usn_note = Some(format!(
                "{MAX_USN_CHAIN} journal runs in a row; walking the tree to resettle the map"
            ));
        }
        if allow_usn && !chain_exhausted {
            #[cfg(windows)]
            match crate::incremental::update(&self.root, &prev, &prev_entries, &self.opts) {
                Ok(r) => {
                    return self.finish(
                        Method::Usn,
                        r.entries,
                        r.root_hash,
                        &prev,
                        &prev_entries,
                        // Store the position read before the run, not the one the
                        // drain ended at. A write that lands while the run hashes
                        // files is then replayed next time. Replaying a change is
                        // harmless; skipping one is not.
                        journal_at_start.or(Some((r.journal_id, r.next_usn))),
                        r.touched,
                        store,
                        include_dirs,
                        started,
                        created_ns,
                        prev.usn_chain + 1,
                        None,
                    );
                }
                Err(e) => usn_note = Some(e.to_string()),
            }
            #[cfg(not(windows))]
            {
                usn_note = Some("the journal fast path is Windows only".to_string());
            }
        }

        let cache = Cache { entries: &prev_entries, created_ns: prev.created_ns };
        let result = scan::scan(&self.root, &self.opts, Some(&cache))?;
        self.finish(
            Method::FullScan,
            result.entries,
            result.root_hash,
            &prev,
            &prev_entries,
            journal_at_start,
            result.hashed,
            store,
            include_dirs,
            started,
            created_ns,
            0,
            usn_note,
        )
    }

    fn first_scan(
        &mut self,
        store: bool,
        started: std::time::Instant,
        created_ns: i64,
        journal: Option<(i64, i64)>,
    ) -> Result<Refresh> {
        let result = scan::scan(&self.root, &self.opts, None)?;
        let entry_count = result.entries.len();
        let read_from_disk = result.hashed;
        let root_hash = result.root_hash;

        let snapshot_id = if store {
            Some(self.store(&result.entries, root_hash, journal, created_ns, 0)?)
        } else {
            None
        };

        Ok(Refresh {
            method: Method::FirstScan,
            changes: Vec::new(),
            root_hash,
            previous_root_hash: None,
            entry_count,
            read_from_disk,
            elapsed_ms: started.elapsed().as_millis(),
            snapshot_id,
            usn_note: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        method: Method,
        entries: Vec<Entry>,
        root_hash: Option<Hash>,
        prev: &Snapshot,
        prev_entries: &HashMap<String, Entry>,
        journal: Option<(i64, i64)>,
        read_from_disk: usize,
        store: bool,
        include_dirs: bool,
        started: std::time::Instant,
        created_ns: i64,
        usn_chain: i64,
        usn_note: Option<String>,
    ) -> Result<Refresh> {
        let changes = diff::diff_scan(prev_entries, &entries, include_dirs);
        let entry_count = entries.len();
        // A TREE THAT DID NOT MOVE IS NOT WRITTEN AGAIN. Its entries are the previous snapshot's, so that
        // snapshot is moved forward in place - the journal position and the time it answers for - instead of
        // copying every entry into a new one: tens of thousands of rows per run on a solution, for nothing.
        let unchanged = changes.is_empty() && root_hash.is_some() && root_hash == prev.root_hash;
        let snapshot_id = if store && unchanged {
            let (journal_id, usn) = journal.map_or((None, None), |(j, u)| (Some(j), Some(u)));
            db::advance_snapshot(&self.conn, prev.id, created_ns, usn, journal_id, usn_chain)?;
            Some(prev.id)
        } else if store {
            let id = self.store(&entries, root_hash, journal, created_ns, usn_chain)?;
            // ONLY THE NEWEST ARE KEPT: a store that keeps every snapshot grows by a whole tree per change.
            db::prune(&self.conn, KEEP)?;
            Some(id)
        } else {
            None
        };
        Ok(Refresh {
            method,
            changes,
            root_hash,
            previous_root_hash: prev.root_hash,
            entry_count,
            read_from_disk,
            elapsed_ms: started.elapsed().as_millis(),
            snapshot_id,
            usn_note,
        })
    }

    fn store(
        &mut self,
        entries: &[Entry],
        root_hash: Option<Hash>,
        journal: Option<(i64, i64)>,
        created_ns: i64,
        usn_chain: i64,
    ) -> Result<i64> {
        let (journal_id, usn) = match journal {
            Some((j, u)) => (Some(j), Some(u)),
            None => (None, None),
        };
        let volume = self.volume_label();
        let id = db::insert_snapshot(
            &self.conn,
            &self.root_label,
            created_ns,
            usn,
            journal_id,
            volume.as_deref(),
            usn_chain,
        )?;
        db::insert_entries(&mut self.conn, id, entries)?;
        db::finish_snapshot(&self.conn, id, entries.len() as i64, root_hash)?;
        Ok(id)
    }

    /// Journal identity and position to record with a full scan, so the next run
    /// can take the fast path. Absent when the journal cannot be read.
    #[cfg(windows)]
    fn journal_position(&self) -> Option<(i64, i64)> {
        let volume = crate::usn::volume_of(&self.root).ok()?;
        let vol = crate::usn::open_volume(&volume).ok()?;
        let info = crate::usn::query_journal(&vol).ok()?;
        Some((info.journal_id as i64, info.next_usn))
    }

    #[cfg(not(windows))]
    fn journal_position(&self) -> Option<(i64, i64)> {
        None
    }

    #[cfg(windows)]
    fn volume_label(&self) -> Option<String> {
        crate::usn::volume_of(&self.root).ok()
    }

    #[cfg(not(windows))]
    fn volume_label(&self) -> Option<String> {
        None
    }
}
