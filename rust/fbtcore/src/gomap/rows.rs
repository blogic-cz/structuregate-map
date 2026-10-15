//! THE GO HALF'S DEEP ROWS, read off one file: its `functions`, `calls`, `consts` and `string_literals`
//! (`deep.rs` stores them). The same columns the plain TypeScript half gives `functions` and `calls`, and rust's
//! `rsmap` gives `string_literals`, so one query reads every language.
//!
//! WHAT IS READ WHERE. A declaration comes off the syntax tree - its name, parameters, receiver and body span. A
//! CALL, a string and a constant's VALUE come off the tokens, as `uses` does in `mod.rs`: `name(` after a dotted
//! chain is a call, whatever the tree makes of it, so a call written in a deferred literal or a generic is found
//! alike. A call counts only INSIDE a function body: `Load(name string)` in an interface is a signature, and the
//! tokens alone cannot tell it from a call at package level. A package-level `var x = f()` is therefore not a row.
//!
//! WHERE A CALL POINTS, as the file graph does (`mod.rs`): a plain `Name(...)` is a top-level func of its own FOLDER
//! (a Go package), `alias.Name(...)` one of the folder the import names through `go.mod`, a method
//! on a value, `x.M(`, is bound to the method `T.M` when the type of `x` is KNOWN (`types.rs`: the receiver, a parameter,
//! a local from a call whose func declares its result, a composite literal, a typed `var`), and `x.y.Z` or a value of an
//! unknown type needs the type checker and is left unbound (`target_path` and `target_name` are '').

use super::{lines, modfile, receiver, types};
use gosyn::ast::{Declaration, Expression};
use gosyn::token::{LitKind, Operator, Token};
use gosyn::LexicalToken;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// What the folders declare. `funcs`: `(folder, name)` -> the file of that folder declaring the top-level func `name`,
/// or the method `T.M` (a func name never holds a '.', so the keys do not meet). A name declared by several files
/// (build-constraint variants) answers with the first by path, so a rerun binds alike. `results`: `(folder, F)` -> the
/// type `T` of a top-level func whose single result is `T` or `*T`, a type of that same folder.
#[derive(Default)]
pub(crate) struct Index {
    pub funcs: BTreeMap<(String, String), String>,
    pub results: BTreeMap<(String, String), String>,
}

/// A constant's source text is cut here: a table literal is not a value worth a row of its own size.
const MAX_VALUE: usize = 200;

pub(crate) struct Function {
    pub line: usize,
    pub end_line: usize,
    pub name: String,
    pub qualname: String,
    pub kind: &'static str,
    pub args: Vec<String>,
    pub exported: bool,
    pub statements: usize,
}

pub(crate) struct Call {
    pub func: String,
    pub line: usize,
    pub callee: String,
    pub target_path: String,
    pub target_name: String,
    pub args: usize,
    pub source: String,
}

pub(crate) struct Const {
    pub line: usize,
    pub name: String,
    pub kind: &'static str,
    pub exported: bool,
    pub value: String,
}

pub(crate) struct Literal {
    pub line: usize,
    pub value: String,
    pub func: String,
    pub cls: String,
}

pub(crate) struct Rows {
    pub package: String,
    pub functions: Vec<Function>,
    pub calls: Vec<Call>,
    pub consts: Vec<Const>,
    pub literals: Vec<Literal>,
}

/// The folder a file is in, `/`-separated and relative to the root - the key `modfile::resolve` answers in.
pub(crate) fn folder_of(root: &Path, abs: &Path) -> String {
    let dir = abs.parent().unwrap_or(abs);
    dir.strip_prefix(root).unwrap_or(dir).to_string_lossy().replace('\\', "/").trim_matches('/').to_string()
}

/// The top-level funcs a file declares (`init` is never called by name) and its methods as `T.M`, for the index, and
/// the result type of each top-level func that declares a single one.
pub(crate) fn declared_funcs(text: &str) -> (Vec<String>, Vec<(String, String)>) {
    let Ok(file) = gosyn::parse_source(text.trim_start_matches('\u{feff}')) else { return (Vec::new(), Vec::new()) };
    let (mut funcs, mut results) = (Vec::new(), Vec::new());
    for d in &file.decl {
        let Declaration::Function(f) = d else { continue };
        if f.name.name == "init" || f.name.name == "_" {
            continue;
        }
        match &f.recv {
            None => {
                funcs.push(f.name.name.clone());
                if let Some(typ) = types::result_of(f) {
                    results.push((f.name.name.clone(), typ));
                }
            }
            Some(recv) => {
                if let Some(cls) = recv.list.first().and_then(|field| receiver(&field.typ)) {
                    funcs.push(format!("{cls}.{}", f.name.name));
                }
            }
        }
    }
    (funcs, results)
}

