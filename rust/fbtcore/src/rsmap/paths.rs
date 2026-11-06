//! WHAT A PATH'S SEGMENTS NAME, decided by its ROOT. Only the first segment of a path says where the rest
//! live: after `crate::`/`super::` they are this tree's, after `std::` or an external crate's name they are
//! not, and after a type or module the file itself defines they are its members. Reading EVERY segment as a
//! name joined `TokenTree::Group` to any file's `pub struct Group`, `std::env::temp_dir` to a `pub fn temp_dir`
//! and `self::reach::orphans` (an inline module) to a file that declares `reach`.
//!
//! - A PATH THAT LEADS INTO THE TREE IS NEVER A BARE NAME: it is a QUALIFIED path (`Qualified`: the module it is
//!   written in, its anchor `crate`/`super`s/`self`, its segments) that the join walks through the crate's module
//!   tree (`graph/rspaths.rs`). Every segment read as a name joined `use crate::rows::schema;` (a MODULE) to another
//!   file's `pub fn schema`, `Session::open` to any `pub fn open`, and a parent's private `query` to a `pub fn query`.
//!   `crate`/`super`/`self` as head: the anchor, the rest the segments. An OWN head (item or module): `self` with the
//!   whole path - a module descends, an item stops in the own file. An in-tree IMPORT as head: the import's full
//!   path replaces the head, written in the module that imported it (a glob brings the parent's). Every in-tree
//!   `use` is itself such a path. `Self` and `std`/`core`/`alloc`/`proc_macro`: nothing.
//! - A head the file IMPORTS from an EXTERN crate `x`: the path goes to `rooted[x]` (the join binds it only into
//!   that crate, when it is an in-tree package); std or own, nothing.
//! - Any other head, in a module that globs in-tree (`use super::*;`): one `glob` CANDIDATE per glob - the glob's
//!   path plus the whole path - accepted only when the item is the target file's own. It is no name. Else it is a
//!   use (a prelude type, a macro), and a lowercase head with later segments is a crate written without a `use`
//!   (`rooted[head]`); an uppercase one (`Vec::new`) names nothing more.
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

/// Where a qualified path starts: `crate::` (the crate root of the module it is written in), `super::` repeated
/// `n` times (the parent `n` times up) or `self::` (the module itself).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Anchor {
    Crate,
    SelfMod,
    Super(usize),
}

/// A PATH THAT LEADS INTO THE TREE: where it is written (`module`, the inline chain), where it starts and the
/// segments after that. The join (`graph/rspaths.rs`) walks it through the module tree. `glob`: a CANDIDATE
/// made by prefixing a glob's path to a bare head - accepted only when the item is the target file's own.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Qualified {
    pub module: Vec<String>,
    pub anchor: Anchor,
    pub segs: Vec<String>,
    pub glob: bool,
}

/// One import: where it points, its full path and the module that wrote it.
struct Imp {
    root: Root,
    full: Vec<String>,
    module: Vec<String>,
}

/// `full` is a path that starts with `crate`, `super`s or `self`: its anchor and the rest.
fn qualify(module: &[String], full: &[String], glob: bool) -> Option<Qualified> {
    let (anchor, skip) = match full.first().map(String::as_str)? {
        "crate" => (Anchor::Crate, 1),
        "self" => (Anchor::SelfMod, 1),
        "super" => {
            let n = full.iter().take_while(|s| *s == "super").count();
            (Anchor::Super(n), n)
        }
        _ => return None,
    };
    Some(Qualified { module: module.to_vec(), anchor, segs: full[skip..].to_vec(), glob })
}

/// An import's name (its alias or its leaf) and the path it points at.
pub(crate) struct Named {
    pub name: String,
    pub q: Qualified,
}

