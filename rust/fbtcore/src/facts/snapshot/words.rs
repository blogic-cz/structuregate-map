//! FACTS THAT ARE NO TABLE: an API spec names its routes in running text and in code spans (`GET /api/items/{id}`),
//! not in a table. These read them out of the `Text` blocks a snapshot is (`reader.rs`) - split at whitespace, never
//! matched as a pattern: a word that starts with `/` is a route, the word before it its verb when it is one.

use super::reader::Text;

/// The HTTP verbs a route may be written after.
pub const VERBS: [&str; 7] = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

/// One fact read out of text: where, under which heading, and the text itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub at: usize,
    pub subsection: String,
    pub key: String,
}

/// Every code span, once each (the first place it is written), in document order.
pub fn code_spans(texts: &[Text]) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    for t in texts.iter().filter(|t| t.code) {
        let key = t.text.trim().to_string();
        if !key.is_empty() && !out.iter().any(|w| w.key == key) {
            out.push(Word { at: t.at, subsection: t.subsection.clone(), key });
        }
    }
    out
}

/// Every route the text names - `/api/items/{id}`, or `GET /api/items/{id}` when a verb is the word before it -
/// once each, in document order. `prefix` keeps only the routes that start with it (ignoring case).
pub fn routes(texts: &[Text], prefix: &str) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    for t in texts {
        for key in routes_in(&t.text) {
            let path = key.rsplit(' ').next().unwrap_or(&key);
            if !path.to_lowercase().starts_with(&prefix.to_lowercase()) || out.iter().any(|w| w.key == key) {
                continue;
            }
            out.push(Word { at: t.at, subsection: t.subsection.clone(), key });
        }
    }
    out
}

/// The routes of one text, each with its verb when the word before it is one - or, in a heading that writes
/// `/api/items POST`, the word after it when that one is no verb of a route after it.
pub fn routes_in(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let verb_at = |j: usize| words.get(j).map(|w| trimmed(w).to_uppercase()).filter(|v| VERBS.contains(&v.as_str()));
    let mut out = Vec::new();
    for (i, raw) in words.iter().enumerate() {
        let Some(path) = route(raw) else { continue };
        let after = || verb_at(i + 1).filter(|_| words.get(i + 2).is_none_or(|w| route(w).is_none()));
        let verb = i.checked_sub(1).and_then(verb_at).or_else(after);
        out.push(match verb {
            Some(verb) => format!("{verb} {path}"),
            None => path,
        });
    }
    out
}

/// A word as a route: it starts with `/` (after the punctuation around it), holds a letter, and is no `//` comment.
/// A full url keeps its path. The query and the fragment are no part of it, nor is a closing `/`.
pub fn route(word: &str) -> Option<String> {
    let word = trimmed(word);
    let path = match word.strip_prefix("http://").or_else(|| word.strip_prefix("https://")) {
        Some(rest) => &rest[rest.find('/')?..],
        None => word,
    };
    if !path.starts_with('/') || path.starts_with("//") {
        return None;
    }
    let path = without_query(path);
    let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    path.chars().any(char::is_alphabetic).then(|| path.to_string())
}

/// A path without its query and fragment: they start at a `?` or `#` outside a parameter - `{id?}` is an optional
/// segment, not a query.
pub fn without_query(path: &str) -> &str {
    let mut open = 0usize;
    for (i, c) in path.char_indices() {
        match c {
            '{' => open += 1,
            '}' => open = open.saturating_sub(1),
            '?' | '#' if open == 0 => return &path[..i],
            _ => {}
        }
    }
    path
}

/// A word without the punctuation a sentence or a markup puts around it.
fn trimmed(word: &str) -> &str {
    word.trim_matches(|c: char| matches!(c, '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '<' | '>' | ',' | '.' | ';' | ':' | '!' | '*' | '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(at: usize, text: &str, code: bool) -> Text {
        Text { at, text: text.into(), code, subsection: "Items".into() }
    }

    #[test]
    fn a_route_is_a_word_starting_with_a_slash_and_its_verb_the_word_before() {
        assert_eq!(routes_in("Call `GET /api/items/{id}`, then POST /api/items/ (see https://x.example/api/help?x=1)."), vec![
            "GET /api/items/{id}",
            "POST /api/items",
            "/api/help"
        ]);
        assert_eq!(routes_in("2.1 /api/items/lookup POST"), vec!["POST /api/items/lookup"]);
        assert_eq!(routes_in("/api/a GET /api/b"), vec!["/api/a", "GET /api/b"]);
        assert_eq!(routes_in("a // comment, //cdn.example/lib, and/or 1/2 and / alone"), Vec::<String>::new());
    }

    #[test]
    fn routes_and_spans_are_read_once_each_and_a_prefix_filters_routes() {
        let texts = vec![
            text(3, "GET /api/items", false),
            text(4, "GET /api/items", true),
            text(5, "/docs/readme", true),
            text(6, "ItemID", true),
            text(7, "ItemID", true),
        ];
        let keys = |w: Vec<Word>| w.into_iter().map(|w| (w.at, w.key)).collect::<Vec<_>>();
        assert_eq!(keys(routes(&texts, "/api/")), vec![(3, "GET /api/items".to_string())]);
        assert_eq!(keys(routes(&texts, "")).len(), 2);
        assert_eq!(keys(code_spans(&texts)), vec![(4, "GET /api/items".into()), (5, "/docs/readme".into()), (6, "ItemID".into())]);
    }
}
