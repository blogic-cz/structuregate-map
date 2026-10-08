//! Content hashing and the directory merkle rollup.

use crate::entry::{Entry, Hash, Kind};
use std::collections::HashMap;
use std::path::Path;

/// Files at or above this size are hashed through a memory map. Below it the
/// read-and-update path is cheaper than setting up the mapping.
///
/// The hashing itself stays single threaded per file, because the caller already
/// hashes many files in parallel; a second rayon layer would only add contention.
const MMAP_THRESHOLD: u64 = 256 * 1024;

/// BLAKE3 of one file's content.
pub fn hash_file(path: &Path, size: u64) -> std::io::Result<Hash> {
    let mut hasher = blake3::Hasher::new();
    if size >= MMAP_THRESHOLD {
        hasher.update_mmap(path)?;
    } else {
        let bytes = std::fs::read(path)?;
        hasher.update(&bytes);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Hash of a symlink or junction: the target path text, not the target content.
/// Following the link would risk cycles and would hide retargeting.
pub fn hash_link(target: &str) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"link:");
    hasher.update(target.to_lowercase().as_bytes());
    *hasher.finalize().as_bytes()
}

/// Hash used when a file cannot be read (locked, permission denied). Folds the
/// stat data in, so a later change is still seen, and the scan does not fail.
pub fn hash_unreadable(size: u64, mtime_ns: i64) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"unreadable:");
    hasher.update(&size.to_le_bytes());
    hasher.update(&mtime_ns.to_le_bytes());
    *hasher.finalize().as_bytes()
}

/// Fill in every directory hash bottom up and return the root hash.
///
/// A directory hash covers each child's name, kind and hash, in `path_key`
/// order. So it changes when a child changes, is added, removed or renamed, and
/// two directories with the same content hash the same wherever they sit.
pub fn merkle_rollup(entries: &mut [Entry]) -> Option<Hash> {
    // Children grouped by parent, and where each entry sits in the slice.
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut max_depth = 0usize;

    for (i, e) in entries.iter().enumerate() {
        index.insert(e.path_key.clone(), i);
        max_depth = max_depth.max(e.depth());
        if let Some(p) = &e.parent_key {
            children.entry(p.clone()).or_default().push(i);
        }
    }

    // Deepest directories first, so a parent always sees finished children.
    let mut dirs: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == Kind::Dir)
        .map(|(i, _)| i)
        .collect();
    dirs.sort_by_key(|i| std::cmp::Reverse(entries[*i].depth()));

    for di in dirs {
        let key = entries[di].path_key.clone();
        let mut kids: Vec<usize> = children.get(&key).cloned().unwrap_or_default();
        kids.sort_by(|a, b| entries[*a].path_key.cmp(&entries[*b].path_key));

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"dir:");
        for ki in kids {
            let child = &entries[ki];
            hasher.update(child.name().to_lowercase().as_bytes());
            hasher.update(&[0x00, child.kind as u8]);
            hasher.update(&child.hash.unwrap_or([0u8; 32]));
        }
        entries[di].hash = Some(*hasher.finalize().as_bytes());
    }

    // The root is the one entry without a parent; with no root row, fold the
    // top-level entries together instead.
    if let Some(i) = entries.iter().position(|e| e.parent_key.is_none() && e.kind == Kind::Dir) {
        return entries[i].hash;
    }
    let mut top: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.parent_key.is_none())
        .map(|(i, _)| i)
        .collect();
    if top.is_empty() {
        return None;
    }
    top.sort_by(|a, b| entries[*a].path_key.cmp(&entries[*b].path_key));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"root:");
    for i in top {
        hasher.update(entries[i].path_key.as_bytes());
        hasher.update(&[0x00]);
        hasher.update(&entries[i].hash.unwrap_or([0u8; 32]));
    }
    Some(*hasher.finalize().as_bytes())
}

pub fn hex(h: &Hash) -> String {
    let mut s = String::with_capacity(64);
    for b in h {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn hex_short(h: &Hash) -> String {
    hex(h)[..12].to_string()
}
