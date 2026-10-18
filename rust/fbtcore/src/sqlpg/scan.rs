//! A TEXT AS POSTGRESQL'S SCANNER READS IT: its tokens (`pg_query_scan`, a `ScanResult` message decoded here - two flat
//! messages of varints, so no protobuf runtime is linked for it), where each line starts, and how the source SPELLS a
//! name the parser folded to lower case.
//!
//! WHY THE SPELLING: PostgreSQL folds an unquoted `Orders` to `orders`. ScriptDom keeps `Orders`, and so did the producer
//! before this one; the fingerprint hashes names, so a query written in both dialects keeps one fingerprint only if a
//! name is what the source spells. A folded name is given back the token it was read from - found by the node's own
//! location, or for a name with none (an alias) where it must sit - and only when that token IS the name; a quoted
//! name is the parser's own, exactly.

use super::ffi::{self, Token, C_COMMENT, IDENT, SQL_COMMENT};

/// The decoded `ScanResult`: `tokens` (2) of `ScanToken { start = 1, end = 2, token = 4, keyword_kind = 5 }`.
pub fn decode(bytes: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    for (field, value) in fields(bytes) {
        if let (2, Field::Bytes(message)) = (field, value) {
            let mut token = Token { start: 0, end: 0, kind: 0, keyword: false };
            for (inner, value) in fields(message) {
                if let Field::Number(n) = value {
                    match inner {
                        1 => token.start = n as usize,
                        2 => token.end = n as usize,
                        4 => token.kind = n as i64,
                        5 => token.keyword = n != 0,
                        _ => {}
                    }
                }
            }
            tokens.push(token);
        }
    }
    tokens
}

enum Field<'a> {
    Number(u64),
    Bytes(&'a [u8]),
}

/// A message's fields in order. A wire type other than a varint or a length is skipped by its width.
fn fields(bytes: &[u8]) -> Vec<(u64, Field<'_>)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let Some(key) = varint(bytes, &mut at) else { break };
        match key & 7 {
            0 => match varint(bytes, &mut at) {
                Some(n) => out.push((key >> 3, Field::Number(n))),
                None => break,
            },
            2 => {
                let Some(len) = varint(bytes, &mut at) else { break };
                let end = at.saturating_add(len as usize).min(bytes.len());
                out.push((key >> 3, Field::Bytes(&bytes[at..end])));
                at = end;
            }
            1 => at += 8,
            5 => at += 4,
            _ => break,
        }
    }
    out
}

fn varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0;
    while let Some(&b) = bytes.get(*at) {
        *at += 1;
        if shift < 64 {
            value |= u64::from(b & 0x7f) << shift;
        }
        if b & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

/// A text the parser read: its tokens and its lines. `first_line` is the file line its first byte is on - 1 for a file,
/// the statement's line for a PL/pgSQL body's statement.
pub struct Source<'a> {
    pub text: &'a str,
    pub tokens: Vec<Token>,
    lines: Vec<usize>,
    first_line: i64,
    first_column: i64,
}

