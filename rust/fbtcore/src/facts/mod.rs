//! FACTS FROM DOCUMENTS OUTSIDE THE TREE, checked against the code. A published document lists item
//! codes the code seeds into a table; when an item is added, nothing told anybody the document is now behind.
//!
//! THE TOOL GIVES THE MEANS, THE CONSUMER THE CONFIGURATION (`config.rs`): `--facts-pull` saves each document as
//! a Markdown snapshot (`pull.rs`, the only part that touches the network), and every map run reads the
//! snapshots (`reader.rs`; a Word file by `docx.rs`) into `doc_facts` - a table's rows, or the code spans and routes
//! a text names (`words.rs`) - and joins each declared list to its code (`links.rs`, `apis.rs`) in `doc_links`:
//! `bound`, `missing in code` (in the document, not in the code) or `missing in doc` (in the code, not in the
//! document). Every section of every source is a row too (`sections.rs`: `doc_sections`, `doc_tables`). Which code rows MUST be documented stays the consumer's rule - a query over `doc_links`.
//!
//! `--facts-suggest` (`suggest.rs`) finds which part of a document lists what the map holds, by exact values, and
//! writes candidates - it never writes the config.
//!
//! It is one of THE PASSES OVER THE FINISHED ROWS (`mapper/deep/derived.rs`): the config and every snapshot are
//! in their key, so an unchanged document costs nothing on a run that changed nothing.