struct Span {
    open: usize,
    close: usize,
    qual: String,
    cls: String,
    typed: types::Typed,
}

pub(crate) fn rows_of(text: &str, root: &Path, abs: &Path, index: &Index) -> Result<Rows, (usize, String)> {
    let text = text.trim_start_matches('\u{feff}');
    let starts = lines::LineStarts::new(text);
    let lexed = gosyn::tokenize_source(text).map_err(|e| (super::line_of(&e), format!("does not lex as Go: {e}")))?;
    let file = gosyn::parse_source(text).map_err(|e| (super::line_of(&e).min(starts.last()), format!("does not parse as Go: {e}")))?;
    let tokens: Vec<LexicalToken> = lexed.into_iter().filter(|t| !matches!(t.token, Token::Comment(_))).collect();
    let chars: Vec<char> = text.chars().collect();
    let slice = |from: usize, to: usize| chars[from.min(chars.len())..to.min(chars.len())].iter().collect::<String>();

    // THE NAME EACH IMPORT IS USED BY, and the folder it names; an import outside the tree's modules is none.
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut import_at: HashSet<usize> = HashSet::new();
    for import in &file.imports {
        import_at.insert(import.path.pos);
        let path = import.path.value.trim_matches('"').trim_matches('`');
        let Some(folder) = modfile::resolve(root, abs, path) else { continue };
        match import.name.as_ref().map(|n| n.name.as_str()) {
            Some(".") | Some("_") => {}
            Some(alias) => {
                aliases.insert(alias.to_string(), folder);
            }
            None => {
                aliases.insert(modfile::default_name(path), folder);
            }
        }
    }

    let own = folder_of(root, abs);
    let mut functions = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut consts = Vec::new();
    for decl in &file.decl {
        match decl {
            Declaration::Function(func) => {
                let cls = func.recv.as_ref().and_then(|r| r.list.first()).and_then(|field| receiver(&field.typ)).unwrap_or_default();
                let name = func.name.name.clone();
                let qualname = if func.recv.is_some() && !cls.is_empty() { format!("{cls}.{name}") } else { name.clone() };
                let mut args = Vec::new();
                for field in &func.typ.params.list {
                    let dots = if matches!(field.typ, Expression::Ellipsis(_)) { "..." } else { "" };
                    if field.name.is_empty() {
                        args.push(format!("{dots}_"));
                    }
                    args.extend(field.name.iter().map(|id| format!("{dots}{}", id.name)));
                }
                let (end_line, statements) = match &func.body {
                    Some(body) => {
                        let typed = types::typed_names(func, &tokens, body.pos, &own, &aliases, index);
                        spans.push(Span { open: body.pos.0, close: body.pos.1, qual: qualname.clone(), cls: cls.clone(), typed });
                        (starts.line(body.pos.1), body.list.len())
                    }
                    None => (starts.line(func.name.pos), 0),
                };
                functions.push(Function {
                    line: starts.line(func.name.pos), end_line, qualname, args, statements,
                    kind: if func.recv.is_some() { "method" } else { "function" },
                    exported: name.chars().next().is_some_and(char::is_uppercase), name,
                });
            }
            Declaration::Const(group) => {
                for spec in &group.specs {
                    consts_of(&mut consts, "const", &spec.name, spec.values.len(), &tokens, &starts, &slice);
                }
            }
            Declaration::Variable(group) => {
                for spec in &group.specs {
                    consts_of(&mut consts, "var", &spec.name, spec.values.len(), &tokens, &starts, &slice);
                }
            }
            Declaration::Type(_) => {}
        }
    }
    let within = |at: usize| spans.iter().find(|s| s.open <= at && at <= s.close);

    let is_op = |at: usize, op: Operator| matches!(tokens.get(at).map(|t| &t.token), Some(Token::Operator(o)) if *o == op);
    let ident = |at: usize| match tokens.get(at).map(|t| &t.token) {
        Some(Token::Literal(LitKind::Ident, name)) => Some(name.as_str()),
        _ => None,
    };
    let mut calls = Vec::new();
    let mut literals = Vec::new();
    for at in 0..tokens.len() {
        if let Token::Literal(LitKind::String, raw) = &tokens[at].token {
            if import_at.contains(&tokens[at].start) {
                continue;
            }
            let value = raw.strip_prefix(['"', '`']).unwrap_or(raw);
            let value = value.strip_suffix(['"', '`']).unwrap_or(value);
            let (func, cls) = within(tokens[at].start).map(|s| (s.qual.clone(), s.cls.clone())).unwrap_or_default();
            literals.push(Literal { line: starts.line(tokens[at].start), value: value.to_string(), func, cls });
            continue;
        }
        if at == 0 || !is_op(at, Operator::ParenLeft) {
            continue;
        }
        let Some(name) = ident(at - 1) else { continue };
        let Some(span) = within(tokens[at - 1].start) else { continue };
        // THE DOTTED CHAIN BEHIND THE NAME: `x.y.Z(` is one callee. A chain that goes on past a `)` or a `]`
        // (`a.b().c(`) has a head that is no name, and is kept as `.c`.
        let mut parts = vec![name];
        let mut head = at - 1;
        while head >= 2 && is_op(head - 1, Operator::Dot) {
            let Some(up) = ident(head - 2) else { break };
            parts.push(up);
            head -= 2;
        }
        parts.reverse();
        let member = head >= 1 && is_op(head - 1, Operator::Dot);
        let (mut nesting, mut commas, mut close) = (0usize, 0usize, None);
        for k in at..tokens.len() {
            match &tokens[k].token {
                Token::Operator(Operator::ParenLeft | Operator::BarackLeft | Operator::BraceLeft) => nesting += 1,
                Token::Operator(Operator::ParenRight | Operator::BarackRight | Operator::BraceRight) => {
                    nesting = nesting.saturating_sub(1);
                    if nesting == 0 {
                        close = Some(k);
                        break;
                    }
                }
                Token::Operator(Operator::Comma) if nesting == 1 => commas += 1,
                _ => {}
            }
        }
        let Some(close) = close else { continue };
        let args = if close == at + 1 { 0 } else { commas + usize::from(!is_op(close - 1, Operator::Comma)) };
        // `x.M(` with a typed x is a METHOD `T.M` of the type's folder, and a typed name wins over an import of the same
        // name (a local shadows it); an interface has no method row, so it finds nothing.
        let mut target = name.to_string();
        let bound = if member {
            None
        } else if parts.len() == 1 {
            index.funcs.get(&(own.clone(), parts[0].to_string()))
        } else if parts.len() == 2 {
            if let Some((folder, typ)) = span.typed.get(parts[0]) {
                target = format!("{typ}.{name}");
                index.funcs.get(&(folder.clone(), target.clone()))
            } else {
                aliases.get(parts[0]).and_then(|folder| index.funcs.get(&(folder.clone(), parts[1].to_string())))
            }
        } else {
            None
        };
        calls.push(Call {
            func: span.qual.clone(),
            line: starts.line(tokens[head].start),
            callee: format!("{}{}", if member { "." } else { "" }, parts.join(".")),
            target_path: bound.cloned().unwrap_or_default(),
            target_name: if bound.is_some() { target } else { String::new() },
            args,
            source: slice(tokens[head].start, tokens[close].end),
        });
    }
    Ok(Rows { package: file.pkg_name.name.clone(), functions, calls, consts, literals })
}

