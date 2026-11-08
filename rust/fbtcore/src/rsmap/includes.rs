//! WHAT A RUST FILE COMPILES IN BY PATH: `include_str!`, `include_bytes!` and `include!`, whose argument is a string
//! literal (relative to the file, as rustc reads it) or `concat!(env!("CARGO_MANIFEST_DIR"), "...")` (relative to the
//! file's package). Each is a SOFT edge to the file it names (`soft_uses_path`): the bytes are in the binary, so the
//! file is not dead - but it is data, not a module, and a path the tree does not map (a `.js` outside `--ext`) is
//! no BROKEN import.
//!
//! Read off the TOKEN TREE, every group descended into, so a call inside another macro's arguments (`vec![..]`, a
//! `const` array) is found too. An argument built from anything else - `env!("OUT_DIR")`, a `const` - names no file
//! here and is passed over.
//!
//! A `macro_rules!` WHOSE BODY INCLUDES A METAVARIABLE (`include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/",
//! $path))`) is a TEMPLATE: every call of that macro in the file includes each string literal of its arguments in
//! the metavariable's place. A literal that is no path (a tag beside the paths) names no file, and a soft edge to
//! nothing is dropped by the join.

use proc_macro2::{Delimiter, Group, Literal, TokenStream, TokenTree};
use std::path::Path;
use syn::punctuated::Punctuated;
use syn::{Expr, Lit, Token};

const INCLUDES: [&str; 3] = ["include_str", "include_bytes", "include"];
const MANIFEST_DIR: &str = "CARGO_MANIFEST_DIR";
/// What a metavariable is read as inside a template, until a call site's literal takes its place.
const HOLE: &str = "\u{0}";

/// What one walk of a file finds: include calls, include templates by macro name, and every macro call's literals.
#[derive(Default)]
struct Found {
    includes: Vec<Vec<Piece>>,
    templates: Vec<(String, Vec<Piece>)>,
    calls: Vec<(String, Vec<String>)>,
}

/// One argument piece: a literal, or the package folder `env!("CARGO_MANIFEST_DIR")` stands for.
#[derive(Clone)]
enum Piece {
    Text(String),
    Package,
}

/// The root-relative paths `source` includes. `rel` is the file's path from `root`; its package is looked up only
/// when an argument names `CARGO_MANIFEST_DIR`.
pub(crate) fn read(root: &Path, rel: &str, source: &str) -> Vec<String> {
    let Ok(tokens) = source.parse::<TokenStream>() else { return Vec::new() };
    let mut walked = Found::default();
    walk(tokens, None, &mut walked);
    let mut found = walked.includes;
    for (name, literals) in &walked.calls {
        for (_, template) in walked.templates.iter().filter(|(t, _)| t == name) {
            found.extend(literals.iter().map(|lit| fill(template, lit)));
        }
    }
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let mut package: Option<Option<String>> = None;
    let mut out: Vec<String> = Vec::new();
    for pieces in found {
        let base = match pieces.first() {
            Some(Piece::Package) => package.get_or_insert_with(|| super::cargo::package_of(root, rel, &mut Default::default())).clone(),
            Some(Piece::Text(first)) if first.starts_with('/') || first.contains(':') => None,
            Some(Piece::Text(_)) => Some(dir.to_string()),
            None => None,
        };
        let Some(base) = base else { continue };
        let mut text = String::new();
        let mut whole = true;
        for (i, piece) in pieces.iter().enumerate() {
            match piece {
                Piece::Text(t) => text.push_str(t),
                Piece::Package if i == 0 => {}
                Piece::Package => whole = false,
            }
        }
        if let Some(path) = whole.then(|| normal(&base, &text)).flatten()
            && !out.contains(&path)
        {
            out.push(path);
        }
    }
    out
}

/// A template's pieces with a call site's literal in the metavariable's place.
fn fill(template: &[Piece], literal: &str) -> Vec<Piece> {
    template
        .iter()
        .map(|p| match p {
            Piece::Text(t) => Piece::Text(t.replace(HOLE, literal)),
            Piece::Package => Piece::Package,
        })
        .collect()
}

/// Every include call in `tokens` as its argument's pieces, every include template, and every macro call's literals.
/// `template`: the `macro_rules!` whose body this is.
fn walk(tokens: TokenStream, template: Option<&str>, found: &mut Found) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;
    while i < trees.len() {
        let tree = &trees[i];
        i += 1;
        if let TokenTree::Group(group) = tree {
            walk(group.stream(), template, found);
            continue;
        }
        let TokenTree::Ident(ident) = tree else { continue };
        if !matches!(trees.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '!') {
            continue;
        }
        let name = ident.to_string();
        // `macro_rules! NAME { .. }`: its body is walked as that template's.
        if name == "macro_rules" {
            if let (Some(TokenTree::Ident(defined)), Some(TokenTree::Group(body))) = (trees.get(i + 1), trees.get(i + 2)) {
                walk(body.stream(), Some(&defined.to_string()), found);
                i += 3;
            }
            continue;
        }
        let Some(TokenTree::Group(args)) = trees.get(i + 1) else { continue };
        if args.delimiter() == Delimiter::None {
            continue;
        }
        i += 2;
        if !INCLUDES.contains(&name.as_str()) {
            found.calls.push((name, literals(args.stream())));
            walk(args.stream(), template, found);
            continue;
        }
        let (stream, holed) = holes(args.stream());
        let Some(pieces) = syn::parse2::<Expr>(stream).ok().and_then(|e| pieces(&e)) else { continue };
        match (holed, template) {
            (false, _) => found.includes.push(pieces),
            (true, Some(macro_name)) => found.templates.push((macro_name.to_string(), pieces)),
            (true, None) => {}
        }
    }
}

