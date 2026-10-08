//! THE STYLESHEETS OF AN ANGULAR WORKSPACE, AS RULES - `stylesheets`, `style_rules`, `class_hides`.
//!
//! The Angular half carried every `.scss` as TEXT and nothing parsed it, so a map that records
//! `[class.not-visible]="!shown"` as a gate could not say that the class hides the element: only the stylesheet
//! says that. These three tables are what it says, read by a PARSER (`raffia`: CSS, SCSS, Sass, Less).
//!
//! WHY HERE AND NOT IN NODE. The Angular half borrows its compilers from the workspace because a template's
//! grammar moves with Angular's major; a stylesheet's does not, and the workspace holds no stylesheet PARSER to
//! borrow - `postcss-scss` is no Angular dependency, and `sass` offers only a compile that needs every `@use`
//! resolved. Like markdown, a language with no host of its own is parsed in rust.
//!
//! THE TABLES ARE WHOLE. They are derived after node's rows are written, on a full run and on a partial one,
//! from every stylesheet `files` row of the half - well over a thousand on a large Angular workspace, a fraction of a second - so no partial
//! path can keep a stale rule beside a fresh one. None has an `owner_file`: `rows::partial::carry` hands such a
//! table back to nobody, and every run deletes its rows here before writing them again. A change to what they
//! say is a change to what the half writes: bump `rows` in `src/TsRows/TsMap.mjs`, or an unchanged workspace
//! keeps the rows an older exe wrote.
//!
//! A SHEET THE PARSER CANNOT READ SAYS SO, and has no rows: `stylesheets.error` and a `diagnostics` row of kind
//! `stylesheet_unparsed`. Half a sheet is not a reading of it.

mod hides;
mod select;
mod sheet;
#[cfg(test)]
mod tests;

use crate::rows::store::{self, WriteRequest};
use anyhow::Result;
use rayon::prelude::*;
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The tables this pass owns, deleted and written again by every run of the half.
const TABLES: &[&str] = &["stylesheets", "style_rules", "class_hides"];

/// The kind of the `diagnostics` row a sheet that did not parse gets.
const UNPARSED: &str = "stylesheet_unparsed";

/// The rows of every table this pass writes, and what is worth saying about them.
struct Derived {
    tables: Map<String, Value>,
    notes: Vec<String>,
}

fn int(flag: bool) -> Value {
    Value::from(i64::from(flag))
}

fn opt(text: Option<String>) -> Value {
    text.map(Value::String).unwrap_or(Value::Null)
}

/// Every stylesheet of the half - `(id, path, syntax)`, in path order so the ids come out the same each run.
fn sheets(db: &Connection, half: &str) -> Result<Vec<(String, String, raffia::Syntax)>> {
    let mut stmt = db.prepare("SELECT id, path, ext FROM files WHERE half = ?1 ORDER BY path")?;
    let rows = stmt.query_map([half], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?))
    })?;
    let mut out = Vec::new();
    for (id, path, ext) in rows.flatten() {
        let (Some(id), Some(path)) = (id, path) else { continue };
        let ext = ext.unwrap_or_default().trim_start_matches('.').to_ascii_lowercase();
        // A FILE OUTSIDE THE WORKSPACE IS NOT THIS APPLICATION'S - the same rule `file_text` keeps.
        if path.starts_with("..") {
            continue;
        }
        if let Some(syntax) = sheet::syntax_of(&ext) {
            out.push((id, path, syntax));
        }
    }
    Ok(out)
}

/// One file read and parsed. A panic inside the parser is that file's error, not the map's.
fn read_one(workspace: &Path, path: &str, syntax: raffia::Syntax) -> sheet::Sheet {
    let bytes = match std::fs::read(workspace.join(path)) {
        Ok(bytes) => bytes,
        Err(e) => return sheet::Sheet { error: Some((format!("unreadable: {e}"), 0)), ..Default::default() },
    };
    let text = String::from_utf8_lossy(&bytes);
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sheet::read(&text, syntax))).unwrap_or_else(|_| {
        sheet::Sheet { error: Some(("the stylesheet parser panicked".to_string(), 0)), ..Default::default() }
    })
}

