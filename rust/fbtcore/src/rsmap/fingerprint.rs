//! The digests of a function body, of each of its statements, and of an expression - for what two files could share.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::collections::BTreeSet;

/// An expression with fewer tokens than this is shared vocabulary, not a copy (`x.get(y).unwrap_or(0)`): the TypeScript
/// half's `MIN_EXPRESSION_LEAVES`, counted the same way - in tokens, so a long name cannot make a trivial expression large.
pub(super) const MIN_EXPRESSION_LEAVES: usize = 12;

/// THE SHAPE OF A BODY, with LOCAL NAMES BLANKED so one idiom pasted under two sets of variable
/// names still fingerprints alike. A name after `.` or `::` is a member or a path step and is KEPT:
/// `fs::read_to_string` is what makes that call that call. Comments never enter - they are not
/// tokens - so two bodies match when they DO the same thing however they are described.
pub(super) fn fingerprint(stream: TokenStream) -> String {
    digest(stream, false, 16)
}

/// ONE DIGEST PER TOP-LEVEL STATEMENT of a body, names AND literals blanked, as a sorted set (python's `shingles`): the
/// whole-body digest sees a copy with one line added as a different function; the graph compares these sets and reports
/// the pair that shares most of them (`similar_bodies`). A literal is blanked to its kind - `retry(3)` and `retry(5)` are
/// one statement shape, `retry("x")` another.
pub(super) fn shingles(block: &syn::Block) -> Vec<String> {
    let set: BTreeSet<String> = block.stmts.iter().map(|s| digest(quote::ToTokens::to_token_stream(s), true, 8)).collect();
    set.into_iter().collect()
}

/// AN EXPRESSION COPIED AS A UNIT, the TypeScript half's kinds in rust: a call, a method call, a binary operation, a struct
/// or array literal, a closure, a macro (`format!`, `vec!`: rust's template literals). A block, an `if` or a `match` holds
/// statements - a body's business, never an expression's.
pub(super) fn copied_unit(e: &syn::Expr) -> bool {
    matches!(e, syn::Expr::Call(_) | syn::Expr::MethodCall(_) | syn::Expr::Binary(_) | syn::Expr::Struct(_)
        | syn::Expr::Array(_) | syn::Expr::Closure(_) | syn::Expr::Macro(_))
}

/// How many tokens a stream holds, a group's delimiters not counted.
pub(super) fn leaves(stream: TokenStream) -> usize {
    stream.into_iter().map(|t| match t {
        TokenTree::Group(g) => leaves(g.stream()),
        _ => 1,
    }).sum()
}

fn digest(stream: TokenStream, literals: bool, len: usize) -> String {
    let mut hasher = blake3::Hasher::new();
    shape(stream, &mut hasher, &mut false, literals);
    hasher.finalize().to_hex()[..len].to_string()
}

fn shape(stream: TokenStream, hasher: &mut blake3::Hasher, keep_next: &mut bool, literals: bool) {
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
                let text = lit.to_string();
                let blank = if text.starts_with(|c: char| c.is_ascii_digit()) { "N" } else { "S" };
                hasher.update(if literals { blank.as_bytes() } else { text.as_bytes() });
            }
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                hasher.update(open.as_bytes());
                shape(group.stream(), hasher, &mut false, literals);
                hasher.update(close.as_bytes());
            }
        }
    }
}
