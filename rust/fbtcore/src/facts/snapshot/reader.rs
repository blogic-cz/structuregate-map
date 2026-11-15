//! A SNAPSHOT AS BLOCKS - headings, paragraphs (and whether they are bold), tables - read by a parser, never by
//! matching text. Two snapshots, one model:
//!
//! - `.json`: the document as Google keeps it (`documents.get` of the Docs API, what `--facts-pull` saves for a
//!   `save` ending in `.json`). A heading is a `HEADING_n` paragraph style, bold is a text run's style, a table is
//!   its rows and cells: nothing went through a conversion, so nothing has to be undone.
//! - anything else: Markdown, parsed by `pulldown-cmark` with GFM tables (the parser the Markdown half uses).
//!   A cell's text is parsed ONCE MORE as inline Markdown, because an export that escapes its own emphasis
//!   (`\*\*7\*\*`, as a Drive export does) leaves the markup in the text, and the second parse is what takes it out.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading { level: usize, text: String },
    /// A paragraph, and whether all of its text is bold - what a document uses as a heading it did not style as one.
    Para { text: String, bold: bool },
    /// Every row, each with where it starts. `head`: the first row is a GFM header row (an empty one too).
    Table { rows: Vec<(usize, Vec<String>)>, head: bool },
    /// THE TEXT OF ONE LEAF - a paragraph, a heading, a list item, a table cell, a line of a code block - or one code
    /// span (`code`), where it starts: what the facts that are no table are read out of (`words.rs`). It sits after the
    /// block it is part of, so a heading's own text is under that heading.
    Text { at: usize, text: String, code: bool },
}

/// One text of a section, and the heading or bold paragraph above it inside the section.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub at: usize,
    pub text: String,
    pub code: bool,
    pub subsection: String,
}

/// The blocks of a snapshot, by what it is: a Word file (`docx.rs`) by its extension, a Docs API document when it
/// parses as one, Markdown otherwise. Only a Word file can fail to read: any text is some Markdown.
pub fn blocks(path: &std::path::Path, bytes: &[u8]) -> Result<Vec<Block>, String> {
    let is = |ext: &str| path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext));
    if is("docx") {
        return super::docx::blocks(bytes);
    }
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes));
    Ok(match is("json").then(|| serde_json::from_str::<Value>(&text).ok()).flatten() {
        Some(doc) => from_docs(&doc),
        None => from_markdown(&text),
    })
}

