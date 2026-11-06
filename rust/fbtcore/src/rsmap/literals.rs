//! EVERY STRING AND NUMBER LITERAL OF A RUST FILE, for the deep map's `string_literals` and `number_literals`,
//! with what it is DOING where it is written - `use`, `callee`, `target` - in the vocabulary the python, C# and
//! TypeScript halves share, so one `--magic` query reads all of them (see `src/Map/Py/PyLiterals.py`).
//!
//! `syn` KEEPS NO PARENT, so the use is handed DOWN: a parent expression says what its next child is for
//! (`next`), and a literal takes it. Every other expression hands its children `other`.
//!
//! A MACRO'S ARGUMENTS ARE TOKENS to `syn`, and most of a rust file's strings are in one - `format!`,
//! `println!`, `panic!`, `vec![..]`. A macro whose body parses as comma-separated expressions is read as such:
//! a string at the macro's std FORMAT POSITION is its `format` (`write!` 1, `assert_eq!` 2, the others 0), the rest
//! are its arguments (`callee` = `format!`). One that does not parse that way is read by `body_pieces`: `json!`,
//! `vec![x; n]` and `matches!` by their grammar, any other by its literal tokens. An item-level macro
//! (`include!`) is read as one, and a `thread_local!` static as a constant. A constant's TYPE is walked too: an
//! array length is a number. An ATTRIBUTE is never read: a doc comment and `#[path = "x.rs"]` are not literals
//! the code uses.

use super::lines::LineStarts;
use proc_macro2::{Spacing, TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

pub struct Literal {
    pub line: usize,
    /// The string's value, or the number as written (`0x10`, `-1`, `2.5e3`).
    pub value: String,
    /// The number it is; None for a string.
    pub number: Option<f64>,
    pub uses: &'static str,
    pub callee: String,
    pub target: String,
    pub func: String,
    pub cls: String,
    pub test: bool,
}

#[derive(Clone)]
struct Use {
    uses: &'static str,
    callee: String,
    target: String,
}

impl Use {
    fn of(uses: &'static str) -> Self {
        Use { uses, callee: String::new(), target: String::new() }
    }
}

pub fn read(file: &syn::File, starts: &LineStarts<'_>, test: bool) -> Vec<Literal> {
    let mut walker = Walker { starts, next: Use::of("other"), negative: false, scrutinee: String::new(), out: Vec::new(), func: Vec::new(), cls: Vec::new(), tests: usize::from(test) };
    walker.visit_file(file);
    walker.out
}

struct Walker<'s, 't> {
    starts: &'s LineStarts<'t>,
    next: Use,
    negative: bool,
    /// The text a pattern is matched against - the `target` of a literal pattern's `compare`.
    scrutinee: String,
    out: Vec<Literal>,
    func: Vec<String>,
    cls: Vec<String>,
    tests: usize,
}

const COMPARES: [&str; 6] = ["==", "!=", "<", "<=", ">", ">="];

impl Walker<'_, '_> {
    fn text(&self, node: &impl Spanned) -> String {
        let span = node.span();
        self.starts.slice(self.starts.offset(span.start()), self.starts.offset(span.end())).trim().to_string()
    }

    fn with(&mut self, uses: Use, expr: &syn::Expr) {
        self.next = uses;
        self.visit_expr(expr);
    }

    fn lit(&mut self, lit: &syn::Lit, uses: Use) {
        let negative = std::mem::take(&mut self.negative);
        let (value, number) = match lit {
            syn::Lit::Str(s) if !s.value().trim().is_empty() => (s.value(), None),
            syn::Lit::Char(c) if !c.value().is_whitespace() => (c.value().to_string(), None),
            syn::Lit::Int(i) => (i.to_string(), i.base10_digits().parse::<f64>().ok()),
            syn::Lit::Float(f) => (f.to_string(), f.base10_digits().parse::<f64>().ok()),
            _ => return,
        };
        let sign = |s: String| if negative { format!("-{s}") } else { s };
        let number = number.map(|n| if negative { -n } else { n });
        let value = if number.is_some() { sign(value) } else { value };
        self.out.push(Literal {
            line: lit.span().start().line,
            value,
            number,
            uses: uses.uses,
            callee: uses.callee,
            target: uses.target,
            func: self.func.last().cloned().unwrap_or_default(),
            cls: self.cls.last().cloned().unwrap_or_default(),
            test: self.tests > 0,
        });
    }

    /// A macro whose body is comma-separated expressions: the string at its format position is the format, the
    /// rest arguments. Any other body is read by `body_pieces`.
    fn mac(&mut self, mac: &syn::Macro) {
        let name = mac.path.segments.last().map(|s| format!("{}!", s.ident)).unwrap_or_default();
        let argument = Use { uses: "argument", callee: name.clone(), target: String::new() };
        if let Ok(args) = mac.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated) {
            let at = format_position(&name);
            for (i, arg) in args.iter().enumerate() {
                let format = Some(i) == at && matches!(arg, syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(_), .. }));
                self.with(if format { Use::of("format") } else { argument.clone() }, arg);
            }
            return;
        }
        for piece in body_pieces(mac) {
            match piece {
                Piece::Expr(e) => self.with(argument.clone(), &e),
                Piece::Lit(l) => self.lit(&l, argument.clone()),
                Piece::Pat(p) => self.visit_pat(&p),
            }
        }
    }
}

