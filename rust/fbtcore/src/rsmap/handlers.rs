//! WHERE A RUST FILE DEALS WITH AN ERROR, for the deep map's `handlers` table - in the columns the python and
//! C# halves write for an `except`/`catch` clause, plus `shape`, because rust has no `catch`: an error is a
//! value, and what is done with it is spelled one of a dozen ways.
//!
//! EVERY SHAPE IS A ROW, the propagating and the panicking ones too - a `?` and an `.unwrap()` are each a
//! decision about an error, and "where can this panic" is a question the map answers only if they are here:
//!
//!   match       an arm whose pattern is `Err(..)`               the arm is the handler
//!   if_let      `if let Err(e) = r { .. }`                      the block is the handler
//!   let_else    `let Ok(x) = r else { .. }`                     the `else` is the handler; the error is gone
//!   question    `r?`                                            handed up as it came - `reraises`
//!   unwrap      `.unwrap()` / `.unwrap_err()`                   `panics`
//!   expect      `.expect(..)` / `.expect_err(..)`               `panics`
//!   fallback    `.unwrap_or(..)`, `_or_default()`, `_or_else`, `.or(..)`, `.or_else(..)` - a value instead
//!   ok          `.ok()`                                         the error is DROPPED into an Option - `passes`
//!   map_err     `.map_err(|e| ..)`                              turned into another error
//!   discard     `let _ = call();`                               ignored - `passes`
//!   catch_unwind `std::panic::catch_unwind(..)`                 the one real catch, of a panic
//!
//! WHICH TYPE a method is called on is not known here - there is no type checker in this process - so an
//! `.unwrap()` on an Option is a row too. `test` marks what sits in a `#[cfg(test)]` module or a `#[test]`
//! function, where unwrapping is the idiom: a query about production code says `test = 0`.

use super::consts::{compact, path_text, Names};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

pub struct Handler {
    pub line: usize,
    pub end_line: usize,
    /// The line of what the error came from - the `match` scrutinee, the receiver, the `?`'s operand.
    pub try_line: usize,
    pub shape: &'static str,
    pub func: String,
    pub cls: String,
    /// `Err(Kind::NotFound)` catches `Kind::NotFound`; `Err(e)` and `Err(_)` catch anything - `bare`.
    pub types: Vec<String>,
    pub bare: bool,
    pub name: String,
    pub name_read: bool,
    /// The handler does nothing: `Err(_) => {}`, `let _ =`, `.ok()`.
    pub passes: bool,
    /// `return Err(..)`, `Err(..)?` and `bail!` in the handler - a NEW error raised.
    pub raises: usize,
    pub reraises: bool,
    pub panics: bool,
    pub calls: Vec<String>,
    pub test: bool,
}

pub fn read(file: &syn::File) -> Vec<Handler> {
    let mut walker = Walker { out: Vec::new(), func: Vec::new(), cls: Vec::new(), tests: 0 };
    walker.visit_file(file);
    walker.out
}

struct Walker {
    out: Vec<Handler>,
    func: Vec<String>,
    cls: Vec<String>,
    tests: usize,
}

/// What a handler's body does, read off it once.
struct Body {
    passes: bool,
    raises: usize,
    reraises: bool,
    panics: bool,
    name_read: bool,
    calls: Vec<String>,
    end_line: usize,
}

const PANICS: [&str; 6] = ["panic", "unreachable", "todo", "unimplemented", "assert", "assert_eq"];

/// A handler's body: an arm's or a closure's expression, or an `if let`'s block.
#[derive(Clone, Copy)]
enum Part<'a> {
    Expr(&'a syn::Expr),
    Block(&'a syn::Block),
}

impl Part<'_> {
    fn visit<'v>(self, visitor: &mut impl Visit<'v>)
    where
        Self: 'v,
    {
        match self {
            Part::Expr(e) => visitor.visit_expr(e),
            Part::Block(b) => visitor.visit_block(b),
        }
    }

    fn end_line(self) -> usize {
        match self {
            Part::Expr(e) => e.span().end().line,
            Part::Block(b) => b.brace_token.span.close().end().line,
        }
    }
}

