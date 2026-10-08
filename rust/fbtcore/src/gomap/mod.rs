//! THE GO HALF - the gate's line count and the map's file-level graph, parsed by `gosyn` inside this process, as the
//! rust half is by `syn`: no `go` toolchain has to be installed wherever the gate runs.
//!
//! WHAT AN EDGE IS HERE. A Go name belongs to its PACKAGE, and a package is a folder: `Load` in one file is `Load`
//! declared by any file of the same folder, and `store.Load` is `Load` of the folder the `store` import names - the
//! nearest `go.mod` says which (`modfile.rs`). So a file DECLARES its top-level names, and USES its plain names (its own
//! folder's) and its `alias.Name`s (the imported folder's, written `dir::Name`); the join keys both by folder
//! (`graph/join.rs`). Two packages that both declare `New` never meet, which a bare name join could not promise.
//!
//! READ OFF THE TOKENS, as the C# half reads identifiers: a name after `.` is a member and is not a use - a method or a
//! field needs the type checker to know whose it is. What declares is read off the syntax tree.
//!
//! A FILE NOTHING NAMES IS STILL RUN when the tool chain runs it: `package main` with `func main`, a `_test.go` (`go
//! test`) are entries; a file with a `func init()` runs when its package is imported, so it is `registered`.

pub(crate) mod lines;
mod modfile;

use gosyn::ast::{Declaration, File};
use gosyn::token::{LitKind, Operator, Token};
use gosyn::LexicalToken;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// A body shorter than this is not a duplication worth reporting - the rust and C# halves' floor.
const MIN_BODY_STATEMENTS: usize = 3;
const MAX_SUMMARY: usize = 160;

pub(crate) fn map_file(root: &Path, rel: &str, abs: &Path) -> Value {
    match std::fs::read_to_string(abs) {
        Ok(text) => read(root, rel, abs, text.trim_start_matches('\u{feff}')),
        Err(e) => json!({ "rel": rel, "error": { "line": 1, "message": format!("could not be read ({e})") } }),
    }
}

fn read(root: &Path, rel: &str, abs: &Path, text: &str) -> Value {
    let starts = lines::LineStarts::new(text);
    let tokens = match gosyn::tokenize_source(text) {
        Ok(tokens) => tokens,
        Err(e) => return json!({ "rel": rel, "error": { "line": line_of(&e), "message": format!("does not lex as Go: {e}") } }),
    };
    let counted = lines::of(&tokens, &starts);
    let file = match gosyn::parse_source(text) {
        Ok(file) => file,
        Err(e) => return json!({ "rel": rel, "lines": counted,
                                "error": { "line": line_of(&e).min(starts.last()), "message": format!("does not parse as Go: {e}") } }),
    };

    // EVERY IMPORT THIS TREE HOLDS, by the name the file uses it under; `.` and `_` take the whole package.
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut whole: BTreeSet<String> = BTreeSet::new();
    for import in &file.imports {
        let path = import.path.value.trim_matches('"').trim_matches('`');
        let Some(folder) = modfile::resolve(root, abs, path) else { continue };
        match import.name.as_ref().map(|n| n.name.as_str()) {
            Some(".") | Some("_") => {
                whole.insert(folder);
            }
            Some(alias) => {
                aliases.insert(alias.to_string(), folder);
            }
            None => {
                aliases.insert(modfile::default_name(path), folder);
            }
        }
    }

    let (mut declares, init, main) = declared(&file);
    // A TEST FILE IS BUILT ONLY BY `go test`: what it declares is never a target of the package's own code, and joined
    // as one, `sort.go` read as importing `search_test.go`. It is an entry, so it needs no reader either.
    if rel.ends_with("_test.go") {
        declares.clear();
    }
    let entry = (file.pkg_name.name == "main" && main) || rel.ends_with("_test.go");
    json!({
        "rel": rel,
        "lines": counted,
        "summary": summary(&file),
        "declares": declares,
        "uses": uses(&tokens, &aliases),
        "uses_dirs": whole,
        "entry": entry,
        "registered": if init { vec!["init"] } else { Vec::new() },
        "generated": generated(&tokens),
        "bodies": bodies(&file, &tokens, &starts, rel),
    })
}

