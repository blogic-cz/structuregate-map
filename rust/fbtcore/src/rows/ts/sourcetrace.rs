//! WHAT THE RAW SOURCE STILL SHOWS OF A KEY NOTHING REFERENCES — the only honest way to
//! separate "dead" from "assembled at runtime".
//!
//! THE TRACE WAS WRONG TWICE BEFORE IT WAS RIGHT, and both corrections are the reason it is
//! trustworthy now:
//!   - the LOCALE FILES were in the scan. They define every key, so every one read as
//!     referenced.
//!   - COMMENT BLOCKS were in the scan. The first residue it reported as an extractor gap
//!     was a carrier binding inside an `<!-- ... -->` opened several lines earlier, which the
//!     template parser drops and the map was right to omit. A line test was not enough;
//!     whole comment blocks are stripped first, and a `//` line only when the slashes do
//!     not follow a `:`, because CALLING A LIVE KEY DEAD is the failure that matters.
//!
//! `unparsed_file` is split OFF from `full_literal`, because nearly all the survivors were in
//! files the extraction never parsed — out of the map's declared scope, not missed by it.
//! The file id rides along with every trace, so the next claim can be checked instead of
//! believed.
//!
//! Every scan is a character walk, never a pattern: see `sourcescan`.

use super::jsstr::trim_start;
use super::sourcescan::{dotted_names, lines, strip_blocks, strip_line_comment, words};
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

/// How a key still appears in source. `Absent` is the only one that means dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// The whole dotted key is spelled out, in a file the extraction parsed.
    FullLiteral,
    /// Spelled out in a file the extraction never parsed — outside scope, not missed.
    UnparsedFile,
    /// Everything but the last segment is spelled out, so the leaf is appended at runtime.
    Prefix,
    /// Only the last segment appears anywhere.
    Leaf,
    Absent,
}

impl How {
    pub fn name(self) -> &'static str {
        match self {
            How::FullLiteral => "full_literal",
            How::UnparsedFile => "unparsed_file",
            How::Prefix => "prefix",
            How::Leaf => "leaf",
            How::Absent => "absent",
        }
    }
}

/// What the scan found, and the one question it answers.
pub struct Trace {
    /// Dotted name -> the FIRST file it was seen in.
    dotted: IndexMap<String, String>,
    seen: IndexSet<String>,
    parsed: IndexSet<String>,
    pub scanned: usize,
    pub comment_lines: usize,
    pub locales: usize,
}

impl Trace {
    pub fn of(&self, key: &str) -> (How, Option<String>) {
        // A hit in a file the extraction never PARSED is not a gap in the map — it is
        // outside what the map claims to cover. Saying so is the difference between "the
        // extractor missed this" and "nothing here ever looked".
        if let Some(file) = self.dotted.get(key) {
            let how = if self.parsed.contains(file) { How::FullLiteral } else { How::UnparsedFile };
            return (how, Some(file.clone()));
        }
        let (prefix, leaf) = match key.rfind('.') {
            Some(i) => (&key[..i], &key[i + 1..]),
            None => ("", key),
        };
        if let Some(file) = self.dotted.get(prefix) {
            return (How::Prefix, Some(file.clone()));
        }
        if self.seen.contains(leaf) {
            return (How::Leaf, None);
        }
        (How::Absent, None)
    }
}

fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Null) | None => None,
        Some(other) => Some(other.to_string()),
    }
}

fn is_parsed(row: &Row) -> bool {
    matches!(row.get("parsed"), Some(Value::Bool(true)))
        || row.get("parsed").and_then(|v| v.as_i64()) == Some(1)
}