impl<'a> Source<'a> {
    pub fn new(text: &'a str, first_line: i64) -> Result<Source<'a>, ffi::Refused> {
        Self::within(text, first_line, 1)
    }

    /// A text that starts inside a file - a function's body - at that file line and column.
    pub fn within(text: &'a str, first_line: i64, first_column: i64) -> Result<Source<'a>, ffi::Refused> {
        let tokens = ffi::scan(text)?;
        let mut lines = vec![0];
        lines.extend(text.bytes().enumerate().filter(|&(_, b)| b == b'\n').map(|(i, _)| i + 1));
        Ok(Source { text, tokens, lines, first_line, first_column })
    }

    /// The source's spelling of `name` at token `i`: the token itself when it is that name unquoted (`Orders` for
    /// `orders`), the parser's own when it is quoted; none when the token is not the name.
    fn spelled(&self, i: usize, name: &str) -> Option<String> {
        let token = self.tokens.get(i)?;
        let word = self.word(token);
        if word.starts_with('"') || word.starts_with("U&") || word.starts_with("u&") {
            return Some(name.to_string());
        }
        ((token.kind == IDENT || token.keyword || word == "*") && word.to_ascii_lowercase() == name).then(|| word.to_string())
    }

    /// The next token from `i` that is no comment.
    fn next(&self, mut i: usize) -> usize {
        while self.tokens.get(i).is_some_and(Self::comment) {
            i += 1;
        }
        i
    }

    /// A dotted name whose first part starts at byte `at`, each part as the source spells it - and the token after it.
    pub fn parts_at(&self, at: usize, parts: &[String]) -> Option<(Vec<String>, usize)> {
        let mut i = self.token_at(at);
        if self.tokens.get(i)?.start != at {
            return None;
        }
        let mut out = Vec::with_capacity(parts.len());
        for (k, part) in parts.iter().enumerate() {
            if k > 0 {
                i = self.next(i);
                if self.word(self.tokens.get(i)?) != "." {
                    return None;
                }
                i += 1;
            }
            i = self.next(i);
            out.push(self.spelled(i, part)?);
            i += 1;
        }
        Some((out, self.next(i)))
    }

    /// The alias after a table name that ends before token `i`: `AS` skipped, then the alias, if it is `name`.
    pub fn alias_at(&self, i: usize, name: &str) -> Option<String> {
        let mut i = self.next(i);
        if self.tokens.get(i).is_some_and(|t| t.keyword && self.word(t).eq_ignore_ascii_case("as")) {
            i = self.next(i + 1);
        }
        self.spelled(i, name)
    }

    /// The last word of a select-list entry starting at byte `at` - where its alias is - if that word is `name`: the
    /// entry ends at the first `,` or `)` outside its own parentheses, or at a reserved keyword (`FROM`, `INTO`, ...).
    pub fn last_word(&self, at: usize, name: &str) -> Option<String> {
        let mut open_parens = 0i64;
        let mut last = None;
        let mut i = self.token_at(at);
        while let Some(token) = self.tokens.get(i) {
            let word = self.word(token);
            match word {
                "(" => open_parens += 1,
                ")" if open_parens == 0 => break,
                ")" => open_parens -= 1,
                "," | ";" if open_parens == 0 => break,
                _ if open_parens == 0 && last.is_some() && token.keyword && self.reserved(token) => break,
                _ => {}
            }
            if !Self::comment(token) {
                last = Some(i);
            }
            i += 1;
        }
        self.spelled(last?, name)
    }

    fn reserved(&self, token: &Token) -> bool {
        ["from", "into", "where", "group", "having", "order", "limit", "offset", "window", "union", "except", "intersect", "returning", "on"]
            .iter()
            .any(|k| self.word(token).eq_ignore_ascii_case(k))
    }

    /// The first unquoted spelling of `name` in `[from, to)` - for a name the parser gives no place of its own.
    pub fn first_in(&self, from: usize, to: usize, name: &str) -> Option<String> {
        (self.token_at(from)..self.tokens.len()).take_while(|&i| self.tokens[i].start < to).find_map(|i| {
            let word = self.word(&self.tokens[i]);
            (!word.starts_with('"')).then(|| self.spelled(i, name)).flatten()
        })
    }

    /// The file line of a byte offset.
    pub fn line(&self, at: usize) -> i64 {
        self.first_line + self.lines.partition_point(|&start| start <= at) as i64 - 1
    }

    /// The 1-based character column of a byte offset.
    pub fn column(&self, at: usize) -> i64 {
        let line = self.lines.partition_point(|&s| s <= at).saturating_sub(1);
        let shift = if line == 0 { self.first_column - 1 } else { 0 };
        shift + self.text.get(self.lines[line]..at).map_or(1, |s| s.chars().count() as i64 + 1)
    }

    /// The text of a token.
    pub fn word(&self, token: &Token) -> &'a str {
        self.text.get(token.start..token.end).unwrap_or("")
    }

    /// The index of the token starting at `at`, or of the first one after it.
    pub fn token_at(&self, at: usize) -> usize {
        self.tokens.partition_point(|t| t.start < at)
    }

    /// Whether a token is a comment - no part of a statement's own text.
    pub fn comment(token: &Token) -> bool {
        token.kind == SQL_COMMENT || token.kind == C_COMMENT
    }

    /// The first and last non-comment, non-`;` tokens inside `[from, to)` - a statement's own extent.
    pub fn extent(&self, from: usize, to: usize) -> Option<(usize, usize)> {
        let inside = |t: &&Token| t.start >= from && t.end <= to && !Self::comment(t) && self.word(t) != ";";
        let first = self.tokens[self.token_at(from)..].iter().take_while(|t| t.start < to).find(inside)?;
        let last = self.tokens[self.token_at(from)..].iter().take_while(|t| t.start < to).filter(inside).last()?;
        Some((first.start, last.end))
    }

    /// Where a statement of a `BEGIN ATOMIC` body (or a `RETURN`) holding byte `at` starts: the first token after the
    /// last `;` or `ATOMIC` before it, or its `RETURN`.
    pub fn atomic_start(&self, from: usize, at: usize) -> usize {
        let mut start = from;
        let mut take = false;
        for token in self.tokens[self.token_at(from)..].iter().take_while(|t| t.start < at) {
            let word = self.word(token);
            if Self::comment(token) {
                continue;
            }
            if take || (token.keyword && word.eq_ignore_ascii_case("return")) {
                start = token.start;
                take = false;
            }
            if word == ";" || (token.keyword && word.eq_ignore_ascii_case("atomic")) {
                take = true;
            }
        }
        if take { at } else { start }
    }

    /// Where the statement starting at `from` ends: the start of the first `;` outside its parentheses, or `to`.
    pub fn statement_end(&self, from: usize, to: usize) -> usize {
        let mut open_parens = 0i64;
        for token in self.tokens[self.token_at(from)..].iter().take_while(|t| t.start < to) {
            match self.word(token) {
                "(" => open_parens += 1,
                ")" => open_parens -= 1,
                ";" if open_parens <= 0 => return token.start,
                _ => {}
            }
        }
        to
    }

    /// The 1-based character column where `word` first appears on file line `line`.
    pub fn find_on_line(&self, line: i64, word: &str) -> Option<i64> {
        let index = usize::try_from(line - self.first_line).ok()?;
        let start = *self.lines.get(index)?;
        let end = self.lines.get(index + 1).copied().unwrap_or(self.text.len());
        let found = self.text.get(start..end)?.find(word).filter(|_| !word.is_empty())?;
        Some(self.column(start + found))
    }

    /// The source from byte `from` to byte `to`.
    pub fn range(&self, from: usize, to: usize) -> String {
        self.text.get(from..to.max(from)).unwrap_or("").to_string()
    }
}
