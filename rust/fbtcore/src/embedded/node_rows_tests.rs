//! THE LIST IS CLOSED: every module a listed file names is listed or said not to shape a stored row (`build.rs`).
//! Resolved the way rustc resolves it - `mod` files by `rsmap::modpath`, `use` aliases followed to what they name -
//! off `syn`'s tree, never off the text.

use super::{FILES, LISTED, NOT_SHAPING};
use crate::rsmap::items::ModRef;
use crate::rsmap::modpath;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

type ModPath = Vec<String>;

/// Every module of the crate and the file it is in, and every `use` a module makes, as written.
#[derive(Default)]
struct Crate {
    files: BTreeMap<ModPath, String>,
    of_file: BTreeMap<String, ModPath>,
    aliases: BTreeMap<(ModPath, String), (ModPath, Vec<String>)>,
}

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("cfg") && a.meta.require_list().is_ok_and(|l| l.tokens.to_string() == "test"))
}

fn path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().filter(|a| a.path().is_ident("path")).find_map(|a| match &a.meta {
        syn::Meta::NameValue(nv) => match &nv.value {
            syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) => Some(s.value()),
            _ => None,
        },
        _ => None,
    })
}

/// `(name it binds, path as written)` for every leaf of a `use` tree; a glob binds `*`.
fn flatten(tree: &syn::UseTree, prefix: &[String], out: &mut Vec<(String, Vec<String>)>) {
    let with = |name: &syn::Ident| prefix.iter().cloned().chain([name.to_string()]).collect::<Vec<_>>();
    match tree {
        syn::UseTree::Path(p) => flatten(&p.tree, &with(&p.ident), out),
        syn::UseTree::Name(n) if n.ident == "self" => out.push((prefix.last().cloned().unwrap_or_default(), prefix.to_vec())),
        syn::UseTree::Name(n) => out.push((n.ident.to_string(), with(&n.ident))),
        syn::UseTree::Rename(r) => out.push((r.rename.to_string(), with(&r.ident))),
        syn::UseTree::Glob(_) => out.push(("*".to_string(), prefix.to_vec())),
        syn::UseTree::Group(g) => g.items.iter().for_each(|t| flatten(t, prefix, out)),
    }
}

fn load() -> Crate {
    let src = src();
    let mut krate = Crate::default();
    let mut queue: Vec<(ModPath, String)> = vec![(Vec::new(), "lib.rs".to_string())];
    let mut seen = BTreeSet::new();
    while let Some((module, rel)) = queue.pop() {
        if !seen.insert(rel.clone()) {
            continue;
        }
        let text = std::fs::read_to_string(src.join(&rel)).unwrap_or_else(|e| panic!("src/{rel}: {e}"));
        let file = syn::parse_file(&text).unwrap_or_else(|e| panic!("src/{rel}: {e}"));
        krate.files.insert(module.clone(), rel.clone());
        krate.of_file.insert(rel.clone(), module.clone());
        items(&mut krate, &mut queue, &src, &rel, &module, &[], &file.items);
    }
    krate
}

#[allow(clippy::too_many_arguments)]
fn items(krate: &mut Crate, queue: &mut Vec<(ModPath, String)>, src: &Path, rel: &str, module: &ModPath, inline: &[String], list: &[syn::Item]) {
    for item in list {
        match item {
            syn::Item::Mod(m) if !is_cfg_test(&m.attrs) => {
                let name = m.ident.to_string();
                let child: ModPath = module.iter().cloned().chain([name.clone()]).collect();
                match &m.content {
                    Some((_, inner)) => {
                        krate.files.insert(child.clone(), rel.to_string());
                        let nested: Vec<String> = inline.iter().cloned().chain([name]).collect();
                        items(krate, queue, src, rel, &child, &nested, inner);
                    }
                    None => {
                        let target = modpath::resolve(src, rel, &ModRef { name, path_attr: path_attr(&m.attrs), inline: inline.to_vec() });
                        queue.push((child, target));
                    }
                }
            }
            syn::Item::Use(u) => {
                let mut leaves = Vec::new();
                flatten(&u.tree, &[], &mut leaves);
                for (name, path) in leaves.into_iter().filter(|(n, _)| n != "*") {
                    krate.aliases.insert((module.clone(), name), (module.clone(), path));
                }
            }
            _ => {}
        }
    }
}

