//! WHICH CRATES A RUST FILE CAN NAME - its own package and the in-tree packages its Cargo.toml depends on by `path`
//! (directly or through `workspace = true`). `build.rs` is its own crate and sees only `[build-dependencies]`.
//! Without it one name table across the tree joined `alpha -> beta` where only beta depends on alpha.
//! Paths are root-relative folders, `""` the root itself.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// The nearest ancestor folder of `rel` (nearest first, `rel`'s own folder included) whose Cargo.toml parses with a
/// `[package]` table: a pure `[workspace]` manifest is no package. Cached per folder.
pub(crate) fn package_of(root: &Path, rel: &str, cache: &mut HashMap<String, Option<String>>) -> Option<String> {
    let mut dir = match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    };
    loop {
        let found = match cache.get(dir) {
            Some(known) => known.clone(),
            None => {
                let known = manifest(root, dir).filter(|m| m.get("package").is_some_and(|p| p.is_table())).map(|_| dir.to_string());
                cache.insert(dir.to_string(), known.clone());
                known
            }
        };
        if found.is_some() {
            return found;
        }
        if dir.is_empty() {
            return None;
        }
        dir = match dir.rfind('/') {
            Some(i) => &dir[..i],
            None => "",
        };
    }
}

/// The crate a file is compiled in: `{package}#build` for the package's own `build.rs`, else the package.
pub(crate) fn unit(rel: &str, package: &str) -> String {
    let build = if package.is_empty() { "build.rs".to_string() } else { format!("{package}/build.rs") };
    if rel == build { format!("{package}#build") } else { package.to_string() }
}

/// The in-tree packages a package depends on, for its code (`normal`, dev-dependencies included) and for its `build.rs`.
#[derive(Default)]
pub(crate) struct Reach {
    pub normal: BTreeSet<String>,
    pub build: BTreeSet<String>,
}

pub(crate) fn reach(root: &Path, package: &str) -> Reach {
    let mut out = Reach::default();
    let Some(doc) = manifest(root, package) else { return out };
    let mut tables: Vec<(&toml::Table, bool)> = Vec::new();
    take(&doc, &mut tables);
    if let Some(targets) = doc.get("target").and_then(|v| v.as_table()) {
        for cfg in targets.values().filter_map(|v| v.as_table()) {
            take(cfg, &mut tables);
        }
    }
    let mut workspace: Option<Option<(String, toml::Table)>> = None;
    for (table, build) in tables {
        for (key, spec) in table {
            let Some(spec) = spec.as_table() else { continue };
            let folder = if let Some(p) = spec.get("path").and_then(|v| v.as_str()) {
                Some(join(package, p))
            } else if spec.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                let ws = workspace.get_or_insert_with(|| workspace_of(root, package));
                ws.as_ref().and_then(|(dir, doc)| {
                    let p = doc.get("workspace")?.get("dependencies")?.get(key)?.get("path")?.as_str()?;
                    Some(join(dir, p))
                })
            } else {
                None
            };
            if let Some(folder) = folder {
                if build { out.build.insert(folder) } else { out.normal.insert(folder) };
            }
        }
    }
    out
}

/// The nearest folder AT OR ABOVE `package` whose Cargo.toml has a `[workspace]` table, with that manifest.
fn workspace_of(root: &Path, package: &str) -> Option<(String, toml::Table)> {
    let mut dir = package.to_string();
    loop {
        if let Some(doc) = manifest(root, &dir).filter(|m| m.get("workspace").is_some_and(|w| w.is_table())) {
            return Some((dir, doc));
        }
        if dir.is_empty() {
            return None;
        }
        dir = dir.rfind('/').map(|i| dir[..i].to_string()).unwrap_or_default();
    }
}

fn take<'a>(holder: &'a toml::Table, tables: &mut Vec<(&'a toml::Table, bool)>) {
    for (key, build) in [("dependencies", false), ("dev-dependencies", false), ("build-dependencies", true)] {
        if let Some(t) = holder.get(key).and_then(|v| v.as_table()) {
            tables.push((t, build));
        }
    }
}

fn manifest(root: &Path, dir: &str) -> Option<toml::Table> {
    std::fs::read_to_string(root.join(dir).join("Cargo.toml")).ok()?.parse::<toml::Table>().ok()
}

/// `base/rel` with `.` and `..` resolved, as a root-relative folder.
fn join(base: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sg-cargo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, text) in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    const WS: &str = "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\n[workspace.dependencies]\na = { path = \"a\" }\n";

    #[test]
    fn path_workspace_and_build_dependencies() {
        let root = tree("reach", &[
            ("Cargo.toml", WS),
            ("a/Cargo.toml", "[package]\nname = \"a\"\n"),
            ("b/Cargo.toml", "[package]\nname = \"b\"\n[dependencies]\na = { path = \"../a\" }\n[build-dependencies]\nz = { path = \"../z\" }\n"),
            ("c/Cargo.toml", "[package]\nname = \"c\"\n[target.'cfg(unix)'.dependencies]\na = { workspace = true }\next = \"1\"\n"),
        ]);
        let b = reach(&root, "b");
        assert_eq!(b.normal.iter().collect::<Vec<_>>(), ["a"]);
        assert_eq!(b.build.iter().collect::<Vec<_>>(), ["z"]);
        assert_eq!(reach(&root, "c").normal.iter().collect::<Vec<_>>(), ["a"]);
        assert!(reach(&root, "a").normal.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn package_skips_a_pure_workspace_manifest() {
        let root = tree("pkg", &[("Cargo.toml", WS), ("a/Cargo.toml", "[package]\nname = \"a\"\n")]);
        let mut cache = HashMap::new();
        assert_eq!(package_of(&root, "a/src/lib.rs", &mut cache).as_deref(), Some("a"));
        assert_eq!(package_of(&root, "tools/x.rs", &mut cache), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_rs_is_its_own_unit() {
        assert_eq!(unit("a/build.rs", "a"), "a#build");
        assert_eq!(unit("a/src/lib.rs", "a"), "a");
        assert_eq!(unit("build.rs", ""), "#build");
    }
}
