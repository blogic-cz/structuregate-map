//! THE REFERENCES A PARTIAL RUN WOULD LEAVE POINTING AT NOTHING, found BEFORE its first delete.
//!
//! A file read again keeps its rows' ids where they are claimable (`carry`), but not every row is - and a row of a
//! file that was NOT read again, naming one of those ids, would then name a row that is gone. The ids that go are
//! exactly the ones the replace deletes (`tsapply::replace_affected`: every row of a table node rebuilds WHOLE, and
//! the affected files' rows of every other) that the payload does not hold again - so they are read here, from the
//! rows still there. Asked AFTER the replace, as it once was, they were already gone: the check found nothing, ever,
//! and a refusal would have left the map with the affected rows deleted and nothing written in their place.
//!
//! WHO IS ASKED, ON EVERY RUN: every row of a whole table the payload sends - carried or made, it is the row the map will
//! hold (`Digest::named`) - every kept row's structures (node rows, by their indexed `value`), every kept row's cell in a
//! column a spec declares a JOIN (indexed: `schema::index_declared`), and every cell of the rows of the files the stored
//! id graph (`deps`) says point into a replaced one. Asked only of the watchers' files, as it once was on a run that
//! read every hop, an id vanishing from a whole table - `files`, when a workspace with no `templateUrl` made it a
//! rollup's - was named by the kept rows of every other file and stored dangling. A run that STOPPED SHORT of every hop
//! (`nested`) also reads every plain cell of every kept row, as node once asked of every row it held (`Store.dangling`,
//! gone). Only ids that existed and are about to vanish count, so a string that merely looks like an id is never a
//! reason to read again.
//!
//! WHAT A HIT DOES: a run that stopped short is asked again over every hop (`STOPPED_SHORT`). A run that already read
//! every hop would be asked the same question again, so it is refused with `EVERY_HOP` and the caller reads EVERYTHING
//! instead (`mapper/deep/ts.rs`), saying which reference it found: a full run cannot leave one, and a hook that failed
//! would leave the map stale until someone ran it by hand.

use super::half::{tables_with_half, HALF_COLUMN};
use super::schema::escape;
use indexmap::{IndexMap, IndexSet};
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, ToSql};
use serde_json::{Map, Value};

/// How a refusal by `dangling` begins, so the caller asks again over every hop rather than reporting an error.
pub const STOPPED_SHORT: &str = "a run that stopped short of every hop";

/// How a refusal of a run that read every hop begins, so the caller reads everything instead.
pub const EVERY_HOP: &str = "a run that read every hop";

/// What a partial run is about to replace, and what it writes in its place.
pub struct Replace<'a> {
    pub half: &'a str,
    /// The affected files, by path.
    pub replaced: &'a [String],
    /// The tables node rebuilds whole: every row of the half goes.
    pub whole: &'a [String],
    /// Every id the payload holds.
    pub kept: &'a IndexSet<String>,
    /// id -> `table.column` of the first payload row of a whole table that names it (`Digest::named`).
    pub named: &'a IndexMap<String, String>,
    /// `table.column` of every join the payload's spec declares - beside the stored spec's (`probed`).
    pub joins: &'a [String],
}

/// An error beginning `STOPPED_SHORT` (asked again over every hop) or `EVERY_HOP` (read everything instead) when the
/// replace would leave a kept row naming nothing, over the id graph the last run stored.
pub fn refuse(db: &Connection, plan: &Replace<'_>, nested: bool) -> anyhow::Result<()> {
    let deps: Map<String, Value> = super::remember::stored_graph(db).into_iter().map(|(target, dependents)| (target, Value::from(dependents))).collect();
    match dangling(db, plan, &deps, nested)? {
        Some((count, first)) => {
            let run = if nested { STOPPED_SHORT } else { EVERY_HOP };
            anyhow::bail!("{run} would leave {count} reference(s) to rows it replaced, first {first}")
        }
        None => Ok(()),
    }
}

