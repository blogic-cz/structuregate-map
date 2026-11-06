//! THE MODULE TREE - where a rust path that leads INTO THE TREE lands, walked exactly instead of joined by name.
//!
//! The half (`rsmap/paths.rs`) hands over every such path as a QUALIFIED path: the module it is written in (a file
//! plus the inline modules around it), an anchor (`crate`, `super` n times, `self`) and its segments. Reading each
//! segment as a bare NAME and joining it to whichever file declares that name drew four classes of false edge on
//! this repo's own tree: `use crate::rows::schema;` (a MODULE) joined to another file's `pub fn schema`;
//! `Session::open` after `use crate::sess::Session;` joined `open` to any `pub fn open`; `use super::query;` or
//! `use super::*; helper()` - the PARENT's private item - joined to another file's `pub fn query`/`helper`; and
//! `rusqlite::Connection::open` in a module with `use super::*;` joined `open`.
//!
//! THE TREE: a module is `(file, inline chain)`. Its children are the `mod x;` it declares (the child is
//! `(target file, [])`) and the inline `mod x { }` it holds (`(same file, chain + [x])`). Its parent is the module
//! one inline level out, or - for a file's top module - the module that DECLARES the file. The crate root is where
//! the parents end.
//!
//! THE WALK: start at the anchor's module (`crate` the root, `super` the parent n times, `self` the module); take each
//! segment that names a CHILD and descend; the first that does not is an ITEM of the module reached, and the walk
//! stops there. The edge goes to that module's file - a path ending on a module goes to the module's file - and
//! never to the file itself. A parent or a root that does not exist draws no edge.
//!
//! AT AN ITEM, THE WALK FOLLOWS THE MODULE'S IMPORTS, as rust resolves a name: (a) an item among the file's OWN names
//! stops there; (b) else an import of the module named like the segment continues from THAT import's path, anchored at
//! the import's own module, then the segments left (`use cells::meta;` in `rows/mod.rs` makes `crate::rows::meta` mean
//! `rows/cells/meta.rs`); (c) else each in-tree glob of the module is tried (glob path + segment + the rest) and the
//! first that ends on an own item or a module wins - so a child's `use super::*;` reaches what the parent imports;
//! (d) else it stops and the edge goes to the module's file. A visited set of (file, inline chain, segment) ends an
//! import cycle at (d); there is no depth cap.
//!
//! A `glob` CANDIDATE is `use`'s glob path plus a bare head the file does not name (`use super::*; helper()`): it is
//! accepted only when that walk ends on an own item or a module, never at (d).

use super::File;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Where a walk ended: the file, whether it ended on an own item or a module (not at step d), and how many segments were left after the stop.
struct Landed {
    rel: String,
    accepted: bool,
    tail: usize,
}

type Seen = HashSet<(String, Vec<String>, String)>;

/// One module declaration of a file: `mod name;` (`target` is the file it points at) or an inline `mod name { }`
/// (`target` none), with the inline modules around it.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct ModEntry {
    pub inline: Vec<String>,
    pub name: String,
    pub target: Option<String>,
}

/// A path that leads into the tree (`rsmap/paths.rs::Qualified`): where it is written, its anchor (`crate`, `self`
/// or `super` with `supers` levels), its segments, and whether it is a glob candidate. As an IMPORT it also has the
/// `name` it binds (its alias or leaf); a glob has none.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Qualified {
    pub module: Vec<String>,
    pub anchor: String,
    pub supers: usize,
    pub segs: Vec<String>,
    pub glob: bool,
    pub name: Option<String>,
}

type Module = (String, Vec<String>);

pub struct Tree<'a> {
    files: &'a BTreeMap<String, File>,
    children: HashMap<Module, HashMap<&'a str, Module>>,
    declared_by: HashMap<&'a str, Module>,
    imports: HashMap<Module, HashMap<&'a str, &'a Qualified>>,
    globs: HashMap<Module, Vec<&'a Qualified>>,
}

