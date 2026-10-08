//! One scan of one tree, against what the database already recorded.
//!
//! The exe asks this once per run and both halves read the same answer. Two scans
//! that pruned a directory differently would let one half call a tree unchanged
//! that the other had not looked at, which is the only way an early exit can lie.

// THE ONE CHANGE DETECTOR: `fbt`, the engine the gate's pass cache and the deep map's file hashes use.
use fbt::db::{self, Snapshot};
use fbt::diff::{self, ChangeKind};
use fbt::entry::{now_unix_ns, Entry};
use fbt::hash;
use fbt::scan::{self as engine, Cache, ScanOptions};
use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What the caller may ask for, as JSON. Every field has a default, so `{}` is a
/// valid request and means "scan this tree the usual way".
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Request {
    /// Apply the built-in list of output directories (`node_modules`, `obj`, ...).
    pub default_skip: bool,
    /// Extra directory names to prune.
    pub skip: Vec<String>,
    /// Exact relative paths to prune with their subtree. The database being written
    /// belongs here; the caller knows its path and this side does not.
    pub skip_paths: Vec<String>,
    /// Read every file even when size and mtime say it has not moved.
    pub rehash_all: bool,
    /// Write the result as a new snapshot. False answers the question and records
    /// nothing, which is what a `--plan` run wants.
    pub store: bool,
    /// Include a hash per directory in the reply. Off by default: on a large tree it
    /// is most of the payload and only a caller asking about subtrees needs it.
    pub dir_hashes: bool,
    /// Keep this many snapshots for this root. Older ones are deleted after a store.
    pub keep: i64,
}

