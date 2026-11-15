//! AN UPLOADED WORD FILE (`.docx`) AS BLOCKS - the same blocks a Markdown or a Docs API snapshot is (`reader.rs`). The
//! Docs API refuses a `.docx` and Drive's Markdown export does too, so `--facts-pull` saves the file as it is
//! (`drive-file`) and it is read here: a zip whose `word/document.xml` is the body, `word/styles.xml` which style is a
//! heading. The zip's directory is read by hand (one entry is wanted), the entry inflated by `miniz_oxide`, the XML
//! parsed by `roxmltree` - nothing matched as text.
//!
//! A heading is a paragraph whose style is named `heading N` or `Title` (the style's NAME, not its id: a Czech Word
//! calls the id `Nadpis1`), or one with an outline level. A run is bold by `w:b`, code when set in a monospace font.
//! A block's place is the body element's number from 1 - a `.docx` has no lines.

use super::reader::{Block, MONOSPACE};
use roxmltree::{Document, Node};
use std::collections::HashMap;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The blocks of a `.docx`, or why it cannot be read.
pub fn blocks(bytes: &[u8]) -> Result<Vec<Block>, String> {
    let body = entry(bytes, "word/document.xml")?.ok_or("no word/document.xml in it - not a Word document")?;
    let styles = entry(bytes, "word/styles.xml")?.map(|s| headings(&s)).unwrap_or_default();
    let text = String::from_utf8_lossy(&body);
    let doc = Document::parse(&text).map_err(|e| format!("word/document.xml is no XML - {e}"))?;
    let body = doc.descendants().find(|n| is(n, "body")).ok_or("word/document.xml has no body")?;
    let mut out = Vec::new();
    for (i, element) in body.children().filter(Node::is_element).enumerate() {
        let at = i + 1;
        if is(&element, "p") {
            let (text, bold) = paragraph(&element);
            if text.is_empty() {
                continue;
            }
            out.push(match level(&element, &styles) {
                Some(level) => Block::Heading { level, text: text.clone() },
                None => Block::Para { text: text.clone(), bold },
            });
            texts(&element, at, text, &mut out);
        } else if is(&element, "tbl") {
            let mut rows = Vec::new();
            let mut cells_text = Vec::new();
            for row in element.children().filter(|n| is(n, "tr")) {
                let cells: Vec<String> = row
                    .children()
                    .filter(|n| is(n, "tc"))
                    .map(|cell| {
                        let parts: Vec<String> = cell.descendants().filter(|n| is(n, "p")).map(|p| paragraph(&p).0).filter(|t| !t.is_empty()).collect();
                        cells_text.extend(cell.descendants().filter(|n| is(n, "p")));
                        parts.join(" ")
                    })
                    .collect();
                rows.push((at, cells));
            }
            out.push(Block::Table { rows, head: false });
            for p in cells_text {
                let text = paragraph(&p).0;
                if !text.is_empty() {
                    texts(&p, at, text, &mut out);
                }
            }
        }
    }
    Ok(out)
}

fn is(node: &Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name && node.tag_name().namespace() == Some(W)
}

fn child<'a, 'i>(node: &Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| is(n, name))
}

fn val<'a>(node: &Node<'a, '_>) -> Option<&'a str> {
    node.attribute((W, "val"))
}

/// A toggle like `w:b`: on unless its value says off.
fn on(node: Option<Node>) -> bool {
    node.is_some_and(|n| !matches!(val(&n), Some("0" | "false" | "off")))
}

/// The paragraph's text (its `w:t` and tabs, in order) and whether every visible character is in a bold run.
fn paragraph(p: &Node) -> (String, bool) {
    let mut text = String::new();
    let (mut bold, mut all) = (0usize, 0usize);
    for run in p.descendants().filter(|n| is(n, "r")) {
        let piece = run_text(&run);
        let n = piece.chars().filter(|c| !c.is_whitespace()).count();
        all += n;
        if on(child(&run, "rPr").and_then(|r| child(&r, "b"))) {
            bold += n;
        }
        text.push_str(&piece);
    }
    (text.split_whitespace().collect::<Vec<_>>().join(" "), all > 0 && bold == all)
}

fn run_text(run: &Node) -> String {
    let mut piece = String::new();
    for n in run.children() {
        if is(&n, "t") {
            piece.push_str(n.text().unwrap_or(""));
        } else if is(&n, "tab") || is(&n, "br") {
            piece.push(' ');
        }
    }
    piece
}

