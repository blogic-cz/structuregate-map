//! WHAT A RUST FILE COMPILES IN BY PATH: `include_str!`, `include_bytes!` and `include!`, whose argument is a string
//! literal (relative to the file, as rustc reads it) or `concat!(env!("CARGO_MANIFEST_DIR"), "...")` (relative to the
//! file's package). Each is a SOFT edge to the file it names (`soft_uses_path`): the bytes are in the binary, so the
//! file is not dead - but it is data, not a module, and a path the tree does not map (a `.js` outside `--ext`) is
//! no BROKEN import.
//!
//! Read off the TOKEN TREE, every group descended into, so a call inside another macro's arguments (`vec![..]`, a
//! `const` array) is found too. An argument built from anything else - a `$path` inside a `macro_rules!` body, a
//! `const` - names no file here and is passed over.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::path::Path;
use syn::punctuated::Punctuated;
use syn::{Expr, Lit, Token};

const INCLUDES: [&str; 3] = ["include_str", "include_bytes", "include"];
const MANIFEST_DIR: &str = "CARGO_MANIFEST_DIR";

/// One argument piece: a literal, or the package folder `env!("CARGO_MANIFEST_DIR")` stands for.
enum Piece {
    Text(String),
    Package,
}

/// The root-relative paths `source` includes. `rel` is the file's path from `root`; its package is looked up only
/// when an argument names `CARGO_MANIFEST_DIR`.
pub(crate) fn read(root: &Path, rel: &str, source: &str) -> Vec<String> {
    let Ok(tokens) = source.parse::<TokenStream>() else { return Vec::new() };
    let mut found = Vec::new();
    walk(tokens, &mut found);
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

/// Every include call in `tokens`, as its argument's pieces.
fn walk(tokens: TokenStream, found: &mut Vec<Vec<Piece>>) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    for (i, tree) in trees.iter().enumerate() {
        if let TokenTree::Group(group) = tree {
            walk(group.stream(), found);
            continue;
        }
        let TokenTree::Ident(ident) = tree else { continue };
        if !INCLUDES.contains(&ident.to_string().as_str()) {
            continue;
        }
        let (Some(TokenTree::Punct(bang)), Some(TokenTree::Group(args))) = (trees.get(i + 1), trees.get(i + 2)) else { continue };
        if bang.as_char() != '!' || args.delimiter() == Delimiter::None {
            continue;
        }
        if let Some(pieces) = syn::parse2::<Expr>(args.stream()).ok().and_then(|e| pieces(&e)) {
            found.push(pieces);
        }
    }
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
