//! Directory enumeration. One backend per platform, one shared output type.

use crate::entry::Kind;
use anyhow::Result;
use ignore::gitignore::Gitignore;
use std::path::Path;

#[cfg(windows)]
pub mod win;

pub mod portable;

/// What a walker reports per node, before any content hashing.
#[derive(Clone, Debug)]
pub struct RawEntry {
    /// Path relative to the scan root, `/` separated, original case.
    pub rel: String,
    pub kind: Kind,
    pub size: u64,
    pub mtime_ns: i64,
    pub file_id: Option<u64>,
    /// Link target text, only for `Kind::Link`.
    pub link_target: Option<String>,
}

/// Which paths the walk skips. Pruning a directory here is the single biggest
/// win in the whole scan, because the subtree is never enumerated at all.
pub struct Filter {
    gitignore: Option<Gitignore>,
    /// Directory names always pruned, matched case insensitively.
    always_skip: Vec<String>,
    /// Exact relative paths pruned with their subtree, lowercased. Used for the
    /// database file, which must never appear in the map it describes.
    skip_prefixes: Vec<String>,
    pub follow_links: bool,
}

/// Directories that are build output or tool state. Scanning them wastes most of
/// the time and none of the results are inputs.
pub const DEFAULT_SKIP: &[&str] = &[
    // The tool's own database lives here. Scanning it would report a change on
    // every run, because writing the snapshot changes the file.
    ".fbt",
    ".git", ".hg", ".svn", ".jj", "node_modules", "target", "bin", "obj",
    ".venv", "venv", "__pycache__", ".gradle", ".idea", ".vs", ".next",
    "dist", "build", ".cargo", ".mypy_cache", ".pytest_cache",
];

impl Filter {
    pub fn new(
        root: &Path,
        use_gitignore: bool,
        extra_skip: &[String],
        skip_paths: &[String],
        no_default_skip: bool,
    ) -> Self {
        let gitignore = if use_gitignore {
            let mut b = ignore::gitignore::GitignoreBuilder::new(root);
            let gi = root.join(".gitignore");
            if gi.exists() {
                let _ = b.add(&gi);
            }
            b.build().ok()
        } else {
            None
        };

        let mut always_skip: Vec<String> = if no_default_skip {
            Vec::new()
        } else {
            DEFAULT_SKIP.iter().map(|s| s.to_string()).collect()
        };
        always_skip.extend(extra_skip.iter().map(|s| s.to_lowercase()));

        let skip_prefixes = skip_paths
            .iter()
            .map(|p| p.replace('\\', "/").trim_matches('/').to_lowercase())
            .filter(|p| !p.is_empty())
            .collect();

        Filter { gitignore, always_skip, skip_prefixes, follow_links: false }
    }

    /// Like `skip`, but for a path that was resolved without walking down to it.
    ///
    /// The walker gets ancestor pruning for free: it never descends into a
    /// skipped directory, so it never asks about anything below one. A path the
    /// journal resolved arrives with no such history, so every ancestor has to be
    /// tested as a directory in its own right. Without this, a rule like
    /// `.nx/cache` would prune the walk but let `.nx/cache/d/daemon.log` through.
    pub fn skip_resolved(&self, rel: &str, is_dir: bool) -> bool {
        let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        for (i, name) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            let prefix = parts[..=i].join("/");
            if self.skip(&prefix, name, if last { is_dir } else { true }) {
                return true;
            }
        }
        false
    }

    /// True when this node must not appear in the map. For a directory that also
    /// means its whole subtree is never enumerated.
    pub fn skip(&self, rel: &str, name: &str, is_dir: bool) -> bool {
        if is_dir {
            let lower = name.to_lowercase();
            if self.always_skip.contains(&lower) {
                return true;
            }
        }
        if !self.skip_prefixes.is_empty() {
            let rel_lower = rel.to_lowercase();
            if self
                .skip_prefixes
                .iter()
                .any(|p| rel_lower == *p || rel_lower.starts_with(&format!("{p}/")))
            {
                return true;
            }
        }
        if let Some(gi) = &self.gitignore
            && gi.matched(rel, is_dir).is_ignore() {
                return true;
            }
        false
    }
}

/// Enumerate the whole tree under `root`, deepest backend available.
pub fn walk(root: &Path, filter: &Filter) -> Result<Vec<RawEntry>> {
    #[cfg(windows)]
    {
        match win::walk(root, filter) {
            Ok(v) => return Ok(v),
            Err(e) => {
                eprintln!("fast walk unavailable ({e}), falling back to portable walk");
            }
        }
    }
    portable::walk(root, filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter_with(skip: &[&str]) -> Filter {
        let skip: Vec<String> = skip.iter().map(|s| s.to_string()).collect();
        Filter::new(Path::new("."), false, &skip, &[], true)
    }

    #[test]
    fn a_resolved_path_is_pruned_by_a_skipped_ancestor() {
        let f = filter_with(&["cache"]);
        // The walker never reaches this, because it prunes at `cache`. A path the
        // journal resolved has to be pruned by testing the ancestor explicitly.
        assert!(f.skip_resolved("nx/cache/d/daemon.log", false));
        assert!(!f.skip_resolved("nx/src/app.ts", false));
    }

    #[test]
    fn a_skip_prefix_prunes_the_whole_subtree() {
        let f = Filter::new(Path::new("."), false, &[], &[".fbt".to_string()], true);
        assert!(f.skip_resolved(".fbt/map.db", false));
        assert!(f.skip_resolved(".fbt", true));
        assert!(!f.skip_resolved("fbt-other/map.db", false));
    }
}
