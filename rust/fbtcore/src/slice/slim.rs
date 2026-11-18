//! `--map-slim <base> --slim-config <json> --map-sqlite <out>`: A SMALL MAP THAT HOLDS WHAT A CONFIG PICKS.
//!
//! A slice (`mod.rs`) cuts a full map by FILES; a review map that goes everywhere - an artifact a CI job uploads, a database a reviewer's
//! agent opens - wants every file and only some TABLES, and some rows of those. Nothing is re-extracted: the slim map is copied out of a
//! full map that is already current, through an attached connection, so it cannot disagree with it.
//!
//! The config (`structuregate.slim.json`), every name checked against the base's own schema - a name the base does not hold is an error,
//! so a list kept by hand cannot drift from what the map writes:
//!
//! - `tables`: the tables kept, each with its node tables (`<table>__<column>`) and its indexes; a full-text table (`file_text`) with its
//!   shadow tables, copied as stored. `_meta` is always kept. A view of the base is kept when every table it reads was.
//! - `where`: `{ table: condition }` - a kept table holds only the rows the SQL condition holds of (read in the BASE's schema); its node
//!   tables follow the rows kept.
//! - `derived`: tables made from the base that it does not hold as such - `ts_reads` (`slimreads.rs`).
//!
//! THE MARK: `_meta` `partial` = 1 (the deep map refuses to write into it, as into a slice), `slim:of` (the base, absolute), `slim:at`
//! (when), `slim:tables` (what it kept); `options:text` = 0 when `file_text` was left out. `--map-query` says on every question that a
//! table missing here was left out, not that the tree has no such rows.

use super::copy::{columns, quoted};
use crate::cli::Out;
use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What the config asks for, read and checked.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    tables: Vec<String>,
    #[serde(default, rename = "where")]
    filters: BTreeMap<String, String>,
    #[serde(default)]
    derived: Vec<String>,
}

/// The tables a config can derive: name, and what makes it.
const DERIVED: [(&str, fn(&Connection) -> Result<usize>); 1] = [("ts_reads", super::slimreads::write)];

/// The slim map, written; the exit code (0, or 2 with the reason said).
pub fn run(base: &str, config: &str, out_db: &str, out: &mut Out) -> i64 {
    match slim(base, config, out_db) {
        Ok(said) => {
            out.line(format!("slim      {said} -> {out_db}"));
            0
        }
        Err(e) => {
            out.error(format!("structuregate: the slim map failed - {e:#}"));
            2
        }
    }
}

