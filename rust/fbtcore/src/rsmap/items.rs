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
//! A `#[cfg(test)]` ITEM DECLARES NOTHING EITHER, for the same reason.
//!
//! A NAME A PATTERN BINDS IS A LOCAL (`locals.rs`): a `let`, a parameter, a closure input, a `match` arm or a `for`
//! binds `n`, and a bare path `n` that reads it is no use of another file's `pub fn n`. Without that, a local `name`
//! joined its file to whichever file declared a `pub fn name`.
//!
//! A NAME THE FILE DEFINES ITSELF IS ITS OWN, private or test-only: rustc resolves it to that item, so a call of a
//! private `fn walk` never joins to another file's `pub fn walk`.
//!
//! A PATH'S ROOT DECIDES WHAT ITS LATER SEGMENTS NAME (`paths.rs`): `TokenTree::Group` or `std::env::temp_dir` name
//! nothing in this tree, `crate::a::B` is a QUALIFIED path the join walks through the module tree (`graph/rspaths.rs`;
//! the inline modules are recorded here for it), and a name reached through another crate is `rooted` under it.
//!
//! MEMBER NAMES ARE NOT USES, as in the C# half: `thing.walk()` names a method of whatever `thing` is,
//! and drawing an edge to a file that declares a free `fn walk` would be the grep this map replaces.

use super::lines::LineStarts;
use super::locals;
use super::literals::Piece;
use super::paths;
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};
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
    /// Declared under `#[cfg(test)]` - on itself or an enclosing inline module.
    pub test: bool,
}

pub struct Body {
    pub digest: String,
    pub line: usize,
    pub name: String,
    pub size: usize,
    /// One digest per statement, names and literals blanked (`fingerprint::shingles`).
    pub shingles: Vec<String>,
}

/// An expression long enough to be a copy (`fingerprint::copied_unit`): its digest, its line and its size in characters.
pub struct Expression {
    pub digest: String,
    pub line: usize,
    pub size: usize,
}

#[derive(Default)]
pub struct Items {
    pub summary: String,
    pub declares: BTreeSet<String>,
    pub uses: BTreeSet<String>,
    /// What the file names through ANOTHER crate, by that crate's name.
    pub rooted: BTreeMap<String, BTreeSet<String>>,
    pub mods: Vec<ModRef>,
    /// The inline `mod x { }` blocks - (the inline chain around it, its name) - and the paths that lead into the tree
    /// (`paths::Qualified`), for the module-tree walk of the join (`graph/rspaths.rs`).
    pub inline_mods: Vec<(Vec<String>, String)>,
    pub qualified: Vec<paths::Qualified>,
    /// The in-tree imports and globs of each module, for the walk to follow (`paths::imports`).
    pub imports: Vec<paths::Named>,
    pub globs: Vec<paths::Qualified>,
    /// Every name the file defines, private included: what a glob of this file brings.
    pub own: BTreeSet<String>,
    pub has_main: bool,
    pub bodies: Vec<Body>,
    pub expressions: Vec<Expression>,
}

pub fn read(file: &syn::File, starts: &LineStarts<'_>) -> Items {
    let mut walker = Walker { items: Items::default(), starts, inline: vec![], tests: 0, fns: 0, locals: vec![], own: BTreeSet::new(), paths: vec![], use_items: vec![], template: false };
    walker.items.summary = summary(&file.attrs);
    walker.visit_file(file);
    let (mut uses, rooted, qualified) = paths::resolve(&walker.paths, &walker.use_items, &walker.own);
    uses.retain(|u| !walker.own.contains(u));
    walker.items.uses = uses;
    walker.items.rooted = rooted;
    walker.items.qualified = qualified;
    (walker.items.imports, walker.items.globs) = paths::imports(&walker.use_items, &walker.own);
    walker.items.own = walker.own.clone();
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
    /// The names bound by the patterns around the walk, innermost last; empty at module level.
    locals: Vec<BTreeSet<String>>,
    /// Every name the file itself defines, at any depth - a fn's local `fn helper` is its own too.
    own: BTreeSet<String>,
    /// Every path's segments, and every `use` leaf - read by `paths::resolve` once the whole file is seen.
    paths: Vec<(Vec<String>, Vec<String>)>,
    use_items: Vec<paths::UseItem>,
    /// Inside a `macro_rules!` body: `$name` and `$name:frag` are the macro's own, not names.
    template: bool,
}

impl Walker<'_, '_> {
    fn declare(&mut self, ident: &syn::Ident, vis: &syn::Visibility, attrs: &[syn::Attribute]) {
        self.own.insert(ident.to_string());
        if self.tests == 0
            && self.fns == 0
            && !matches!(vis, syn::Visibility::Inherited)
            && !is_cfg_test(attrs)
        {
            self.items.declares.insert(ident.to_string());
        }
    }