/// WHERE std's macros put the format string: `write!(out, "..")` after the writer, `assert_eq!(a, b, "..")` after
/// both operands. `vec!` has none - its first string is an element.
fn format_position(name: &str) -> Option<usize> {
    match name {
        "write!" | "writeln!" | "assert!" | "debug_assert!" => Some(1),
        "assert_eq!" | "assert_ne!" | "debug_assert_eq!" | "debug_assert_ne!" => Some(2),
        "vec!" => None,
        _ => Some(0),
    }
}

/// WHAT A MACRO BODY HOLDS when it is not a plain comma list - owned, as syn parses it out of the tokens.
pub(super) enum Piece {
    Expr(syn::Expr),
    Lit(syn::Lit),
    Pat(syn::Pat),
}

/// A macro body as the pieces a reader walks: a comma list of expressions; `json!` by its object and array
/// grammar (a key is a literal, a value a nested object, array or expression); `vec![x; n]`; `matches!(e, pat if
/// guard)`; any other body by EVERY LITERAL TOKEN in it, at any depth. A body that parses no way is never lost
/// whole: its literals are still the file's.
pub(super) fn body_pieces(mac: &syn::Macro) -> Vec<Piece> {
    if let Ok(args) = mac.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated) {
        return args.into_iter().map(Piece::Expr).collect();
    }
    let last = mac.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
    let mut out = Vec::new();
    match last.as_str() {
        "json" => {
            json_value(mac.tokens.clone(), &mut out);
            return out;
        }
        "vec" => {
            let both = |input: syn::parse::ParseStream| -> syn::Result<(syn::Expr, syn::Expr)> {
                let value: syn::Expr = input.parse()?;
                input.parse::<syn::Token![;]>()?;
                Ok((value, input.parse()?))
            };
            if let Ok((value, count)) = mac.parse_body_with(both) {
                return vec![Piece::Expr(value), Piece::Expr(count)];
            }
        }
        "matches" => {
            let all = |input: syn::parse::ParseStream| -> syn::Result<Vec<Piece>> {
                let subject: syn::Expr = input.parse()?;
                input.parse::<syn::Token![,]>()?;
                let pat = syn::Pat::parse_multi_with_leading_vert(input)?;
                let mut pieces = vec![Piece::Expr(subject), Piece::Pat(pat)];
                if input.parse::<Option<syn::Token![if]>>()?.is_some() {
                    pieces.push(Piece::Expr(input.parse()?));
                }
                input.parse::<Option<syn::Token![,]>>()?;
                Ok(pieces)
            };
            if let Ok(pieces) = mac.parse_body_with(all) {
                return pieces;
            }
        }
        _ => {}
    }
    token_literals(mac.tokens.clone(), &mut out);
    out
}

fn token_literals(tokens: TokenStream, out: &mut Vec<Piece>) {
    for tree in tokens {
        match tree {
            TokenTree::Literal(l) => out.push(Piece::Lit(syn::Lit::new(l))),
            TokenTree::Group(g) => token_literals(g.stream(), out),
            _ => {}
        }
    }
}

