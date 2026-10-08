//! `--doc-root <dir>`: THE DOCS OF A PROJECT WHOSE CODE IS MAPPED FROM SEVERAL ROOTS. A python project
//! maps each import tree as its own `--root`, because each is its own `sys.path` entry; its CLAUDE.md, its
//! `.claude/` and every doc at the project's top sit in none of them, and a doc inside one names paths from the
//! PROJECT root (`py/core/CLAUDE.md`) that no root resolves. `--root .` is no answer: every source would be
//! mapped twice and the imports stop resolving.
//!
//! So the docs get their own root. Every `.md` under it is mapped ONCE, keyed by its path from that root, and
//! the code roots map none. A path a doc names is resolved from the doc root and the doc's own folder, as a
//! reader would, and one that lands inside a code root is named as THAT root names it (`core/config.py`), so
//! a mention joins the file the code half mapped.

use crate::sources;
use std::collections::HashSet;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

/// The doc root's walk: every file under it (where a mention may resolve) and the docs to map.
pub struct Docs {
    pub root: String,
    pub tree: Vec<String>,
    pub markdown: Vec<(String, String)>,
}

pub fn walk(root: &str, skip: &[String], tracked: bool, include_untracked: bool, is_doc: &dyn Fn(&str) -> bool) -> Docs {
    // IN A GIT REPOSITORY, WHAT GIT WOULD KEEP: tracked files and untracked ones it does not ignore. A doc root is a
    // project's top, and pytest's `.pytest_cache/README.md` - ignored by the cache's own `.gitignore` - was mapped and
    // reported DOC-ORPHAN. The code roots keep their own walk.
    let in_git = sources::repository(Path::new(root)).is_some();
    let found = sources::walked(&sources::Input {
        root: root.to_string(),
        skip: skip.to_vec(),
        tracked: tracked || in_git,
        include_untracked: if tracked { include_untracked } else { in_git },
        generated_cs: false,
    });
    let mut docs = Docs { root: root.to_string(), tree: Vec::new(), markdown: Vec::new() };
    let top = lexical(Path::new(root));
    for (abs, listed) in found.found {
        // GIT LISTS FROM THE REPOSITORY'S TOP; a doc is keyed from the doc root.
        let rel = lexical(Path::new(&abs)).strip_prefix(&top).map_or(listed, |inside| inside.to_string_lossy().replace('\\', "/"));
        docs.tree.push(rel.clone());
        if is_doc(&rel) {
            docs.markdown.push((rel, abs));
        }
    }
    docs
}

/// A path a doc under `doc_root` names, as the map keys it: a DOC mapped from the doc root by its key there (`docs`) -
/// a doc inside a code root is mapped once, from the doc root, and keyed by the code root its mention never joined
/// it; a code file under a code root with that root's prefix; anything else from the doc root.
pub fn keyed(doc_root: &str, roots: &[(String, String)], docs: &HashSet<&str>, path: &str) -> String {
    let abs = lexical(&Path::new(doc_root).join(path.replace('/', std::path::MAIN_SEPARATOR_STR)));
    let from_doc_root = abs.strip_prefix(lexical(Path::new(doc_root))).ok().map(|inside| inside.to_string_lossy().replace('\\', "/"));
    if let Some(rel) = &from_doc_root
        && docs.contains(rel.as_str())
    {
        return rel.clone();
    }
    for (root, prefix) in roots {
        let root = lexical(Path::new(root.trim_end_matches(MAIN_SEPARATOR)));
        if let Ok(inside) = abs.strip_prefix(&root) {
            return format!("{prefix}{}", inside.to_string_lossy().replace('\\', "/"));
        }
    }
    from_doc_root.unwrap_or_else(|| path.to_string())
}

fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_under_a_code_root_is_named_as_that_root_names_it() {
        let sep = MAIN_SEPARATOR;
        let doc_root = format!("{sep}p");
        let roots = vec![(format!("{sep}p{sep}py{sep}core"), "core/".to_string()), (format!("{sep}p{sep}scripts"), "scripts/".to_string())];
        let docs: HashSet<&str> = ["py/core/notes.md"].into();
        assert_eq!(keyed(&doc_root, &roots, &docs, "py/core/config.py"), "core/config.py");
        assert_eq!(keyed(&doc_root, &roots, &docs, "py/core"), "core/");
        assert_eq!(keyed(&doc_root, &roots, &docs, "scripts/check.py"), "scripts/check.py");
        assert_eq!(keyed(&doc_root, &roots, &docs, "docs/guide.md"), "docs/guide.md");
        assert_eq!(keyed(&doc_root, &roots, &docs, "py/core/../other/x.py"), "py/other/x.py");
        // A doc inside a code root keeps the key it is mapped under.
        assert_eq!(keyed(&doc_root, &roots, &docs, "py/core/notes.md"), "py/core/notes.md");
    }
}
