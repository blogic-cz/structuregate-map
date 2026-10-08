//! What one rust file DECLARES, what it NAMES, which module files it pulls in, and the bodies it
//! could share with another file - read off `syn`'s tree, never off the text.
//!
//! DECLARED MEANS NAMEABLE FROM ANOTHER FILE. A private item can only be reached from its own module,
//! so declaring it would put two files' private `fn walk` into one name, make the name ambiguous, and
//! cost the edge a real `pub fn walk` should have drawn. `macro_rules!` is the exception: textual scope
//! is how a macro reaches the files declared after it, visibility keywords or not.
//!
//! A `#[cfg(test)]` MODULE DECLARES NOTHING. Its items are never named from outside it; its uses are
//! kept, because a test reaching into another file is still a reader of that file.
//!
//! MEMBER NAMES ARE NOT USES, as in the C# half: `thing.walk()` names a method of whatever `thing` is,
//! and drawing an edge to a file that declares a free `fn walk` would be the grep this map replaces.

use super::lines::LineStarts;
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::BTreeSet;
use syn::visit::{self, Visit};

/// Fewer statements than this is an idiom, not a copied body - the C# half's threshold.
const MIN_BODY_STATEMENTS: usize = 3;
const MAX_SUMMARY: usize = 160;

/// A `mod name;` that pulls another file in, with what decides WHERE that file is.
pub struct ModRef {
    pub name: String,
    pub path_attr: Option<String>,
    /// The inline `mod a { ... }` blocks around it, outermost first - each is a directory.
    pub inline: Vec<String>,
}

pub struct Body {
    pub digest: String,
    pub line: usize,
    pub name: String,
    pub size: usize,
}

#[derive(Default)]
pub struct Items {
    pub summary: String,
    pub declares: BTreeSet<String>,
    pub uses: BTreeSet<String>,
    pub mods: Vec<ModRef>,
    pub has_main: bool,
    pub bodies: Vec<Body>,
}

pub fn read(file: &syn::File, starts: &LineStarts<'_>) -> Items {
    let mut walker = Walker { items: Items::default(), starts, inline: vec![], tests: 0, fns: 0 };
    walker.items.summary = summary(&file.attrs);
    walker.visit_file(file);
    walker.items
}

struct Walker<'s, 't> {
    items: Items,
    starts: &'s LineStarts<'t>,
    inline: Vec<String>,
    /// How many `#[cfg(test)]` modules enclose the walk.
    tests: usize,
    /// How many function bodies enclose it - an item inside a body is local to that body.
    fns: usize,
}

impl Walker<'_, '_> {
    fn declare(&mut self, ident: &syn::Ident, vis: &syn::Visibility) {
        if self.tests == 0 && self.fns == 0 && !matches!(vis, syn::Visibility::Inherited) {
            self.items.declares.insert(ident.to_string());
        }
    }

    fn use_name(&mut self, ident: &syn::Ident) {
        let name = ident.to_string();
        if !matches!(name.as_str(), "self" | "Self" | "super" | "crate") {
            self.items.uses.insert(name.trim_start_matches("r#").to_string());
        }
    }

    fn body(&mut self, block: &syn::Block, name: String) {
        if block.stmts.len() < MIN_BODY_STATEMENTS {
            return;
        }
        let span = block.brace_token.span.join();
        self.items.bodies.push(Body {
            digest: fingerprint(block.to_token_stream()),
            line: span.start().line,
            name,
            size: self.starts.chars_between(span.start(), span.end()),
        });
    }

    fn in_fn(&mut self, walk: impl FnOnce(&mut Self)) {
        self.fns += 1;
        walk(self);
        self.fns -= 1;
    }
}