/// `tokens` with every `$name` read as the HOLE literal, and whether there was one.
fn holes(tokens: TokenStream) -> (TokenStream, bool) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::new();
    let mut holed = false;
    let mut i = 0;
    while i < trees.len() {
        match (&trees[i], trees.get(i + 1)) {
            (TokenTree::Punct(p), Some(TokenTree::Ident(_))) if p.as_char() == '$' => {
                out.push(TokenTree::Literal(Literal::string(HOLE)));
                holed = true;
                i += 2;
                continue;
            }
            (TokenTree::Group(g), _) => {
                let (inner, h) = holes(g.stream());
                holed |= h;
                let mut group = Group::new(g.delimiter(), inner);
                group.set_span(g.span());
                out.push(TokenTree::Group(group));
            }
            (other, _) => out.push(other.clone()),
        }
        i += 1;
    }
    (out.into_iter().collect(), holed)
}

/// Every string literal in `tokens`, at any depth.
fn literals(tokens: TokenStream) -> Vec<String> {
    let mut out = Vec::new();
    for tree in tokens {
        match tree {
            TokenTree::Group(g) => out.extend(literals(g.stream())),
            TokenTree::Literal(l) => {
                if let Lit::Str(s) = Lit::new(l) {
                    out.push(s.value());
                }
            }
            _ => {}
        }
    }
    out
}

/// An include's argument as pieces, or None when any part of it is not a literal or the package folder.
fn pieces(expr: &Expr) -> Option<Vec<Piece>> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(s) => Some(vec![Piece::Text(s.value())]),
            _ => None,
        },
        Expr::Macro(m) if m.mac.path.is_ident("concat") => {
            let parts = m.mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated).ok()?;
            let mut out = Vec::new();
            for part in &parts {
                out.extend(pieces(part)?);
            }
            Some(out)
        }
        Expr::Macro(m) if m.mac.path.is_ident("env") => {
            let name: syn::LitStr = m.mac.parse_body().ok()?;
            (name.value() == MANIFEST_DIR).then(|| vec![Piece::Package])
        }
        _ => None,
    }
}

/// `base` joined with `path`, `.` and `..` folded; None when it climbs above the root.
fn normal(base: &str, path: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for part in base.split('/').chain(path.split(['/', '\\'])) {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            other => out.push(other),
        }
    }
    Some(out.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("fbt-includes-{}", blake3::hash(format!("{files:?}").as_bytes()).to_hex()));
        for (file, text) in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }

    #[test]
    fn a_literal_is_relative_to_the_file_and_the_manifest_dir_to_the_package() {
        let root = tree(&[("crates/a/Cargo.toml", "[package]\nname = \"a\"\n")]);
        let source = r#"
            const PAGE: &str = include_str!("page.html");
            const LIBS: [&str; 2] = [
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/vendor/lib.min.js")),
                include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", "logo.png")),
            ];
        "#;
        let found = read(&root, "crates/a/src/view.rs", source);
        assert_eq!(found, ["crates/a/src/page.html", "web/vendor/lib.min.js", "crates/a/assets/logo.png"]);
    }

    #[test]
    fn a_macro_rules_template_includes_each_literal_its_calls_pass() {
        let root = tree(&[("crates/a/Cargo.toml", "[package]\nname = \"a\"\n")]);
        let source = r#"
            macro_rules! set { ($tag:literal: $($p:literal),+) => { &[$(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/", $p))),+] }; }
            pub const ONE: &[&[u8]] = set!("one": "Ts/A.mjs", "Ts/Sub/B.mjs");
        "#;
        // The tag is read as a path too; it names no file, and the join drops a soft edge to nothing.
        assert_eq!(read(&root, "crates/a/src/embedded.rs", source), ["src/one", "src/Ts/A.mjs", "src/Ts/Sub/B.mjs"]);
    }

    #[test]
    fn what_names_no_file_here_is_passed_over() {
        let source = r#"
            macro_rules! set { ($p:literal) => { include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/", $p)) }; }
            const A: &str = include_str!(concat!(env!("OUT_DIR"), "/gen.rs"));
            const B: &str = include_str!("/etc/abs.txt");
            const C: &str = include_str!("../../../above.txt");
            // include_str!("commented.txt")
        "#;
        assert!(read(Path::new("no-such-root"), "src/lib.rs", source).is_empty());
    }
}