/// A Markdown document. A row's place is the line it starts on.
pub fn from_markdown(text: &str) -> Vec<Block> {
    let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let line_of = |offset: usize| starts.partition_point(|s| *s <= offset);
    let mut out = Vec::new();
    let mut heading: Option<(usize, String)> = None;
    // A paragraph's text, and how much of it sat inside `**`/`__`.
    let mut para: Option<(String, usize, usize)> = None;
    let mut strong = 0usize;
    let mut rows: Option<Vec<(usize, Vec<String>)>> = None;
    let mut row: Vec<String> = Vec::new();
    let mut row_line = 0;
    let mut cell: Option<String> = None;
    let mut leaf = Leaf::default();
    for (event, range) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
        // A LEAF ENDS where the next block of text starts, so its text goes out after the block it was part of.
        if matches!(&event, Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::Item | Tag::TableCell | Tag::CodeBlock(_))) {
            leaf.flush(&mut out);
        }
        match &event {
            Event::Start(Tag::CodeBlock(_)) => leaf.block = true,
            Event::Text(t) => leaf.add(line_of(range.start), t, false),
            Event::Code(t) => leaf.add(line_of(range.start), t, true),
            Event::SoftBreak | Event::HardBreak => leaf.add(line_of(range.start), " ", false),
            _ => {}
        }
        match event {
            Event::Start(Tag::Heading { level, .. }) => heading = Some((level as usize, String::new())),
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = heading.take() {
                    out.push(Block::Heading { level, text: inline(&squeezed(&text)).0 });
                }
            }
            Event::Start(Tag::Paragraph) if cell.is_none() && heading.is_none() => para = Some((String::new(), 0, 0)),
            Event::End(TagEnd::Paragraph) => {
                if let Some((text, bold, all)) = para.take() {
                    let text = squeezed(&text);
                    if !text.is_empty() {
                        let plain = inline(&text);
                        // EITHER BOLD AS MARKDOWN, or bold as the text an escaping export left: parsed again.
                        let bold = (all > 0 && bold == all) || (plain.1 && !plain.0.is_empty());
                        out.push(Block::Para { text: plain.0, bold });
                    }
                }
            }
            Event::Start(Tag::Strong) => strong += 1,
            Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
            Event::Start(Tag::Table(_)) => rows = Some(Vec::new()),
            Event::End(TagEnd::Table) => {
                if let Some(done) = rows.take() {
                    out.push(Block::Table { rows: done, head: true });
                }
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                row.clear();
                row_line = line_of(range.start);
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                if let Some(t) = rows.as_mut() {
                    t.push((row_line, std::mem::take(&mut row)));
                }
            }
            Event::Start(Tag::TableCell) => cell = Some(String::new()),
            Event::End(TagEnd::TableCell) => row.push(inline(&squeezed(&cell.take().unwrap_or_default())).0),
            Event::Text(t) | Event::Code(t) => {
                if let Some(c) = cell.as_mut() {
                    c.push_str(&t);
                } else if let Some((_, h)) = heading.as_mut() {
                    h.push_str(&t);
                } else if let Some((p, bold, all)) = para.as_mut() {
                    p.push_str(&t);
                    let n = t.chars().filter(|c| !c.is_whitespace()).count();
                    *all += n;
                    if strong > 0 {
                        *bold += n;
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(c) = cell.as_mut() {
                    c.push(' ');
                } else if let Some((_, h)) = heading.as_mut() {
                    h.push(' ');
                } else if let Some((p, _, _)) = para.as_mut() {
                    p.push(' ');
                }
            }
            _ => {}
        }
    }
    leaf.flush(&mut out);
    out
}

/// The text of the leaf being read, where it started, its code spans, and whether it is a code block.
#[derive(Default)]
struct Leaf {
    at: usize,
    text: String,
    spans: Vec<(usize, String)>,
    block: bool,
}

impl Leaf {
    fn add(&mut self, line: usize, piece: &str, code: bool) {
        if self.text.is_empty() && self.spans.is_empty() {
            self.at = line;
        }
        self.text.push_str(piece);
        if code {
            self.spans.push((line, piece.to_string()));
        }
    }

    /// A code block is one text per line; anything else one text, squeezed, then each of its code spans.
    fn flush(&mut self, out: &mut Vec<Block>) {
        let leaf = std::mem::take(self);
        if leaf.block {
            for (i, line) in leaf.text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
                out.push(Block::Text { at: leaf.at + i, text: line.trim().to_string(), code: false });
            }
            return;
        }
        let text = squeezed(&leaf.text);
        if !text.is_empty() {
            out.push(Block::Text { at: leaf.at, text, code: false });
        }
        for (at, span) in leaf.spans {
            out.push(Block::Text { at, text: squeezed(&span), code: true });
        }
    }
}

/// Text that may still hold inline Markdown, as its plain text, and whether all of it was bold.
fn inline(text: &str) -> (String, bool) {
    let mut plain = String::new();
    let (mut strong, mut bold, mut all) = (0usize, 0usize, 0usize);
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::Strong) => strong += 1,
            Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
            Event::Text(t) | Event::Code(t) => {
                let n = t.chars().filter(|c| !c.is_whitespace()).count();
                all += n;
                if strong > 0 {
                    bold += n;
                }
                plain.push_str(&t);
            }
            Event::SoftBreak | Event::HardBreak => plain.push(' '),
            _ => {}
        }
    }
    (squeezed(&plain), all > 0 && bold == all)
}

