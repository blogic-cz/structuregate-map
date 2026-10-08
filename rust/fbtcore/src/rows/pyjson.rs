//! `json.dumps` as python writes it, and text as python reads it.
//!
//! Both halves write into ONE database and consumers query the result, so a cell has
//! to hold the same bytes whichever end stored it. Two differences were found by
//! running the same payload through both ends and diffing the databases; each would
//! have been invisible until a query missed a row.

use serde_json::ser::Formatter;
use serde_json::Value;
use std::io;

/// Python's `json.dumps` puts a SPACE after every comma and colon — its default
/// separators are `(', ', ': ')` whenever `indent` is None. serde_json writes neither.
///
/// So a list cell stored by python reads `["x", "y"]` and the same cell stored here
/// read `["x","y"]`. Same value, different bytes, and a query with a LIKE over that
/// column matches one and not the other.
pub struct PythonSeparators;

impl Formatter for PythonSeparators {
    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        if first { Ok(()) } else { writer.write_all(b", ") }
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        if first { Ok(()) } else { writer.write_all(b", ") }
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(b": ")
    }
}

/// A list or dict cell, spelled the way `json.dumps(value, ensure_ascii=False)` spells
/// it. serde_json already leaves non-ASCII alone, which is what `ensure_ascii=False`
/// asks for.
pub fn dumps(value: &Value) -> String {
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, PythonSeparators);
    match serde::Serialize::serialize(value, &mut ser) {
        Ok(()) => String::from_utf8(out).unwrap_or_else(|_| value.to_string()),
        Err(_) => value.to_string(),
    }
}

/// Source text, read the way python's `open(path, "r")` reads it.
///
/// UNIVERSAL NEWLINES. Python translates `\r\n` and a lone `\r` to `\n` unless it is
/// told not to; this is the same trap the TypeScript half already carries a note about
/// ("python's `open()` TRANSLATES `\r\n` and node does not"), one language further on.
/// Without this, every `file_text` row of a CRLF repository differs between the two
/// ends, and a search for a two-line phrase finds one half's files and not the other's.
pub fn read_text(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    // Lossy, exactly as `errors="replace"`: a byte that is not UTF-8 must not cost the
    // whole file its searchable text.
    Some(universal_newlines(&String::from_utf8_lossy(&bytes)))
}

fn universal_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            // `\r\n` collapses to one `\n`; a lone `\r` becomes one too.
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_list_cell_is_spelled_the_way_python_spells_it() {
        assert_eq!(dumps(&json!(["x", "y"])), r#"["x", "y"]"#);
        assert_eq!(dumps(&json!({"a": 1, "b": 2})), r#"{"a": 1, "b": 2}"#);
        assert_eq!(dumps(&json!([])), "[]");
    }

    #[test]
    fn non_ascii_is_left_alone_as_ensure_ascii_false_asks() {
        assert_eq!(dumps(&json!(["é"])), r#"["é"]"#);
    }

    #[test]
    fn every_line_ending_reads_as_one_newline() {
        assert_eq!(universal_newlines("a\r\nb"), "a\nb");
        assert_eq!(universal_newlines("a\rb"), "a\nb");
        assert_eq!(universal_newlines("a\nb"), "a\nb");
        assert_eq!(universal_newlines("a\r\n\r\nb"), "a\n\nb");
    }
}
