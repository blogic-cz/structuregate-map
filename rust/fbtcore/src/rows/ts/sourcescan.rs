//! Reading names out of source text, without a regular expression.
//!
//! Every one of these replaces a pattern the ported passes used, and the build refuses a
//! regular expression anywhere in this tool — so each is the pattern's behaviour written
//! out, including the parts an engine gives away for free.
//!
//! WHAT AN IDENTIFIER IS MADE OF, as byte sets and not as predicates. These were two
//! functions in an earlier port and the dead-key trace called them HUNDREDS OF MILLIONS of times
//! on a real workspace — split about evenly between the start and the continuation test — which
//! made the trace most of the whole closure.
//!
//! THE SETS ARE ASCII ON PURPOSE. The pattern was `[A-Za-z_$]` and `[\w$]`, and `\w` in a
//! unicode-aware engine is far wider than that. Spelling them out is what keeps a non-ASCII
//! identifier from being read as an English one.

use super::jsstr::trim;

fn is_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_cont(b: u8) -> bool {
    is_start(b) || b.is_ascii_digit()
}

/// Every DOTTED identifier a line spells: `[A-Za-z_$][\w$]*(?:\.[\w$]+)+`.
///
/// A match may begin part way through a run — `1abc.def` yields `abc.def`, because a
/// digit cannot start an identifier but the letters after it can.
pub fn dotted_names(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let n = bytes.len();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < n {
        if !is_start(bytes[i]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < n && is_cont(bytes[j]) {
            j += 1;
        }
        let mut end = j;
        let mut groups = 0usize;
        while end + 1 < n && bytes[end] == b'.' && is_cont(bytes[end + 1]) {
            end += 1;
            while end < n && is_cont(bytes[end]) {
                end += 1;
            }
            groups += 1;
        }
        if groups == 0 {
            // A FAILED ATTEMPT SKIPS THE WHOLE RUN, not one character. An engine advances
            // by one, and so did the first version of this — but every start position
            // inside one maximal run reaches the SAME end, so the dot test that just
            // failed fails identically from each of them. Jumping to the end is the same
            // answer for a fraction of the work, and a differential test over hundreds of real
            // files proved it.
            i = j;
            continue;
        }
        out.push(line[i..end].to_string());
        i = end;
    }
    out
}

/// Every identifier of THREE characters or more: `[A-Za-z_$][\w$]{2,}`.
pub fn words(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let n = bytes.len();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < n {
        if !is_start(bytes[i]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < n && is_cont(bytes[j]) {
            j += 1;
        }
        if j - i >= 3 {
            out.push(line[i..j].to_string());
        }
        // THE RUN IS SKIPPED EITHER WAY: a run of one or two characters cannot become
        // long enough by starting one character later.
        i = j;
    }
    out
}

/// Every name a `{{ ... }}` marker encloses: `\{\{\s*([\w$.]+)\s*\}\}`.
///
/// `{{routes.auth.login}}` is the supported convention's marker for "the string around me
/// holds a translation key", which is why the braces are READ and a bare substring is
/// not: a key that merely occurs inside another string proves nothing.
pub fn braced_names(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0usize;

    while let Some(offset) = text[at..].find("{{") {
        let start = at + offset;
        let Some(tail) = text[start + 2..].find("}}") else { break };
        let end = start + 2 + tail;

        let name = trim(&text[start + 2..end]);
        // The inner run must be entirely word, `$` or `.` characters — anything else and
        // the expression does not match, so neither does this.
        let ok = !name.is_empty()
            && name.bytes().all(|b| is_cont(b) || b == b'.');
        if !ok {
            // A FAILED ATTEMPT ADVANCES BY ONE, so `{{{name}}` still finds `{name}`'s
            // opening pair one character along.
            at = start + 1;
            continue;
        }
        out.push(name.to_string());
        at = end + 2;
    }
    out
}

/// The lines of a body, split the way `/\r?\n/` splits it.
pub fn lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

/// Every block a pair of markers encloses, replaced by one newline — `<!-- -->` and the
/// C-style pair.
///
/// AN UNCLOSED OPENER IS LEFT ALONE, which is what the non-greedy pattern this replaces
/// does: with no closing marker it matches nothing at all, and swallowing the rest of the
/// file instead would hide every reference below it and report live keys as dead.
pub fn strip_blocks(text: &str, open_marker: &str, close_marker: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    while let Some(rel) = text[at..].find(open_marker) {
        let start = at + rel;
        let after = start + open_marker.len();
        let Some(rel_end) = text[after..].find(close_marker) else { break };
        out.push_str(&text[at..start]);
        out.push('\n');
        at = after + rel_end + close_marker.len();
    }
    out.push_str(&text[at..]);
    out
}

/// The line with a `//` comment removed — and ONLY when the slashes do not follow a `:`.
///
/// `https://` inside a real string is not a comment, and the pattern this replaces says so
/// with `(^|[^:])`, keeping the character it looked at. It is not global: the FIRST
/// qualifying `//` ends the line, so everything from there is dropped.
pub fn strip_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while let Some(rel) = line[i..].find("//") {
        let at = i + rel;
        if at == 0 || bytes[at - 1] != b':' {
            return &line[..at];
        }
        // FROM THE NEXT CHARACTER, which is what advancing by one and rescanning comes to
        // and runs without a loop over hundreds of thousands of lines in this language.
        i = at + 1;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unclosed_opener_is_left_alone_and_does_not_swallow_the_file() {
        // Swallowing the rest would hide every reference below it and report live keys as
        // dead, which is the failure that matters.
        assert_eq!(strip_blocks("a <!-- b --> c", "<!--", "-->"), "a \n c");
        assert_eq!(strip_blocks("a <!-- b c", "<!--", "-->"), "a <!-- b c");
        assert_eq!(strip_blocks("/* x */y/* z */", "/*", "*/"), "\ny\n");
    }

    #[test]
    fn slashes_that_follow_a_colon_are_a_url_and_not_a_comment() {
        assert_eq!(strip_line_comment("a // b"), "a ");
        assert_eq!(strip_line_comment("url: 'https://x' // b"), "url: 'https://x' ");
        assert_eq!(strip_line_comment("// all of it"), "");
        assert_eq!(strip_line_comment("plain line"), "plain line");
        // The first QUALIFYING one ends the line; a colon-led one does not requalify it.
        assert_eq!(strip_line_comment("https://a // b // c"), "https://a ");
    }

    #[test]
    fn a_dotted_name_may_begin_part_way_through_a_run() {
        // A digit cannot start an identifier, but the letters after it can.
        assert_eq!(dotted_names("1abc.def"), vec!["abc.def"]);
        assert_eq!(dotted_names("this.that.other"), vec!["this.that.other"]);
        assert_eq!(dotted_names("$a.b_c"), vec!["$a.b_c"]);
    }

    #[test]
    fn an_identifier_with_no_dot_is_not_a_dotted_name() {
        assert!(dotted_names("plain").is_empty());
        // And a trailing dot does not make one: the pattern needs a run AFTER it.
        assert!(dotted_names("plain.").is_empty());
        assert!(dotted_names("plain. ").is_empty());
    }

    #[test]
    fn two_dotted_names_on_one_line_are_both_found() {
        assert_eq!(dotted_names("a.b + c.d"), vec!["a.b", "c.d"]);
    }

    #[test]
    fn a_word_needs_three_characters() {
        assert_eq!(words("ab abc abcd"), vec!["abc", "abcd"]);
        assert!(words("a b cd").is_empty());
    }

    #[test]
    fn the_identifier_sets_are_ascii_so_an_accented_word_is_not_one() {
        // `\w` in a unicode-aware engine would take the whole word; the pattern these
        // replace did not, and neither does this.
        assert_eq!(words("cafes"), vec!["cafes"]);
        // The accented letter breaks the run, and `s` is then too short to be
        // a word at all. Checked against the python pass rather than reasoned about.
        assert_eq!(words("cafés"), vec!["caf"]);
    }

    #[test]
    fn only_a_braced_marker_yields_a_key() {
        assert_eq!(braced_names("{{routes.auth.login}}"), vec!["routes.auth.login"]);
        assert_eq!(braced_names("go to {{a.b}} now"), vec!["a.b"]);
        // A bare occurrence proves nothing, which is the whole reason the braces are read.
        assert!(braced_names("routes.auth.login").is_empty());
    }

    #[test]
    fn whitespace_inside_the_marker_is_trimmed_the_javascript_way() {
        assert_eq!(braced_names("{{  a.b  }}"), vec!["a.b"]);
        assert_eq!(braced_names("{{\u{FEFF}a.b}}"), vec!["a.b"], "a byte order mark is whitespace here");
    }

    #[test]
    fn a_marker_holding_anything_but_a_name_does_not_match() {
        assert!(braced_names("{{ a + b }}").is_empty());
        assert!(braced_names("{{}}").is_empty());
        assert!(braced_names("{{ }}").is_empty());
    }

    #[test]
    fn a_failed_attempt_advances_by_one_rather_than_skipping_the_pair() {
        // The first `{{` opens a run that does not match; the scan must not skip past the
        // second `{{` that does.
        assert_eq!(braced_names("{{ a+b }} {{c.d}}"), vec!["c.d"]);
        assert_eq!(braced_names("{{{a.b}}"), vec!["a.b"]);
    }

    #[test]
    fn an_unclosed_marker_ends_the_scan_rather_than_looping() {
        assert!(braced_names("{{ a.b").is_empty());
        assert_eq!(braced_names("{{a.b}} {{ unclosed"), vec!["a.b"]);
    }

    #[test]
    fn lines_split_the_way_the_pattern_splits_them() {
        assert_eq!(lines("a\r\nb\nc"), vec!["a", "b", "c"]);
        assert_eq!(lines(""), vec![""]);
        assert_eq!(lines("a\n"), vec!["a", ""]);
    }
}