/// The first reference found and how many there are; None when nothing kept would point at nothing.
pub fn dangling(db: &Connection, plan: &Replace<'_>, deps: &Map<String, Value>, nested: bool) -> anyhow::Result<Option<(usize, String)>> {
    let half = plan.half;
    let gone_paths: IndexSet<&str> = plan.replaced.iter().map(String::as_str).collect();
    let mut watchers: IndexSet<String> = IndexSet::new();
    for p in &gone_paths {
        for d in deps.get(*p).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if !gone_paths.contains(d) {
                watchers.insert(d.to_string());
            }
        }
    }
    let replaced_ids = file_ids(db, half, gone_paths.iter().copied())?;
    let watcher_ids = file_ids(db, half, watchers.iter().map(String::as_str))?;
    // A NODE TABLE IS NOT A ROW ANYONE NAMES: its ids are its own tree's, and its rows go with their owner.
    let names: Vec<String> = tables_with_half(db)?.into_iter().filter(|t| super::cells::parent_of(db, t).is_none()).collect();
    let wholly: IndexSet<&str> = plan.whole.iter().map(String::as_str).collect();
    let mut holders: Vec<(String, bool)> = Vec::new();
    let mut vanished: IndexSet<String> = IndexSet::new();
    for table in &names {
        let columns = super::schema::existing_columns(db, table)?;
        let (has_id, has_owner) = (columns.iter().any(|c| c == "id"), columns.iter().any(|c| c == "owner_file"));
        let whole = wholly.contains(table.as_str());
        if !whole && has_owner {
            holders.push((table.clone(), has_id));
        }
        // AS THE REPLACE SELECTS THEM, so what is counted gone is what it deletes.
        let sql = if whole && has_id {
            format!("SELECT id FROM \"{}\" WHERE \"{HALF_COLUMN}\" = ?", escape(table))
        } else if has_id && has_owner && !replaced_ids.is_empty() {
            format!("SELECT id FROM \"{}\" WHERE \"{HALF_COLUMN}\" = ? AND owner_file IN ({})", escape(table), marks(replaced_ids.len()))
        } else {
            continue;
        };
        let mut params: Vec<&dyn ToSql> = vec![&half];
        if !whole {
            params.extend(replaced_ids.iter().map(|i| i as &dyn ToSql));
        }
        let mut stmt = db.prepare(&sql)?;
        for id in stmt.query_map(params.as_slice(), |r| r.get::<_, Option<String>>(0))?.flatten().flatten() {
            if !plan.kept.contains(&id) {
                vanished.insert(id);
            }
        }
    }
    if vanished.is_empty() {
        return Ok(None);
    }
    // THE IDS THAT GO, as a table every probe below joins: the probes stay in SQLite, never a row per cell into this side.
    db.execute_batch("DROP TABLE IF EXISTS temp.sg_vanished; CREATE TEMP TABLE sg_vanished (id TEXT PRIMARY KEY)")?;
    {
        let mut put = db.prepare("INSERT INTO temp.sg_vanished (id) VALUES (?1)")?;
        for id in &vanished {
            put.execute([id])?;
        }
    }
    // ONE REFERENCE PER `table.column -> id`, however many probes reach it.
    let mut found: IndexSet<String> = IndexSet::new();
    let mut hit = |label: String| {
        found.insert(label);
    };
    for (id, label) in plan.named {
        if vanished.contains(id) {
            hit(format!("{label} -> {id}"));
        }
    }
    {
        for (table, has_id) in &holders {
            let mut home = db.prepare(&format!("SELECT owner_file FROM \"{}\" WHERE {} = ?1", escape(table), if *has_id { "id" } else { "rowid" }))?;
            let mut home_at = db.prepare(&format!("SELECT owner_file FROM \"{}\" WHERE rowid = ?1", escape(table)))?;
            for (column, child) in super::cells::columns_of(db, table) {
                let sql = format!("SELECT DISTINCT owner, value FROM \"{}\" WHERE value IN temp.sg_vanished ORDER BY value, owner", escape(&child));
                let mut stmt = db.prepare(&sql)?;
                let hits: Vec<(String, String)> = stmt
                    .query_map([], |r| Ok((r.get::<_, Sql>(0)?, r.get::<_, String>(1)?)))?
                    .flatten()
                    .filter_map(|(owner, value)| match owner {
                        Sql::Text(t) => Some((t, value)),
                        _ => None,
                    })
                    .collect();
                for (owner, value) in hits {
                    // A ROW OF A REPLACED FILE goes with it; one of any other file is kept, and would name nothing.
                    let file: Option<String> = match owner.strip_prefix('#').and_then(|at| at.parse::<i64>().ok()) {
                        Some(at) => home_at.query_row([at], |r| r.get(0)).ok().flatten(),
                        None => home.query_row([&owner], |r| r.get(0)).ok().flatten(),
                    };
                    if file.is_some_and(|f| !replaced_ids.contains(&f)) {
                        hit(format!("{table}.{column} -> {value}"));
                    }
                }
            }
        }
    }
    // A JOIN'S CELL OF EVERY KEPT ROW, by the column's index: a run over every hop scans nothing whole.
    if !nested {
        let joins = probed(db, plan.joins);
        let scope = format!("(owner_file IS NULL OR owner_file NOT IN ({}))", marks(replaced_ids.len()));
        for (table, _) in &holders {
            for column in joins.get(table.as_str()).into_iter().flatten() {
                let sql = format!("SELECT \"{c}\" FROM \"{t}\" WHERE \"{c}\" IN temp.sg_vanished AND \"{HALF_COLUMN}\" = ? AND {scope}",
                    c = escape(column), t = escape(table));
                let mut stmt = db.prepare(&sql)?;
                let mut params: Vec<&dyn ToSql> = vec![&half];
                params.extend(replaced_ids.iter().map(|i| i as &dyn ToSql));
                for value in stmt.query_map(params.as_slice(), |r| r.get::<_, String>(0))?.flatten() {
                    hit(format!("{table}.{column} -> {value}"));
                }
            }
        }
    }
    // A PLAIN CELL: the watchers' rows - or, stopped short, every kept row of every table that is not replaced whole.
    let scanned: Vec<&String> = if nested { holders.iter().map(|(t, _)| t).collect() } else { holders.iter().filter(|(_, id)| *id).map(|(t, _)| t).collect() };
    let (scope, scoped) = if nested {
        (format!("(owner_file IS NULL OR owner_file NOT IN ({}))", marks(replaced_ids.len())), &replaced_ids)
    } else {
        (format!("owner_file IN ({})", marks(watcher_ids.len())), &watcher_ids)
    };
    if nested || !watcher_ids.is_empty() {
        for table in scanned {
            let columns: Vec<String> =
                super::schema::existing_columns(db, table)?.into_iter().filter(|c| !["id", "owner_file", HALF_COLUMN].contains(&c.as_str())).collect();
            if columns.is_empty() {
                continue;
            }
            let quoted: Vec<String> = columns.iter().map(|c| format!("\"{}\"", escape(c))).collect();
            let named: Vec<String> = quoted.iter().map(|c| format!("{c} IN temp.sg_vanished")).collect();
            let sql = format!("SELECT {} FROM \"{}\" WHERE \"{HALF_COLUMN}\" = ? AND {scope} AND ({})", quoted.join(", "), escape(table), named.join(" OR "));
            let mut stmt = db.prepare(&sql)?;
            let mut params: Vec<&dyn ToSql> = vec![&half];
            params.extend(scoped.iter().map(|i| i as &dyn ToSql));
            let mut rows = stmt.query(params.as_slice())?;
            while let Some(row) = rows.next()? {
                for i in 0..columns.len() {
                    if let Ok(Some(text)) = row.get::<_, Option<String>>(i)
                        && vanished.contains(&text)
                    {
                        hit(format!("{table}.{} -> {text}", columns[i]));
                    }
                }
            }
        }
    }
    db.execute_batch("DROP TABLE IF EXISTS temp.sg_vanished")?;
    Ok(found.first().map(|f| (found.len(), f.clone())))
}

