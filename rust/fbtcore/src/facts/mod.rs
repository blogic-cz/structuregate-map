//! FACTS FROM DOCUMENTS OUTSIDE THE TREE, checked against the code. A published document lists item
//! codes the code seeds into a table; when an item is added, nothing told anybody the document is now behind.
//!
//! THE TOOL GIVES THE MEANS, THE CONSUMER THE CONFIGURATION (`config.rs`): `--facts-pull` saves each document as
//! a Markdown snapshot (`pull.rs`, the only part that touches the network), and every map run reads the
//! snapshots (`reader.rs`; a Word file by `docx.rs`) into `doc_facts` - a table's rows, or the code spans and routes
//! a text names (`words.rs`) - and joins each declared list to its code (`links.rs`, `apis.rs`) in `doc_links`:
//! `bound`, `missing in code` (in the document, not in the code) or `missing in doc` (in the code, not in the
//! document). Which code rows MUST be documented stays the consumer's rule - a query over `doc_links`.
//!
//! `--facts-suggest` (`suggest.rs`) finds which part of a document lists what the map holds, by exact values, and
//! writes candidates - it never writes the config.
//!
//! It is one of THE PASSES OVER THE FINISHED ROWS (`mapper/deep/derived.rs`): the config and every snapshot are
//! in their key, so an unchanged document costs nothing on a run that changed nothing.

mod apis;
pub mod config;
#[path = "snapshot/docx.rs"]
mod docx;
mod links;
pub mod pull;
pub mod review;
#[path = "snapshot/reader.rs"]
mod reader;
pub mod suggest;
#[path = "snapshot/table.rs"]
mod table;
#[path = "snapshot/words.rs"]
mod words;

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
    // A fact's `cells` is an object, so nodes (`doc_facts__cells`): dropped with the table it belongs to.
    crate::rows::cells::drop_children(conn, "doc_facts").map_err(failed)?;
    crate::rows::cells::drop_children(conn, "doc_links").map_err(failed)?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS doc_facts; DROP TABLE IF EXISTS doc_links;
         CREATE TABLE doc_facts (id, facts, source, section, subsection, key, label, file, line, version);
         CREATE TABLE doc_links (id, facts, kind, fact, key, label, target, file, line, value, name, condition, status);",
    )?;
    let mut made = Made { facts: 0, links: [0; 3], notes: Vec::new() };
    // EACH LIST'S ROWS, kept for its links: (fact id, key, label).
    let mut lists: Vec<(String, Vec<(String, String, String)>)> = Vec::new();
    let mut written: Vec<Value> = Vec::new();
    for facts in &config.facts {
        let Some(source) = config.sources.iter().find(|s| s.name == facts.source) else { continue };
        let Ok(text) = std::fs::read(&source.path) else {
            made.notes.push(format!("the facts `{}` were not read: `{}` has no snapshot at {} - run --facts-pull", facts.name, source.name, source.shown));
            continue;
        };
        let version = version(&source.path);
        let read = reader::blocks(&source.path, &text).and_then(|blocks| read_facts(&blocks, facts));
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
            written.push(serde_json::json!({
                "id": id, "facts": facts.name, "source": source.name, "section": facts.section, "subsection": row.subsection,
                "key": row.key, "label": row.label, "cells": Value::Object(row.cells), "file": source.shown,
                "line": row.at as i64, "version": version,
            }));
            rows.push((id, row.key, row.label));
        }
        lists.push((facts.name.clone(), rows));
    }
    crate::rows::store::insert_rows(conn, "doc_facts", &written).map_err(failed)?;

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
            let hits: Vec<usize> = (0..codes.len()).filter(|&i| links::matches(&link.kind, &codes[i].value, key)).collect();
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

/// The rows of one list: a table's, or each code span or route of the section as a row with only its key.
fn read_facts(blocks: &[reader::Block], facts: &config::Facts) -> Result<Vec<table::Row>, String> {
    if facts.from == "table" {
        return reader::section(blocks, &facts.section, facts.table).and_then(|found| table::rows(&found, &facts.key, &facts.label));
    }
    let texts = reader::texts(blocks, &facts.section)?;
    let words = if facts.from == "code" { words::code_spans(&texts) } else { words::routes(&texts, &facts.prefix) };
    Ok(words.into_iter().map(|w| table::Row { at: w.at, subsection: w.subsection, key: w.key, label: String::new(), cells: serde_json::Map::new() }).collect())
}

fn failed(e: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{e:#}"))
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
