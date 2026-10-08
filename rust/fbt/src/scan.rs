//! Turn a directory tree into `Entry` rows: walk, hash what moved, roll up merkle.

use crate::entry::{key_of, parent_key_of, Entry, Hash, Kind};
use crate::hash;
use crate::walk::{self, Filter, RawEntry};
use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub use_gitignore: bool,
    pub extra_skip: Vec<String>,
    pub no_default_skip: bool,
    /// Exact relative paths pruned with their subtree. The database file goes
    /// here, so the map never describes itself.
    pub skip_paths: Vec<String>,
    /// Read every file even when size and mtime say it is unchanged. Slow; use
    /// it on a volume with coarse timestamps, or to verify the cache.
    pub rehash_all: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            use_gitignore: true,
            extra_skip: Vec::new(),
            no_default_skip: false,
            skip_paths: Vec::new(),
            rehash_all: false,
        }
    }
}

#[derive(Debug)]
pub struct ScanResult {
    pub entries: Vec<Entry>,
    pub root_hash: Option<Hash>,
    /// Files whose content was read this scan.
    pub hashed: usize,
    /// Files whose hash came from the previous snapshot.
    pub reused: usize,
    pub walk_ms: u128,
    pub hash_ms: u128,
}

/// The previous snapshot used as a stat cache: its entries, and the wall clock
/// at which it was taken.
pub struct Cache<'a> {
    pub entries: &'a HashMap<String, Entry>,
    pub created_ns: i64,
}

pub fn scan(root: &Path, opts: &ScanOptions, cache: Option<&Cache<'_>>) -> Result<ScanResult> {
    let filter = Filter::new(
        root,
        opts.use_gitignore,
        &opts.extra_skip,
        &opts.skip_paths,
        opts.no_default_skip,
    );

    let t0 = Instant::now();
    let raw = walk::walk(root, &filter)?;
    let walk_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    let hashed = AtomicUsize::new(0);
    let reused = AtomicUsize::new(0);

    let mut entries: Vec<Entry> = raw
        .par_iter()
        .map(|r| build_entry(root, r, opts, cache, &hashed, &reused))
        .collect();

    let root_hash = hash::merkle_rollup(&mut entries);
    let hash_ms = t1.elapsed().as_millis();

    Ok(ScanResult {
        entries,
        root_hash,
        hashed: hashed.load(Ordering::Relaxed),
        reused: reused.load(Ordering::Relaxed),
        walk_ms,
        hash_ms,
    })
}

fn build_entry(
    root: &Path,
    r: &RawEntry,
    opts: &ScanOptions,
    cache: Option<&Cache<'_>>,
    hashed: &AtomicUsize,
    reused: &AtomicUsize,
) -> Entry {
    let path = r.rel.clone();
    let path_key = key_of(&path);
    let parent_key = parent_key_of(&path_key);

    let hash_value = match r.kind {
        // Directory hashes are filled in by the merkle rollup.
        Kind::Dir => None,
        Kind::Link => Some(hash::hash_link(r.link_target.as_deref().unwrap_or(""))),
        Kind::File => {
            if let Some(h) = cached_hash(r, &path_key, opts, cache) {
                reused.fetch_add(1, Ordering::Relaxed);
                Some(h)
            } else {
                hashed.fetch_add(1, Ordering::Relaxed);
                let abs = root.join(&path);
                Some(
                    hash::hash_file(&abs, r.size)
                        .unwrap_or_else(|_| hash::hash_unreadable(r.size, r.mtime_ns)),
                )
            }
        }
    };

    Entry {
        path,
        path_key,
        parent_key,
        kind: r.kind,
        size: r.size,
        mtime_ns: r.mtime_ns,
        file_id: r.file_id,
        hash: hash_value,
    }
}

/// Reuse the stored hash only when size and mtime both match, and the file was
/// already settled when the previous snapshot was taken.
///
/// The second condition is the racy-timestamp guard. A file written in the same
/// clock tick as the snapshot can keep its size and mtime while its content
/// differs, so any file whose mtime is not strictly older than the snapshot is
/// read again.
fn cached_hash(
    r: &RawEntry,
    path_key: &str,
    opts: &ScanOptions,
    cache: Option<&Cache<'_>>,
) -> Option<Hash> {
    if opts.rehash_all {
        return None;
    }
    let cache = cache?;
    let prev = cache.entries.get(path_key)?;
    if prev.kind != Kind::File || prev.size != r.size || prev.mtime_ns != r.mtime_ns {
        return None;
    }
    if r.mtime_ns >= cache.created_ns {
        return None;
    }
    prev.hash
}
