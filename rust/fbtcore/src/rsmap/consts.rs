//! THE CONSTANTS OF ONE RUST FILE, for the deep map's `consts` table - in the columns the C#, python and
//! TypeScript halves already write, so one query reads every language's constants alike.
//!
//! A CONSTANT IS A `const` OR A `static` ITEM another item can name: at module level, in an inline `mod`, or
//! associated with an `impl` or a `trait` (the C# half's `const` field). One inside a function body is a
//! local, as a python constant inside a `def` is, and one in a `#[cfg(test)]` module is a fixture; neither is
//! a fact about the codebase somebody looks for by name.
//!
//! `value` IS WHAT A LITERAL HOLDS, as the C# half folds `Prefix + "/root"` into the string it is: the text
//! of `"x"` is a quoted token, its value is `x`. Anything that is not a literal keeps `source` and no value -
//! rust has no compile-time evaluator in this process, and a guessed value is worse than none.

use super::lines::LineStarts;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

pub struct Const {
    pub line: usize,
    pub name: String,
    /// `const` or `static`.
    pub kind: &'static str,
    /// The type an associated constant belongs to (`Config`, `Shape for Circle`), empty at module level.
    pub owner: String,
    pub exported: bool,
    pub ty: String,
    pub source: String,
    pub value: String,
    pub value_kind: &'static str,
    /// Every path the initializer names, in order, once each - a python constant's `reads`.
    pub reads: Vec<String>,
    /// What the initializer calls: a function by its path, a method as `.name`, a macro as `name!`.
    pub calls: Vec<String>,
}

pub fn read(file: &syn::File, starts: &LineStarts<'_>) -> Vec<Const> {
    let mut walker = Walker { starts, owner: Vec::new(), out: Vec::new() };
    walker.visit_file(file);
    walker.out
}

struct Walker<'s, 't> {
    starts: &'s LineStarts<'t>,
    owner: Vec<String>,
    out: Vec<Const>,
}

impl Walker<'_, '_> {
    fn add(&mut self, kind: &'static str, ident: &syn::Ident, exported: bool, ty: &syn::Type, expr: Option<&syn::Expr>) {
        let (value, value_kind) = expr.map_or((String::new(), ""), literal);
        let mut names = Names::default();
        if let Some(expr) = expr {
            names.visit_expr(expr);
        }
        self.out.push(Const {
            line: ident.span().start().line,
            name: ident.to_string(),
            kind,
            owner: self.owner.last().cloned().unwrap_or_default(),
            exported,
            ty: compact(&ty.to_token_stream().to_string()),
            source: expr.map_or(String::new(), |e| self.text(e)),
            value,
            value_kind,
            reads: names.reads,
            calls: names.calls,
        });
    }

    /// The initializer AS WRITTEN, cut out of the file by its span - never re-printed from the tree, which
    /// spaces every token (`1 << 20` would read back fine, `PREFIX . len ()` would not).
    fn text(&self, expr: &syn::Expr) -> String {
        let span = expr.span();
        let (from, to) = (self.starts.offset(span.start()), self.starts.offset(span.end()));
        self.starts.slice(from, to).trim().to_string()
    }
}

fn public(vis: &syn::Visibility) -> bool {
    !matches!(vis, syn::Visibility::Inherited)
}

impl<'ast> Visit<'ast> for Walker<'_, '_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !super::items::is_cfg_test(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    // A FUNCTION BODY IS NOT WALKED: what is declared inside it is a local.
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
    fn visit_impl_item_fn(&mut self, _: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _: &'ast syn::TraitItemFn) {}

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        self.add("const", &c.ident, public(&c.vis), &c.ty, Some(&c.expr));
    }

    fn visit_item_static(&mut self, s: &'ast syn::ItemStatic) {
        self.add("static", &s.ident, public(&s.vis), &s.ty, Some(&s.expr));
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let ty = compact(&i.self_ty.to_token_stream().to_string());
        let owner = match &i.trait_ {
            Some((_, path, _)) => format!("{} for {ty}", compact(&path.to_token_stream().to_string())),
            None => ty,
        };
        // An inherent impl's constant is as visible as it says; a trait impl's is as visible as the trait.
        self.owner.push(owner);
        for item in &i.items {
            if let syn::ImplItem::Const(c) = item {
                let exported = i.trait_.is_some() || public(&c.vis);
                self.add("const", &c.ident, exported, &c.ty, Some(&c.expr));
            }
        }
        self.owner.pop();
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        self.owner.push(t.ident.to_string());
        for item in &t.items {
            if let syn::TraitItem::Const(c) = item {
                let default = c.default.as_ref().map(|(_, e)| e);
                self.add("const", &c.ident, public(&t.vis), &c.ty, default);
            }
        }
        self.owner.pop();
    }
}

/// The names an initializer reads and the things it calls.
#[derive(Default)]
pub(super) struct Names {
    pub(super) reads: Vec<String>,
    pub(super) calls: Vec<String>,
}