/// The top-level names a file declares, and whether it has an `init` and a `main` function. A METHOD declares its
/// RECEIVER'S TYPE, not its own name - it is reached through a value of that type, so the file holding `func (f *Float)
/// Text` is read wherever `Float` is: `math/big/ftoa.go` read as unread while every formatter called it.
fn declared(file: &File) -> (BTreeSet<String>, bool, bool) {
    let mut names = BTreeSet::new();
    let (mut init, mut main) = (false, false);
    let mut add = |name: &str| {
        if name != "_" {
            names.insert(name.to_string());
        }
    };
    for decl in &file.decl {
        match decl {
            Declaration::Function(func) if func.recv.is_none() => match func.name.name.as_str() {
                "init" => init = true,
                "main" => main = true,
                name => add(name),
            },
            Declaration::Function(func) => {
                if let Some(name) = func.recv.as_ref().and_then(|r| r.list.first()).and_then(|field| receiver(&field.typ)) {
                    add(&name);
                }
            }
            Declaration::Type(group) => group.specs.iter().for_each(|spec| add(&spec.name.name)),
            Declaration::Const(group) => group.specs.iter().flat_map(|spec| &spec.name).for_each(|id| add(&id.name)),
            Declaration::Variable(group) => group.specs.iter().flat_map(|spec| &spec.name).for_each(|id| add(&id.name)),
        }
    }
    (names, init, main)
}

/// The type a receiver names: `T`, `*T`, `T[K]`, `*T[K, V]`, `(T)`.
fn receiver(typ: &gosyn::ast::Expression) -> Option<String> {
    use gosyn::ast::Expression;
    match typ {
        Expression::Ident(id) => Some(id.name.clone()),
        Expression::Star(star) => receiver(&star.right),
        Expression::TypePointer(pointer) => receiver(&pointer.typ),
        Expression::Index(index) => receiver(&index.left),
        Expression::IndexList(list) => receiver(&list.left),
        Expression::Paren(paren) => receiver(&paren.expr),
        _ => None,
    }
}

/// Every name the file reads: a plain identifier is its own package's, `alias.Name` the imported folder's
/// (`dir::Name`), and a name after any other `.` is a member, which needs types to place.
fn uses(tokens: &[LexicalToken], aliases: &HashMap<String, String>) -> BTreeSet<String> {
    let ident = |at: usize| match tokens.get(at).map(|t| &t.token) {
        Some(Token::Literal(LitKind::Ident, name)) => Some(name.as_str()),
        _ => None,
    };
    let dot = |at: usize| matches!(tokens.get(at).map(|t| &t.token), Some(Token::Operator(Operator::Dot)));
    let mut found = BTreeSet::new();
    let mut at = 0;
    while at < tokens.len() {
        if let Some(name) = ident(at) {
            let member = at > 0 && dot(at - 1);
            match (aliases.get(name), dot(at + 1), ident(at + 2)) {
                (Some(folder), true, Some(selected)) if !member => {
                    found.insert(format!("{folder}::{selected}"));
                    at += 3;
                    continue;
                }
                _ if !member => {
                    found.insert(name.to_string());
                }
                _ => {}
            }
        }
        at += 1;
    }
    found
}

/// `// Code generated ... DO NOT EDIT.` before the package clause - Go's own marker for a file a tool wrote.
fn generated(tokens: &[LexicalToken]) -> bool {
    tokens.iter()
        .take_while(|t| matches!(t.token, Token::Comment(_)))
        .any(|t| match &t.token {
            Token::Comment(text) => text.starts_with("// Code generated") && text.trim_end().ends_with("DO NOT EDIT."),
            _ => false,
        })
}

/// The file's headline: the first line of its package doc comment.
fn summary(file: &File) -> String {
    for comment in &file.docs {
        for line in comment.text.lines() {
            let line = line.trim().trim_start_matches("//").trim_start_matches("/*").trim_end_matches("*/").trim();
            if line.is_empty() {
                continue;
            }
            return match line.char_indices().nth(MAX_SUMMARY) {
                Some((cut, _)) => format!("{} …", &line[..cut]),
                None => line.to_string(),
            };
        }
    }
    String::new()
}