/// The paragraph as texts: its own, then each run set in a monospace font.
fn texts(p: &Node, at: usize, text: String, out: &mut Vec<Block>) {
    out.push(Block::Text { at, text, code: false });
    for run in p.descendants().filter(|n| is(n, "r")) {
        let font = child(&run, "rPr").and_then(|r| child(&r, "rFonts")).and_then(|f| f.attribute((W, "ascii")).or_else(|| f.attribute((W, "hAnsi"))));
        let code = run_text(&run).split_whitespace().collect::<Vec<_>>().join(" ");
        if font.is_some_and(|f| MONOSPACE.iter().any(|m| m.eq_ignore_ascii_case(f))) && !code.is_empty() {
            out.push(Block::Text { at, text: code, code: true });
        }
    }
}

/// A heading's level: its own outline level, else its style's.
fn level(p: &Node, styles: &HashMap<String, usize>) -> Option<usize> {
    let props = child(p, "pPr")?;
    if let Some(n) = child(&props, "outlineLvl").and_then(|o| val(&o)?.parse::<usize>().ok()) {
        return Some(n + 1);
    }
    styles.get(val(&child(&props, "pStyle")?)?).copied()
}

/// Which style ids are headings, and of which level: by the style's NAME (`heading 2`, `Title`) or its outline level.
fn headings(xml: &[u8]) -> HashMap<String, usize> {
    let text = String::from_utf8_lossy(xml);
    let Ok(doc) = Document::parse(&text) else { return HashMap::new() };
    let mut out = HashMap::new();
    for style in doc.descendants().filter(|n| is(n, "style")) {
        let Some(id) = style.attribute((W, "styleId")) else { continue };
        let name = child(&style, "name").and_then(|n| val(&n)).unwrap_or("").to_lowercase();
        let outline = child(&style, "pPr").and_then(|p| child(&p, "outlineLvl")).and_then(|o| val(&o)?.parse::<usize>().ok());
        let level = match name.strip_prefix("heading ").and_then(|n| n.trim().parse::<usize>().ok()) {
            Some(n) => Some(n),
            None if name == "title" => Some(1),
            None => outline.map(|n| n + 1),
        };
        if let Some(level) = level {
            out.insert(id.to_string(), level);
        }
    }
    out
}

/// One entry of a zip, inflated - None when the zip has no entry of that name. Read from the central directory, so
/// an entry whose sizes follow its data (a streamed zip) is read as well.
pub fn entry(zip: &[u8], name: &str) -> Result<Option<Vec<u8>>, String> {
    let broken = || "it is no zip a Word document is - the file is broken or not a .docx".to_string();
    // THE END RECORD is the last `PK\5\6` within the 64 KB a comment may take.
    let floor = zip.len().saturating_sub(22 + 65535);
    let end = (floor..zip.len().saturating_sub(21)).rev().find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or_else(broken)?;
    let u16_at = |i: usize| zip.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |i: usize| zip.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let count = u16_at(end + 10).ok_or_else(broken)?;
    let mut at = u32_at(end + 16).ok_or_else(broken)?;
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err(broken());
        }
        let method = u16_at(at + 10).ok_or_else(broken)?;
        let packed = u32_at(at + 20).ok_or_else(broken)?;
        let (name_len, extra, comment) = (u16_at(at + 28).ok_or_else(broken)?, u16_at(at + 30).ok_or_else(broken)?, u16_at(at + 32).ok_or_else(broken)?);
        let local = u32_at(at + 42).ok_or_else(broken)?;
        let this = zip.get(at + 46..at + 46 + name_len).ok_or_else(broken)?;
        at += 46 + name_len + extra + comment;
        if this != name.as_bytes() {
            continue;
        }
        let (lname, lextra) = (u16_at(local + 26).ok_or_else(broken)?, u16_at(local + 28).ok_or_else(broken)?);
        let start = local + 30 + lname + lextra;
        let data = zip.get(start..start + packed).ok_or_else(broken)?;
        return match method {
            0 => Ok(Some(data.to_vec())),
            8 => miniz_oxide::inflate::decompress_to_vec(data).map(Some).map_err(|e| format!("{name} could not be inflated - {e:?}")),
            other => Err(format!("{name} is packed by method {other}, which a .docx does not use")),
        };
    }
    Ok(None)
}

#[cfg(test)]
#[path = "docx_tests.rs"]
mod tests;