    fn body(&mut self, block: &syn::Block, name: String) {
        if block.stmts.len() < MIN_BODY_STATEMENTS {
            return;
        }
        let span = block.brace_token.span.join();
        self.items.bodies.push(Body {
            digest: super::fingerprint::fingerprint(block.to_token_stream()),
            line: span.start().line,
            name,
            size: self.starts.chars_between(span.start(), span.end()),
            shingles: super::fingerprint::shingles(block),
        });
    }

    fn in_fn(&mut self, inputs: Vec<&syn::Pat>, walk: impl FnOnce(&mut Self)) {
        self.fns += 1;
        // AN ITEM INSIDE A BODY CANNOT SEE THE OUTER LOCALS.
        let outer = std::mem::take(&mut self.locals);
        self.locals.push(BTreeSet::new());
        for pat in inputs {
            self.bind(pat);
        }
        walk(self);
        self.locals = outer;
        self.fns -= 1;
    }

    fn bound(&self, name: &str) -> bool {
        self.locals.iter().any(|s| s.contains(name))
    }

    fn bind(&mut self, pat: &syn::Pat) {
        if let Some(scope) = self.locals.last_mut() {
            scope.extend(locals::bindings(pat));
        }
    }

    fn scoped(&mut self, walk: impl FnOnce(&mut Self)) {
        self.locals.push(BTreeSet::new());
        walk(self);
        self.locals.pop();
    }
}