mod apis;
mod asking;
pub mod config;
#[path = "snapshot/docx.rs"]
mod docx;
mod generics;
mod links;
pub mod pull;
mod rank;
mod uses;
pub mod review;
#[path = "snapshot/reader.rs"]
mod reader;
pub mod suggest;
#[path = "snapshot/sections.rs"]
mod sections;
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
         CREATE TABLE doc_links (id, facts, kind, fact, key, label, target, file, line, value, name, condition, status, nesting);",
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

    let mut insert = conn.prepare("INSERT INTO doc_links VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)")?;
    let mut n = 0;
    // EVERY LINK OF ONE LIST TOGETHER: a table describing a response and the types nested in it is linked to each of
    // them, and a row only one of them holds is in the code - it is `missing in code` only when no link binds it.
    let mut done: HashSet<&str> = HashSet::new();
    for first in &config.links {
        if !done.insert(first.facts.as_str()) {
            continue;
        }
        let Some((_, rows)) = lists.iter().find(|(name, _)| *name == first.facts) else { continue };
        let linked: Vec<(&config::Link, Vec<links::Code>)> = config
            .links
            .iter()
            .filter(|l| l.facts == first.facts)
            .map(|link| {
                let codes = links::codes(conn, link).unwrap_or_else(|why| {
                    made.notes.push(format!("the link from `{}`: {why}, so none of its facts is bound by it", link.facts));
                    Vec::new()
                });
                (link, codes)
            })
            .collect();
        let mut matched: Vec<HashSet<usize>> = vec![HashSet::new(); linked.len()];
        // EACH ROW'S TYPE, by its depth: a child row (`-> Name`) is first a property of the type its parent row's
        // property holds - linked or not (an array's, a list's element) - and only then one of the linked types.
        let typed = linked.iter().any(|(l, _)| l.kind == "property").then(|| apis::Properties::read(conn)).transpose()?;
        let in_type = |t: &str, name: &str| -> Vec<links::Code> {
            typed.as_ref().map(|p| p.codes(&apis::element(t), "").into_iter().filter(|c| links::matches("property", &c.value, name)).collect()).unwrap_or_default()
        };
        let mut parents: Vec<(usize, String)> = Vec::new();
        for (fact, key, label) in rows {
            let (depth, name) = nested(key);
            let open = parents.clone();
            let parent = open.iter().rev().find(|(d, _)| *d < depth).filter(|_| depth > 0);
            let mut hits = parent.filter(|(_, t)| !t.is_empty()).map(|(_, t)| in_type(t, name)).unwrap_or_default();
            // A ROW THE DOCUMENT NESTED WRONG - an arrow too few, one too many, none between child rows - is read in
            // another open row's type, nearest first, and said so (`nesting`: the depth its arrows should have shown).
            let linked_has = linked.iter().any(|(link, codes)| codes.iter().any(|c| links::matches(&link.kind, &c.value, name)));
            let mut fixed: Option<usize> = None;
            if hits.is_empty() && (depth > 0 || !linked_has) {
                for (d, t) in open.iter().rev().filter(|(d, t)| !t.is_empty() && *d + 1 != depth) {
                    hits = in_type(t, name);
                    if !hits.is_empty() {
                        fixed = Some(d + 1);
                        break;
                    }
                }
            }
            let (mut bound, mut own) = (false, String::new());
            let status = if fixed.is_some() { "bound, nesting fixed" } else { "bound" };
            for c in &hits {
                if !bound {
                    own = c.typed.clone();
                }
                bound = true;
                // A LINKED TYPE'S PROPERTY a child row names is documented there too.
                for (l, (_, codes)) in linked.iter().enumerate() {
                    matched[l].extend(codes.iter().enumerate().filter(|(_, x)| x.target == c.target).map(|(i, _)| i));
                }
                n += 1;
                made.links[0] += 1;
                insert.execute(params![format!("dl:{n}"), first.facts, "property", fact, key, label, c.target, c.file, c.line, c.value, c.name, c.condition, status, fixed.map(|d| d as i64)])?;
            }
            let read_already = bound;
            for (l, (link, codes)) in linked.iter().enumerate().filter(|_| !read_already) {
                for (i, c) in codes.iter().enumerate().filter(|(_, c)| links::matches(&link.kind, &c.value, name)) {
                    if !bound {
                        own = c.typed.clone();
                    }
                    bound = true;
                    matched[l].insert(i);
                    n += 1;
                    made.links[0] += 1;
                    insert.execute(params![format!("dl:{n}"), link.facts, link.kind, fact, key, label, c.target, c.file, c.line, c.value, c.name, c.condition, "bound", None::<i64>])?;
                }
            }
            // THE ROW STANDS WHERE IT WAS READ: the rows after it nest under it at that depth.
            let at = fixed.unwrap_or(depth);
            parents = open.into_iter().filter(|(d, _)| *d < at).collect();
            parents.push((at, own));
            if !bound {
                n += 1;
                made.links[1] += 1;
                insert.execute(params![format!("dl:{n}"), first.facts, first.kind, fact, key, label, "", "", 0, "", "", "", "missing in code", None::<i64>])?;
            }
        }
        for (l, (link, codes)) in linked.iter().enumerate() {
            for c in codes.iter().enumerate().filter(|(i, _)| !matched[l].contains(i)).map(|(_, c)| c) {
                n += 1;
                made.links[2] += 1;
                insert.execute(params![format!("dl:{n}"), link.facts, link.kind, "", "", "", c.target, c.file, c.line, c.value, c.name, c.condition, "missing in doc", None::<i64>])?;
            }
        }
    }
    conn.execute_batch("CREATE INDEX ix_doc_links_status ON doc_links(status); CREATE INDEX ix_doc_links_target ON doc_links(target);")?;
    let parts = sections::write(conn, &config.sources)?;
    made.notes.extend(parts.notes);
    made.notes.push(format!("the documents: {} section(s) in doc_sections, {} table cell(s) in doc_tables", parts.sections, parts.cells));
    Ok(made)
}

/// A documented key's depth and the name it gives: a row written `-> Name` is a child of the nearest row above it one
/// level up, `-> -> Name` a grandchild - the arrows (`->`, `–>`, `—>`, `→`, spaced or not) only its indent.
fn nested(key: &str) -> (usize, &str) {
    let (mut depth, mut rest) = (0, key.trim());
    while let Some(after) = ["->", "–>", "—>", "→"].iter().find_map(|arrow| rest.strip_prefix(arrow)) {
        depth += 1;
        rest = after.trim_start();
    }
    (depth, rest)
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
