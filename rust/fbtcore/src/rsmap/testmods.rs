//! WHICH RUST FILES ARE TEST CODE AS A WHOLE - decided where they are DECLARED, which no file's own text says:
//! `#[cfg(test)] #[path = "x_tests.rs"] mod tests;` makes all of `x_tests.rs` a test module, with every file it
//! declares in turn, and every file of a `tests/` integration crate is test code. Read per file, such a file's
//! unwraps were production unwraps and its fixtures were constants.

use super::{forget_spans, items, lines, modpath};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// A file of a cargo `tests/` integration crate: a `tests` folder not inside `src/` (there it is a module folder).
fn integration(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let dirs = &parts[..parts.len() - 1];
    dirs.iter().position(|d| *d == "tests").is_some_and(|at| !dirs[..at].contains(&"src"))
}

/// The files declared by `text`'s `mod x;` lines, resolved - only the test ones unless `all`.
fn declared(root: &Path, rel: &str, text: &str, all: bool) -> Vec<String> {
    let source = lines::without_shebang(text.trim_start_matches('\u{feff}'));
    let out = match syn::parse_file(&source) {
        Ok(file) => items::read(&file, &lines::LineStarts::new(&source))
            .mods
            .iter()
            .filter(|m| all || m.test)
            .map(|m| modpath::resolve(root, rel, m))
            .collect(),
        Err(_) => Vec::new(),
    };
    forget_spans();
    out
}

/// The `rel` of every file that is test code as a whole, given every file's `(rel, text)`.
pub(crate) fn test_files(root: &Path, texts: &[(&str, &str)]) -> BTreeSet<String> {
    let mut found: BTreeSet<String> = texts.iter().map(|(rel, _)| *rel).filter(|rel| integration(rel)).map(str::to_string).collect();
    let by_rel: HashMap<&str, &str> = texts.iter().copied().collect();
    let mut queue: Vec<String> = Vec::new();
    for (rel, text) in texts {
        // Every `#[cfg(test)]` and `cfg(all(test, ..))` spells `test`: a cheap filter before a parse.
        if !text.contains("test") {
            continue;
        }
        for target in declared(root, rel, text, false) {
            if found.insert(target.clone()) {
                queue.push(target);
            }
        }
    }
    let mut visited: BTreeSet<String> = BTreeSet::new();
    while let Some(rel) = queue.pop() {
        let Some(text) = by_rel.get(rel.as_str()) else { continue };
        if !visited.insert(rel.clone()) {
            continue;
        }
        for target in declared(root, &rel, text, true) {
            if found.insert(target.clone()) {
                queue.push(target);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cfg_test_path_mod_and_what_it_declares_are_test_code_and_a_tests_crate_is_too() {
        let texts = [
            ("src/lib.rs", "mod a;\n#[cfg(test)]\n#[path = \"lib_tests.rs\"]\nmod tests;\n"),
            ("src/lib_tests.rs", "mod deep;\n"),
            ("src/lib_tests/deep.rs", "fn d() {}\n"),
            ("src/a.rs", "fn a() {}\n"),
            ("tests/it.rs", "fn t() {}\n"),
            ("src/tests/x.rs", "fn x() {}\n"),
        ];
        let found = test_files(Path::new("no-such-root"), &texts);
        assert!(found.contains("src/lib_tests.rs"));
        assert!(found.contains("src/lib_tests/deep.rs"));
        assert!(found.contains("tests/it.rs"));
        assert!(!found.contains("src/tests/x.rs"));
        assert!(!found.contains("src/a.rs"));
    }
}