/// THE IMPORTS AND GLOBS THAT POINT INTO THE TREE, per module, privacy aside: where a name points is the same for a `use`
/// and a `pub use`. The walk follows them (`graph/rspaths.rs`): a re-export (`use cells::meta;` in a `mod.rs`) and a child's
/// `use super::*;` reaching the parent's own imports both resolve through them. A root that is the file's own (`use cells::meta;`
/// with `mod cells;` beside) is `self`-anchored. A glob of the file's own module (`use model::*;`) is kept: the walk takes it when it ends at the segment or later, so
/// a glob of an own enum (`use Kind::*;`) accepts nothing.
pub(crate) fn imports(uses: &[UseItem], own: &BTreeSet<String>) -> (Vec<Named>, Vec<Qualified>) {
    let (mut named, mut globs) = (Vec::new(), Vec::new());
    for u in uses {
        let root = root_of_use(u, own);
        if !matches!(root, Root::InTree | Root::Own) {
            continue;
        }
        let full: Vec<String> = u.segs.iter().cloned().chain((!u.glob).then(|| u.leaf.clone())).collect();
        let Some(q) = qualify(&u.module, &full, false).or_else(|| Some(Qualified { module: u.module.clone(), anchor: Anchor::SelfMod, segs: full.clone(), glob: false })) else { continue };
        if u.glob {
            globs.push(q);
        } else {
            named.push(Named { name: u.alias.clone().unwrap_or_else(|| u.leaf.clone()), q });
        }
    }
    (named, globs)
}