/// A Docs API document. A row's place is its `startIndex` in the document - a JSON snapshot has no lines.
pub fn from_docs(doc: &Value) -> Vec<Block> {
    let mut out = Vec::new();
    // A DOCUMENT WITH TABS keeps its body under each tab; one without, under `body`.
    let mut bodies: Vec<&Value> = Vec::new();
    collect_tabs(&doc["tabs"], &mut bodies);
    if bodies.is_empty() {
        bodies.push(&doc["body"]);
    }
    for body in bodies {
        for element in body["content"].as_array().into_iter().flatten() {
            if let Some(p) = element.get("paragraph") {
                let (text, bold) = paragraph(p);
                if text.is_empty() {
                    continue;
                }
                let style = p["paragraphStyle"]["namedStyleType"].as_str().unwrap_or("");
                let level = match style {
                    "TITLE" => Some(1),
                    s => s.strip_prefix("HEADING_").and_then(|n| n.parse::<usize>().ok()),
                };
                let at = element["startIndex"].as_u64().unwrap_or(0) as usize;
                out.push(match level {
                    Some(level) => Block::Heading { level, text: text.clone() },
                    None => Block::Para { text: text.clone(), bold },
                });
                texts_of(p, at, text, &mut out);
            } else if let Some(t) = element.get("table") {
                let rows = t["tableRows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| {
                        let at = r["startIndex"].as_u64().unwrap_or(0) as usize;
                        let cells = r["tableCells"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|c| {
                                let parts: Vec<String> = c["content"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|e| e.get("paragraph").map(|p| paragraph(p).0))
                                    .filter(|t| !t.is_empty())
                                    .collect();
                                parts.join(" ")
                            })
                            .collect();
                        (at, cells)
                    })
                    .collect();
                let cells: Vec<(usize, &Value)> = t["tableRows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|r| r["tableCells"].as_array().into_iter().flatten())
                    .flat_map(|c| c["content"].as_array().into_iter().flatten())
                    .filter_map(|e| e.get("paragraph").map(|p| (e["startIndex"].as_u64().unwrap_or(0) as usize, p)))
                    .collect();
                out.push(Block::Table { rows, head: false });
                for (at, p) in cells {
                    let text = paragraph(p).0;
                    if !text.is_empty() {
                        texts_of(p, at, text, &mut out);
                    }
                }
            }
        }
    }
    out
}

fn collect_tabs<'a>(tabs: &'a Value, out: &mut Vec<&'a Value>) {
    for tab in tabs.as_array().into_iter().flatten() {
        out.push(&tab["documentTab"]["body"]);
        collect_tabs(&tab["childTabs"], out);
    }
}

/// A Docs paragraph as texts: its own, then each run of it set in a monospace font - what a Google Doc has for a code span.
fn texts_of(p: &Value, at: usize, text: String, out: &mut Vec<Block>) {
    out.push(Block::Text { at, text, code: false });
    for element in p["elements"].as_array().into_iter().flatten() {
        let run = &element["textRun"];
        let font = run["textStyle"]["weightedFontFamily"]["fontFamily"].as_str().unwrap_or("");
        let code = squeezed(run["content"].as_str().unwrap_or(""));
        if MONOSPACE.iter().any(|m| m.eq_ignore_ascii_case(font)) && !code.is_empty() {
            out.push(Block::Text { at: element["startIndex"].as_u64().unwrap_or(at as u64) as usize, text: code, code: true });
        }
    }
}

/// The monospace fonts a document sets code in (Google Docs' own list, and Word's).
pub const MONOSPACE: [&str; 12] = [
    "Courier New", "Courier", "Consolas", "Roboto Mono", "Source Code Pro", "Cousine", "Inconsolata", "Ubuntu Mono",
    "Fira Code", "JetBrains Mono", "Lucida Console", "Cascadia Code",
];

/// A paragraph's text, and whether every visible character of it is in a bold run.
fn paragraph(p: &Value) -> (String, bool) {
    let mut text = String::new();
    let (mut bold, mut all) = (0usize, 0usize);
    for element in p["elements"].as_array().into_iter().flatten() {
        let Some(content) = element["textRun"]["content"].as_str() else { continue };
        let n = content.chars().filter(|c| !c.is_whitespace()).count();
        all += n;
        if element["textRun"]["textStyle"]["bold"].as_bool() == Some(true) {
            bold += n;
        }
        text.push_str(content);
    }
    (squeezed(&text), all > 0 && bold == all)
}

