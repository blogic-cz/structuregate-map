//! The USN fast path: rebuild a tree map from the previous one plus the journal.
//!
//! A full walk costs one directory enumeration per directory, whatever changed.
//! This path reads the volume journal instead, touches only the nodes it names,
//! and copies the rest of the map forward untouched. On a large tree with a few
//! edits it is two orders of magnitude cheaper.
//!
//! It refuses rather than guesses. Any condition it cannot resolve exactly — a
//! recreated journal, a wrapped ring buffer, a path it cannot place under the
//! root — returns an error, and the caller falls back to a full scan.

use crate::db::Snapshot;
use crate::entry::{key_of, now_unix_ns, parent_key_of, systemtime_to_unix_ns, Entry, Hash, Kind};
use crate::hash;
use crate::scan::ScanOptions;
use crate::usn;
use crate::walk::{self, Filter};
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

#[derive(Debug)]
pub struct IncrementalResult {
    pub entries: Vec<Entry>,
    pub root_hash: Option<Hash>,
    pub journal_id: i64,
    pub next_usn: i64,
    /// Journal rows that landed inside the scan root.
    pub relevant: usize,
    /// Nodes re-read from disk.
    pub touched: usize,
    pub elapsed_ms: u128,
}

/// Rebuild the map for `root` from `prev` plus the journal entries after it.
pub fn update(
    root: &Path,
    prev: &Snapshot,
    prev_entries: &HashMap<String, Entry>,
    opts: &ScanOptions,
) -> Result<IncrementalResult> {
    let t0 = Instant::now();

    let stored_usn = prev.usn.context("previous snapshot has no USN position")?;
    let stored_journal = prev.journal_id.context("previous snapshot has no journal id")?;

    let volume = usn::volume_of(root)?;
    if prev.volume.as_deref().map(str::to_uppercase) != Some(volume.to_uppercase()) {
        bail!("snapshot was taken on a different volume");
    }
    let vol = usn::open_volume(&volume)?;
    let info = usn::query_journal(&vol)?;
    if info.journal_id != stored_journal as u64 {
        bail!("journal was recreated since the last scan");
    }

    let (changes, next_usn) = usn::read_changes(&vol, &info, stored_usn)?;

    let filter = Filter::new(
        root,
        opts.use_gitignore,
        &opts.extra_skip,
        &opts.skip_paths,
        opts.no_default_skip,
    );
    let root_abs = normalized_root(root)?;

    // Reverse index of the stored map, so a journal record resolves to a path
    // without any syscall in the common case.
    let by_id: HashMap<u64, &Entry> = prev_entries
        .values()
        .filter_map(|e| e.file_id.map(|id| (id, e)))
        .collect();

    let mut candidates: HashSet<String> = HashSet::new();
    let mut subtrees: HashSet<String> = HashSet::new();
    let mut relevant = 0usize;

    for c in &changes {
        // Old location, known whenever the node was already in the map. This is
        // what a delete or a rename-away resolves through.
        if let Some(e) = by_id.get(&c.file_ref) {
            relevant += 1;
            candidates.insert(e.path.clone());
            if e.kind == Kind::Dir {
                subtrees.insert(e.path.clone());
            }
        }

        // New location, resolved through the parent. A created or moved-in node
        // is not in the map yet, so this is the only way to place it.
        if let Some(rel) = resolve_new_path(&vol, c, &by_id, &root_abs) {
            if filter.skip_resolved(&rel, c.is_dir()) {
                continue;
            }
            relevant += 1;
            candidates.insert(rel.clone());
            if c.is_dir() && c.created_or_moved_in() {
                subtrees.insert(rel);
            }
        }
    }

    let mut map: HashMap<String, Entry> = prev_entries.clone();
    let mut touched = 0usize;

    for rel in &candidates {
        touched += apply_one(root, rel, &filter, &mut map)?;
    }
    // A directory that appeared, or moved in as a whole, brings children the
    // journal never listed individually. Enumerate those subtrees.
    for rel in &subtrees {
        let abs = root.join(rel);
        if abs.is_dir() {
            touched += graft_subtree(root, rel, &filter, &mut map)?;
        }
    }

    let mut entries: Vec<Entry> = map.into_values().collect();
    entries.sort_by(|a, b| a.path_key.cmp(&b.path_key));
    let root_hash = hash::merkle_rollup(&mut entries);

    Ok(IncrementalResult {
        entries,
        root_hash,
        journal_id: info.journal_id as i64,
        next_usn,
        relevant,
        touched,
        elapsed_ms: t0.elapsed().as_millis(),
    })
}

/// The scan root as a plain absolute path, lowercased, `/` separated.
fn normalized_root(root: &Path) -> Result<String> {
    let abs = std::fs::canonicalize(root)?;
    let s = abs.to_string_lossy().replace("\\\\?\\", "").replace('\\', "/");
    Ok(s.trim_end_matches('/').to_lowercase())
}