/// MAKE ROOM FOR A PARTIAL PAYLOAD: this half's rows for the files being re-extracted, and nothing
/// else.
///
/// TWO KINDS OF TABLE, and node says which is which rather than this side keeping a second copy of
/// the knowledge. A table node rebuilt WHOLE has its rows for this half removed outright — they are
/// derived from the tree and a partial tree cannot produce them correctly. Every other table is per
/// FILE, and only the affected files' rows go.
///
/// BY `owner_file`, which is why that column exists: 21 of the 53 tables carry no `file` of their
/// own, and the parent chain that would stand in for one cannot attribute the module-scope `calls`
/// at all.
///
/// THE OTHER HALVES ARE NEVER TOUCHED — every statement is scoped by `half`.
pub fn replace_affected(db: &Connection, half: &str, affected: &[String], ids: &[String], whole: &[String]) -> anyhow::Result<usize> {
    let wholly: IndexSet<&str> = whole.iter().map(String::as_str).collect();
    let mut removed = 0usize;
    for table in tables_with_half(db)? {
        let columns = super::schema::existing_columns(db, &table)?;
        let escaped = escape(&table);
        // A STRUCTURE'S NODES GO FIRST, THROUGH THE ROWS THAT OWN THEM: a node carries no scope of its own (`cells`).
        let count = if wholly.contains(table.as_str()) {
            super::half::delete_half_rows(db, &table, half)?
        } else if columns.iter().any(|c| c == "owner_file") && !ids.is_empty() {
            let marks = vec!["?"; ids.len()].join(",");
            let sql = format!(
                "DELETE FROM \"{escaped}\" WHERE \"{HALF_COLUMN}\" = ? AND owner_file IN ({marks})"
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() + 1);
            params.push(&half);
            for id in ids {
                params.push(id);
            }
            super::cells::delete_children(db, &table, &format!("t.\"{HALF_COLUMN}\" = ? AND t.owner_file IN ({marks})"), params.as_slice())?;
            db.execute(&sql, params.as_slice())?
        } else {
            continue
        };
        removed += count;
    }

    // THE `files` ROWS OF THE AFFECTED FILES TOO, and last: the lookup above reads them. The write
    // re-creates each one from the payload.
    if !affected.is_empty() {
        let marks = vec!["?"; affected.len()].join(",");
        let sql = format!("DELETE FROM files WHERE half = ? AND path IN ({marks})");
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(affected.len() + 1);
        params.push(&half);
        for path in affected {
            params.push(path);
        }
        super::cells::delete_children(db, "files", &format!("t.half = ? AND t.path IN ({marks})"), params.as_slice())?;
        db.execute(&sql, params.as_slice())?;
    }
    Ok(removed)
}

