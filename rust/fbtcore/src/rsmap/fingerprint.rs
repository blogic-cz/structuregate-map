//! The digest of a function body, for the bodies two files could share.

use proc_macro2::{Delimiter, TokenStream, TokenTree};

/// THE SHAPE OF A BODY, with LOCAL NAMES BLANKED so one idiom pasted under two sets of variable
/// names still fingerprints alike. A name after `.` or `::` is a member or a path step and is KEPT:
/// `fs::read_to_string` is what makes that call that call. Comments never enter - they are not
/// tokens - so two bodies match when they DO the same thing however they are described.
pub(super) fn fingerprint(stream: TokenStream) -> String {
    let mut hasher = blake3::Hasher::new();
    shape(stream, &mut hasher, &mut false);
    hasher.finalize().to_hex()[..16].to_string()
}

fn shape(stream: TokenStream, hasher: &mut blake3::Hasher, keep_next: &mut bool) {
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
                hasher.update(lit.to_string().as_bytes());
            }
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                hasher.update(open.as_bytes());
                shape(group.stream(), hasher, &mut false);
                hasher.update(close.as_bytes());
            }
        }
    }
}
