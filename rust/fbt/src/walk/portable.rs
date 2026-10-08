//! Portable walker on `std::fs::read_dir`.
//!
//! Correct everywhere, but it costs one `stat` syscall per node and cannot
//! supply a file id, so rename detection falls back to content matching.

use super::{Filter, RawEntry};
use crate::entry::{systemtime_to_unix_ns, Kind};
use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;

pub fn walk(root: &Path, filter: &Filter) -> Result<Vec<RawEntry>> {
    let mut out = vec![RawEntry {
        rel: String::new(),
        kind: Kind::Dir,
        size: 0,
        mtime_ns: 0,
        file_id: None,
        link_target: None,
    }];

    let mut level = vec![String::new()];
    while !level.is_empty() {
        let results: Vec<(Vec<RawEntry>, Vec<String>)> = level
            .par_iter()
            .map(|rel| read_one(root, rel, filter))
            .collect();

        let mut next = Vec::new();
        for (entries, dirs) in results {
            out.extend(entries);
            next.extend(dirs);
        }
        level = next;
    }
    Ok(out)
}

fn read_one(root: &Path, rel: &str, filter: &Filter) -> (Vec<RawEntry>, Vec<String>) {
    let dir = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
    let mut entries = Vec::new();
    let mut subdirs = Vec::new();

    let iter = match std::fs::read_dir(&dir) {
        Ok(i) => i,
        Err(_) => return (entries, subdirs),
    };

    for item in iter.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };

        let meta = match item.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_symlink = meta.file_type().is_symlink();
        let is_dir = meta.is_dir() && !is_symlink;

        if filter.skip(&child_rel, &name, is_dir) {
            continue;
        }

        let mtime_ns = meta.modified().map(systemtime_to_unix_ns).unwrap_or(0);
        let kind = if is_symlink {
            Kind::Link
        } else if is_dir {
            Kind::Dir
        } else {
            Kind::File
        };
        let link_target = if is_symlink {
            std::fs::read_link(item.path())
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        } else {
            None
        };

        if is_dir {
            subdirs.push(child_rel.clone());
        }
        entries.push(RawEntry {
            rel: child_rel,
            kind,
            size: if kind == Kind::File { meta.len() } else { 0 },
            mtime_ns,
            file_id: None,
            link_target,
        });
    }
    (entries, subdirs)
}
