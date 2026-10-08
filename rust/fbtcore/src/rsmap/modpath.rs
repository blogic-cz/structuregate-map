//! WHERE `mod name;` POINTS - the one import in rust that names a FILE, resolved the way rustc does.
//!
//! It is an exact edge, so it is resolved here against the filesystem and handed back as a path; a
//! declaration that points at no file is BROKEN, not external. Everything else a rust file names goes
//! through the name join, like every other half's names do.
//!
//! THE RULE, from the reference: a file that OWNS its directory - a crate root, or a `mod.rs` - puts
//! its children beside itself; any other `a.rs` puts them in `a/`. Each enclosing inline `mod x { }`
//! adds a directory. `#[path]` overrides the name and, outside an inline block, is relative to the
//! declaring file's own directory.

use super::items::ModRef;
use std::path::Path;

/// A file cargo compiles as the ROOT of a crate: its `mod`s resolve beside it, and nothing imports
/// it, so it may have no reader. `lib.rs` and `main.rs` are the conventional roots; `src/bin/*.rs`,
/// `tests/*.rs`, `examples/*.rs`, `benches/*.rs` and `build.rs` are the ones cargo finds by place.
pub fn is_crate_root(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let name = parts[parts.len() - 1];
    if matches!(name, "lib.rs" | "main.rs" | "build.rs") {
        return true;
    }
    let parent = parts.len().checked_sub(2).map(|i| parts[i]);
    let grandparent = parts.len().checked_sub(3).map(|i| parts[i]);
    // A `tests/` folder INSIDE `src/` is a module directory like any other, not cargo's.
    let under_src = parts[..parts.len() - 1].contains(&"src");
    (matches!(parent, Some("tests" | "examples" | "benches")) && !under_src)
        || (parent == Some("bin") && grandparent == Some("src"))
}

/// The file a `mod` declaration names, relative to the same root as `rel`. When neither `name.rs`
/// nor `name/mod.rs` exists the first is returned anyway: the map then reports the declaration as
/// BROKEN, which is what it is.
pub fn resolve(root: &Path, rel: &str, m: &ModRef) -> String {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let stem = file.strip_suffix(".rs").unwrap_or(file);
    let owns_dir = stem == "mod" || is_crate_root(rel);
    let mut base: Vec<String> = segments(dir);
    if !owns_dir {
        base.push(stem.to_string());
    }
    for inline in &m.inline {
        base.extend(segments(inline));
    }

    if let Some(path) = &m.path_attr {
        let mut from = if m.inline.is_empty() { segments(dir) } else { base };
        from.extend(segments(path));
        return normal(&from);
    }

    let beside = {
        let mut p = base.clone();
        p.push(format!("{}.rs", m.name));
        normal(&p)
    };
    let nested = {
        let mut p = base;
        p.push(m.name.clone());
        p.push("mod.rs".to_string());
        normal(&p)
    };
    if !root.join(&beside).is_file() && root.join(&nested).is_file() { nested } else { beside }
}

fn segments(path: &str) -> Vec<String> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// `a/./b/../c` -> `a/c`, so a `#[path = "../x.rs"]` lands on the key the map holds the file under.
fn normal(parts: &[String]) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in parts {
        match part.as_str() {
            "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(name: &str, path_attr: Option<&str>, inline: &[&str]) -> ModRef {
        ModRef {
            name: name.to_string(),
            path_attr: path_attr.map(str::to_string),
            inline: inline.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn tree(files: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("fbt-modpath-{}", blake3::hash(files.join("|").as_bytes()).to_hex()));
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        root
    }

    #[test]
    fn a_crate_root_and_a_mod_rs_put_children_beside_themselves() {
        let root = tree(&["src/lib.rs", "src/a.rs", "src/b/mod.rs", "src/b/c.rs"]);
        assert_eq!(resolve(&root, "src/lib.rs", &declared("a", None, &[])), "src/a.rs");
        assert_eq!(resolve(&root, "src/lib.rs", &declared("b", None, &[])), "src/b/mod.rs");
        assert_eq!(resolve(&root, "src/b/mod.rs", &declared("c", None, &[])), "src/b/c.rs");
    }

    #[test]
    fn any_other_file_puts_its_children_in_a_folder_named_after_it() {
        let root = tree(&["src/rows.rs", "src/rows/ts.rs"]);
        assert_eq!(resolve(&root, "src/rows.rs", &declared("ts", None, &[])), "src/rows/ts.rs");
    }

    #[test]
    fn an_inline_block_adds_a_directory_and_a_path_attribute_is_relative_to_the_file() {
        let root = tree(&["src/lib.rs"]);
        assert_eq!(resolve(&root, "src/lib.rs", &declared("c", None, &["outer"])), "src/outer/c.rs");
        assert_eq!(
            resolve(&root, "src/rows/ts/mod.rs", &declared("g", Some("gate/g.rs"), &[])),
            "src/rows/ts/gate/g.rs"
        );
        assert_eq!(resolve(&root, "src/x.rs", &declared("y", Some("../y.rs"), &[])), "y.rs");
    }

    #[test]
    fn a_declaration_that_names_no_file_resolves_to_the_file_it_would_need() {
        let root = tree(&["src/main.rs"]);
        assert_eq!(resolve(&root, "src/main.rs", &declared("gone", None, &[])), "src/gone.rs");
    }

    #[test]
    fn cargo_finds_crate_roots_by_place() {
        for rel in ["src/lib.rs", "build.rs", "src/bin/tool.rs", "tests/it.rs", "examples/demo.rs"] {
            assert!(is_crate_root(rel), "{rel}");
        }
        for rel in ["src/scan.rs", "src/bin/helpers/x.rs", "src/tests/y.rs"] {
            assert!(!is_crate_root(rel), "{rel}");
        }
    }
}