/// A json value: a lone `{..}` is an object, a lone `[..]` an array, anything else an expression - or, when it
/// is not one (`$x`, `(key)`), its literal tokens.
fn json_value(tokens: TokenStream, out: &mut Vec<Piece>) {
    let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    if let [TokenTree::Group(g)] = trees.as_slice() {
        match g.delimiter() {
            proc_macro2::Delimiter::Brace => {
                for entry in split_top(g.stream(), &is_comma) {
                    let mut at = None;
                    let items: Vec<TokenTree> = entry.clone().into_iter().collect();
                    for (i, t) in items.iter().enumerate() {
                        if is_single_colon(&items, i, t) {
                            at = Some(i);
                            break;
                        }
                    }
                    let Some(at) = at else {
                        token_literals(entry, out);
                        continue;
                    };
                    let key: TokenStream = items[..at].iter().cloned().collect();
                    match &items[..at] {
                        [TokenTree::Literal(l)] => out.push(Piece::Lit(syn::Lit::new(l.clone()))),
                        _ => token_literals(key, out),
                    }
                    json_value(items[at + 1..].iter().cloned().collect(), out);
                }
                return;
            }
            proc_macro2::Delimiter::Bracket => {
                for element in split_top(g.stream(), &is_comma) {
                    json_value(element, out);
                }
                return;
            }
            _ => {}
        }
    }
    match syn::parse2::<syn::Expr>(tokens.clone()) {
        Ok(e) => out.push(Piece::Expr(e)),
        Err(_) => token_literals(tokens, out),
    }
}

fn is_comma(t: &TokenTree) -> bool {
    matches!(t, TokenTree::Punct(p) if p.as_char() == ',')
}

/// A `:` that is not half of `::` - the first of the pair is `Joint`, the second follows a `Joint` `:`.
fn is_single_colon(items: &[TokenTree], i: usize, t: &TokenTree) -> bool {
    let TokenTree::Punct(p) = t else { return false };
    if p.as_char() != ':' || p.spacing() == Spacing::Joint {
        return false;
    }
    !matches!(i.checked_sub(1).and_then(|j| items.get(j)), Some(TokenTree::Punct(q)) if q.as_char() == ':' && q.spacing() == Spacing::Joint)
}

/// The stream cut at every top-level token `cut` accepts (a group is one token); an empty tail is dropped.
fn split_top(tokens: TokenStream, cut: &dyn Fn(&TokenTree) -> bool) -> Vec<TokenStream> {
    let mut parts = vec![Vec::new()];
    for tree in tokens {
        if cut(&tree) {
            parts.push(Vec::new());
        } else {
            parts.last_mut().map(|p| p.push(tree));
        }
    }
    parts.into_iter().filter(|p| !p.is_empty()).map(|p| p.into_iter().collect()).collect()
}

fn test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test")) || super::items::is_cfg_test(attrs)
}