/// One `const` or `var` spec's names, each with the text of the value written for it. The values are found on the
/// tokens after the first `=`: a comma at the top level ends one, and so does a line that ends the statement (not
/// one that stops on an operator). A spec that gives fewer values than names (`a, b = f()`, an `iota` line) has
/// none to say, and a source text is cut at `MAX_VALUE`.
fn consts_of(out: &mut Vec<Const>, kind: &'static str, names: &[gosyn::ast::Ident], values: usize, tokens: &[LexicalToken],
             starts: &lines::LineStarts, slice: &dyn Fn(usize, usize) -> String) {
    let mut texts: Vec<String> = Vec::new();
    if values > 0 {
        let last = names.last().and_then(|n| tokens.binary_search_by_key(&n.pos, |t| t.start).ok());
        let mut k = last.map_or(tokens.len(), |at| at + 1);
        while k < tokens.len() && !matches!(tokens[k].token, Token::Operator(Operator::Assign)) {
            k += 1;
        }
        k += 1;
        let (mut first, mut nesting) = (k, 0i32);
        let mut end = tokens.len();
        let mut segments: Vec<(usize, usize)> = Vec::new();
        while k < tokens.len() {
            match &tokens[k].token {
                Token::Operator(Operator::ParenLeft | Operator::BarackLeft | Operator::BraceLeft) => nesting += 1,
                Token::Operator(Operator::ParenRight | Operator::BarackRight | Operator::BraceRight) => {
                    if nesting == 0 {
                        end = k;
                        break;
                    }
                    nesting -= 1;
                }
                Token::Operator(Operator::SemiColon) if nesting == 0 => {
                    end = k;
                    break;
                }
                Token::Operator(Operator::Comma) if nesting == 0 => {
                    segments.push((first, k));
                    first = k + 1;
                    k += 1;
                    continue;
                }
                _ => {}
            }
            let ends_statement = !matches!(&tokens[k].token, Token::Operator(o)
                if !matches!(o, Operator::ParenRight | Operator::BarackRight | Operator::BraceRight | Operator::Inc | Operator::Dec));
            if nesting == 0 && ends_statement && k + 1 < tokens.len()
                && starts.line(tokens[k + 1].start) > starts.line(tokens[k].end.saturating_sub(1).max(tokens[k].start)) {
                end = k + 1;
                break;
            }
            k += 1;
        }
        segments.push((first, end));
        for (from, to) in segments {
            let (Some(a), Some(b)) = (tokens.get(from), tokens.get(to.saturating_sub(1))) else {
                texts.push(String::new());
                continue;
            };
            let text = if to > from { slice(a.start, b.end) } else { String::new() };
            texts.push(if text.chars().count() > MAX_VALUE { format!("{} …", text.chars().take(MAX_VALUE).collect::<String>()) } else { text });
        }
    }
    for (i, name) in names.iter().enumerate() {
        if name.name == "_" {
            continue;
        }
        let value = if texts.len() == names.len() { texts[i].clone() } else { String::new() };
        out.push(Const { line: starts.line(name.pos), name: name.name.clone(), kind, exported: name.name.chars().next().is_some_and(char::is_uppercase), value });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str, index: &Index) -> Rows {
        rows_of(text, Path::new("r"), Path::new("r/pkg/a.go"), index).ok().unwrap()
    }

    #[test]
    fn a_function_a_method_and_their_calls_are_rows_and_a_plain_name_binds_in_its_folder() {
        let mut index = Index::default();
        index.funcs.insert(("pkg".into(), "check".into()), "pkg/b.go".into());
        let found = rows("package pkg\n\ntype S struct{}\n\nfunc (s *S) Load(name string, rest ...int) error { return check(name) }\n\nfunc New() {}\n", &index);
        assert_eq!(found.package, "pkg");
        let load = &found.functions[0];
        assert_eq!((load.qualname.as_str(), load.kind, load.exported, load.statements), ("S.Load", "method", true, 1));
        assert_eq!(load.args, vec!["name", "...rest"]);
        let call = &found.calls[0];
        assert_eq!((call.func.as_str(), call.callee.as_str(), call.target_path.as_str(), call.target_name.as_str(), call.args), ("S.Load", "check", "pkg/b.go", "check", 1));
    }

    #[test]
    fn a_constant_has_its_value_and_a_string_its_function() {
        let found = rows("package pkg\n\nconst Limit = 3\n\nvar (\n\ta, b = 1, \"x\"\n\tsum = 1 +\n\t\t2\n)\n\nfunc f() string { return \"hi\" }\n", &Index::default());
        let by: Vec<(&str, &str)> = found.consts.iter().map(|c| (c.name.as_str(), c.value.as_str())).collect();
        assert_eq!(by, vec![("Limit", "3"), ("a", "1"), ("b", "\"x\""), ("sum", "1 +\n\t\t2")]);
        let seen: Vec<(&str, &str)> = found.literals.iter().map(|l| (l.value.as_str(), l.func.as_str())).collect();
        assert_eq!(seen, vec![("x", ""), ("hi", "f")]);
    }

    #[test]
    fn a_method_called_on_its_receiver_binds_to_the_method() {
        let mut index = Index::default();
        index.funcs.insert(("pkg".into(), "S.check".into()), "pkg/b.go".into());
        let found = rows("package pkg\n\ntype S struct{}\n\nfunc (s *S) Load() { s.check() }\n", &index);
        let call = &found.calls[0];
        assert_eq!((call.callee.as_str(), call.target_path.as_str(), call.target_name.as_str()), ("s.check", "pkg/b.go", "S.check"));
    }
}