impl Crate {
    /// A path as written in `from`, made absolute; None for one outside the crate (std, a dependency, a local).
    fn absolute(&self, from: &ModPath, raw: &[String], seen: &mut BTreeSet<(ModPath, String)>) -> Option<ModPath> {
        let first = raw.first()?;
        match first.as_str() {
            "crate" => Some(raw[1..].to_vec()),
            "self" => Some(from.iter().chain(&raw[1..]).cloned().collect()),
            "super" => {
                let ups = raw.iter().take_while(|s| *s == "super").count();
                let base = from.get(..from.len().checked_sub(ups)?)?;
                Some(base.iter().chain(&raw[ups..]).cloned().collect())
            }
            name => {
                let child: ModPath = from.iter().cloned().chain([name.to_string()]).collect();
                if self.files.contains_key(&child) {
                    return Some(from.iter().chain(raw).cloned().collect());
                }
                let (declared, target) = self.aliases.get(&(from.clone(), name.to_string()))?;
                if !seen.insert((from.clone(), name.to_string())) {
                    return None;
                }
                let resolved = self.absolute(declared, target, seen)?;
                Some(resolved.into_iter().chain(raw[1..].iter().cloned()).collect())
            }
        }
    }

    /// The file an absolute path lands in: its longest module prefix, through any alias that module declares.
    fn file_of(&self, path: &ModPath, seen: &mut BTreeSet<(ModPath, String)>) -> Option<String> {
        let k = (0..=path.len()).rev().find(|k| self.files.contains_key(&path[..*k]))?;
        let module = path[..k].to_vec();
        if let Some(next) = path.get(k)
            && let Some((declared, target)) = self.aliases.get(&(module.clone(), next.clone()))
            && seen.insert((module.clone(), next.clone()))
        {
            let resolved = self.absolute(declared, target, seen)?;
            return self.file_of(&resolved.into_iter().chain(path[k + 1..].iter().cloned()).collect(), seen);
        }
        self.files.get(&module).cloned()
    }
}

/// Every path a file names outside its test modules, as written - macro bodies read as expressions where they parse.
#[derive(Default)]
struct Named(Vec<Vec<String>>);

impl<'a> Visit<'a> for Named {
    fn visit_item_mod(&mut self, m: &'a syn::ItemMod) {
        if !is_cfg_test(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_path(&mut self, p: &'a syn::Path) {
        if p.leading_colon.is_none() {
            self.0.push(p.segments.iter().map(|s| s.ident.to_string()).collect());
        }
        visit::visit_path(self, p);
    }

    fn visit_item_use(&mut self, u: &'a syn::ItemUse) {
        let mut leaves = Vec::new();
        flatten(&u.tree, &[], &mut leaves);
        self.0.extend(leaves.into_iter().map(|(_, path)| path));
    }

    fn visit_macro(&mut self, m: &'a syn::Macro) {
        use syn::punctuated::Punctuated;
        if let Ok(args) = m.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated) {
            args.iter().for_each(|e| self.visit_expr(e));
        }
        visit::visit_macro(self, m);
    }
}

fn covered(entries: &[(&str, &str)], file: &str) -> bool {
    entries.iter().any(|(e, _)| if e.ends_with('/') { file.starts_with(e) } else { file == *e })
}

#[test]
fn every_module_the_listed_code_names_is_classified() {
    let krate = load();
    let mut unclassified: BTreeSet<String> = BTreeSet::new();
    for file in FILES {
        let module = krate.of_file.get(*file).unwrap_or_else(|| panic!("src/{file} is listed and no module of the crate"));
        let text = std::fs::read_to_string(src().join(file)).unwrap();
        let mut named = Named::default();
        named.visit_file(&syn::parse_file(&text).unwrap());
        for raw in &named.0 {
            let mut seen = BTreeSet::new();
            let Some(path) = krate.absolute(module, raw, &mut seen) else { continue };
            let Some(target) = krate.file_of(&path, &mut seen) else { continue };
            if target != *file && !FILES.contains(&target.as_str()) && !covered(NOT_SHAPING, &target) {
                unclassified.insert(format!("src/{file} names `{}` (src/{target})", raw.join("::")));
            }
        }
    }
    assert!(unclassified.is_empty(), "neither listed nor said not to shape a stored row in build.rs:\n{}",
        unclassified.into_iter().collect::<Vec<_>>().join("\n"));
}

#[test]
fn every_entry_of_the_list_is_there() {
    for (entry, _) in LISTED.iter().chain(NOT_SHAPING) {
        let at = src().join(entry);
        assert!(if entry.ends_with('/') { at.is_dir() } else { at.is_file() }, "build.rs names src/{entry}, which is not there");
    }
    assert!(FILES.contains(&"rows/tsapply.rs") && !FILES.contains(&"rows/derive/tsderive.rs"), "the list as hashed is not the list as written");
}

#[test]
fn the_extractor_moves_with_the_listed_rust_and_the_scripts() {
    let value = super::extractor();
    assert!(value.starts_with(super::DIGEST), "the listed rust's digest leads the value");
    assert!(value.contains(&crate::embedded::digest(&crate::embedded::TSROWS)), "the scripts' digest is in the value");
}