impl<'a> Tree<'a> {
    /// Built once per join, from every file that declares a module.
    pub fn new(files: &'a BTreeMap<String, File>) -> Self {
        let mut children: HashMap<Module, HashMap<&str, Module>> = HashMap::new();
        let mut declared_by: HashMap<&str, Module> = HashMap::new();
        for file in files.values() {
            for m in &file.mods {
                let at = (file.rel.clone(), m.inline.clone());
                let child = match &m.target {
                    Some(target) => {
                        declared_by.entry(target.as_str()).or_insert_with(|| at.clone());
                        (target.clone(), Vec::new())
                    }
                    None => (file.rel.clone(), m.inline.iter().cloned().chain([m.name.clone()]).collect()),
                };
                children.entry(at).or_default().entry(m.name.as_str()).or_insert(child);
            }
        }
        let mut imports: HashMap<Module, HashMap<&str, &Qualified>> = HashMap::new();
        let mut globs: HashMap<Module, Vec<&Qualified>> = HashMap::new();
        for file in files.values() {
            for i in &file.imports {
                if let Some(name) = &i.name {
                    imports.entry((file.rel.clone(), i.module.clone())).or_default().entry(name.as_str()).or_insert(i);
                }
            }
            for g in &file.globs {
                globs.entry((file.rel.clone(), g.module.clone())).or_default().push(g);
            }
        }
        Tree { files, children, declared_by, imports, globs }
    }

    fn parent(&self, m: &Module) -> Option<Module> {
        match m.1.split_last() {
            Some((_, outer)) => Some((m.0.clone(), outer.to_vec())),
            None => self.declared_by.get(m.0.as_str()).cloned(),
        }
    }

    fn root(&self, m: Module) -> Module {
        let mut seen = HashSet::new();
        let mut cur = m;
        while seen.insert(cur.clone()) {
            match self.parent(&cur) {
                Some(up) => cur = up,
                None => break,
            }
        }
        cur
    }

    /// The module a path anchored at `anchor` starts from, when written in `here`.
    fn start(&self, here: Module, anchor: &str, supers: usize) -> Option<Module> {
        match anchor {
            "crate" => Some(self.root(here)),
            "super" => {
                let mut up = here;
                for _ in 0..supers {
                    up = self.parent(&up)?;
                }
                Some(up)
            }
            _ => Some(here),
        }
    }

    /// THE WALK: descend through child modules; at an item follow the module's imports and globs (the module doc).
    fn walk(&self, here: Module, anchor: &str, supers: usize, segs: &[String], seen: &mut Seen) -> Option<Landed> {
        let mut cur = self.start(here, anchor, supers)?;
        for (i, seg) in segs.iter().enumerate() {
            if let Some(next) = self.children.get(&cur).and_then(|c| c.get(seg.as_str())) {
                cur = next.clone();
                continue;
            }
            let rest = &segs[i + 1..];
            if self.files.get(&cur.0).is_some_and(|f| f.own.contains(seg)) {
                return Some(Landed { rel: cur.0, accepted: true, tail: rest.len() });
            }
            if seen.insert((cur.0.clone(), cur.1.clone(), seg.clone())) {
                if let Some(imp) = self.imports.get(&cur).and_then(|m| m.get(seg.as_str())) {
                    let full: Vec<String> = imp.segs.iter().chain(rest).cloned().collect();
                    if let Some(found) = self.walk(cur.clone(), &imp.anchor, imp.supers, &full, seen) {
                        return Some(found);
                    }
                }
                for g in self.globs.get(&cur).into_iter().flatten() {
                    let full: Vec<String> = g.segs.iter().chain([seg]).chain(rest).cloned().collect();
                    // The stop must be at the segment or after it: a glob of an enum would otherwise accept every name.
                    if let Some(found) = self.walk(cur.clone(), &g.anchor, g.supers, &full, seen)
                        && found.accepted
                        && found.tail <= rest.len()
                    {
                        return Some(found);
                    }
                }
            }
            return Some(Landed { rel: cur.0, accepted: false, tail: rest.len() });
        }
        Some(Landed { rel: cur.0, accepted: true, tail: 0 })
    }

