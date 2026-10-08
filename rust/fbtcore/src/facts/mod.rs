//! FACTS FROM DOCUMENTS OUTSIDE THE TREE, checked against the code. A published document lists item
//! codes the code seeds into a table; when an item is added, nothing told anybody the document is now behind.
//!
//! THE TOOL GIVES THE MEANS, THE CONSUMER THE CONFIGURATION (`config.rs`): `--facts-pull` saves each document as
//! a Markdown snapshot (`pull.rs`, the only part that touches the network), and every map run reads the
//! snapshots (`reader.rs`) into `doc_facts` and joins each declared list to its code (`links.rs`) in `doc_links`:
//! `bound`, `missing in code` (in the document, not in the code) or `missing in doc` (in the code, not in the
//! document). Which code rows MUST be documented stays the consumer's rule - a query over `doc_links`.
//!
//! It is one of THE PASSES OVER THE FINISHED ROWS (`mapper/deep/derived.rs`): the config and every snapshot are
//! in their key, so an unchanged document costs nothing on a run that changed nothing.

pub mod config;
mod links;
pub mod pull;
mod reader;
mod table;

use config::Config;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashSet;

/// What a pass made: rows per table and the notes worth saying.
pub struct Made {
    pub facts: usize,
    pub links: [usize; 3],
    pub notes: Vec<String>,
}

/// `doc_facts` and `doc_links` rewritten whole. A source, section or link that cannot be read is a NOTE - the
/// others are still checked - and the rows of a link whose code is not in the map are `missing in code`.
pub fn run(db: &str, config: &Config) -> Result<Made, String> {
    let mut conn = Connection::open(db).map_err(|e| format!("the database could not be opened - {e}"))?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let made = write(&tx, config).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(made)
}

fn write(conn: &Connection, config: &Config) -> rusqlite::Result<Made> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS doc_facts; DROP TABLE IF EXISTS doc_links;
         CREATE TABLE doc_facts (id, facts, source, section, subsection, key, label, cells, file, line, version);
         CREATE TABLE doc_links (id, facts, kind, fact, key, label, target, file, line, value, name, condition, status);",
    )?;
    let mut made = Made { facts: 0, links: [0; 3], notes: Vec::new() };
    // EACH LIST'S ROWS, kept for its links: (fact id, key, label).
    let mut lists: Vec<(String, Vec<(String, String, String)>)> = Vec::new();
    let mut insert = conn.prepare("INSERT INTO doc_facts VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)")?;
    for facts in &config.facts {
        let Some(source) = config.sources.iter().find(|s| s.name == facts.source) else { continue };
        let Ok(text) = std::fs::read(&source.path) else {
            made.notes.push(format!("the facts `{}` were not read: `{}` has no snapshot at {} - run --facts-pull", facts.name, source.name, source.shown));
            continue;
        };
        let text = String::from_utf8_lossy(text.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&text)).into_owned();
        let version = version(&source.path);
        let blocks = reader::blocks(&source.path, &text);
        let read = reader::section(&blocks, &facts.section, facts.table).and_then(|found| table::rows(&found, &facts.key, &facts.label));
        let found = match read {
            Ok(found) => found,
            Err(why) => {
                made.notes.push(format!("the facts `{}` were not read from {}: {why}", facts.name, source.shown));
                continue;
            }
        };
        let mut rows = Vec::new();
        for row in found {
            made.facts += 1;
            let id = format!("df:{}", made.facts);
            insert.execute(params![
                id, facts.name, source.name, facts.section, row.subsection, row.key, row.label, Value::Object(row.cells).to_string(),
                source.shown, row.at as i64, version,
            ])?;
            rows.push((id, row.key, row.label));
        }
        lists.push((facts.name.clone(), rows));
    }

    let mut insert = conn.prepare("INSERT INTO doc_links VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)")?;
    let mut n = 0;
    for link in &config.links {
        let Some((_, rows)) = lists.iter().find(|(name, _)| *name == link.facts) else { continue };
        let codes = match links::codes(conn, link) {
            Ok(codes) => codes,
            Err(why) => {
                made.notes.push(format!("the link from `{}`: {why}, so every fact in it is `missing in code`", link.facts));
                Vec::new()
            }
        };
        let mut matched: HashSet<usize> = HashSet::new();
        for (fact, key, label) in rows {
            let hits: Vec<usize> = (0..codes.len()).filter(|&i| links::same(&codes[i].value, key)).collect();
            if hits.is_empty() {
                n += 1;
                made.links[1] += 1;
                insert.execute(params![format!("dl:{n}"), link.facts, link.kind, fact, key, label, "", "", 0, "", "", "", "missing in code"])?;
            }
            for i in hits {
                matched.insert(i);
                let c = &codes[i];
                n += 1;
                made.links[0] += 1;
                insert.execute(params![format!("dl:{n}"), link.facts, link.kind, fact, key, label, c.target, c.file, c.line, c.value, c.name, c.condition, "bound"])?;
            }
        }
        for c in codes.iter().enumerate().filter(|(i, _)| !matched.contains(i)).map(|(_, c)| c) {
            n += 1;
            made.links[2] += 1;
            insert.execute(params![format!("dl:{n}"), link.facts, link.kind, "", "", "", c.target, c.file, c.line, c.value, c.name, c.condition, "missing in doc"])?;
        }
    }
    conn.execute_batch("CREATE INDEX ix_doc_links_status ON doc_links(status); CREATE INDEX ix_doc_links_target ON doc_links(target);")?;
    Ok(made)
}

/// The document version `--facts-pull` recorded beside the snapshot, or "" for a snapshot kept by hand.
fn version(snapshot: &std::path::Path) -> String {
    let meta = pull::meta_path(snapshot);
    std::fs::read_to_string(meta)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|m| match &m["version"] {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}