fn body_of(part: Option<Part<'_>>, name: &str, fallback_line: usize) -> Body {
    let Some(part) = part else {
        return Body { passes: false, raises: 0, reraises: false, panics: false, name_read: false, calls: vec![], end_line: fallback_line };
    };
    let mut names = Names::default();
    part.visit(&mut names);
    let mut acts = Acts { name, raises: 0, reraises: false, panics: false };
    part.visit(&mut acts);
    Body {
        passes: empty(part),
        raises: acts.raises,
        reraises: acts.reraises,
        panics: acts.panics,
        name_read: !name.is_empty() && names.reads.iter().any(|r| r == name),
        // `Ok(..)`, `Err(..)` and `Some(..)` build a value; they are not something the handler calls.
        calls: names.calls.into_iter().filter(|c| !["Ok", "Err", "Some"].contains(&c.as_str())).collect(),
        end_line: part.end_line(),
    }
}

/// `{}` or `()` - a handler that does nothing.
fn empty(part: Part<'_>) -> bool {
    match part {
        Part::Block(b) => b.stmts.is_empty(),
        Part::Expr(syn::Expr::Block(b)) => b.block.stmts.is_empty(),
        Part::Expr(syn::Expr::Tuple(t)) => t.elems.is_empty(),
        Part::Expr(_) => false,
    }
}

/// What a handler raises, re-raises and panics with.
struct Acts<'n> {
    name: &'n str,
    raises: usize,
    reraises: bool,
    panics: bool,
}

impl Acts<'_> {
    /// `Err(x)` - and whether `x` is the error this handler bound, which makes it the same error going on.
    fn err(&mut self, expr: &syn::Expr) -> bool {
        let syn::Expr::Call(call) = expr else { return false };
        let syn::Expr::Path(p) = &*call.func else { return false };
        if p.path.segments.last().is_none_or(|s| s.ident != "Err") {
            return false;
        }
        let same = matches!(call.args.first(), Some(syn::Expr::Path(a)) if !self.name.is_empty() && a.path.is_ident(self.name));
        if same { self.reraises = true } else { self.raises += 1 }
        true
    }
}

impl<'ast> Visit<'ast> for Acts<'_> {
    fn visit_expr_return(&mut self, r: &'ast syn::ExprReturn) {
        if let Some(value) = &r.expr
            && self.err(value)
        {
            return;
        }
        visit::visit_expr_return(self, r);
    }

    fn visit_expr_try(&mut self, t: &'ast syn::ExprTry) {
        if !self.err(&t.expr) {
            visit::visit_expr_try(self, t);
        }
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let last = m.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
        if PANICS.contains(&last.as_str()) {
            self.panics = true;
        } else if last == "bail" {
            self.raises += 1;
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        if ["unwrap", "expect"].contains(&m.method.to_string().as_str()) {
            self.panics = true;
        }
        visit::visit_expr_method_call(self, m);
    }
}

/// `Err(inner)` - the inner pattern, when the pattern is one.
fn err_inner(pat: &syn::Pat) -> Option<&syn::Pat> {
    let syn::Pat::TupleStruct(ts) = pat else { return None };
    if ts.path.segments.last()?.ident != "Err" {
        return None;
    }
    ts.elems.first()
}

fn is_ok(pat: &syn::Pat) -> bool {
    matches!(pat, syn::Pat::TupleStruct(ts) if ts.path.segments.last().is_some_and(|s| s.ident == "Ok"))
}

/// What an `Err(..)` pattern binds and catches: `(name, types, bare)`.
fn caught(inner: Option<&syn::Pat>) -> (String, Vec<String>, bool) {
    match inner {
        None | Some(syn::Pat::Wild(_)) => (String::new(), vec![], true),
        Some(syn::Pat::Ident(i)) => (i.ident.to_string(), vec![], true),
        Some(syn::Pat::Path(p)) => (String::new(), vec![path_text(&p.path)], false),
        Some(syn::Pat::TupleStruct(t)) => (String::new(), vec![path_text(&t.path)], false),
        Some(syn::Pat::Struct(s)) => (String::new(), vec![path_text(&s.path)], false),
        Some(syn::Pat::Or(o)) => {
            let types: Vec<String> = o.cases.iter().flat_map(|c| caught(Some(c)).1).collect();
            let bare = types.is_empty();
            (String::new(), types, bare)
        }
        Some(other) => (String::new(), vec![compact(&quote::ToTokens::to_token_stream(other).to_string())], false),
    }
}

fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test")) || super::items::is_cfg_test(attrs)
}

