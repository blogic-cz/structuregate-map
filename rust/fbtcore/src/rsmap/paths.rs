//! WHAT A PATH'S SEGMENTS NAME, decided by its ROOT. Only the first segment of a path says where the rest
//! live: after `crate::`/`super::` they are this tree's, after `std::` or an external crate's name they are
//! not, and after a type or module the file itself defines they are its members. Reading EVERY segment as a
//! name joined `TokenTree::Group` to any file's `pub struct Group`, `std::env::temp_dir` to a `pub fn temp_dir`
//! and `self::reach::orphans` (an inline module) to a file that declares `reach`.
//!
//! - `crate`/`super`: in-tree, every later segment is a use. `self`: the same unless its next segment is a
//!   name of the file's own. `Self` and `std`/`core`/`alloc`/`proc_macro`: nothing. An own name (item or
//!   module) as head: nothing for what follows it.
//! - A head the file IMPORTS by `use` takes that import's root: in-tree, then head and later segments are
//!   uses; an EXTERN crate `x`, then they go to `rooted[x]` (the join binds them only into that crate, when it
//!   is an in-tree package); std or own, nothing.
//! - Any other head is a use (a prelude type, a glob-imported name). Its later segments are uses when the file
//!   globs in-tree (`use super::*;` brings `Kind` and `Kind::A` is then a variant of an in-tree enum); else a
//!   lowercase head is a crate written without a `use` (`rooted[head]`), and an uppercase one (`Vec::new`) names
//!   nothing.
//! - `self`, `Self`, `super` and `crate` are keywords and never a name.
//! - AN IMPORT IS VISIBLE ONLY IN THE MODULE THAT WRITES IT, not even in its child modules: the imports and the
//!   in-tree glob are kept per module, and a path is read with its own module's. A `use super::*;` in a
//!   `#[cfg(test)] mod tests` made `syn::UseTree::Group` elsewhere in the file a use of `Group`. The file's own
//!   names stay file-wide. A module that globs in-tree (`use super::*;`) also sees its PARENT's imports, which the
//!   glob brings in.

use std::collections::{BTreeMap, BTreeSet};

/// Where a path or a `use` starts.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Root {
    InTree,
    Own,
    Std,
    Extern(String),
}

/// One `use` leaf: the segments before it, the leaf, its alias and whether it is a glob (`leaf` is then empty).
pub(crate) struct UseItem {
    pub segs: Vec<String>,
    pub leaf: String,
    pub alias: Option<String>,
    pub glob: bool,
    /// The inline modules around the `use`, outermost first - the only module that sees it.
    pub module: Vec<String>,
}

pub(crate) type Rooted = BTreeMap<String, BTreeSet<String>>;

fn keyword(s: &str) -> bool {
    matches!(s, "self" | "Self" | "super" | "crate")
}

fn std_root(s: &str) -> bool {
    matches!(s, "std" | "core" | "alloc" | "proc_macro")
}

fn clean(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_string()
}

/// Every leaf of a use tree, flattened.
pub(crate) fn flatten(tree: &syn::UseTree) -> Vec<UseItem> {
    let mut out = Vec::new();
    walk(tree, &mut Vec::new(), &mut out);
    out
}

fn walk(tree: &syn::UseTree, prefix: &mut Vec<String>, out: &mut Vec<UseItem>) {
    let mut leaf = |ident: &syn::Ident, alias: Option<String>, prefix: &Vec<String>| {
        let mut segs = prefix.clone();
        let mut name = clean(ident);
        // `use a::b::{self}` names `b`.
        if name == "self" && let Some(last) = segs.pop() {
            name = last;
        }
        out.push(UseItem { segs, leaf: name, alias, glob: false, module: Vec::new() });
    };
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(clean(&p.ident));
            walk(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => leaf(&n.ident, None, prefix),
        syn::UseTree::Rename(r) => leaf(&r.ident, Some(clean(&r.rename)), prefix),
        syn::UseTree::Glob(_) => out.push(UseItem { segs: prefix.clone(), leaf: String::new(), alias: None, glob: true, module: Vec::new() }),
        syn::UseTree::Group(g) => {
            for item in &g.items {
                walk(item, prefix, out);
            }
        }
    }
}

fn root_of(first: &str, second: Option<&str>, own: &BTreeSet<String>) -> Root {
    match first {
        "crate" | "super" => Root::InTree,
        "self" => match second {
            Some(s) if own.contains(s) => Root::Own,
            _ => Root::InTree,
        },
        s if std_root(s) => Root::Std,
        s if own.contains(s) => Root::Own,
        s => Root::Extern(s.to_string()),
    }
}

fn root_of_use(u: &UseItem, own: &BTreeSet<String>) -> Root {
    let full: Vec<&str> = u.segs.iter().map(String::as_str).chain((!u.glob).then_some(u.leaf.as_str())).collect();
    root_of(full.first().copied().unwrap_or(""), full.get(1).copied(), own)
}