impl<'ast> Visit<'ast> for Walker<'_, '_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        self.own.insert(m.ident.to_string());
        if m.content.is_none() {
            if self.fns == 0 {
                self.items.mods.push(ModRef {
                    name: m.ident.to_string(),
                    path_attr: path_attr(&m.attrs),
                    inline: self.inline.clone(),
                    test: self.tests > 0 || is_cfg_test(&m.attrs),
                });
            }
            return;
        }
        if self.fns == 0 {
            self.items.inline_mods.push((self.inline.clone(), m.ident.to_string()));
        }
        let test = is_cfg_test(&m.attrs);
        self.inline.push(path_attr(&m.attrs).unwrap_or_else(|| m.ident.to_string()));
        self.tests += usize::from(test);
        visit::visit_item_mod(self, m);
        self.tests -= usize::from(test);
        self.inline.pop();
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.declare(&f.sig.ident, &f.vis, &f.attrs);
        if f.sig.ident == "main" && self.fns == 0 && self.inline.is_empty() {
            self.items.has_main = true;
        }
        self.body(&f.block, f.sig.ident.to_string());
        self.in_fn(typed(&f.sig), |w| visit::visit_item_fn(w, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.body(&f.block, f.sig.ident.to_string());
        self.in_fn(typed(&f.sig), |w| visit::visit_impl_item_fn(w, f));
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        if let Some(block) = &f.default {
            self.body(block, f.sig.ident.to_string());
        }
        self.in_fn(typed(&f.sig), |w| visit::visit_trait_item_fn(w, f));
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.declare(&i.ident, &i.vis, &i.attrs);
        visit::visit_item_struct(self, i);
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.declare(&i.ident, &i.vis, &i.attrs);
        visit::visit_item_enum(self, i);
    }

    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        self.declare(&i.ident, &i.vis, &i.attrs);
        visit::visit_item_union(self, i);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.declare(&i.ident, &i.vis, &i.attrs);
        visit::visit_item_trait(self, i);
    }

    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.declare(&i.ident, &i.vis, &i.attrs);
        visit::visit_item_type(self, i);
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        self.declare(&i.ident, &i.vis, &i.attrs);
        visit::visit_item_const(self, i);
    }

    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        self.declare(&i.ident, &i.vis, &i.attrs);
        visit::visit_item_static(self, i);
    }

    fn visit_item_macro(&mut self, i: &'ast syn::ItemMacro) {
        if let Some(ident) = &i.ident
            && i.mac.path.is_ident("macro_rules")
        {
            self.own.insert(ident.to_string());
        }
        if let Some(ident) = &i.ident
            && i.mac.path.is_ident("macro_rules")
            && self.tests == 0
            && self.fns == 0
            && !is_cfg_test(&i.attrs)
        {
            self.items.declares.insert(ident.to_string());
        }
        // A `thread_local!` static is declared like any `static`: own here, and named from outside only if visible.
        if i.mac.path.is_ident("thread_local") {
            for t in super::consts::thread_locals(&i.mac) {
                self.declare(&t.ident, &t.vis, &t.attrs);
            }
        }
        let template = i.mac.path.is_ident("macro_rules");
        self.template = template;
        visit::visit_item_macro(self, i);
        self.template = false;
    }

    /// A SECOND UNIT BESIDE THE BODIES: an idiom pasted INSIDE larger functions is invisible to a body digest.
    fn visit_expr(&mut self, e: &'ast syn::Expr) {
        if super::fingerprint::copied_unit(e) {
            let tokens = e.to_token_stream();
            if super::fingerprint::leaves(tokens.clone()) >= super::fingerprint::MIN_EXPRESSION_LEAVES {
                let span = syn::spanned::Spanned::span(e);
                self.items.expressions.push(Expression {
                    digest: super::fingerprint::fingerprint(tokens),
                    line: span.start().line,
                    size: self.starts.chars_between(span.start(), span.end()),
                });
            }
        }
        visit::visit_expr(self, e);
    }

    fn visit_block(&mut self, b: &'ast syn::Block) {
        self.scoped(|w| visit::visit_block(w, b));
    }

    /// THE INITIALIZER IS READ BEFORE THE NAME IS BOUND: `let n = n + 1;` reads the outer `n`.
    fn visit_local(&mut self, l: &'ast syn::Local) {
        if let Some(init) = &l.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
        self.visit_pat(&l.pat);
        self.bind(&l.pat);
    }

    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        self.scoped(|w| {
            for pat in &c.inputs {
                w.bind(pat);
            }
            visit::visit_expr_closure(w, c);
        });
    }

    fn visit_arm(&mut self, a: &'ast syn::Arm) {
        self.scoped(|w| {
            w.bind(&a.pat);
            visit::visit_arm(w, a);
        });
    }

    fn visit_expr_for_loop(&mut self, f: &'ast syn::ExprForLoop) {
        self.visit_expr(&f.expr);
        self.scoped(|w| {
            w.visit_pat(&f.pat);
            w.bind(&f.pat);
            w.visit_block(&f.body);
        });
    }

    fn visit_expr_if(&mut self, i: &'ast syn::ExprIf) {
        self.scoped(|w| {
            w.visit_expr(&i.cond);
            w.visit_block(&i.then_branch);
        });
        if let Some((_, other)) = &i.else_branch {
            self.visit_expr(other);
        }
    }

    fn visit_expr_while(&mut self, x: &'ast syn::ExprWhile) {
        self.scoped(|w| visit::visit_expr_while(w, x));
    }

    fn visit_expr_let(&mut self, l: &'ast syn::ExprLet) {
        self.visit_expr(&l.expr);
        self.visit_pat(&l.pat);
        self.bind(&l.pat);
    }

    /// A BARE PATH THAT READS A LOCAL IS NO USE of an item of that name.
    fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
        if p.qself.is_none()
            && p.path.segments.len() == 1
            && self.bound(&p.path.segments[0].ident.to_string())
        {
            return;
        }
        visit::visit_expr_path(self, p);
    }

    /// `pub(crate)`, `pub(super)` AND `pub(in a::b)` ARE VISIBILITY, never a use: read as the path `crate` they drew an edge to the crate root.
    fn visit_visibility(&mut self, _: &'ast syn::Visibility) {}

    fn visit_path(&mut self, p: &'ast syn::Path) {
        let segs = p.segments.iter().map(|s| s.ident.to_string().trim_start_matches("r#").to_string()).collect();
        self.paths.push((self.inline.clone(), segs));
        visit::visit_path(self, p);
    }

    /// A MACRO'S ARGUMENTS ARE NOT PARSED by syn - `vec![Store::new()]` is tokens to it - so their
    /// identifiers are read off the tokens. Anything after a `.` is a member, and is left out as a
    /// member is everywhere else.
    ///
    /// ARGUMENTS THAT PARSE ARE CODE, walked as code: a closure parameter, a struct-literal label and a `matches!`
    /// binding are patterns and fields there, not uses. The token scrape is only for a body that does not parse.
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let pieces = if self.template { None } else { super::literals::parsed_pieces(m) };
        if let Some(pieces) = pieces {
            self.scoped(|w| {
                for piece in &pieces {
                    match piece {
                        Piece::Expr(e) => w.visit_expr(e),
                        Piece::Pat(p) => {
                            w.visit_pat(p);
                            w.bind(p);
                        }
                        Piece::Lit(_) => {}
                    }
                }
            });
        } else {
            let mut found = vec![];
            macro_paths(m.tokens.clone(), self.template, &BTreeSet::new(), &mut |path| found.push(path));
            found.retain(|p| !self.bound(&p[0]));
            self.paths.extend(found.into_iter().map(|p| (self.inline.clone(), p)));
        }
        visit::visit_macro(self, m);
    }

    /// A `use` is read whole (`paths::flatten`): its root says what every segment of it names.
    fn visit_item_use(&mut self, i: &'ast syn::ItemUse) {
        let module = self.inline.clone();
        self.use_items.extend(paths::flatten(&i.tree).into_iter().map(|u| paths::UseItem { module: module.clone(), ..u }));
    }
}