/// Each function body worth comparing, as the shape of its tokens with LOCAL NAMES BLANKED, so one idiom pasted under
/// two sets of variable names fingerprints alike; a name after `.` is a member or a package step and is kept.
fn bodies(file: &File, tokens: &[LexicalToken], starts: &lines::LineStarts, rel: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for decl in &file.decl {
        let Declaration::Function(func) = decl else { continue };
        let Some(body) = &func.body else { continue };
        if body.list.len() < MIN_BODY_STATEMENTS {
            continue;
        }
        let (open, close) = body.pos;
        let mut hasher = blake3::Hasher::new();
        let mut keep = false;
        for token in tokens.iter().filter(|t| t.start >= open && t.end <= close + 1) {
            match &token.token {
                Token::Comment(_) => continue,
                Token::Literal(LitKind::Ident, name) => hasher.update(if keep { name.as_bytes() } else { b"_" }),
                Token::Literal(_, text) => hasher.update(text.as_bytes()),
                Token::Keyword(word) => hasher.update(format!("{word:?}").as_bytes()),
                Token::Operator(op) => hasher.update(format!("{op:?}").as_bytes()),
            };
            keep = matches!(token.token, Token::Operator(Operator::Dot));
            hasher.update(b" ");
        }
        let name = match &func.recv {
            Some(_) => format!("(method) {}", func.name.name),
            None => func.name.name.clone(),
        };
        out.push(json!({
            "digest": hasher.finalize().to_hex()[..16].to_string(),
            "where": format!("{rel}:{}:{name}", starts.line(func.name.pos)),
            "size": close.saturating_sub(open),
        }));
    }
    out
}

/// The line a `gosyn` error names, or 1. ITS LINES COUNT FROM 0 - an error on line 3 says 2 - so one is added; only
/// the missing package clause is reported from 1, and that one lands a line late.
fn line_of(error: &anyhow::Error) -> usize {
    match error.downcast_ref::<gosyn::Error>() {
        Some(gosyn::Error::UnexpectedToken { location, .. }) | Some(gosyn::Error::Else { location, .. }) => location.0 + 1,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(text: &str) -> Value {
        read(Path::new("no-such-root"), "pkg/a.go", Path::new("no-such-root/pkg/a.go"), text)
    }

    #[test]
    fn a_file_declares_its_top_level_names_and_a_method_its_receiver_type() {
        let found = row("package pkg\n\ntype Store struct{}\n\nfunc (s Store) Load() {}\n\nfunc New() Store { return Store{} }\n\nconst Limit, Floor = 1, 0\n");
        assert_eq!(found["declares"], json!(["Floor", "Limit", "New", "Store"]));
        let methods = row("package pkg\n\nfunc (f *Float) Text() string { return \"\" }\n\nfunc (m Map[K, V]) Len() int { return 0 }\n");
        assert_eq!(methods["declares"], json!(["Float", "Map"]));
    }

    #[test]
    fn a_member_is_not_a_use_and_a_plain_name_is() {
        let found = row("package pkg\n\nfunc A() { b := New(); b.Load(); _ = Limit }\n");
        let uses: Vec<&str> = found["uses"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(uses.contains(&"New") && uses.contains(&"Limit"));
        assert!(!uses.contains(&"Load"));
    }

    #[test]
    fn main_and_tests_are_entries_and_init_is_registered() {
        assert_eq!(row("package main\n\nfunc main() {}\n")["entry"], true);
        assert_eq!(row("package pkg\n\nfunc init() {}\n")["registered"], json!(["init"]));
        let test = read(Path::new("r"), "pkg/a_test.go", Path::new("r/pkg/a_test.go"), "package pkg\n\nfunc TestA() {}\n");
        assert_eq!(test["entry"], true);
    }

    #[test]
    fn a_syntax_error_is_reported_with_its_line_and_keeps_its_count() {
        let found = row("package pkg\n\nfunc A( {\n}\n");
        assert_eq!(found["error"]["line"], 3);
        assert_eq!(found["lines"], 3);
    }

    #[test]
    fn the_generated_marker_is_read_before_the_package_clause() {
        assert_eq!(row("// Code generated by stringer. DO NOT EDIT.\n\npackage pkg\n")["generated"], true);
        assert_eq!(row("// Package pkg does things.\npackage pkg\n")["generated"], false);
    }
}