impl Walker {
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, shape: &'static str, line: usize, try_line: usize, caught: (String, Vec<String>, bool), body: Body, passes: bool, panics: bool) {
        let (name, types, bare) = caught;
        self.out.push(Handler {
            line,
            end_line: body.end_line.max(line),
            try_line,
            shape,
            func: self.func.last().cloned().unwrap_or_default(),
            cls: self.cls.last().cloned().unwrap_or_default(),
            types,
            bare,
            name,
            name_read: body.name_read,
            passes: passes || body.passes,
            raises: body.raises,
            reraises: body.reraises || shape == "question",
            panics: panics || body.panics,
            calls: body.calls,
            test: self.tests > 0,
        });
    }

    /// A method whose closure argument is the handler: `.unwrap_or_else(|e| ..)`, `.map_err(|e| ..)`.
    fn closure(&mut self, shape: &'static str, m: &syn::ExprMethodCall) {
        let line = m.method.span().start().line;
        let try_line = m.receiver.span().end().line;
        let Some(syn::Expr::Closure(c)) = m.args.first() else {
            let body = body_of(m.args.first().map(Part::Expr), "", line);
            self.add(shape, line, try_line, (String::new(), vec![], true), body, false, false);
            return;
        };
        let name = match c.inputs.first() {
            Some(syn::Pat::Ident(i)) => i.ident.to_string(),
            _ => String::new(),
        };
        let body = body_of(Some(Part::Expr(&c.body)), &name, line);
        self.add(shape, line, try_line, (name, vec![], true), body, false, false);
    }
}

impl<'ast> Visit<'ast> for Walker {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        let test = usize::from(is_test(&m.attrs));
        self.tests += test;
        visit::visit_item_mod(self, m);
        self.tests -= test;
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        self.cls.push(compact(&quote::ToTokens::to_token_stream(&*i.self_ty).to_string()));
        visit::visit_item_impl(self, i);
        self.cls.pop();
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let test = usize::from(is_test(&f.attrs));
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

    fn visit_expr_match(&mut self, m: &'ast syn::ExprMatch) {
        let try_line = m.expr.span().start().line;
        for arm in &m.arms {
            if let Some(inner) = err_inner(&arm.pat) {
                let caught = caught(Some(inner));
                let line = arm.pat.span().start().line;
                let body = body_of(Some(Part::Expr(&arm.body)), &caught.0, line);
                self.add("match", line, try_line, caught, body, false, false);
            } else if let syn::Pat::TupleStruct(ts) = &arm.pat
                && ts.path.segments.last().is_some_and(|s| s.ident == "Err")
                && ts.elems.is_empty()
            {
                let line = arm.pat.span().start().line;
                let body = body_of(Some(Part::Expr(&arm.body)), "", line);
                self.add("match", line, try_line, caught(None), body, false, false);
            }
        }
        visit::visit_expr_match(self, m);
    }

    fn visit_expr_if(&mut self, i: &'ast syn::ExprIf) {
        if let syn::Expr::Let(l) = &*i.cond
            && let Some(inner) = err_inner(&l.pat)
        {
            let caught = caught(Some(inner));
            let line = i.if_token.span.start().line;
            let body = body_of(Some(Part::Block(&i.then_branch)), &caught.0, line);
            self.add("if_let", line, l.expr.span().start().line, caught, body, false, false);
        }
        visit::visit_expr_if(self, i);
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        if let Some(init) = &l.init {
            let line = l.let_token.span.start().line;
            let pat = match &l.pat {
                syn::Pat::Type(t) => &*t.pat,
                other => other,
            };
            if let Some((_, diverge)) = &init.diverge
                && is_ok(pat)
            {
                let body = body_of(Some(Part::Expr(diverge)), "", line);
                self.add("let_else", line, init.expr.span().start().line, caught(None), body, false, false);
            } else if matches!(pat, syn::Pat::Wild(_))
                && matches!(&*init.expr, syn::Expr::Call(_) | syn::Expr::MethodCall(_) | syn::Expr::Await(_) | syn::Expr::Macro(_))
            {
                let body = body_of(None, "", line);
                self.add("discard", line, init.expr.span().start().line, caught(None), body, true, false);
            }
        }
        visit::visit_local(self, l);
    }