fn derive(db: &Connection, workspace: &Path, half: &str) -> Result<Derived> {
    let files = sheets(db, half)?;
    let read: Vec<sheet::Sheet> = files.par_iter().map(|(_, path, syntax)| read_one(workspace, path, *syntax)).collect();
    let (mut sheet_rows, mut rule_rows, mut diagnostics) = (Vec::new(), Vec::new(), Vec::new());
    let mut unparsed: Vec<String> = Vec::new();
    for ((file, path, syntax), parsed) in files.iter().zip(read) {
        let mut sheet_row = Map::new();
        sheet_row.insert("id".into(), Value::String(format!("cssf:{}", sheet_rows.len() + 1)));
        sheet_row.insert("file".into(), Value::String(file.clone()));
        sheet_row.insert("syntax".into(), Value::String(format!("{syntax:?}").to_ascii_lowercase()));
        sheet_row.insert("declarations".into(), Value::from(parsed.decls.len()));
        sheet_row.insert("recovered".into(), Value::from(parsed.recovered));
        sheet_row.insert("error".into(), opt(parsed.error.as_ref().map(|e| e.0.clone())));
        sheet_row.insert("error_line".into(), parsed.error.as_ref().map(|e| Value::from(e.1)).unwrap_or(Value::Null));
        sheet_rows.push(Value::Object(sheet_row));
        if let Some((message, line)) = &parsed.error {
            unparsed.push(format!("{path}:{line}: {message}"));
            let mut row = Map::new();
            row.insert("kind".into(), Value::String(UNPARSED.into()));
            row.insert("file".into(), Value::String(file.clone()));
            row.insert("path".into(), Value::String(path.clone()));
            row.insert("line".into(), Value::from(*line));
            row.insert("message".into(), Value::String(message.clone()));
            diagnostics.push(Value::Object(row));
        }
        for decl in parsed.decls {
            let subject = select::subject(&decl.resolved, *syntax);
            let mut row = Map::new();
            row.insert("id".into(), Value::String(format!("css:{}", rule_rows.len() + 1)));
            row.insert("file".into(), Value::String(file.clone()));
            row.insert("line".into(), Value::from(decl.line));
            row.insert("col".into(), Value::from(decl.col));
            row.insert("rule_line".into(), Value::from(decl.rule_line));
            row.insert("selector".into(), Value::String(decl.selector));
            row.insert("resolved".into(), Value::String(decl.resolved));
            row.insert("subject".into(), opt(subject.as_ref().map(|s| s.text.clone())));
            row.insert("classes".into(), Value::from(subject.as_ref().map(|s| s.classes.clone()).unwrap_or_default()));
            row.insert("on_element".into(), subject.as_ref().map(|s| int(s.on_element)).unwrap_or(Value::Null));
            row.insert("media".into(), opt(decl.media));
            row.insert("context".into(), opt(decl.context));
            row.insert("property".into(), Value::String(decl.property));
            row.insert("value".into(), Value::String(decl.value));
            row.insert("important".into(), int(decl.important));
            row.insert("hides".into(), int(decl.hides));
            row.insert("live".into(), int(decl.live));
            rule_rows.push(Value::Object(row));
        }
    }
    let joined = hides::join(db, half, &rule_rows);
    let mut notes = vec![format!(
        "the typescript half read {} stylesheet(s) into {} rule declaration(s), {} of them hiding; {} class binding \
         match(es) to a hiding rule",
        files.len(),
        rule_rows.len(),
        rule_rows.iter().filter(|r| r.get("hides").and_then(Value::as_i64) == Some(1)).count(),
        joined.len()
    )];
    if let Some(first) = unparsed.first() {
        notes.push(format!("the typescript half could not parse {} stylesheet(s), first {first}", unparsed.len()));
    }
    let mut tables = Map::new();
    tables.insert("stylesheets".into(), Value::Array(sheet_rows));
    tables.insert("style_rules".into(), Value::Array(rule_rows));
    tables.insert("class_hides".into(), Value::Array(joined));
    if !diagnostics.is_empty() {
        tables.insert("diagnostics".into(), Value::Array(diagnostics));
    }
    Ok(Derived { tables, notes })
}

/// DERIVE AND STORE, after node's rows are in: the old rows of this pass go first, whatever kind of run left
/// them, and a `diagnostics` row only by its own kind - the rest of that table is node's.
pub fn store(db_path: &Path, root: &str, workspace: &Path, half: &str, notes: &mut Vec<String>) -> Result<BTreeMap<String, i64>> {
    let derived = {
        let db = Connection::open(db_path)?;
        // A TABLE NOT THERE YET HAS NOTHING TO DELETE - the first run of this pass over a database.
        for table in TABLES {
            let _ = db.execute(&format!("DELETE FROM \"{table}\" WHERE half = ?1"), [half]);
        }
        let _ = db.execute("DELETE FROM diagnostics WHERE half = ?1 AND kind = ?2", [half, UNPARSED]);
        derive(&db, workspace, half)?
    };
    notes.extend(derived.notes);
    let applied = store::write(db_path, &WriteRequest {
        tables: &derived.tables,
        texts: &store::Texts::Held(&BTreeMap::new()),
        root,
        shas: &Map::new(),
        gone: &BTreeSet::new(),
        full: false,
        counters: &Map::new(),
        lang: half,
        half: Some(half),
        setup: None,
        extra: &BTreeMap::new(),
        keep_existing: true,
        counts: true,
    })?;
    Ok(applied.written)
}