impl<'ast> Visit<'ast> for Walker<'_, '_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if m.content.is_none() {
            if self.fns == 0 {
                self.items.mods.push(ModRef {
                    name: m.ident.to_string(),
                    path_attr: path_attr(&m.attrs),
                    inline: self.inline.clone(),
                });
            }
            return;
        }
        let test = is_cfg_test(&m.attrs);
        self.inline.push(path_attr(&m.attrs).unwrap_or_else(|| m.ident.to_string()));
        self.tests += usize::from(test);
        visit::visit_item_mod(self, m);
        self.tests -= usize::from(test);
        self.inline.pop();
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.declare(&f.sig.ident, &f.vis);
        if f.sig.ident == "main" && self.fns == 0 && self.inline.is_empty() {
            self.items.has_main = true;
        }
        self.body(&f.block, f.sig.ident.to_string());
        self.in_fn(|w| visit::visit_item_fn(w, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.body(&f.block, f.sig.ident.to_string());
        self.in_fn(|w| visit::visit_impl_item_fn(w, f));
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        if let Some(block) = &f.default {
            self.body(block, f.sig.ident.to_string());
        }
        self.in_fn(|w| visit::visit_trait_item_fn(w, f));
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.declare(&i.ident, &i.vis);
        visit::visit_item_struct(self, i);
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.declare(&i.ident, &i.vis);
        visit::visit_item_enum(self, i);
    }

    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        self.declare(&i.ident, &i.vis);
        visit::visit_item_union(self, i);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.declare(&i.ident, &i.vis);
        visit::visit_item_trait(self, i);
    }

    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.declare(&i.ident, &i.vis);
        visit::visit_item_type(self, i);
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        self.declare(&i.ident, &i.vis);
        visit::visit_item_const(self, i);
    }

    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        self.declare(&i.ident, &i.vis);
        visit::visit_item_static(self, i);
    }

    fn visit_item_macro(&mut self, i: &'ast syn::ItemMacro) {
        if let Some(ident) = &i.ident
            && i.mac.path.is_ident("macro_rules")
            && self.tests == 0
            && self.fns == 0
        {
            self.items.declares.insert(ident.to_string());
        }
        visit::visit_item_macro(self, i);
    }

    fn visit_path(&mut self, p: &'ast syn::Path) {
        for segment in &p.segments {
            self.use_name(&segment.ident);
        }
        visit::visit_path(self, p);
    }

    /// A MACRO'S ARGUMENTS ARE NOT PARSED by syn - `vec![Store::new()]` is tokens to it - so their
    /// identifiers are read off the tokens. Anything after a `.` is a member, and is left out as a
    /// member is everywhere else.
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        macro_idents(m.tokens.clone(), &mut |ident| self.use_name(ident));
        visit::visit_macro(self, m);
    }

    fn visit_use_tree(&mut self, tree: &'ast syn::UseTree) {
        match tree {
            syn::UseTree::Path(p) => self.use_name(&p.ident),
            syn::UseTree::Name(n) => self.use_name(&n.ident),
            syn::UseTree::Rename(r) => self.use_name(&r.ident),
            _ => {}
        }
        visit::visit_use_tree(self, tree);
    }
}

fn macro_idents(stream: TokenStream, found: &mut impl FnMut(&syn::Ident)) {
    let mut after_dot = false;
    for token in stream {
        match &token {
            TokenTree::Ident(ident) if !after_dot => found(ident),
            TokenTree::Group(group) => macro_idents(group.stream(), found),
            _ => {}
        }
        after_dot = matches!(&token, TokenTree::Punct(p) if p.as_char() == '.');
    }
}

/// THE SHAPE OF A BODY, with LOCAL NAMES BLANKED so one idiom pasted under two sets of variable
/// names still fingerprints alike. A name after `.` or `::` is a member or a path step and is KEPT:
/// `fs::read_to_string` is what makes that call that call. Comments never enter - they are not
/// tokens - so two bodies match when they DO the same thing however they are described.
fn fingerprint(stream: TokenStream) -> String {
    let mut hasher = blake3::Hasher::new();
    shape(stream, &mut hasher, &mut false);
    hasher.finalize().to_hex()[..16].to_string()
}

fn shape(stream: TokenStream, hasher: &mut blake3::Hasher, keep_next: &mut bool) {
    for token in stream {
        let keep = *keep_next;
        *keep_next = false;
        match token {
            TokenTree::Ident(ident) => {
                let text = ident.to_string();
                hasher.update(if keep { text.as_bytes() } else { b"_" });
            }
            TokenTree::Punct(p) => {
                hasher.update(p.as_char().to_string().as_bytes());
                *keep_next = matches!(p.as_char(), '.' | ':');
            }
            TokenTree::Literal(lit) => {
                hasher.update(lit.to_string().as_bytes());
            }
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                hasher.update(open.as_bytes());
                shape(group.stream(), hasher, &mut false);
                hasher.update(close.as_bytes());
            }
        }
    }
}