fn typed(sig: &syn::Signature) -> Vec<&syn::Pat> {
    sig.inputs
        .iter()
        .filter_map(|a| match a {
            syn::FnArg::Typed(t) => Some(&*t.pat),
            syn::FnArg::Receiver(_) => None,
        })
        .collect()
}

/// Each path the tokens spell, as its segments: `TokenTree::Group` is one path of two, not two bare names, for the root
/// rule (`paths.rs`) reads the first segment. A path after a `.` is a member and is left out.
/// In a `macro_rules!` TEMPLATE the ident after a `$` is a metavariable (`$crate` too) and the one after `$name :`
/// its fragment specifier - neither is a name of this tree.
/// A `let NAME` (`let mut NAME`) followed by `=`, `:` or `;` BINDS NAME from the next `;` on, in its group and the
/// groups inside it (a template's `let value = $body; value`): a later bare `NAME` is that local. Its own right-hand
/// side (`let maker = maker();`) still names the item, and a pattern (`let Shape { a } = ..`) binds no name here.
fn macro_paths(stream: TokenStream, template: bool, outer: &BTreeSet<String>, found: &mut impl FnMut(Vec<String>)) {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();
    let colon = |t: Option<&TokenTree>| matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == ':');
    let punct = |t: Option<&TokenTree>, c: &[char]| matches!(t, Some(TokenTree::Punct(p)) if c.contains(&p.as_char()));
    let mut bound = outer.clone();
    let mut pending: Vec<String> = Vec::new();
    let mut i = 0;
    let mut after_dot = false;
    let dollar = |j: Option<usize>| matches!(j.and_then(|j| tokens.get(j)), Some(TokenTree::Punct(p)) if p.as_char() == '$');
    while i < tokens.len() {
        let meta = template && (dollar(i.checked_sub(1)) || (colon(i.checked_sub(1).and_then(|j| tokens.get(j))) && dollar(i.checked_sub(3))));
        match &tokens[i] {
            TokenTree::Ident(_) if meta => {}
            TokenTree::Ident(ident) if ident == "let" => {
                let at = if matches!(tokens.get(i + 1), Some(TokenTree::Ident(m)) if m == "mut") { i + 2 } else { i + 1 };
                if let Some(TokenTree::Ident(name)) = tokens.get(at)
                    && punct(tokens.get(at + 1), &['=', ':', ';'])
                {
                    pending.push(name.to_string());
                    i = at;
                }
            }
            TokenTree::Ident(ident) if !after_dot => {
                let mut path = vec![ident.to_string().trim_start_matches("r#").to_string()];
                while colon(tokens.get(i + 1)) && colon(tokens.get(i + 2)) {
                    let Some(TokenTree::Ident(next)) = tokens.get(i + 3) else { break };
                    path.push(next.to_string().trim_start_matches("r#").to_string());
                    i += 3;
                }
                // `name: ..` is a field label or a binding's type (`R { files: .. }`), never a use.
                let label = path.len() == 1 && matches!(tokens.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':' && p.spacing() == proc_macro2::Spacing::Alone);
                let local = path.len() == 1 && bound.contains(&path[0]);
                if !label && !local {
                    found(path);
                }
            }
            TokenTree::Punct(p) if p.as_char() == ';' => bound.extend(pending.drain(..)),
            TokenTree::Group(group) => macro_paths(group.stream(), template, &bound, found),
            _ => {}
        }
        after_dot = matches!(&tokens[i], TokenTree::Punct(p) if p.as_char() == '.');
        i += 1;
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

/// A `cfg` THAT REQUIRES `test`, by evaluating its predicate: `test`, `all(..)` with a child that requires it,
/// `any(..)` whose every child does. `not(test)` is PRODUCTION code - read as test it declared nothing, and every
/// caller of the item lost its edge.
pub(super) fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Meta>().is_ok_and(|m| requires_test(&m))
    })
}

fn requires_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(p) => p.is_ident("test"),
        syn::Meta::List(list) => {
            let children = list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated);
            let Ok(children) = children else { return false };
            if list.path.is_ident("all") {
                children.iter().any(requires_test)
            } else if list.path.is_ident("any") {
                !children.is_empty() && children.iter().all(requires_test)
            } else {
                false
            }
        }
        syn::Meta::NameValue(_) => false,
    }
}

#[cfg(test)]
#[path = "tests/items_tests.rs"]
mod tests;
