//! HOW MANY LINES - for every file no host parser has to count. C# is counted by Roslyn and PowerShell and
//! TypeScript by their own hosts; everything else is counted here.
//!
//! SOURCE LINES are neither blank nor comment-only, with one counter per comment syntax, because a shared
//! approximation would penalise exactly the rationale comments these codebases depend on. Rust is counted
//! by `syn`'s lexer and Go by `gosyn`'s - a line a token sits on - and drops to the `//` scanner only for a file
//! that lexer refuses. A DOC counts its non-blank lines.
//!
//! A FILE OVER 4 MB IS STREAMED: it has failed every limit by a wide margin already, so the number only has
//! to be right enough to report, and it is read line by line as `File.ReadLines` splits them (`\r\n`,
//! `\n` or a lone `\r`) instead of parsed whole.


/// Above this a file is counted streamed - see the module note.
const STREAM_ABOVE_BYTES: u64 = 4 * 1024 * 1024;


pub(crate) fn count(path: &std::path::Path, doc: bool, streamed: bool) -> Result<usize, &'static str> {
    let bytes = std::fs::read(path).map_err(|e| exception(&e))?;
    let streamed = streamed || bytes.len() as u64 > STREAM_ABOVE_BYTES;
    let text = decode(&bytes);
    let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let lines: Vec<&str> = if streamed { read_lines(&text) } else { text.split('\n').collect() };
    if doc {
        return Ok(lines.iter().filter(|l| !l.trim().is_empty()).count());
    }
    Ok(match extension.as_str() {
        "py" => python(&lines),
        "rs" if !streamed => crate::rsmap::lines::count(text.trim_start_matches('\u{feff}')).unwrap_or_else(|_| c_style(&lines)),
        // Go BY ITS OWN LEXER too, as the map counts it (`gomap/lines.rs`): a raw string holding `/*` is code.
        "go" if !streamed => crate::gomap::lines::count(text.trim_start_matches('\u{feff}')).unwrap_or_else(|_| c_style(&lines)),
        _ => c_style(&lines),
    })
}

/// The text as .NET reads it: a UTF-8 or UTF-16 byte order mark decides the encoding, and anything else is
/// UTF-8 with an unreadable byte as a replacement character.
fn decode(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], big: bool| {
        let units: Vec<u16> =
            rest.chunks_exact(2).map(|p| if big { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) }).collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Lines as `File.ReadLines` splits them: at `\r\n`, `\n` or a lone `\r`, with no empty line after a final
/// terminator.
fn read_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\n' => {
                lines.push(&text[start..at]);
                start = at + 1;
            }
            b'\r' => {
                lines.push(&text[start..at]);
                if bytes.get(at + 1) == Some(&b'\n') {
                    at += 1;
                }
                start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    if start < bytes.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Python: non-blank, non-`#`-only lines. DOCSTRINGS COUNT - they are content, and a 300-line docstring is
/// still a file that is hard to navigate.
fn python(lines: &[&str]) -> usize {
    lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).count()
}

/// `//`-comment languages. A `/* ... */` block is tracked ACROSS lines and stripped even when it opens or
/// closes mid-line: testing each line alone counts the middle of every block comment as source, which
/// penalises a documented file for being documented. Code with a trailing comment counts once, as code.
fn c_style(lines: &[&str]) -> usize {
    let mut n = 0;
    let mut in_block = false;
    for raw in lines {
        let mut line = raw.trim().to_string();
        if in_block {
            let Some(end) = line.find("*/") else { continue };
            in_block = false;
            line = line[end + 2..].trim().to_string();
            if line.is_empty() {
                continue;
            }
        }
        while let Some(start) = line.find("/*") {
            match line[start + 2..].find("*/") {
                None => {
                    in_block = true;
                    line = line[..start].trim().to_string();
                    break;
                }
                Some(offset) => {
                    let end = start + 2 + offset;
                    line = format!("{} {}", &line[..start], &line[end + 2..]).trim().to_string();
                }
            }
        }
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        n += 1;
    }
    n
}

/// What the .NET exception for this error was called - the gate's "could not be read" note names it.
fn exception(e: &std::io::Error) -> &'static str {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "UnauthorizedAccessException",
        std::io::ErrorKind::NotFound => "FileNotFoundException",
        _ => "IOException",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_comment_is_stripped_across_lines_and_mid_line() {
        let lines = ["int a; /* one", "two */ int b;", "/* whole */", "// line", "", "c(); /* x */ d();"];
        assert_eq!(c_style(&lines), 3);
    }

    #[test]
    fn python_counts_docstrings_and_not_comments() {
        assert_eq!(python(&["\"\"\"doc", "more\"\"\"", "# note", "  ", "x = 1"]), 3);
    }

    #[test]
    fn streamed_lines_split_like_readlines() {
        assert_eq!(read_lines("a\r\nb\rc\n"), ["a", "b", "c"]);
        assert_eq!(read_lines("a\n\nb"), ["a", "", "b"]);
    }
}