impl<'ast> Visit<'ast> for Walker<'_, '_> {
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        let test = usize::from(test_attr(&m.attrs));
        self.tests += test;
        visit::visit_item_mod(self, m);
        self.tests -= test;
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        self.cls.push(super::consts::compact(&quote::ToTokens::to_token_stream(&*i.self_ty).to_string()));
        visit::visit_item_impl(self, i);
        self.cls.pop();
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let test = usize::from(test_attr(&f.attrs));
        self.tests += test;
        self.func.push(f.sig.ident.to_string());
        visit::visit_item_fn(self, f);
        self.func.pop();
        self.tests -= test;
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        // A `#[test]` or `#[cfg(test)]` method counts as test code exactly as a free fn does.
        let test = usize::from(test_attr(&f.attrs));
        self.tests += test;
        self.func.push(f.sig.ident.to_string());
        visit::visit_impl_item_fn(self, f);
        self.func.pop();
        self.tests -= test;
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        self.visit_type(&c.ty);
        self.with(Use::of("declared"), &c.expr);
    }

    fn visit_item_static(&mut self, s: &'ast syn::ItemStatic) {
        self.visit_type(&s.ty);
        self.with(Use::of("declared"), &s.expr);
    }

    fn visit_impl_item_const(&mut self, c: &'ast syn::ImplItemConst) {
        self.visit_type(&c.ty);
        self.with(Use::of("declared"), &c.expr);
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        self.visit_pat(&l.pat);
        if let Some(init) = &l.init {
            self.with(Use::of("assign"), &init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.with(Use::of("other"), diverge);
            }
        }
    }

    fn visit_pat(&mut self, p: &'ast syn::Pat) {
        match p {
            // A literal PATTERN is a comparison: `match code { 404 => .. }`.
            syn::Pat::Lit(l) => self.lit(&l.lit, Use { uses: "compare", callee: String::new(), target: self.scrutinee.clone() }),
            syn::Pat::Range(r) => {
                for end in [&r.start, &r.end].into_iter().flatten() {
                    self.with(Use { uses: "compare", callee: String::new(), target: self.scrutinee.clone() }, end);
                }
            }
            _ => visit::visit_pat(self, p),
        }
    }

    fn visit_expr(&mut self, e: &'ast syn::Expr) {
        let given = std::mem::replace(&mut self.next, Use::of("other"));
        match e {
            syn::Expr::Lit(l) => self.lit(&l.lit, given),
            // A SIGN IS PART OF THE NUMBER; brackets, `&`, arrays and tuples hand their use down unchanged.
            syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Neg(_)) && matches!(&*u.expr, syn::Expr::Lit(_)) => {
                self.negative = true;
                self.with(given, &u.expr);
            }
            syn::Expr::Paren(p) => self.with(given, &p.expr),
            syn::Expr::Group(g) => self.with(given, &g.expr),
            syn::Expr::Reference(r) => self.with(given, &r.expr),
            syn::Expr::Cast(c) => self.with(given, &c.expr),
            // AN ELEMENT IS NOT THE ARGUMENT: the strings of `[a, "x"].join(",")` are not separators. Only a
            // constant's value is the collection's - `const SIZES: [u8; 2] = [1, 2]` declares both.
            syn::Expr::Array(a) => {
                let inner = if given.uses == "declared" { given } else { Use::of("other") };
                a.elems.iter().for_each(|x| self.with(inner.clone(), x));
            }
            syn::Expr::Tuple(t) => {
                let inner = if given.uses == "declared" { given } else { Use::of("other") };
                t.elems.iter().for_each(|x| self.with(inner.clone(), x));
            }
            syn::Expr::Binary(b) => {
                let op = quote::ToTokens::to_token_stream(&b.op).to_string();
                let uses = if COMPARES.contains(&op.as_str()) { "compare" } else { "arith" };
                // A comparison's literal is told the TEXT of the operand it is compared with.
                let side = |other: &syn::Expr| Use { uses, callee: String::new(), target: if uses == "compare" { self.text(other) } else { String::new() } };
                let (left, right) = (side(&b.right), side(&b.left));
                self.with(left, &b.left);
                self.with(right, &b.right);
            }
            // A `match` arm and an `if let` / `while let` pattern are compared with the scrutinee.
            syn::Expr::Match(m) => {
                self.visit_expr(&m.expr);
                let subject = self.text(&*m.expr);
                let kept = std::mem::replace(&mut self.scrutinee, subject);
                m.arms.iter().for_each(|arm| self.visit_arm(arm));
                self.scrutinee = kept;
            }
            syn::Expr::Let(l) => {
                self.visit_expr(&l.expr);
                let subject = self.text(&*l.expr);
                let kept = std::mem::replace(&mut self.scrutinee, subject);
                self.visit_pat(&l.pat);
                self.scrutinee = kept;
            }
            syn::Expr::Assign(a) => {
                self.visit_expr(&a.left);
                self.with(Use::of("assign"), &a.right);
            }
            syn::Expr::Index(i) => {
                self.visit_expr(&i.expr);
                let target = self.text(&*i.expr);
                self.with(Use { uses: "index", callee: String::new(), target }, &i.index);
            }
            syn::Expr::Call(c) => {
                self.visit_expr(&c.func);
                let callee = match &*c.func {
                    syn::Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
                    _ => String::new(),
                };
                c.args.iter().for_each(|a| self.with(Use { uses: "argument", callee: callee.clone(), target: String::new() }, a));
            }
            syn::Expr::MethodCall(m) => {
                self.visit_expr(&m.receiver);
                let target = self.text(&*m.receiver);
                let callee = m.method.to_string();
                m.args.iter().for_each(|a| self.with(Use { uses: "argument", callee: callee.clone(), target: target.clone() }, a));
            }
            syn::Expr::Return(r) => {
                if let Some(value) = &r.expr {
                    self.with(Use::of("return"), value);
                }
            }
            syn::Expr::Macro(m) => self.mac(&m.mac),
            _ => visit::visit_expr(self, e),
        }
    }

    fn visit_item_macro(&mut self, i: &'ast syn::ItemMacro) {
        if i.mac.path.is_ident("thread_local") {
            for t in super::consts::thread_locals(&i.mac) {
                self.visit_type(&t.ty);
                self.with(Use::of("declared"), &t.expr);
            }
        } else if !i.mac.path.is_ident("macro_rules") {
            self.mac(&i.mac);
        }
    }

    fn visit_stmt_macro(&mut self, m: &'ast syn::StmtMacro) {
        self.mac(&m.mac);
    }
}

#[cfg(test)]
#[path = "tests/literals_tests.rs"]
mod tests;