/// (the names no path resolves, the names the paths and uses take through another crate by its name, the paths
/// that lead into this tree). A path is `(its module, its segments)` and is resolved with the imports and the glob
/// of THAT module only.
pub(crate) fn resolve(paths: &[(Vec<String>, Vec<String>)], uses: &[UseItem], own: &BTreeSet<String>) -> (BTreeSet<String>, Rooted, Vec<Qualified>) {
    let mut names = BTreeSet::new();
    let mut rooted = Rooted::new();
    let mut tree: BTreeSet<Qualified> = BTreeSet::new();
    let mut all_imports: BTreeMap<&Vec<String>, BTreeMap<String, Imp>> = BTreeMap::new();
    let mut globs: BTreeSet<&Vec<String>> = BTreeSet::new();
    // The in-tree globs of each module, as the path they glob from.
    let mut tree_globs: BTreeMap<&Vec<String>, Vec<Vec<String>>> = BTreeMap::new();
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
            if root == Root::InTree {
                tree_globs.entry(&u.module).or_default().push(u.segs.clone());
            }
        } else {
            let full: Vec<String> = u.segs.iter().cloned().chain([u.leaf.clone()]).collect();
            let imp = Imp { root: root.clone(), full, module: u.module.clone() };
            all_imports.entry(&u.module).or_default().insert(u.alias.clone().unwrap_or_else(|| u.leaf.clone()), imp);
        }
        match &root {
            // AN IN-TREE `use` IS ITSELF A PATH into the tree.
            Root::InTree => {
                let full: Vec<String> = u.segs.iter().cloned().chain((!u.glob).then(|| u.leaf.clone())).collect();
                tree.extend(qualify(&u.module, &full, false));
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
        match head.as_str() {
            "crate" | "super" | "self" => tree.extend(qualify(module, path, false)),
            "Self" => {}
            h if std_root(h) => {}
            // AN OWN HEAD (item or module) is walked from the file's own module: a module descends, an item stops here.
            h if own.contains(h) => {
                tree.insert(Qualified { module: module.clone(), anchor: Anchor::SelfMod, segs: path.clone(), glob: false });
            }
            h => match own_imports.get(h).or_else(|| parent_imports.get(h)) {
                // THE IMPORT'S FULL PATH REPLACES THE HEAD, written in the module that imported it.
                Some(Imp { root: Root::InTree, full, module: at }) => {
                    let whole: Vec<String> = full.iter().chain(rest).cloned().collect();
                    tree.extend(qualify(at, &whole, false));
                }
                Some(Imp { root: Root::Std | Root::Own, .. }) => {}
                Some(Imp { root: Root::Extern(x), .. }) => {
                    let set = rooted.entry(x.clone()).or_default();
                    set.extend(path.iter().filter(|s| !keyword(s) && !(*s == head && is_alias)).cloned());
                }
                None => match tree_globs.get(module) {
                    // A HEAD NO IMPORT OR ITEM OF THE FILE NAMES may come through an in-tree glob: one candidate per glob.
                    Some(from) => {
                        for g in from {
                            let whole: Vec<String> = g.iter().chain(path).cloned().collect();
                            tree.extend(qualify(module, &whole, true));
                        }
                        if h.starts_with(|c: char| c.is_lowercase()) && !rest.is_empty() {
                            rooted.entry(h.to_string()).or_default().extend(rest.iter().filter(|s| !keyword(s)).cloned());
                        }
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
            },
        }
    }
    rooted.retain(|_, v| !v.is_empty());
    (names, rooted, tree.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Out = (BTreeSet<String>, Rooted, Vec<Qualified>);

    fn run(src: &str, paths: &[&str], own: &[&str]) -> Out {
        run_in(src, &[], paths, own)
    }

    /// The `use`s of `src` sit in module `use_mod`; every path sits in module [].
    fn run_in(src: &str, use_mod: &[&str], paths: &[&str], own: &[&str]) -> Out {
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

    fn q(module: &[&str], anchor: Anchor, segs: &[&str], glob: bool) -> Qualified {
        Qualified { module: module.iter().map(|s| s.to_string()).collect(), anchor, segs: segs.iter().map(|s| s.to_string()).collect(), glob }
    }

    #[test]
    fn an_external_path_and_std_name_nothing_in_the_tree() {
        let (names, rooted, tree) = run("use proc_macro2::TokenTree;", &["TokenTree::Group", "std::env::temp_dir"], &[]);
        assert!(names.is_empty() && tree.is_empty(), "{names:?} {tree:?}");
        assert_eq!(rooted["proc_macro2"], set(&["TokenTree", "Group"]));
    }

    #[test]
    fn crate_super_and_self_are_qualified_paths_never_names() {
        let (names, _, tree) = run("", &["crate::a::B", "super::super::c", "self::d", "Self::D"], &[]);
        assert!(names.is_empty(), "{names:?}");
        let want = [q(&[], Anchor::Crate, &["a", "B"], false), q(&[], Anchor::Super(2), &["c"], false), q(&[], Anchor::SelfMod, &["d"], false)];
        for w in want {
            assert!(tree.contains(&w), "{w:?} in {tree:?}");
        }
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn an_own_head_is_walked_from_the_files_own_module() {
        let (names, _, tree) = run("", &["Kind::A", "local::f"], &["Kind", "local"]);
        assert!(names.is_empty());
        assert!(tree.contains(&q(&[], Anchor::SelfMod, &["Kind", "A"], false)));
        assert!(tree.contains(&q(&[], Anchor::SelfMod, &["local", "f"], false)));
    }

    #[test]
    fn an_in_tree_import_replaces_the_head_with_its_full_path() {
        let (names, rooted, tree) = run("use crate::store::Store;", &["Store", "Store::new"], &[]);
        assert!(names.is_empty(), "{names:?}");
        assert!(rooted.is_empty());
        assert!(tree.contains(&q(&[], Anchor::Crate, &["store", "Store"], false)));
        assert!(tree.contains(&q(&[], Anchor::Crate, &["store", "Store", "new"], false)));
    }

    #[test]
    fn an_in_tree_use_is_itself_a_path_and_a_glob_is_its_segments() {
        let (_, _, tree) = run("use crate::rows::schema;\nuse super::*;\nuse crate::a::b as c;", &[], &[]);
        assert!(tree.contains(&q(&[], Anchor::Crate, &["rows", "schema"], false)));
        assert!(tree.contains(&q(&[], Anchor::Super(1), &[], false)));
        assert!(tree.contains(&q(&[], Anchor::Crate, &["a", "b"], false)));
        let (_, _, tree) = run_in("use crate::a::b as c;", &["m"], &[], &[]);
        assert!(tree.iter().any(|t| t.module == ["m"]), "{tree:?}");
    }

    #[test]
    fn an_extern_import_goes_to_rooted() {
        let (names, rooted, tree) = run("use alpha::go;", &["go"], &[]);
        assert!(names.is_empty() && tree.is_empty());
        assert_eq!(rooted["alpha"], set(&["go"]));
    }

    #[test]
    fn a_std_import_names_nothing() {
        let (names, rooted, tree) = run("use std::collections::BTreeMap;", &["BTreeMap::new"], &[]);
        assert!(names.is_empty() && rooted.is_empty() && tree.is_empty());
    }

    #[test]
    fn an_unknown_head_under_an_in_tree_glob_is_a_candidate_per_glob_and_no_name() {
        let (names, _, tree) = run("use super::*;\nuse crate::a::*;", &["helper", "Kind::A"], &[]);
        assert!(names.is_empty(), "{names:?}");
        assert!(tree.contains(&q(&[], Anchor::Super(1), &["helper"], true)));
        assert!(tree.contains(&q(&[], Anchor::Crate, &["a", "Kind", "A"], true)));
        // the glob's own path is a plain `use` path, never a candidate.
        assert!(tree.contains(&q(&[], Anchor::Super(1), &[], false)));
    }

    #[test]
    fn an_import_is_visible_only_in_its_own_module() {
        let (names, _, tree) = run_in("use super::*;", &["tests"], &["Kind::A"], &[]);
        assert_eq!(names, set(&["Kind"]));
        assert!(tree.iter().all(|t| !t.glob), "{tree:?}");
        let (names, rooted, _) = run_in("use alpha::go;", &["tests"], &["go"], &[]);
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
        let (names, rooted, _) = resolve(&paths, &uses, &BTreeSet::new());
        assert!(names.is_empty(), "{names:?}");
        assert_eq!(rooted["proc_macro2"], set(&["TokenTree", "Group"]));
    }

    #[test]
    fn a_parents_in_tree_import_is_walked_in_the_parents_module() {
        let mut uses = flatten(&syn::parse_str::<syn::ItemUse>("use crate::sess::Session;").unwrap().tree);
        let mut glob = flatten(&syn::parse_str::<syn::ItemUse>("use super::*;").unwrap().tree);
        glob[0].module = vec!["tests".to_string()];
        uses.extend(glob);
        let paths = vec![(vec!["tests".to_string()], vec!["Session".to_string(), "open".to_string()])];
        let (names, _, tree) = resolve(&paths, &uses, &BTreeSet::new());
        assert!(names.is_empty(), "{names:?}");
        assert!(tree.contains(&q(&[], Anchor::Crate, &["sess", "Session", "open"], false)), "{tree:?}");
    }

    #[test]
    fn an_unimported_head_is_a_use_and_a_lowercase_one_may_be_a_crate() {
        let (names, rooted, tree) = run("", &["Vec::new", "alpha::go"], &[]);
        assert_eq!(names, set(&["Vec", "alpha"]));
        assert!(tree.is_empty());
        assert_eq!(rooted["alpha"], set(&["go"]));
    }

    #[test]
    fn an_alias_is_no_name_in_the_module_that_writes_it() {
        let (names, _, _) = run_in("use crate::a::b as c;", &["tests"], &["c"], &[]);
        assert!(names.contains("c"), "{names:?}");
        let (names, _, tree) = run("use crate::a::b as c;", &["c"], &[]);
        assert!(!names.contains("c"), "{names:?}");
        assert!(tree.contains(&q(&[], Anchor::Crate, &["a", "b"], false)));
    }

    #[test]
    fn a_use_group_and_a_rename_flatten() {
        let (names, rooted, tree) = run("use crate::a::{self, b as c};\nuse alpha::{x, y as z};", &["c", "z"], &[]);
        assert!(names.is_empty(), "{names:?}");
        assert_eq!(rooted["alpha"], set(&["x", "y"]));
        assert!(tree.contains(&q(&[], Anchor::Crate, &["a"], false)));
        assert!(tree.contains(&q(&[], Anchor::Crate, &["a", "b"], false)));
    }

    #[test]
    fn imports_and_globs_are_kept_per_module_with_their_full_path() {
        let file = syn::parse_file("use crate::a::{b as c, d};\nuse super::*;\nuse cells::meta;\nuse Kind::*;\nuse std::fmt;\nuse alpha::go;").unwrap();
        let mut uses = Vec::new();
        for item in &file.items {
            if let syn::Item::Use(u) = item {
                uses.extend(flatten(&u.tree));
            }
        }
        let own = set(&["cells", "Kind"]);
        let (named, globs) = imports(&uses, &own);
        let by: Vec<(&str, &Anchor, Vec<&str>)> = named.iter().map(|n| (n.name.as_str(), &n.q.anchor, n.q.segs.iter().map(String::as_str).collect())).collect();
        assert_eq!(by, [("c", &Anchor::Crate, vec!["a", "b"]), ("d", &Anchor::Crate, vec!["a", "d"]), ("meta", &Anchor::SelfMod, vec!["cells", "meta"])]);
        assert_eq!(globs.len(), 2);
        assert_eq!(globs[0].anchor, Anchor::Super(1));
    }
}