/// (the names the paths and uses take from this tree, the names they take through another crate, by its name).
/// A path is `(its module, its segments)` and is resolved with the imports and the glob of THAT module only.
pub(crate) fn resolve(paths: &[(Vec<String>, Vec<String>)], uses: &[UseItem], own: &BTreeSet<String>) -> (BTreeSet<String>, Rooted) {
    let mut names = BTreeSet::new();
    let mut rooted = Rooted::new();
    let mut all_imports: BTreeMap<&Vec<String>, BTreeMap<String, Root>> = BTreeMap::new();
    let mut globs: BTreeSet<&Vec<String>> = BTreeSet::new();
    // AN ALIAS IS NOT A NAME of any file, and is visible only in the module that writes it (like an import); the leaf it renames is already recorded by the `use`.
    let mut aliased: BTreeSet<(&Vec<String>, &str)> = BTreeSet::new();
    let add = |names: &mut BTreeSet<String>, s: &str| {
        if !keyword(s) {
            names.insert(s.to_string());
        }
    };
    for u in uses {
        let root = root_of_use(u, own);
        if let Some(a) = &u.alias {
            aliased.insert((&u.module, a));
        }
        if u.glob {
            if matches!(root, Root::InTree | Root::Own) {
                globs.insert(&u.module);
            }
        } else {
            all_imports.entry(&u.module).or_default().insert(u.alias.clone().unwrap_or_else(|| u.leaf.clone()), root.clone());
        }
        match &root {
            Root::InTree => {
                for s in u.segs.iter().chain((!u.glob).then_some(&u.leaf)) {
                    add(&mut names, s);
                }
            }
            Root::Extern(x) if !u.segs.is_empty() => {
                let set = rooted.entry(x.clone()).or_default();
                for s in u.segs[1..].iter().chain((!u.glob).then_some(&u.leaf)) {
                    set.insert(s.clone());
                }
            }
            _ => {}
        }
    }
    let no_imports = BTreeMap::new();
    for (module, path) in paths {
        let glob = globs.contains(module);
        // A MODULE'S `use super::*;` BRINGS ITS PARENT'S IMPORTS with it (a `tests` module reads `Connection` the
        // parent imported from an extern crate), so the parent's table answers for a head this module does not import.
        let parent = (glob && !module.is_empty()).then(|| &module[..module.len() - 1]);
        let own_imports = all_imports.get(module).unwrap_or(&no_imports);
        let parent_imports = parent.and_then(|m| all_imports.get(&m.to_vec())).unwrap_or(&no_imports);
        let Some((head, rest)) = path.split_first() else { continue };
        let is_alias = aliased.contains(&(module, head.as_str()));
        let all = |names: &mut BTreeSet<String>| {
            for s in path.iter().filter(|s| !(*s == head && is_alias)) {
                add(names, s);
            }
        };
        match head.as_str() {
            "crate" | "super" => all(&mut names),
            "self" => {
                if !path.get(1).is_some_and(|s| own.contains(s)) {
                    all(&mut names);
                }
            }
            "Self" => {}
            h if std_root(h) || own.contains(h) => {}
            h => match own_imports.get(h).or_else(|| parent_imports.get(h)) {
                Some(Root::InTree) => all(&mut names),
                Some(Root::Std | Root::Own) => {}
                Some(Root::Extern(x)) => {
                    let set = rooted.entry(x.clone()).or_default();
                    set.extend(path.iter().filter(|s| !keyword(s) && !(*s == head && is_alias)).cloned());
                }
                None => {
                    add(&mut names, h);
                    if glob {
                        for s in rest {
                            add(&mut names, s);
                        }
                    } else if h.starts_with(|c: char| c.is_lowercase()) && !rest.is_empty() {
                        rooted.entry(h.to_string()).or_default().extend(rest.iter().filter(|s| !keyword(s)).cloned());
                    }
                }
            },
        }
    }
    rooted.retain(|_, v| !v.is_empty());
    (names, rooted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, paths: &[&str], own: &[&str]) -> (BTreeSet<String>, Rooted) {
        run_in(src, &[], paths, own)
    }

    /// The `use`s of `src` sit in module `use_mod`; every path sits in module [].
    fn run_in(src: &str, use_mod: &[&str], paths: &[&str], own: &[&str]) -> (BTreeSet<String>, Rooted) {
        let file = syn::parse_file(src).unwrap();
        let mut uses = Vec::new();
        for item in &file.items {
            if let syn::Item::Use(u) = item {
                uses.extend(flatten(&u.tree));
            }
        }
        for u in &mut uses {
            u.module = use_mod.iter().map(|s| s.to_string()).collect();
        }
        let paths: Vec<(Vec<String>, Vec<String>)> = paths.iter().map(|p| (Vec::new(), p.split("::").map(String::from).collect())).collect();
        let own = own.iter().map(|s| s.to_string()).collect();
        resolve(&paths, &uses, &own)
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_external_path_and_std_name_nothing_in_the_tree() {
        let (names, rooted) = run("use proc_macro2::TokenTree;", &["TokenTree::Group", "std::env::temp_dir"], &[]);
        assert!(names.is_empty(), "{names:?}");
        assert_eq!(rooted["proc_macro2"], set(&["TokenTree", "Group"]));
    }

    #[test]
    fn self_names_the_tree_unless_its_next_segment_is_the_files_own() {
        let (names, _) = run("", &["self::reach::orphans"], &["reach"]);
        assert!(names.is_empty());
        let (names, _) = run("", &["self::other::f"], &["reach"]);
        assert_eq!(names, set(&["other", "f"]));
    }

    #[test]
    fn crate_and_super_name_every_later_segment_and_self_type_nothing() {
        let (names, _) = run("", &["crate::a::B", "super::c", "Self::D"], &[]);
        assert_eq!(names, set(&["a", "B", "c"]));
    }

    #[test]
    fn an_own_head_names_nothing_after_it() {
        let (names, _) = run("", &["Kind::A", "local::f"], &["Kind", "local"]);
        assert!(names.is_empty());
    }

    #[test]
    fn an_in_tree_import_names_head_and_the_rest() {
        let (names, rooted) = run("use crate::store::Store;", &["Store", "Store::new"], &[]);
        assert_eq!(names, set(&["store", "Store", "new"]));
        assert!(rooted.is_empty());
    }

    #[test]
    fn an_extern_import_goes_to_rooted() {
        let (names, rooted) = run("use alpha::go;", &["go"], &[]);
        assert!(names.is_empty());
        assert_eq!(rooted["alpha"], set(&["go"]));
    }

    #[test]
    fn a_std_import_names_nothing() {
        let (names, rooted) = run("use std::collections::BTreeMap;", &["BTreeMap::new"], &[]);
        assert!(names.is_empty() && rooted.is_empty());
    }

    #[test]
    fn a_glob_makes_later_segments_uses() {
        let (names, _) = run("use super::*;", &["Kind::A"], &[]);
        assert_eq!(names, set(&["Kind", "A"]));
    }

    #[test]
    fn an_import_is_visible_only_in_its_own_module() {
        let (names, _) = run_in("use super::*;", &["tests"], &["Kind::A"], &[]);
        assert_eq!(names, set(&["Kind"]));
        let (names, rooted) = run_in("use alpha::go;", &["tests"], &["go"], &[]);
        // the bare `go` of module [] is not the import's: it is a use; only the `use` itself reaches `alpha`.
        assert_eq!(names, set(&["go"]));
        assert_eq!(rooted["alpha"], set(&["go"]));
    }

    #[test]
    fn a_glob_brings_the_parents_imports() {
        let file = syn::parse_file("use proc_macro2::TokenTree;").unwrap();
        let mut uses = Vec::new();
        for item in &file.items {
            if let syn::Item::Use(u) = item {
                uses.extend(flatten(&u.tree));
            }
        }
        let mut glob = flatten(&syn::parse_str::<syn::ItemUse>("use super::*;").unwrap().tree);
        glob[0].module = vec!["tests".to_string()];
        uses.extend(glob);
        let paths = vec![(vec!["tests".to_string()], vec!["TokenTree".to_string(), "Group".to_string()])];
        let (names, rooted) = resolve(&paths, &uses, &BTreeSet::new());
        assert!(names.is_empty(), "{names:?}");
        assert_eq!(rooted["proc_macro2"], set(&["TokenTree", "Group"]));
    }

    #[test]
    fn an_unimported_head_is_a_use_and_a_lowercase_one_may_be_a_crate() {
        let (names, rooted) = run("", &["Vec::new", "alpha::go"], &[]);
        assert_eq!(names, set(&["Vec", "alpha"]));
        assert_eq!(rooted["alpha"], set(&["go"]));
    }

    #[test]
    fn an_alias_hides_its_head_only_in_the_module_that_writes_it() {
        let (names, _) = run_in("use crate::a::b as c;", &["tests"], &["c"], &[]);
        assert!(names.contains("c"), "{names:?}");
        let (names, _) = run("use crate::a::b as c;", &["c"], &[]);
        assert!(!names.contains("c"), "{names:?}");
    }

    #[test]
    fn a_use_group_and_a_rename_flatten() {
        let (names, rooted) = run("use crate::a::{self, b as c};\nuse alpha::{x, y as z};", &["c", "z"], &[]);
        assert_eq!(names, set(&["a", "b"]));
        assert_eq!(rooted["alpha"], set(&["x", "y"]));
    }
}
