//! EVERY STRING AND NUMBER LITERAL OF A RUST FILE, for the deep map's `string_literals` and `number_literals`,
//! with what it is DOING where it is written - `use`, `callee`, `target` - in the vocabulary the python, C# and
//! TypeScript halves share, so one `--magic` query reads all of them (see `src/Map/Py/PyLiterals.py`).
//!
//! `syn` KEEPS NO PARENT, so the use is handed DOWN: a parent expression says what its next child is for
//! (`next`), and a literal takes it. Every other expression hands its children `other`.
//!
//! A MACRO'S ARGUMENTS ARE TOKENS to `syn`, and most of a rust file's strings are in one - `format!`,
//! `println!`, `panic!`, `vec![..]`. A macro whose body parses as comma-separated expressions is read as such:
//! a leading string is its `format`, the rest are its arguments (`callee` = `format!`). One that does not parse
//! (`vec![0; n]`, `matches!`) is skipped rather than guessed at. An ATTRIBUTE is never read: a doc comment and
//! `#[path = "x.rs"]` are not literals the code uses.

use super::lines::LineStarts;
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

pub fn read(file: &syn::File, starts: &LineStarts<'_>) -> Vec<Literal> {
    let mut walker = Walker { starts, next: Use::of("other"), negative: false, out: Vec::new(), func: Vec::new(), cls: Vec::new(), tests: 0 };
    walker.visit_file(file);
    walker.out
}

struct Walker<'s, 't> {
    starts: &'s LineStarts<'t>,
    next: Use,
    negative: bool,
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

    /// A macro whose body is comma-separated expressions: a leading string is its format, the rest arguments.
    fn mac(&mut self, mac: &syn::Macro) {
        let name = mac.path.segments.last().map(|s| format!("{}!", s.ident)).unwrap_or_default();
        let Ok(args) = mac.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated) else { return };
        for (i, arg) in args.iter().enumerate() {
            let format = i == 0 && matches!(arg, syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(_), .. })) && name != "vec!";
            let uses = if format { Use::of("format") } else { Use { uses: "argument", callee: name.clone(), target: String::new() } };
            self.with(uses, arg);
        }
    }
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
        self.func.push(f.sig.ident.to_string());
        visit::visit_impl_item_fn(self, f);
        self.func.pop();
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        self.with(Use::of("declared"), &c.expr);
    }

    fn visit_item_static(&mut self, s: &'ast syn::ItemStatic) {
        self.with(Use::of("declared"), &s.expr);
    }

    fn visit_impl_item_const(&mut self, c: &'ast syn::ImplItemConst) {
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
            syn::Pat::Lit(l) => self.lit(&l.lit, Use::of("compare")),
            syn::Pat::Range(r) => {
                for end in [&r.start, &r.end].into_iter().flatten() {
                    self.with(Use::of("compare"), end);
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
                self.with(Use::of(uses), &b.left);
                self.with(Use::of(uses), &b.right);
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

    fn visit_stmt_macro(&mut self, m: &'ast syn::StmtMacro) {
        self.mac(&m.mac);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literals(text: &str) -> Vec<Literal> {
        let file = syn::parse_file(text).unwrap();
        let found = read(&file, &LineStarts::new(text));
        super::super::forget_spans();
        found
    }

    fn row(l: &Literal) -> String {
        format!("{}:{}:{}:{}:{}", l.line, l.value, l.uses, l.callee, l.target)
    }

    #[test]
    fn every_literal_says_what_it_is_doing() {
        let text = "/// doc text\nconst LIMIT: usize = 450;\nfn go(line: &str, n: i32) -> i32 {\n    \
                    if n > 3 { return line.split(':').nth(2).map_or(-1, |s| s.len() as i32 * 60); }\n    \
                    let parts: Vec<&str> = line.split(\",\").collect();\n    let x = parts[1];\n    \
                    match n { 404 => 0, _ => n }\n}\n";
        let rows: Vec<String> = literals(text).iter().map(row).collect();
        assert_eq!(rows, [
            "2:450:declared::",
            "4:3:compare::",
            "4:::argument:split:line",
            "4:2:argument:nth:line.split(':')",
            "4:-1:argument:map_or:line.split(':').nth(2)",
            "4:60:arith::",
            "5:,:argument:split:line",
            "6:1:index::parts",
            "7:404:compare::",
            "7:0:other::",
        ]);
    }

    #[test]
    fn a_macro_s_format_string_and_arguments_are_read_and_test_code_is_marked() {
        let found = literals("fn a() { println!(\"{} of {}\", 5, \"x\"); }\n#[test]\nfn t() { assert_eq!(f(), 7); }\n");
        let rows: Vec<String> = found.iter().map(row).collect();
        assert_eq!(rows, ["1:{} of {}:format::", "1:5:argument:println!:", "1:x:argument:println!:", "3:7:argument:assert_eq!:"]);
        assert_eq!((found[0].func.as_str(), found[0].test, found[3].test), ("a", false, true));
        assert_eq!(found[1].number, Some(5.0));
    }
}