/// `texts` is `path -> content`, read once by the caller.
pub fn source_trace(store: &Store<'_>, texts: &IndexMap<String, String>) -> Trace {
    let mut parsed: IndexSet<String> = IndexSet::new();
    let mut id_by_path: IndexMap<String, String> = IndexMap::new();
    for f in store.table("files").iter() {
        if let (Some(path), Some(id)) = (text(f, "path"), text(f, "id")) {
            id_by_path.insert(path, id.clone());
            if is_parsed(f) {
                parsed.insert(id);
            }
        } else if is_parsed(f)
            && let Some(id) = text(f, "id")
        {
            // A row with an id but no path is still a parsed file, and the trace reads
            // `parsed` by id alone.
            parsed.insert(id);
        }
    }

    // THE LOCALE FILES ARE OUT. They define every key, so with them in the scan every
    // key read as referenced and the trace answered nothing.
    let mut locales: IndexSet<String> = IndexSet::new();
    for t in store.table("translations").iter() {
        if let Some(file) = text(t, "file") {
            locales.insert(file);
        }
    }

    let mut dotted: IndexMap<String, String> = IndexMap::new();
    let mut seen: IndexSet<String> = IndexSet::new();
    let mut scanned = 0usize;
    let mut comment_lines = 0usize;
    for (path, content) in texts {
        let Some(file_id) = id_by_path.get(path) else { continue };
        if locales.contains(file_id) {
            continue;
        }
        scanned += 1;
        // WHOLE BLOCKS FIRST, both pairs, because a binding nine lines inside an open
        // `<!--` is not live and a line test cannot see that.
        let body = strip_blocks(&strip_blocks(content, "<!--", "-->"), "/*", "*/");
        for raw in lines(&body) {
            let t = trim_start(raw);
            if t.starts_with("//") || t.starts_with('*') {
                comment_lines += 1;
                continue;
            }
            let line = strip_line_comment(raw);
            for name in dotted_names(line) {
                // THE FIRST FILE WINS, so the id a trace reports is stable across runs.
                dotted.entry(name).or_insert_with(|| file_id.clone());
            }
            for w in words(line) {
                seen.insert(w);
            }
        }
    }
    Trace { dotted, seen, parsed, scanned, comment_lines, locales: locales.len() }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store_of(files: Value, translations: Value) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert("files".to_string(), files);
        m.insert("translations".to_string(), translations);
        m
    }

    fn texts(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs.iter().map(|(p, c)| (p.to_string(), c.to_string())).collect()
    }

    const FILES: fn() -> Value = || {
        json!([
            {"id": "f:1", "path": "app/a.ts", "parsed": 1},
            {"id": "f:2", "path": "app/b.html", "parsed": 0},
            {"id": "f:9", "path": "assets/i18n/en.json", "parsed": 0}
        ])
    };

    fn trace(pairs: &[(&str, &str)], translations: Value) -> Trace {
        let tables = store_of(FILES(), translations);
        let store = Store::from_payload(tables, "typescript");
        source_trace(&store, &texts(pairs))
    }

    #[test]
    fn a_key_spelled_out_in_a_parsed_file_is_a_full_literal() {
        let t = trace(&[("app/a.ts", "t('menu.home.title')")], json!([]));
        assert_eq!(t.of("menu.home.title"), (How::FullLiteral, Some("f:1".to_string())));
    }

    #[test]
    fn the_same_key_in_an_unparsed_file_is_out_of_scope_and_not_a_gap() {
        // Nearly all the survivors were here, and calling them extractor gaps was wrong.
        let t = trace(&[("app/b.html", "{{ 'menu.home.title' | translate }}")], json!([]));
        assert_eq!(t.of("menu.home.title"), (How::UnparsedFile, Some("f:2".to_string())));
    }

    #[test]
    fn a_locale_file_is_never_scanned_because_it_defines_every_key() {
        // With them in, every key read as referenced and the trace answered nothing.
        let t = trace(
            &[("assets/i18n/en.json", "\"menu.home.title\": \"Home\"")],
            json!([{"file": "f:9"}]),
        );
        assert_eq!(t.of("menu.home.title").0, How::Absent);
        assert_eq!(t.scanned, 0);
        assert_eq!(t.locales, 1);
    }

    #[test]
    fn a_binding_inside_an_open_comment_block_is_not_a_reference() {
        // The first residue reported as an extractor gap was exactly this: the block opened
        // several lines earlier, the template parser drops it, the map was right to omit it.
        let body = "<!-- old markup\n  {{ 'menu.home.title' | translate }}\n-->\n<p></p>";
        let t = trace(&[("app/a.ts", body)], json!([]));
        assert_eq!(t.of("menu.home.title").0, How::Absent);
    }

    #[test]
    fn an_unclosed_comment_block_does_not_hide_the_rest_of_the_file() {
        // Swallowing to end of file would call live keys dead, which is the failure that
        // matters.
        let t = trace(&[("app/a.ts", "/* open\nt('menu.home.title')")], json!([]));
        assert_eq!(t.of("menu.home.title").0, How::FullLiteral);
    }

    #[test]
    fn a_line_comment_counts_as_a_comment_line_and_contributes_nothing() {
        let t = trace(&[("app/a.ts", "  // t('menu.home.title')\n * also.a.comment")], json!([]));
        assert_eq!(t.comment_lines, 2);
        assert_eq!(t.of("menu.home.title").0, How::Absent);
    }

    #[test]
    fn a_key_commented_out_at_the_END_of_a_live_line_is_still_commented_out() {
        // The line does not START with the slashes, so the line test above never sees it.
        let t = trace(&[("app/a.ts", "const x = 1; // t('menu.home.title')")], json!([]));
        assert_eq!(t.of("menu.home.title").0, How::Absent);
        assert_eq!(t.comment_lines, 0, "it is a live line that happens to carry a comment");
    }

    #[test]
    fn a_url_is_not_a_line_comment_so_the_key_beside_it_still_counts() {
        let t = trace(&[("app/a.ts", "url: 'https://x' + t('menu.home.title')")], json!([]));
        assert_eq!(t.of("menu.home.title").0, How::FullLiteral);
    }

    #[test]
    fn a_key_whose_prefix_alone_is_spelled_out_is_assembled_at_runtime() {
        let t = trace(&[("app/a.ts", "t('menu.home.' + which)")], json!([]));
        assert_eq!(t.of("menu.home.title"), (How::Prefix, Some("f:1".to_string())));
    }

    #[test]
    fn only_the_leaf_appearing_anywhere_is_the_weakest_answer_that_is_still_not_dead() {
        let t = trace(&[("app/a.ts", "const title = 1;")], json!([]));
        assert_eq!(t.of("menu.home.title"), (How::Leaf, None));
        // And a word under three characters is not a word: see `sourcescan::words`.
        assert_eq!(t.of("menu.home.ab").0, How::Absent);
    }

    #[test]
    fn the_first_file_a_name_appears_in_wins_so_the_reported_id_is_stable() {
        let t = trace(
            &[("app/a.ts", "t('menu.home.title')"), ("app/b.html", "menu.home.title")],
            json!([]),
        );
        assert_eq!(t.of("menu.home.title").1, Some("f:1".to_string()));
        assert_eq!(t.scanned, 2);
    }

    #[test]
    fn a_text_for_a_path_the_map_never_recorded_is_skipped() {
        let t = trace(&[("app/stray.ts", "t('menu.home.title')")], json!([]));
        assert_eq!(t.scanned, 0);
        assert_eq!(t.of("menu.home.title").0, How::Absent);
    }
}