/// Path of a journal record relative to the scan root, or `None` when it sits
/// outside the root. The journal covers the whole volume, so most records do.
fn resolve_new_path(
    vol: &usn::Volume,
    c: &usn::UsnChange,
    by_id: &HashMap<u64, &Entry>,
    root_abs: &str,
) -> Option<String> {
    if let Some(parent) = by_id.get(&c.parent_ref) {
        return Some(if parent.path.is_empty() {
            c.name.clone()
        } else {
            format!("{}/{}", parent.path, c.name)
        });
    }
    // Parent unknown: ask the filesystem. Only reached for directories created
    // since the last scan, so the syscall cost stays bounded.
    let parent_abs = usn::path_of_ref(vol, c.parent_ref)?;
    let parent_abs = parent_abs.replace('\\', "/");
    let parent_lower = parent_abs.to_lowercase();
    let parent_lower = parent_lower.trim_end_matches('/');

    if parent_lower == root_abs {
        return Some(c.name.clone());
    }
    let prefix = format!("{root_abs}/");
    let rest = parent_lower.strip_prefix(&prefix)?;
    // Keep the original case from the resolved path, not the lowercased copy.
    let start = parent_abs.len() - rest.len();
    Some(format!("{}/{}", &parent_abs[start..], c.name))
}

/// Re-read one node and write it into the map, or remove it if it is gone.
/// Returns how many nodes were read from disk.
fn apply_one(
    root: &Path,
    rel: &str,
    filter: &Filter,
    map: &mut HashMap<String, Entry>,
) -> Result<usize> {
    let key = key_of(rel);
    let abs = root.join(rel);

    let meta = match std::fs::symlink_metadata(&abs) {
        Ok(m) => m,
        Err(_) => {
            // Gone. A directory takes its whole subtree with it.
            let prefix = format!("{key}/");
            map.remove(&key);
            map.retain(|k, _| !k.starts_with(&prefix));
            return Ok(0);
        }
    };

    let is_symlink = meta.file_type().is_symlink();
    let is_dir = meta.is_dir() && !is_symlink;
    if filter.skip_resolved(rel, is_dir) {
        let prefix = format!("{key}/");
        map.remove(&key);
        map.retain(|k, _| !k.starts_with(&prefix));
        return Ok(0);
    }

    let mut read = ensure_ancestors(root, rel, map)?;
    map.insert(key.clone(), entry_from_disk(root, rel, &key, &meta, is_symlink, is_dir)?);
    read += 1;
    Ok(read)
}

/// Add any directory on the way to `rel` that the map does not hold yet.
///
/// A node whose parent is missing is invisible to the merkle rollup: nothing
/// lists it as a child, so the root hash does not move even though the map grew.
/// A file created inside a directory that is also new arrives exactly that way,
/// because the journal names the file and the directory as separate records and
/// nothing orders them.
fn ensure_ancestors(
    root: &Path,
    rel: &str,
    map: &mut HashMap<String, Entry>,
) -> Result<usize> {
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    let mut added = 0usize;
    for i in 0..parts.len().saturating_sub(1) {
        let prefix = parts[..=i].join("/");
        let key = key_of(&prefix);
        if map.contains_key(&key) {
            continue;
        }
        let abs = root.join(&prefix);
        let Ok(meta) = std::fs::symlink_metadata(&abs) else {
            continue;
        };
        let is_symlink = meta.file_type().is_symlink();
        let is_dir = meta.is_dir() && !is_symlink;
        map.insert(
            key.clone(),
            entry_from_disk(root, &prefix, &key, &meta, is_symlink, is_dir)?,
        );
        added += 1;
    }
    Ok(added)
}

/// Walk a directory that the journal reported only as a single node, and add
/// everything under it.
fn graft_subtree(
    root: &Path,
    rel: &str,
    filter: &Filter,
    map: &mut HashMap<String, Entry>,
) -> Result<usize> {
    let sub_root = root.join(rel);
    let raw = walk::walk(&sub_root, filter)?;
    let mut n = 0usize;
    for r in raw {
        if r.rel.is_empty() {
            continue;
        }
        let child_rel = format!("{}/{}", rel, r.rel);
        let key = key_of(&child_rel);
        let abs = root.join(&child_rel);
        let meta = match std::fs::symlink_metadata(&abs) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_symlink = meta.file_type().is_symlink();
        let is_dir = meta.is_dir() && !is_symlink;
        map.insert(key.clone(), entry_from_disk(root, &child_rel, &key, &meta, is_symlink, is_dir)?);
        n += 1;
    }
    Ok(n)
}

fn entry_from_disk(
    root: &Path,
    rel: &str,
    key: &str,
    meta: &std::fs::Metadata,
    is_symlink: bool,
    is_dir: bool,
) -> Result<Entry> {
    let abs = root.join(rel);
    let kind = if is_symlink {
        Kind::Link
    } else if is_dir {
        Kind::Dir
    } else {
        Kind::File
    };
    let size = if kind == Kind::File { meta.len() } else { 0 };
    let mtime_ns = meta.modified().map(systemtime_to_unix_ns).unwrap_or(now_unix_ns());

    let hash_value = match kind {
        Kind::Dir => None,
        Kind::Link => Some(hash::hash_link(
            &std::fs::read_link(&abs)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        )),
        Kind::File => Some(
            hash::hash_file(&abs, size).unwrap_or_else(|_| hash::hash_unreadable(size, mtime_ns)),
        ),
    };

    #[cfg(windows)]
    let file_id = walk::win::file_id_of(&abs).ok();
    #[cfg(not(windows))]
    let file_id = None;

    Ok(Entry {
        path: rel.to_string(),
        path_key: key.to_string(),
        parent_key: parent_key_of(key),
        kind,
        size,
        mtime_ns,
        file_id,
        hash: hash_value,
    })
}
