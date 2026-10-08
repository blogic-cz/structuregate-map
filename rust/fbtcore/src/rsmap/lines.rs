//! SOURCE LINES OF A RUST FILE, by the lexer: a line a TOKEN sits on.
//!
//! The same definition the C# counter gets from Roslyn, and for the same reason: a comment and a
//! blank line are trivia, so a file is not penalised for being documented. A scanner that looked for
//! `//` at the start of a line would count the inside of a raw string that happens to hold one, and
//! miss a code line that ends in a block comment's opener.
//!
//! DOC COMMENTS ARE COMMENTS HERE, although the lexer disagrees. `proc-macro2` hands `///` and `//!`
//! back as `#[doc = "..."]` attribute TOKENS, so counted naively every documented item would pay a
//! line per line of its documentation - the opposite of what the limit is for. The attribute is told
//! apart by what the SOURCE holds where its `#` token starts: a comment begins with `/`, an attribute
//! someone typed begins with `#`.

use proc_macro2::{Delimiter, LineColumn, Span, TokenStream, TokenTree};
use std::collections::HashSet;

/// Byte offset of every line start, so a span's (line, column) finds the text under it.
pub struct LineStarts<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineStarts<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        LineStarts { text, starts }
    }

    /// The byte offset of a span position. The column is in CHARACTERS, not bytes, so a line
    /// holding a non-ASCII identifier before the position is walked rather than indexed.
    pub fn offset(&self, at: LineColumn) -> usize {
        let Some(&start) = self.starts.get(at.line.saturating_sub(1)) else {
            return self.text.len();
        };
        self.text[start..]
            .char_indices()
            .nth(at.column)
            .map_or(self.text.len(), |(i, _)| start + i)
    }

    /// The text between two byte offsets, as `offset` returns them - empty when they are out of order.
    pub fn slice(&self, from: usize, to: usize) -> &'a str {
        self.text.get(from..to).unwrap_or("")
    }

    /// The 1-based line a byte offset sits on - the other direction, for a parser that answers in
    /// byte ranges (the markdown half) rather than in line/column spans.
    pub fn line_of(&self, at: usize) -> usize {
        self.starts.partition_point(|&start| start <= at)
    }

    /// Characters between two positions - what a duplicate is SIZED in, the unit every half shares.
    pub fn chars_between(&self, from: LineColumn, to: LineColumn) -> usize {
        let (a, b) = (self.offset(from), self.offset(to));
        if b <= a { 0 } else { self.text[a..b].chars().count() }
    }

    fn starts_comment(&self, span: Span) -> bool {
        let rest = &self.text[self.offset(span.start())..];
        rest.starts_with("//") || rest.starts_with("/*")
    }
}

/// A shebang is not a token and the lexer refuses it, so its line is blanked - blanked, not
/// removed, or every line number after it would be one out. `#![` is an inner attribute, not one.
pub fn without_shebang(text: &str) -> std::borrow::Cow<'_, str> {
    if text.starts_with("#!") && !text.starts_with("#![") {
        let end = text.find('\n').unwrap_or(text.len());
        return std::borrow::Cow::Owned(format!("{}{}", " ".repeat(end), &text[end..]));
    }
    std::borrow::Cow::Borrowed(text)
}

/// The count, or why the file could not be lexed - an unclosed delimiter, a stray quote. A file the
/// lexer refuses does not compile either, and the caller falls back to the plain `//` counter
/// rather than inventing a number here.
pub fn count(text: &str) -> Result<usize, String> {
    let text = without_shebang(text);
    let stream: TokenStream = text
        .parse()
        .map_err(|e: proc_macro2::LexError| format!("line {}: {e}", e.span().start().line))?;
    let starts = LineStarts::new(&text);
    let mut lines = HashSet::new();
    walk(stream, &starts, &mut lines);
    Ok(lines.len())
}

fn walk(stream: TokenStream, starts: &LineStarts<'_>, lines: &mut HashSet<usize>) {
    let mut tokens = stream.into_iter().peekable();
    while let Some(token) = tokens.next() {
        match &token {
            // `#`, an optional `!`, then `[doc = "..."]` - all three carry the comment's span.
            TokenTree::Punct(p) if p.as_char() == '#' && starts.starts_comment(p.span()) => {
                if matches!(tokens.peek(), Some(TokenTree::Punct(bang)) if bang.as_char() == '!') {
                    tokens.next();
                }
                if matches!(tokens.peek(), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket)
                {
                    tokens.next();
                }
            }
            // A DELIMITER IS A TOKEN, as `}` is to Roslyn: a line holding only the closing brace of a
            // block is a line of the block. `None` groups are invisible and have no text of their own.
            TokenTree::Group(group) => {
                if group.delimiter() != Delimiter::None {
                    mark(group.span_open(), lines);
                    mark(group.span_close(), lines);
                }
                walk(group.stream(), starts, lines);
            }
            other => mark(other.span(), lines),
        }
    }
}

/// Every line a token covers - a multi-line string literal is source on each of them.
fn mark(span: Span, lines: &mut HashSet<usize>) {
    for line in span.start().line..=span.end().line {
        lines.insert(line);
    }
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn comments_of_every_kind_and_blank_lines_are_not_source() {
        let text = "//! crate doc\n/// item doc\n/** block doc */\n// plain\n/* block\n   still */\n\nfn a() {}\n";
        assert_eq!(count(text), Ok(1));
    }

    #[test]
    fn an_attribute_someone_typed_is_source_and_a_doc_comment_is_not() {
        assert_eq!(count("#[derive(Debug)]\n/// doc\nstruct A;\n"), Ok(2));
        assert_eq!(count("#[doc = \"typed\"]\nstruct A;\n"), Ok(2));
    }

    #[test]
    fn a_closing_brace_alone_is_a_line_and_a_trailing_comment_counts_once() {
        assert_eq!(count("fn a() { // why\n    1\n}\n"), Ok(3));
    }

    #[test]
    fn a_multi_line_string_is_source_on_every_line_even_where_it_spells_a_comment() {
        assert_eq!(count("const A: &str = \"one\n// two\nthree\";\n"), Ok(3));
    }

    #[test]
    fn a_shebang_is_blanked_and_the_lines_after_it_keep_their_numbers() {
        assert_eq!(count("#!/usr/bin/env run-cargo-script\nfn main() {}\n"), Ok(1));
    }

    #[test]
    fn a_file_the_lexer_refuses_is_an_error_with_its_line_never_a_number() {
        assert!(count("fn a() {\n").unwrap_err().starts_with("line "));
    }
}