/// Whitespace squeezed to single spaces, and the ZERO-WIDTH characters a pasted text carries (U+200B..U+200D, U+2060,
/// U+FEFF) dropped: they are invisible in the document and split `/api/x` into two words nobody wrote.
pub(super) fn squeezed(text: &str) -> String {
    let visible: String = text.chars().filter(|c| !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}')).collect();
    visible.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One table of a section, and the bold paragraph above it inside the section (its subsection), if any.
#[derive(Debug)]
pub struct Found<'a> {
    pub subsection: String,
    pub rows: &'a [(usize, Vec<String>)],
    pub head: bool,
}

/// The tables under the heading that names `section` - the one equal to it ignoring case, else the only one that
/// contains it - up to the next heading of the same level or higher. `which` is 0 for all of them, n for the nth.
pub fn section<'a>(all: &'a [Block], section: &str, which: usize) -> Result<Vec<Found<'a>>, String> {
    let (name, blocks) = scope(all, section)?;
    let mut found = Vec::new();
    let mut subsection = String::new();
    for block in blocks {
        match block {
            Block::Heading { text, .. } => subsection = text.clone(),
            Block::Para { text, bold: true } => subsection = text.clone(),
            Block::Table { rows, head } => found.push(Found { subsection: subsection.clone(), rows, head: *head }),
            Block::Para { .. } | Block::Text { .. } => {}
        }
    }
    if found.is_empty() {
        return Err(format!("no table under `{name}`"));
    }
    if which == 0 {
        return Ok(found);
    }
    let n = found.len();
    found.into_iter().nth(which - 1).map(|f| vec![f]).ok_or_else(|| format!("`{name}` has {n} table(s), not {which}"))
}

/// Every text under the heading that names `section` (the whole document for ""), each with its subsection.
pub fn texts(all: &[Block], section: &str) -> Result<Vec<Text>, String> {
    let (_, blocks) = scope(all, section)?;
    let mut out = Vec::new();
    let mut subsection = String::new();
    for block in blocks {
        match block {
            Block::Heading { text, .. } | Block::Para { text, bold: true } => subsection = text.clone(),
            Block::Text { at, text, code } => out.push(Text { at: *at, text: text.clone(), code: *code, subsection: subsection.clone() }),
            _ => {}
        }
    }
    Ok(out)
}

/// The blocks under the heading that names `section` - the one equal to it ignoring case, else the only one that
/// contains it - up to the next heading of the same level or higher, and the heading's text. "" is the whole document.
pub fn scope<'a>(all: &'a [Block], section: &str) -> Result<(String, &'a [Block]), String> {
    if section.trim().is_empty() {
        return Ok(("the document".into(), all));
    }
    let wanted = section.trim().to_lowercase();
    let headings: Vec<(usize, &String)> =
        all.iter().enumerate().filter_map(|(i, b)| if let Block::Heading { text, .. } = b { Some((i, text)) } else { None }).collect();
    let exact: Vec<usize> = headings.iter().filter(|(_, h)| h.to_lowercase() == wanted).map(|(i, _)| *i).collect();
    let near: Vec<usize> = headings.iter().filter(|(_, h)| h.to_lowercase().contains(&wanted)).map(|(i, _)| *i).collect();
    let at = match (exact.len(), near.len()) {
        (1, _) => exact[0],
        (0, 1) => near[0],
        (0, 0) => return Err(format!("no heading `{section}`")),
        (_, n) => return Err(format!("`{section}` names {n} headings - spell it as the one you mean")),
    };
    let Block::Heading { level, text: name } = &all[at] else { unreachable!() };
    let end = all[at + 1..].iter().position(|b| matches!(b, Block::Heading { level: l, .. } if l <= level)).map_or(all.len(), |n| at + 1 + n);
    Ok((name.clone(), &all[at + 1..end]))
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