    fn visit_expr_try(&mut self, t: &'ast syn::ExprTry) {
        let line = t.question_token.span.start().line;
        self.add("question", line, t.expr.span().start().line, caught(None), body_of(None, "", line), false, false);
        visit::visit_expr_try(self, t);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        let line = m.method.span().start().line;
        let try_line = m.receiver.span().end().line;
        match m.method.to_string().as_str() {
            "unwrap" | "unwrap_err" => self.add("unwrap", line, try_line, caught(None), body_of(None, "", line), false, true),
            "expect" | "expect_err" => self.add("expect", line, try_line, caught(None), body_of(None, "", line), false, true),
            "ok" if m.args.is_empty() => self.add("ok", line, try_line, caught(None), body_of(None, "", line), true, false),
            "unwrap_or" | "unwrap_or_default" | "or" => {
                let body = body_of(m.args.first().map(Part::Expr), "", line);
                self.add("fallback", line, try_line, caught(None), body, false, false);
            }
            "unwrap_or_else" | "or_else" => self.closure("fallback", m),
            "map_err" => self.closure("map_err", m),
            _ => {}
        }
        visit::visit_expr_method_call(self, m);
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func
            && p.path.segments.last().is_some_and(|s| s.ident == "catch_unwind")
        {
            let line = c.func.span().start().line;
            let body = match c.args.first() {
                Some(syn::Expr::Closure(closure)) => body_of(Some(Part::Expr(&closure.body)), "", line),
                other => body_of(other.map(Part::Expr), "", line),
            };
            self.add("catch_unwind", line, line, (String::new(), vec!["panic".into()], false), body, false, false);
        }
        visit::visit_expr_call(self, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handlers(text: &str) -> Vec<Handler> {
        let file = syn::parse_file(text).unwrap();
        let found = read(&file);
        super::super::forget_spans();
        found
    }

    fn shapes(found: &[Handler]) -> Vec<&str> {
        found.iter().map(|h| h.shape).collect()
    }

    #[test]
    fn a_match_arm_on_err_is_a_handler_with_what_it_binds_reads_and_raises() {
        let found = handlers("fn go() -> Result<(), E> {\n    match load() {\n        Ok(v) => use_it(v),\n        \
                              Err(e) => { log(&e); return Err(Wrap(e)); }\n    }\n}\n");
        assert_eq!(shapes(&found), ["match"]);
        let h = &found[0];
        assert_eq!((h.line, h.try_line, h.end_line, h.name.as_str(), h.bare), (4, 2, 4, "e", true));
        assert_eq!((h.name_read, h.raises, h.reraises, h.passes, h.func.as_str()), (true, 1, false, false, "go"));
        assert_eq!(h.calls, ["log", "Wrap"]);
    }

    #[test]
    fn a_swallowed_error_passes_and_a_returned_one_is_the_same_error_going_on() {
        let found = handlers("fn a() { match f() { Err(Kind::Gone) => {}, Err(e) => return Err(e), _ => () } }\n");
        assert_eq!((found[0].types.clone(), found[0].bare, found[0].passes), (vec!["Kind::Gone".to_string()], false, true));
        assert_eq!((found[1].reraises, found[1].raises), (true, 0));
    }

    #[test]
    fn every_other_shape_is_a_row_and_test_code_says_so() {
        let text = "impl Store { fn open(&self) -> Result<(), E> {\n    let Ok(f) = File::open(p) else { return Ok(()) };\n    \
                    if let Err(e) = f.sync() { warn(e); }\n    let n = read(f)?;\n    let _ = flush();\n    \
                    let v = n.parse::<u8>().unwrap_or_default();\n    let w = n.parse::<u8>().map_err(|e| Bad(e))?;\n    \
                    let x = cfg().ok();\n    let r = std::panic::catch_unwind(|| run());\n    Ok(())\n} }\n\
                    #[cfg(test)]\nmod tests { #[test] fn t() { go().unwrap(); go().expect(\"x\"); } }\n";
        let found = handlers(text);
        assert_eq!(shapes(&found), ["let_else", "if_let", "question", "discard", "fallback", "question", "map_err", "ok", "catch_unwind", "unwrap", "expect"]);
        assert_eq!((found[0].cls.as_str(), found[0].func.as_str(), found[0].test), ("Store", "open", false));
        assert_eq!((found[1].name.as_str(), found[1].name_read), ("e", true));
        assert!(found[2].reraises && found[3].passes && found[7].passes);
        assert_eq!(found[6].name, "e");
        assert!(found[9].panics && found[9].test && found[10].test);
    }
}