fn once(list: &mut Vec<String>, name: String) {
    if !list.contains(&name) {
        list.push(name);
    }
}

impl<'ast> Visit<'ast> for Names {
    fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
        once(&mut self.reads, path_text(&p.path));
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func {
            once(&mut self.calls, path_text(&p.path));
        }
        // The callee is a call, not a read; only the arguments are walked.
        for argument in &c.args {
            self.visit_expr(argument);
        }
    }

    // THE RECEIVER FIRST, so the calls read in the order they are written: `a(x).max(1)` is `a`, then `.max`.
    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        self.visit_expr(&m.receiver);
        once(&mut self.calls, format!(".{}", m.method));
        for argument in &m.args {
            self.visit_expr(argument);
        }
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        once(&mut self.calls, format!("{}!", path_text(&m.path)));
    }
}

pub(super) fn path_text(path: &syn::Path) -> String {
    let names: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let joined = names.join("::");
    if path.leading_colon.is_some() { format!("::{joined}") } else { joined }
}

/// What a LITERAL initializer holds, and which kind of literal it is - a negative number included.
fn literal(expr: &syn::Expr) -> (String, &'static str) {
    match expr {
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Str(s) => (s.value(), "string"),
            syn::Lit::Int(i) => (i.base10_digits().to_string(), "int"),
            syn::Lit::Float(f) => (f.base10_digits().to_string(), "float"),
            syn::Lit::Bool(b) => (b.value.to_string(), "bool"),
            syn::Lit::Char(c) => (c.value().to_string(), "char"),
            _ => (String::new(), ""),
        },
        syn::Expr::Unary(syn::ExprUnary { op: syn::UnOp::Neg(_), expr, .. }) => match literal(expr) {
            (digits, kind @ ("int" | "float")) => (format!("-{digits}"), kind),
            _ => (String::new(), ""),
        },
        syn::Expr::Paren(p) => literal(&p.expr),
        _ => (String::new(), ""),
    }
}

/// A type or path as `quote` prints it, without the space it puts around every token: `& 'static str`
/// reads back as `&'static str`.
pub(super) fn compact(printed: &str) -> String {
    let mut out = String::with_capacity(printed.len());
    let chars: Vec<char> = printed.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == ' ' {
            let before = i.checked_sub(1).and_then(|j| chars.get(j)).copied().unwrap_or(' ');
            let after = chars.get(i + 1).copied().unwrap_or(' ');
            let word = |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '\'';
            // A space is kept only between two words - `dyn Trait`, `Shape for Circle`, `mut T`.
            if word(before) && word(after) {
                out.push(' ');
            }
            continue;
        }
        out.push(*c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consts(text: &str) -> Vec<Const> {
        let file = syn::parse_file(text).unwrap();
        let found = read(&file, &LineStarts::new(text));
        super::super::forget_spans();
        found
    }

    #[test]
    fn a_module_constant_keeps_its_source_and_its_literal_value() {
        let found = consts("pub const LIMIT: usize = 450;\nstatic NAME: &str = \"gate\";\n");
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].name.as_str(), found[0].kind, found[0].exported), ("LIMIT", "const", true));
        assert_eq!((found[0].value.as_str(), found[0].value_kind, found[0].line), ("450", "int", 1));
        assert_eq!((found[1].name.as_str(), found[1].kind, found[1].exported), ("NAME", "static", false));
        assert_eq!((found[1].ty.as_str(), found[1].source.as_str(), found[1].value.as_str()), ("&str", "\"gate\"", "gate"));
    }

    #[test]
    fn an_expression_keeps_no_value_and_names_what_it_reads_and_calls() {
        let found = consts("const MAX: usize = BASE * 2 + limits::extra(SHIFT).max(1) + concat!(\"a\").len();\n");
        assert_eq!(found[0].value, "");
        assert_eq!(found[0].source, "BASE * 2 + limits::extra(SHIFT).max(1) + concat!(\"a\").len()");
        assert_eq!(found[0].reads, ["BASE", "SHIFT"]);
        assert_eq!(found[0].calls, ["limits::extra", ".max", "concat!", ".len"]);
    }

    #[test]
    fn an_associated_constant_names_its_owner_and_a_local_or_a_test_fixture_is_not_a_constant() {
        let text = "struct Cfg;\nimpl Cfg { pub const SIZE: i32 = -3; }\ntrait Shape { const SIDES: u8; }\n\
                    fn f() { const LOCAL: u8 = 1; }\n#[cfg(test)]\nmod tests { const FIXTURE: u8 = 2; }\n";
        let found = consts(text);
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["SIZE", "SIDES"]);
        assert_eq!((found[0].owner.as_str(), found[0].value.as_str()), ("Cfg", "-3"));
        assert_eq!((found[1].owner.as_str(), found[1].source.as_str()), ("Shape", ""));
    }
}
