//! WHERE AN IMPORT PATH POINTS, by the module it is in. A Go import names a PACKAGE by its path - `example.com/m/
//! internal/store` - and a package is a folder: the nearest `go.mod` above a file says which path prefix is this tree's
//! own and which folder it starts at. A path under no module of the tree is the standard library or a dependency, and
//! is not an edge.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Folder -> the module that folder is in, `(module folder, module path)`, asked once per folder for the whole run:
/// files are mapped on every core, and most share a handful of folders.
fn known() -> &'static Mutex<HashMap<PathBuf, Option<(PathBuf, String)>>> {
    static KNOWN: OnceLock<Mutex<HashMap<PathBuf, Option<(PathBuf, String)>>>> = OnceLock::new();
    KNOWN.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The module a folder is in: the nearest `go.mod` at or above it, no higher than `root`.
fn module_of(root: &Path, folder: &Path) -> Option<(PathBuf, String)> {
    if let Some(found) = known().lock().ok().and_then(|k| k.get(folder).cloned()) {
        return found;
    }
    let found = match std::fs::read_to_string(folder.join("go.mod")).ok().and_then(|text| module_path(&text)) {
        Some(module) => Some((folder.to_path_buf(), module)),
        None if folder == root => None,
        None => folder.parent().filter(|up| up.starts_with(root)).and_then(|up| module_of(root, up)),
    };
    if let Ok(mut map) = known().lock() {
        map.insert(folder.to_path_buf(), found.clone());
    }
    found
}

/// The `module` directive of a `go.mod`, quoted or not, a trailing comment dropped.
fn module_path(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("module") {
            let path = rest.trim().trim_matches('"').trim_matches('`');
            if !path.is_empty() && rest.starts_with(char::is_whitespace) {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// The folder an import names, `/`-separated and relative to `root` - None for a package outside this tree's modules.
/// As the go command resolves it: the module's own path prefix; then a VENDORED copy, `<module>/vendor/<import>`; and
/// in the standard library (`module std`), whose import paths carry no prefix at all, the folder the path spells.
pub(crate) fn resolve(root: &Path, file: &Path, import: &str) -> Option<String> {
    let (folder, module) = module_of(root, file.parent()?)?;
    let target = if import == module {
        folder
    } else if let Some(rest) = import.strip_prefix(&format!("{module}/")) {
        folder.join(rest)
    } else if folder.join("vendor").join(import).is_dir() {
        folder.join("vendor").join(import)
    } else if module == "std" && folder.join(import).is_dir() {
        folder.join(import)
    } else {
        return None;
    };
    let rel = target.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
    Some(rel.trim_matches('/').to_string())
}

/// The name an import is used by when it carries no alias: its last element, a major version suffix (`/v2`) and
/// a dotted one (`yaml.v3`) passed over, as the package clause of such a module almost always reads.
pub(crate) fn default_name(import: &str) -> String {
    let mut parts: Vec<&str> = import.split('/').collect();
    if parts.len() > 1 && parts.last().is_some_and(|last| last.len() > 1 && last.starts_with('v') && last[1..].chars().all(|c| c.is_ascii_digit())) {
        parts.pop();
    }
    let last = parts.last().copied().unwrap_or(import);
    last.split('.').next().unwrap_or(last).replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_directive_is_read_quoted_or_not() {
        assert_eq!(module_path("// c\nmodule example.com/m // note\n\ngo 1.22\n").as_deref(), Some("example.com/m"));
        assert_eq!(module_path("module \"example.com/q\"\n").as_deref(), Some("example.com/q"));
        assert_eq!(module_path("go 1.22\n"), None);
    }

    #[test]
    fn an_unaliased_import_is_used_by_its_last_element() {
        assert_eq!(default_name("example.com/m/internal/store"), "store");
        assert_eq!(default_name("github.com/x/y/v2"), "y");
        assert_eq!(default_name("gopkg.in/yaml.v3"), "yaml");
        assert_eq!(default_name("fmt"), "fmt");
    }
}