fn slim(base: &str, config: &str, out_db: &str) -> Result<String> {
    if !Path::new(base).is_file() {
        bail!("no base map at {base} - build it with --map-sqlite first");
    }
    if Path::new(out_db) == Path::new(base) {
        bail!("the slim map cannot be written over its base map");
    }
    let text = std::fs::read_to_string(config).with_context(|| format!("read {config}"))?;
    let config: Config = serde_json::from_str(&text).with_context(|| format!("{config} is no slim config"))?;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(format!("{out_db}{suffix}"));
    }
    let db = Connection::open(out_db).with_context(|| format!("open {out_db}"))?;
    db.execute("ATTACH DATABASE ?1 AS b", [base])?;
    let schema: Vec<(String, String, String, String)> = {
        let mut stmt = db.prepare("SELECT type, name, tbl_name, sql FROM b.sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let tables: BTreeMap<&str, &str> = schema.iter().filter(|(t, ..)| t == "table").map(|(_, n, _, s)| (n.as_str(), s.as_str())).collect();
    let virtual_table = |name: &str| tables.get(name).is_some_and(|sql| sql.trim_start().to_uppercase().starts_with("CREATE VIRTUAL"));
    let mut asked: BTreeSet<&str> = config.tables.iter().map(String::as_str).collect();
    asked.insert("_meta");
    let unknown: Vec<&str> = asked.iter().copied().filter(|t| !tables.contains_key(t)).collect();
    if !unknown.is_empty() {
        bail!("the base map holds no table {}", unknown.join(", "));
    }
    if let Some(t) = config.filters.keys().find(|t| !asked.contains(t.as_str())) {
        bail!("`where` names {t}, which `tables` does not keep");
    }
    if let Some(d) = config.derived.iter().find(|d| !DERIVED.iter().any(|(n, _)| n == d)) {
        bail!("no derived table {d} (one of: {})", DERIVED.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "));
    }
    // EVERY TABLE MADE: the kept ones, their node tables, and a full-text table's shadows (`<name>_data`, ...).
    let owned = |name: &str, by: &str| name.strip_prefix(by).is_some_and(|rest| rest.starts_with("__") || (virtual_table(by) && rest.starts_with('_')));
    let made: Vec<&str> = tables.keys().copied().filter(|n| asked.contains(n) || asked.iter().any(|a| owned(n, a))).collect();
    db.execute_batch("BEGIN;")?;
    let mut rows = 0usize;
    for name in made.iter().filter(|n| !n.contains("__") && !asked.iter().any(|a| owned(n, a))) {
        db.execute_batch(tables[name]).with_context(|| format!("create {name}"))?;
        if virtual_table(name) {
            // A FULL-TEXT TABLE IS ITS SHADOWS: they are made with it, and their rows - the index as built - are copied as stored.
            for shadow in made.iter().filter(|s| owned(s, name)) {
                db.execute_batch(&format!("DELETE FROM main.\"{shadow}\"; INSERT INTO main.\"{shadow}\" SELECT * FROM b.\"{shadow}\";"))
                    .with_context(|| format!("copy {shadow}"))?;
            }
            continue;
        }
        let filter = config.filters.get(*name).map(|c| format!(" WHERE ({c})")).unwrap_or_default();
        let rowid = !tables[name].to_uppercase().contains("WITHOUT ROWID");
        let cols = quoted(&columns(&db, "b", name)?);
        let both = if rowid { format!("rowid, {cols}") } else { cols };
        rows += db.execute(&format!("INSERT INTO main.\"{name}\" ({both}) SELECT {both} FROM b.\"{name}\"{filter}"), [])
            .with_context(|| format!("copy {name}{filter}"))?;
        for node in made.iter().filter(|n| owned(n, name) && n.contains("__")) {
            db.execute_batch(tables[node]).with_context(|| format!("create {node}"))?;
            let held = quoted(&columns(&db, "b", node)?);
            // THE NODE ROWS OF THE ROWS KEPT: an owner is its row's `id`, or `#<rowid>` where a row has none.
            let keep = if filter.is_empty() {
                String::new()
            } else {
                let by_id = if columns(&db, "b", name)?.iter().any(|c| c == "id") { format!("owner IN (SELECT id FROM main.\"{name}\") OR ") } else { String::new() };
                format!(" WHERE {by_id}owner IN (SELECT '#' || rowid FROM main.\"{name}\")")
            };
            db.execute_batch(&format!("INSERT INTO main.\"{node}\" ({held}) SELECT {held} FROM b.\"{node}\"{keep};")).with_context(|| format!("copy {node}"))?;
        }
    }
    for (kind, name, on, sql) in &schema {
        if kind == "index" && made.contains(&on.as_str()) && !virtual_table(on) && !made.iter().any(|m| virtual_table(m) && owned(on, m)) {
            db.execute_batch(sql).with_context(|| format!("index {name}"))?;
        }
    }
    let mut derived = Vec::new();
    for name in &config.derived {
        let (_, make) = DERIVED.iter().find(|(n, _)| n == name).expect("checked above");
        derived.push(format!("{name} {}", make(&db)?));
    }
    // A VIEW OF THE BASE stays when what it reads was kept: made, then asked once for nothing - a view over a table left out fails there.
    let mut views = 0usize;
    for (kind, name, _, sql) in &schema {
        if kind != "view" {
            continue;
        }
        db.execute_batch(&format!("SAVEPOINT view; {sql};"))?;
        if db.prepare(&format!("SELECT * FROM main.\"{name}\" LIMIT 0")).is_ok() {
            db.execute_batch("RELEASE view;")?;
            views += 1;
        } else {
            db.execute_batch("ROLLBACK TO view; RELEASE view;")?;
        }
    }
    db.execute_batch("COMMIT; DETACH DATABASE b;")?;
    let kept: Vec<&str> = asked.iter().copied().collect();
    crate::rows::meta::put(&db, "partial", "1")?;
    crate::rows::meta::put(&db, "slim:of", &std::fs::canonicalize(base)?.to_string_lossy().replace('\\', "/"))?;
    crate::rows::meta::put(&db, "slim:at", &crate::rows::keys::source::now_utc())?;
    crate::rows::meta::put_value(&db, "slim:tables", &serde_json::json!(kept))?;
    if !asked.contains("file_text") {
        crate::rows::meta::put(&db, crate::rows::cells::text::KEY, "0")?;
    }
    let derived = if derived.is_empty() { String::new() } else { format!(", derived {}", derived.join(", ")) };
    Ok(format!("{} table(s) of {}, {rows} row(s), {views} view(s){derived}", kept.len(), tables.keys().filter(|n| !n.contains("__")).count()))
}

/// What `--map-query` says of a slim map on every question; None for any other map.
pub fn note(c: &Connection) -> Option<String> {
    let of = crate::rows::meta::get(c, "slim:of")?;
    Some(format!("slim map of {of}: it keeps only the tables its config picked (`_meta` `slim:tables`) - a table or row missing here was left out, not absent from the tree"))
}
