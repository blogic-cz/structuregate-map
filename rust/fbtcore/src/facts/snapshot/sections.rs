//! EVERY SECTION OF EVERY SOURCE, AS ROWS - not only the parts a `facts` entry names: a consumer that decides for
//! itself which part of a document describes which code reads the document from the map, through the same reader
//! (`reader.rs`, `docx.rs`) the facts are read with, instead of parsing a Docs API document or a Word file again.
//!
//! - `doc_sections`: one row per heading, and one (`level` 0, no heading) for what comes before the first - its
//!   `path` the headings above it and its own (` > ` between), its `text` every text it holds, a table's cells too,
//!   one line each in document order.
//! - `doc_tables`: one row per cell - the section, which table of it (from 1), the row (`0` a header row, data from
//!   1) and the column (from 1).

use super::config::Source;
use super::reader::{self, Block};
use rusqlite::{params, Connection};

/// What was written: sections, cells, and a note per source that could not be read.
pub struct Written {
    pub sections: usize,
    pub cells: usize,
    pub notes: Vec<String>,
}

pub fn write(conn: &Connection, sources: &[Source]) -> rusqlite::Result<Written> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS doc_sections; DROP TABLE IF EXISTS doc_tables;
         CREATE TABLE doc_sections (id, source, file, n, level, heading, path, line, text);
         CREATE TABLE doc_tables (section, source, tbl, row, col, value, line);",
    )?;
    let mut done = Written { sections: 0, cells: 0, notes: Vec::new() };
    let mut section = conn.prepare("INSERT INTO doc_sections VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?;
    let mut cell = conn.prepare("INSERT INTO doc_tables VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
    for source in sources {
        // NO SNAPSHOT YET is the facts' note already (`--facts-pull`); a Word file that does not open is this one's.
        let Ok(bytes) = std::fs::read(&source.path) else { continue };
        let blocks = match reader::blocks(&source.path, &bytes) {
            Ok(blocks) => blocks,
            Err(why) => {
                done.notes.push(format!("the document `{}` has no sections in the map: {why}", source.name));
                continue;
            }
        };
        for part in parts(&blocks) {
            done.sections += 1;
            let id = format!("ds:{}", done.sections);
            let line = part.texts.first().map_or(0, |(at, _)| *at as i64);
            let text: Vec<&str> = part.texts.iter().map(|(_, t)| t.as_str()).collect();
            section.execute(params![id, source.name, source.shown, part.n as i64, part.level as i64, part.heading, part.path, line, text.join("\n")])?;
            for (t, (rows, head)) in part.tables.iter().enumerate() {
                for (r, (at, cells)) in rows.iter().enumerate() {
                    let row = if *head { r } else { r + 1 } as i64;
                    for (c, value) in cells.iter().enumerate() {
                        done.cells += 1;
                        cell.execute(params![id, source.name, t as i64 + 1, row, c as i64 + 1, value, *at as i64])?;
                    }
                }
            }
        }
    }
    conn.execute_batch("CREATE INDEX ix_doc_tables_section ON doc_tables(section);")?;
    Ok(done)
}

/// One section: its place among the document's sections, its heading and level, the headings down to it, its texts
/// with where each starts, and its tables (each row with where it starts, and whether the first is a header).
struct Part<'a> {
    n: usize,
    level: usize,
    heading: String,
    path: String,
    texts: Vec<(usize, String)>,
    tables: Vec<(&'a Vec<(usize, Vec<String>)>, bool)>,
}

/// The document cut at its headings; what precedes the first is a part of level 0 when it holds anything.
fn parts(blocks: &[Block]) -> Vec<Part<'_>> {
    let mut out: Vec<Part> = Vec::new();
    let mut above: Vec<(usize, String)> = Vec::new();
    let mut at = Part { n: 0, level: 0, heading: String::new(), path: String::new(), texts: vec![], tables: vec![] };
    for block in blocks {
        match block {
            Block::Heading { level, text } => {
                let done = std::mem::replace(&mut at, Part { n: 0, level: *level, heading: text.clone(), path: String::new(), texts: vec![], tables: vec![] });
                if done.level > 0 || !done.texts.is_empty() || !done.tables.is_empty() {
                    out.push(done);
                }
                while above.last().is_some_and(|(l, _)| l >= level) {
                    above.pop();
                }
                above.push((*level, text.clone()));
                at.path = above.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" > ");
            }
            Block::Text { at: line, text, code: false } => at.texts.push((*line, text.clone())),
            Block::Table { rows, head } => at.tables.push((rows, *head)),
            _ => {}
        }
    }
    if at.level > 0 || !at.texts.is_empty() || !at.tables.is_empty() {
        out.push(at);
    }
    for (i, part) in out.iter_mut().enumerate() {
        part.n = i + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_is_cut_at_its_headings_each_with_the_path_down_to_it() {
        let blocks = reader::from_markdown("Intro line.\n\n# API\n\nText.\n\n## Fields\n\n| Name | Type |\n|---|---|\n| a | int |\n\n# Other\n\nMore.\n");
        let parts = parts(&blocks);
        let seen: Vec<(usize, usize, &str, &str)> = parts.iter().map(|p| (p.n, p.level, p.heading.as_str(), p.path.as_str())).collect();
        assert_eq!(seen, [(1, 0, "", ""), (2, 1, "API", "API"), (3, 2, "Fields", "API > Fields"), (4, 1, "Other", "Other")]);
        assert_eq!(parts[0].texts.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["Intro line."]);
        assert_eq!(parts[2].tables.len(), 1);
        assert!(parts[2].tables[0].1, "a Markdown table's first row is its header");
        assert_eq!(parts[2].tables[0].0[1].1, ["a", "int"]);
    }
}
