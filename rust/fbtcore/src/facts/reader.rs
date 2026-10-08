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
}

/// The blocks of a snapshot, by what it is: a Docs API document when it parses as one, Markdown otherwise.
pub fn blocks(path: &std::path::Path, text: &str) -> Vec<Block> {
    let json = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
    match json.then(|| serde_json::from_str::<Value>(text).ok()).flatten() {
        Some(doc) => from_docs(&doc),
        None => from_markdown(text),
    }
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
    for (event, range) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
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
    out
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
                out.push(match level {
                    Some(level) => Block::Heading { level, text },
                    None => Block::Para { text, bold },
                });
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
                out.push(Block::Table { rows, head: false });
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

fn squeezed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
    let mut found = Vec::new();
    let mut subsection = String::new();
    for block in &all[at + 1..] {
        match block {
            Block::Heading { level: l, .. } if l <= level => break,
            Block::Heading { text, .. } => subsection = text.clone(),
            Block::Para { text, bold: true } => subsection = text.clone(),
            Block::Table { rows, head } => found.push(Found { subsection: subsection.clone(), rows, head: *head }),
            Block::Para { .. } => {}
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

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
