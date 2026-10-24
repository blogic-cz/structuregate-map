//! `--map-slice <base> --slice-files <file|@list> [--slice-hops N] --map-sqlite <out>`: A PARTIAL DEEP MAP FOR A REVIEW (#111).
//!
//! A pull-request review needs the rows around a diff - who calls a changed method, which request reaches a changed
//! endpoint, which statement reads a changed table - and a full map of a large tree is gigabytes. A map that WROTE only
//! some files' rows would break the store's own rule (every row belongs to a file whose recorded sha matches the tree),
//! and a later incremental run would read the missing rows as "unchanged, nothing there". So nothing is extracted here:
//! the slice is CUT out of a full map that is already current (CI keeps one and refreshes it incrementally).
//!
//! THE FILES: the listed ones (map paths, as `files.path` spells them), plus their DEPENDENTS - the files whose rows a
//! RESOLVED link points into them (`links.rs`: a call's or literal's target, an import's file, a C# `file_refs` row, an
//! `http_links` or `sql_links` row), `--slice-hops` deep (default 1), never by a name searched for. THE ROWS
//! (`copy.rs`): every per-file table's rows of those files with their node rows, every whole-program table whole, so
//! no closure, graph or link table is truncated. THE MARK: `_meta` `partial` = 1, `slice:files`, `slice:hops`,
//! `slice:base`; `files.sliced` says which files are in it. `--map-query` says so on every question, and the deep map
//! refuses to write into a slice (`is_slice`).

mod copy;
mod links;

use crate::cli::Out;
use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::Path;

/// The slice, cut and written; the exit code (0, or 2 with the reason said).
pub fn run(base: &str, files: &str, hops: usize, out_db: &str, out: &mut Out) -> i64 {
    match cut(base, files, hops, out_db) {
        Ok((kept, of)) => {
            out.line(format!("slice     {kept} of {of} file(s), {hops} hop(s) from the listed ones -> {out_db}"));
            0
        }
        Err(e) => {
            out.error(format!("structuregate: the slice failed - {e:#}"));
            2
        }
    }
}

fn cut(base: &str, files: &str, hops: usize, out_db: &str) -> Result<(usize, usize)> {
    if !Path::new(base).is_file() {
        bail!("no base map at {base} - build it with --map-sqlite first");
    }
    if Path::new(out_db) == Path::new(base) {
        bail!("the slice cannot be written over its base map");
    }
    let listed = listed(files)?;
    let db = Connection::open_with_flags(base, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {base}"))?;
    let (ids, unknown) = links::resolve(&db, &listed)?;
    if !unknown.is_empty() {
        bail!("not a file of the base map: {} (paths as `files.path` spells them)", unknown.join(", "));
    }
    let kept = links::dependents(&db, &ids, hops)?;
    let of: usize = db.query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0))? as usize;
    drop(db);
    copy::write(base, out_db, &kept)?;
    let target = Connection::open(out_db)?;
    let paths: Vec<String> = {
        let mut stmt = target.prepare("SELECT path FROM files WHERE sliced = 1 ORDER BY path")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    crate::rows::meta::put(&target, "partial", "1")?;
    crate::rows::meta::put(&target, "slice:hops", &hops.to_string())?;
    crate::rows::meta::put(&target, "slice:base", &std::fs::canonicalize(base)?.to_string_lossy().replace('\\', "/"))?;
    crate::rows::meta::put(&target, "slice:of", &of.to_string())?;
    crate::rows::meta::put_value(&target, "slice:files", &serde_json::json!(paths))?;
    Ok((kept.len(), of))
}

/// The files `--slice-files` names: one path, or `@list` - one path a line, blank lines and `#` lines skipped.
fn listed(files: &str) -> Result<BTreeSet<String>> {
    let lines: Vec<String> = match files.strip_prefix('@') {
        Some(list) => std::fs::read_to_string(list).with_context(|| format!("read {list}"))?.lines().map(str::to_string).collect(),
        None => vec![files.to_string()],
    };
    let out: BTreeSet<String> = lines.iter().map(|l| l.trim().replace('\\', "/")).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    if out.is_empty() {
        bail!("--slice-files names no file");
    }
    Ok(out)
}

/// Whether a database is a slice: `_meta` `partial` = 1. The deep map refuses to write into one - its missing rows would read as "unchanged".
pub fn is_slice(db: &Path) -> bool {
    db.is_file()
        && Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .and_then(|c| crate::rows::meta::get(&c, "partial"))
            .is_some_and(|v| v == "1")
}

/// What `--map-query` says of a slice on every question: `partial map: N of M files ..`. None for a full map.
pub fn note(db: &Path) -> Option<String> {
    if !is_slice(db) {
        return None;
    }
    let c = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let kept: i64 = c.query_row("SELECT count(*) FROM files WHERE sliced = 1", [], |r| r.get(0)).ok()?;
    let of = crate::rows::meta::get(&c, "slice:of").unwrap_or_default();
    let hops = crate::rows::meta::get(&c, "slice:hops").unwrap_or_default();
    Some(format!(
        "partial map: {kept} of {of} files ({hops} hop(s) of dependents) - a row missing here may be in a file outside the slice, not absent from the tree"
    ))
}