/// The file's headline: the first line of its `//!` documentation.
fn summary(attrs: &[syn::Attribute]) -> String {
    for attr in attrs {
        if !matches!(attr.style, syn::AttrStyle::Inner(_)) || !attr.path().is_ident("doc") {
            continue;
        }
        let Some(text) = doc_text(attr) else { continue };
        let line = text.trim();
        if line.is_empty() {
            continue;
        }
        return match line.char_indices().nth(MAX_SUMMARY) {
            Some((cut, _)) => format!("{} …", &line[..cut]),
            None => line.to_string(),
        };
    }
    String::new()
}

fn doc_text(attr: &syn::Attribute) -> Option<String> {
    let syn::Meta::NameValue(pair) = &attr.meta else { return None };
    let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(text), .. }) = &pair.value else {
        return None;
    };
    Some(text.value())
}

fn path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().filter(|a| a.path().is_ident("path")).find_map(doc_text)
}

/// `#[cfg(test)]` exactly. A `cfg(all(test, windows))` is still a test module, so any `cfg` whose
/// tokens mention `test` counts - a false yes costs a declaration nobody could name anyway.
pub(super) fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && matches!(&a.meta, syn::Meta::List(list)
                if list.tokens.clone().into_iter().any(|t| names_test(&t)))
    })
}

fn names_test(token: &TokenTree) -> bool {
    match token {
        TokenTree::Ident(ident) => ident == "test",
        TokenTree::Group(group) => group.stream().into_iter().any(|t| names_test(&t)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(text: &str) -> Items {
        let file = syn::parse_file(text).expect("parses");
        read(&file, &LineStarts::new(text))
    }

    #[test]
    fn only_what_another_file_can_name_is_declared() {
        let found = items("pub struct A;\nstruct Hidden;\npub(crate) fn b() {}\nmacro_rules! m { () => {} }\n");
        assert_eq!(found.declares.into_iter().collect::<Vec<_>>(), ["A", "b", "m"]);
    }

    #[test]
    fn a_test_module_and_an_item_inside_a_body_declare_nothing() {
        let found = items("#[cfg(test)]\nmod tests { pub struct T; }\npub fn f() { pub struct Local; }\n");
        assert_eq!(found.declares.into_iter().collect::<Vec<_>>(), ["f"]);
    }

    #[test]
    fn a_path_a_macro_argument_and_a_use_are_uses_and_a_member_is_not() {
        let found = items("use crate::store::Store;\nfn f() { let x = vec![Rows::new()]; x.walk(); }\n");
        for name in ["store", "Store", "Rows", "new", "vec"] {
            assert!(found.uses.contains(name), "{name} is a use");
        }
        assert!(!found.uses.contains("walk"), "a method after a dot is a member");
        assert!(!found.uses.contains("crate"), "a path keyword is not a name");
    }

    #[test]
    fn a_mod_declaration_is_recorded_with_its_path_attribute_and_its_inline_parents() {
        let found = items("mod a;\n#[path = \"x/y.rs\"]\nmod b;\nmod outer { mod c; }\n");
        let shape: Vec<_> = found.mods.iter().map(|m| (m.name.as_str(), m.path_attr.clone(), m.inline.clone())).collect();
        assert_eq!(shape[0], ("a", None, vec![]));
        assert_eq!(shape[1], ("b", Some("x/y.rs".to_string()), vec![]));
        assert_eq!(shape[2], ("c", None, vec!["outer".to_string()]));
    }

    #[test]
    fn two_bodies_that_differ_only_in_local_names_and_comments_fingerprint_alike() {
        let one = items("fn a(p: &str) { let x = p.len(); let y = x + 1; println!(\"{}\", y); }\n");
        let two = items("fn b(q: &str) {\n // why\n let m = q.len(); let n = m + 1; println!(\"{}\", n); }\n");
        assert_eq!(one.bodies[0].digest, two.bodies[0].digest);
        assert_eq!(one.bodies[0].name, "a");
    }

    #[test]
    fn a_body_under_the_statement_threshold_is_not_recorded() {
        assert!(items("fn a() { let x = 1; x; }\n").bodies.is_empty());
    }

    #[test]
    fn the_summary_is_the_first_line_of_the_crate_doc() {
        assert_eq!(items("//! First line.\n//! Second.\nfn a() {}\n").summary, "First line.");
    }

    #[test]
    fn main_at_the_top_is_an_entry_and_main_inside_a_module_is_not() {
        assert!(items("fn main() {}\n").has_main);
        assert!(!items("mod m { fn main() {} }\n").has_main);
    }
}