/// Every `table -> [column]` a join is declared on - by the spec the last run stored and by this payload's - that the
/// table has, each indexed: the stored ones already are (`schema::index_declared`), a new one is made here.
fn probed(db: &Connection, payload: &[String]) -> IndexMap<String, Vec<String>> {
    let mut keys: IndexSet<String> = payload.iter().cloned().collect();
    for (_, raw) in super::meta::scan(db, Some("spec:%")) {
        let Ok(spec) = serde_json::from_str::<Value>(&raw) else { continue };
        keys.extend(spec.get("joins").and_then(Value::as_object).into_iter().flat_map(|j| j.keys().cloned()));
    }
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    for key in keys {
        let Some((table, column)) = key.split_once('.') else { continue };
        let has = super::schema::existing_columns(db, table).is_ok_and(|c| c.iter().any(|c| c == column));
        if has && super::schema::index_column(db, table, column).is_ok() {
            out.entry(table.to_string()).or_default().push(column.to_string());
        }
    }
    out
}

fn marks(n: usize) -> String {
    vec!["?"; n].join(",")
}

fn file_ids<'a>(db: &Connection, half: &str, paths: impl Iterator<Item = &'a str>) -> anyhow::Result<IndexSet<String>> {
    let mut out = IndexSet::new();
    let mut stmt = db.prepare("SELECT id FROM files WHERE half = ?1 AND path = ?2")?;
    for p in paths {
        for id in stmt.query_map([half, p], |r| r.get::<_, String>(0))?.flatten() {
            out.insert(id);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "dangling_tests.rs"]
mod tests;