    /// The file `q`, written in `file`, lands in - none when it lands in `file` itself, in a file that is not mapped, or
    /// when a parent or a root does not exist.
    pub fn target(&self, file: &File, q: &Qualified) -> Option<String> {
        let here: Module = (file.rel.clone(), q.module.clone());
        let landed = self.walk(here, &q.anchor, q.supers, &q.segs, &mut Seen::new())?;
        self.files.get(&landed.rel)?;
        if landed.rel == file.rel || (q.glob && !landed.accepted) {
            return None;
        }
        Some(landed.rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rel: &str, mods: &[(&[&str], &str, Option<&str>)], own: &[&str]) -> File {
        File {
            rel: rel.to_string(),
            mods: mods.iter().map(|(i, n, t)| ModEntry { inline: i.iter().map(|s| s.to_string()).collect(), name: n.to_string(), target: t.map(String::from) }).collect(),
            own: own.iter().map(|s| s.to_string()).collect(),
            ..File::default()
        }
    }

    fn q(module: &[&str], anchor: &str, supers: usize, segs: &[&str], glob: bool) -> Qualified {
        Qualified { module: module.iter().map(|s| s.to_string()).collect(), anchor: anchor.to_string(), supers, segs: segs.iter().map(|s| s.to_string()).collect(), glob, name: None }
    }

    fn named(name: &str, module: &[&str], anchor: &str, supers: usize, segs: &[&str]) -> Qualified {
        Qualified { name: Some(name.to_string()), ..q(module, anchor, supers, segs, false) }
    }

    fn map(all: Vec<File>) -> BTreeMap<String, File> {
        all.into_iter().map(|f| (f.rel.clone(), f)).collect()
    }

    fn tree() -> BTreeMap<String, File> {
        let all = vec![
            file("src/lib.rs", &[(&[], "a", Some("src/a.rs")), (&[], "inl", None), (&["inl"], "deep", None), (&[], "other", Some("src/other.rs"))], &[]),
            file("src/a.rs", &[(&[], "child", Some("src/a/child.rs"))], &["helper"]),
            file("src/a/child.rs", &[], &[]),
            file("src/other.rs", &[], &["Thing"]),
        ];
        all.into_iter().map(|f| (f.rel.clone(), f)).collect()
    }

    #[test]
    fn a_path_walks_into_a_child_file_and_stops_at_the_item() {
        let files = tree();
        let t = Tree::new(&files);
        let from = &files["src/lib.rs"];
        assert_eq!(t.target(from, &q(&[], "self", 0, &["a", "child", "f"], false)).as_deref(), Some("src/a/child.rs"));
        assert_eq!(t.target(from, &q(&[], "self", 0, &["a", "Item", "open"], false)).as_deref(), Some("src/a.rs"));
        assert_eq!(t.target(from, &q(&[], "self", 0, &["a"], false)).as_deref(), Some("src/a.rs"));
    }

    #[test]
    fn an_inline_module_is_the_same_file_and_draws_no_edge() {
        let files = tree();
        let t = Tree::new(&files);
        let from = &files["src/lib.rs"];
        assert_eq!(t.target(from, &q(&[], "self", 0, &["inl", "deep", "f"], false)), None);
        assert_eq!(t.target(from, &q(&["inl"], "super", 1, &["a", "f"], false)).as_deref(), Some("src/a.rs"));
    }

    #[test]
    fn super_goes_up_from_a_file_and_from_an_inline_module() {
        let files = tree();
        let t = Tree::new(&files);
        let child = &files["src/a/child.rs"];
        assert_eq!(t.target(child, &q(&[], "super", 1, &["helper"], false)).as_deref(), Some("src/a.rs"));
        assert_eq!(t.target(child, &q(&[], "super", 2, &["other", "Thing"], false)).as_deref(), Some("src/other.rs"));
        assert_eq!(t.target(child, &q(&[], "super", 3, &["x"], false)), None);
        assert_eq!(t.target(&files["src/lib.rs"], &q(&["inl", "deep"], "super", 2, &["other"], false)).as_deref(), Some("src/other.rs"));
    }

    #[test]
    fn crate_is_the_root_the_parents_end_at() {
        let files = tree();
        let t = Tree::new(&files);
        let child = &files["src/a/child.rs"];
        assert_eq!(t.target(child, &q(&[], "crate", 0, &["other", "Thing"], false)).as_deref(), Some("src/other.rs"));
        assert_eq!(t.target(child, &q(&[], "crate", 0, &["Top"], false)).as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn a_glob_candidate_is_accepted_only_for_the_targets_own_name() {
        let files = tree();
        let t = Tree::new(&files);
        let child = &files["src/a/child.rs"];
        assert_eq!(t.target(child, &q(&[], "super", 1, &["helper"], true)).as_deref(), Some("src/a.rs"));
        assert_eq!(t.target(child, &q(&[], "super", 1, &["open"], true)), None);
        assert_eq!(t.target(child, &q(&[], "super", 1, &[], true)).as_deref(), Some("src/a.rs"));
    }

    #[test]
    fn a_re_export_is_followed_one_hop_and_two() {
        let mut rows = file("src/rows/mod.rs", &[(&[], "cells", Some("src/rows/cells/mod.rs"))], &[]);
        rows.imports.push(named("meta", &[], "self", 0, &["cells", "meta"]));
        let mut lib = file("src/lib.rs", &[(&[], "rows", Some("src/rows/mod.rs")), (&[], "r1", Some("src/r1.rs"))], &[]);
        // two hops: the crate root re-exports `rows::meta` as `m`.
        lib.imports.push(named("m", &[], "crate", 0, &["rows", "meta"]));
        let files = map(vec![
            lib,
            rows,
            file("src/rows/cells/mod.rs", &[(&[], "meta", Some("src/rows/cells/meta.rs"))], &[]),
            file("src/rows/cells/meta.rs", &[], &["get"]),
            file("src/r1.rs", &[], &[]),
        ]);
        let t = Tree::new(&files);
        let from = &files["src/r1.rs"];
        assert_eq!(t.target(from, &q(&[], "crate", 0, &["rows", "meta", "get"], false)).as_deref(), Some("src/rows/cells/meta.rs"));
        assert_eq!(t.target(from, &q(&[], "crate", 0, &["m", "get"], false)).as_deref(), Some("src/rows/cells/meta.rs"));
    }

    #[test]
    fn a_crate_root_pub_use_group_names_the_defining_file() {
        let mut lib = file("src/lib.rs", &[(&[], "a", Some("src/a.rs")), (&[], "user", Some("src/user.rs"))], &[]);
        lib.imports.push(named("x", &[], "self", 0, &["a", "x"]));
        let files = map(vec![lib, file("src/a.rs", &[], &["x"]), file("src/user.rs", &[], &[])]);
        let t = Tree::new(&files);
        assert_eq!(t.target(&files["src/user.rs"], &q(&[], "crate", 0, &["x"], false)).as_deref(), Some("src/a.rs"));
    }

    #[test]
    fn a_childs_glob_reaches_what_the_parent_imports() {
        let mut parent = file("src/g.rs", &[(&[], "g_tests", Some("src/g/g_tests.rs"))], &[]);
        parent.imports.push(named("Session", &[], "crate", 0, &["sess", "Session"]));
        let mut child = file("src/g/g_tests.rs", &[], &[]);
        child.globs.push(q(&[], "super", 1, &[], false));
        let files = map(vec![
            file("src/lib.rs", &[(&[], "g", Some("src/g.rs")), (&[], "sess", Some("src/sess.rs"))], &[]),
            parent,
            child,
            file("src/sess.rs", &[], &["Session"]),
        ]);
        let t = Tree::new(&files);
        let from = &files["src/g/g_tests.rs"];
        assert_eq!(t.target(from, &q(&[], "super", 1, &["Session", "open"], true)).as_deref(), Some("src/sess.rs"));
        // a name nobody defines or imports is rejected, not guessed.
        assert_eq!(t.target(from, &q(&[], "super", 1, &["Vec", "new"], true)), None);
    }

    #[test]
    fn an_import_cycle_ends_at_the_item_step() {
        let mut a = file("src/a.rs", &[], &[]);
        a.imports.push(named("x", &[], "crate", 0, &["b", "x"]));
        let mut b = file("src/b.rs", &[], &[]);
        b.imports.push(named("x", &[], "crate", 0, &["a", "x"]));
        let files = map(vec![file("src/lib.rs", &[(&[], "a", Some("src/a.rs")), (&[], "b", Some("src/b.rs")), (&[], "c", Some("src/c.rs"))], &[]), a, b, file("src/c.rs", &[], &[])]);
        let t = Tree::new(&files);
        // it terminates, lands in a file of the cycle and is no glob match.
        assert!(t.target(&files["src/c.rs"], &q(&[], "crate", 0, &["a", "x"], false)).is_some());
        assert_eq!(t.target(&files["src/c.rs"], &q(&[], "crate", 0, &["a", "x"], true)), None);
    }
}
