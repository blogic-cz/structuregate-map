//! A GO FILE'S SOURCE LINES, by its own lexer: a line a token sits on, comments excluded - as the rust half counts by
//! `syn`'s and the gate counts C# by Roslyn's. A `//`-scanner would count the inside of a raw string holding `/*` as a
//! comment. The gate and the map both count through here, so they agree on what a line is.

use gosyn::LexicalToken;
use gosyn::token::Token;

/// Where each line starts, in the CHARACTER offsets `gosyn` reports positions in (Unicode scalar values, not bytes).
pub(crate) struct LineStarts(Vec<usize>);

impl LineStarts {
    pub(crate) fn new(text: &str) -> LineStarts {
        let mut starts = vec![0];
        for (at, ch) in text.chars().enumerate() {
            if ch == '\n' {
                starts.push(at + 1);
            }
        }
        LineStarts(starts)
    }

    /// The last line - an error past the final newline is still on a line the file has.
    pub(crate) fn last(&self) -> usize {
        self.0.len().saturating_sub(1).max(1)
    }

    /// The 1-based line a character offset is on.
    pub(crate) fn line(&self, offset: usize) -> usize {
        match self.0.binary_search(&offset) {
            Ok(at) => at + 1,
            Err(at) => at,
        }
    }
}

/// The source lines, or `Err` when the lexer refuses the file - the caller then counts it by the `//` scanner.
pub(crate) fn count(text: &str) -> Result<usize, String> {
    let tokens = gosyn::tokenize_source(text).map_err(|e| e.to_string())?;
    Ok(of(&tokens, &LineStarts::new(text)))
}

/// The lines the tokens sit on - a raw string over five lines is five.
pub(crate) fn of(tokens: &[LexicalToken], starts: &LineStarts) -> usize {
    let mut lines = std::collections::BTreeSet::new();
    for token in tokens.iter().filter(|t| !matches!(t.token, Token::Comment(_))) {
        let last = starts.line(token.end.saturating_sub(1).max(token.start));
        for line in starts.line(token.start)..=last {
            lines.insert(line);
        }
    }
    lines.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_blank_lines_are_not_source_and_a_raw_string_holding_a_comment_marker_is() {
        let text = "// header\npackage a\n\n/* block\n   comment */\nvar s = `one /* not\ntwo */`\n";
        assert_eq!(count(text).unwrap(), 3);
    }
}