impl Default for Request {
    fn default() -> Self {
        Request {
            default_skip: true,
            skip: Vec::new(),
            skip_paths: Vec::new(),
            rehash_all: false,
            store: true,
            dir_hashes: false,
            keep: 3,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Reply {
    /// True when the tree hashes exactly as the last snapshot did. A caller may stop
    /// here; `stale` and `gone` are empty when it is set.
    pub unchanged: bool,
    /// Set on the first scan of a root, when there is nothing to compare against and
    /// every file is therefore stale.
    pub first_scan: bool,
    pub root_hash: Option<String>,
    pub previous_root_hash: Option<String>,
    pub entries: usize,
    /// Files whose bytes were read this run.
    pub read_from_disk: usize,
    pub elapsed_ms: u128,
    pub snapshot_id: Option<i64>,
    /// Files a half has to parse again: added, modified, type-changed, and the
    /// landing side of a rename.
    pub stale: Vec<String>,
    /// Files whose rows a half has to drop: deleted, and the leaving side of a
    /// rename. A rename appears in both lists, because the rows are keyed by path.
    pub gone: Vec<String>,
    /// Renames, for a caller that can move rows instead of rebuilding them.
    pub renamed: Vec<Renamed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir_hashes: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct Renamed {
    pub from: String,
    pub to: String,
}

pub struct Session {
    conn: Connection,
}

impl Session {
    pub fn open(db_path: &Path) -> Result<Session> {
        if let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Session { conn: db::open(db_path)? })
    }

    pub fn scan(&mut self, root: &Path, req: &Request) -> Result<Reply> {
        let started = std::time::Instant::now();
        // Taken BEFORE anything is read. A file written while the scan runs must look
        // newer than the snapshot, or the next run trusts its stale hash for ever.
        let created_ns = now_unix_ns();

        let root = std::fs::canonicalize(root)?;
        let root_label = label_of(&root);

        let opts = ScanOptions {
            // The probe prunes what it is TOLD to, never what a `.gitignore` says.
            use_gitignore: false,
            extra_skip: req.skip.clone(),
            no_default_skip: !req.default_skip,
            skip_paths: req.skip_paths.clone(),
            rehash_all: req.rehash_all,
        };

        let prev = db::latest_snapshot(&self.conn, &root_label)?;
        let prev_entries = match &prev {
            Some(p) => db::load_entries(&self.conn, p.id)?,
            None => HashMap::new(),
        };
        let cache = prev.as_ref().map(|p| Cache {
            entries: &prev_entries,
            created_ns: p.created_ns,
        });

        let result = engine::scan(&root, &opts, cache.as_ref())?;
        let first_scan = prev.is_none();
        let changes = if first_scan {
            Vec::new()
        } else {
            diff::diff_scan(&prev_entries, &result.entries, false)
        };

        let mut stale = Vec::new();
        let mut gone = Vec::new();
        let mut renamed = Vec::new();
        if first_scan {
            // Nothing to compare against, so every file is stale by definition. Saying
            // "no changes" here would let a half skip a tree it has never mapped.
            stale.extend(
                result
                    .entries
                    .iter()
                    .filter(|e| e.kind == fbt::entry::Kind::File)
                    .map(|e| e.path.clone()),
            );
        } else {
            for c in &changes {
                match c.kind {
                    ChangeKind::Added | ChangeKind::Modified | ChangeKind::TypeChanged => {
                        stale.push(c.path.clone())
                    }
                    ChangeKind::Deleted => gone.push(c.path.clone()),
                    ChangeKind::Renamed => {
                        stale.push(c.path.clone());
                        if let Some(from) = &c.from {
                            gone.push(from.clone());
                            renamed.push(Renamed { from: from.clone(), to: c.path.clone() });
                        }
                    }
                }
            }
        }
        stale.sort();
        gone.sort();

        let previous_root_hash = prev.as_ref().and_then(|p| p.root_hash);
        let unchanged = !first_scan
            && stale.is_empty()
            && gone.is_empty()
            && previous_root_hash == result.root_hash;

        let snapshot_id = if req.store && !unchanged {
            Some(self.store(&root_label, &result.entries, result.root_hash, created_ns, req.keep)?)
        } else {
            prev.as_ref().map(|p: &Snapshot| p.id)
        };

        let dir_hashes = req.dir_hashes.then(|| {
            result
                .entries
                .iter()
                .filter(|e| e.kind == fbt::entry::Kind::Dir)
                // THE ROOT IS NOT LISTED HERE. Its path is the empty string, and a JSON member with an
                // empty name — legal, but rejected outright by some readers, PowerShell's
                // ConvertFrom-Json among them — would make this map unparseable to the test suite. The
                // root's own hash is already `root_hash`, so leaving it out loses nothing.
                .filter(|e| !e.path.is_empty())
                .filter_map(|e| e.hash.map(|h| (e.path.clone(), hash::hex(&h))))
                .collect()
        });

        Ok(Reply {
            unchanged,
            first_scan,
            root_hash: result.root_hash.map(|h| hash::hex(&h)),
            previous_root_hash: previous_root_hash.map(|h| hash::hex(&h)),
            entries: result.entries.len(),
            read_from_disk: result.hashed,
            elapsed_ms: started.elapsed().as_millis(),
            snapshot_id,
            stale,
            gone,
            renamed,
            dir_hashes,
        })
    }

    fn store(
        &mut self,
        root_label: &str,
        entries: &[Entry],
        root_hash: Option<fbt::entry::Hash>,
        created_ns: i64,
        keep: i64,
    ) -> Result<i64> {
        let id = db::insert_snapshot(&self.conn, root_label, created_ns, None, None, None, 0)?;
        db::insert_entries(&mut self.conn, id, entries)?;
        db::finish_snapshot(&self.conn, id, entries.len() as i64, root_hash)?;
        if keep > 0 {
            db::prune(&self.conn, keep)?;
        }
        Ok(id)
    }
}

/// The canonical spelling a snapshot is keyed by: no `\\?\` prefix, forward slashes.
fn label_of(root: &Path) -> String {
    root.to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/")
}

/// Resolve a caller's path without requiring it to exist yet.
pub fn absolute(p: &str) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| PathBuf::from(p))
}
