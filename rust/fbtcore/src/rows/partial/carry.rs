//! THE ROWS OF EVERY FILE THIS RUN IS NOT RE-EXTRACTING, handed back so node can see a whole
//! map.
//!
//! The rollups are derived from every file, and the selector registry from every declaration,
//! so a run that re-extracts a fraction would compute both from a fraction and be wrong while
//! looking right. node cannot read this database, so the rest is written out for it.
//!
//! EVERYTHING PER-FILE, not a list of the tables node happens to need. A list is a thing to
//! keep in step with two other files, and this half has already shipped two that drifted;
//! node ignores what it is going to rebuild anyway, which costs a small part of the file and cannot
//! be subtly wrong. Rows with no `owner_file` are the derived tables, and those are rebuilt
//! whole — they are not sent at all.
//!
//! MEASURED ON A LARGE WORKSPACE: hundreds of thousands of rows, hundreds of MB, written in about a
//! third of the time of the python end it replaces - both timed as a whole process, and the two
//! files identical byte for byte, compared while the python end was still in the tree.
//!
//! A STRUCTURE IS NODES IN THE DATABASE (`cells`: `<table>__<column>`), so a carried row gets each of
//! its structures rebuilt from its node table - only for the rows that go out - and a node table is
//! never carried as a table of its own: node would load it as one. node still turns a cell that
//! arrives as TEXT back into a structure (`json_columns`); one that arrives as a structure it keeps.

use super::half::{tables_with_half, HALF_COLUMN};
use super::pyjson::PythonSeparators;
use super::schema;
use anyhow::{Context, Result};
use indexmap::{IndexMap, IndexSet};
use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// What went out, for the `MAP-CARRY` line.
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct Carried {
    pub rows: usize,
    pub tables: usize,
}

/// A cell, exactly as SQLite holds it.
///
/// A BLOB cannot be reached from here in practice: the python end this replaces hands the
/// same rows to `json.dump`, which raises on bytes, so a blob column would have failed there
/// long before it reached node. It is carried as lossy text rather than dropped, because a
/// dropped column reads as a NULL one and that is the difference this file exists to avoid.
fn cell(raw: Sql) -> Option<Value> {
    match raw {
        Sql::Null => None,
        Sql::Integer(i) => Some(Value::from(i)),
        Sql::Real(f) => Some(Value::from(f)),
        Sql::Text(s) => Some(Value::String(s)),
        Sql::Blob(b) => Some(Value::String(String::from_utf8_lossy(&b).into_owned())),
    }
}

/// The columns that identify a row INSIDE its own file, joined — see the reader's `identityKeys`.
///
/// A column that is absent, null or a structure makes the row UNCLAIMABLE rather than claimable by a
/// shorter key: a key that is not the whole key would match a row that merely looks alike, and the
/// reference it then kept would name the wrong declaration.
fn identity_key(row: &Map<String, Value>, columns: &[String]) -> Option<String> {
    let mut parts: Vec<String> = Vec::with_capacity(columns.len());
    for column in columns {
        match row.get(column.as_str()) {
            None | Some(Value::Null) => return None,
            Some(Value::String(s)) => parts.push(s.clone()),
            Some(Value::Bool(b)) => parts.push(if *b { "1".into() } else { "0".into() }),
            Some(Value::Number(n)) => parts.push(n.to_string()),
            Some(_) => return None,
        }
    }
    Some(parts.join("\u{0}"))
}

/// A row's identity under its table's key: one key, or the first ALTERNATIVE it holds every column of, numbered as
/// node numbers it (`identityOf`) - a claim made here is looked up there.
fn identity_of_row(row: &Map<String, Value>, keys: &[Vec<String>]) -> Option<String> {
    match keys {
        [one] => identity_key(row, one),
        many => many.iter().enumerate().find_map(|(i, key)| identity_key(row, key).map(|k| format!("{i}\u{1}{k}"))),
    }
}

/// The file ids this run is about to replace, so their rows are not sent twice.
fn affected_ids(db: &Connection, lang: &str, affected: &IndexSet<String>) -> Result<IndexSet<String>> {
    let mut ids = IndexSet::new();
    if affected.is_empty() {
        return Ok(ids);
    }
    let marks = vec!["?"; affected.len()].join(",");
    let sql = format!("SELECT id FROM files WHERE half = ? AND path IN ({marks})");
    let mut stmt = db.prepare(&sql)?;
    let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(affected.len() + 1);
    params.push(&lang);
    for p in affected {
        params.push(p);
    }
    let found = stmt.query_map(params.as_slice(), |r| r.get::<_, String>(0))?;
    for id in found.flatten() {
        ids.insert(id);
    }
    Ok(ids)
}

/// One table's rows, minus the ones the run is replacing.
type Claims = IndexMap<String, String>;

fn held(
    db: &Connection,
    table: &str,
    columns: &[String],
    lang: &str,
    ids: &IndexSet<String>,
    identity: Option<&Vec<Vec<String>>>,
) -> Result<(Vec<Map<String, Value>>, Claims)> {
    let by_owner = columns.iter().any(|c| c == "owner_file");
    let sql = format!(
        "SELECT rowid, * FROM \"{}\" WHERE \"{}\" = ?1",
        schema::escape(table),
        HALF_COLUMN
    );
    let mut stmt = db.prepare(&sql)?;
    // The rowid first: what a row of a table with no id is owned by in its node tables.
    let names: Vec<String> = stmt.column_names().into_iter().skip(1).map(String::from).collect();

    // THE ROW'S STRUCTURES, read from their node tables by the row's owner key: its id, or `#<rowid>`.
    let mut nodes: Vec<(String, super::cells::Reader)> = Vec::new();
    for (column, child) in super::cells::columns_of(db, table) {
        nodes.push((column, super::cells::Reader::new(db, &child)?));
    }

    let mut out = Vec::new();
    let mut claims: Claims = IndexMap::new();
    let mut rows = stmt.query([lang])?;
    while let Some(record) = rows.next()? {
        // THE WHOLE ROW IS READ BEFORE ANYTHING IS DECIDED ABOUT IT. Stopping at the column that
        // decides left the rest of it unread, so the key of a row being replaced could not be built
        // from what had been gathered - and the id that key exists to keep was lost.
        let mut row = Map::new();
        for (i, name) in names.iter().enumerate() {
            if name == HALF_COLUMN {
                continue;
            }
            if let Some(value) = cell(record.get::<_, Sql>(i + 1)?) {
                row.insert(name.clone(), value);
            }
        }
        let at: i64 = record.get(0)?;

        // A ROW THAT BELONGS TO NO FILE IS A DERIVED ROW, whatever table it sits in. The test at the
        // top is the same one a level up: a table with no `owner_file` COLUMN is rebuilt whole, and so
        // is a row that has the column and no value in it. Some `renders` are like that — the edges a
        // rollup reads out of `createComponent` calls — and carrying them put a copy beside each
        // freshly derived one, naming the same edge through a different `call`. It multiplied the
        // render paths several times over.
        if by_owner && !matches!(row.get("owner_file"), Some(Value::String(_))) {
            continue;
        }
        let replaced = match row.get("owner_file") {
            Some(Value::String(owner)) => by_owner && ids.contains(owner),
            _ => false,
        };
        if !replaced {
            if !nodes.is_empty() {
                let owner = match row.get("id") {
                    Some(Value::String(id)) => id.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    _ => format!("#{at}"),
                };
                for (column, reader) in nodes.iter_mut() {
                    if row.contains_key(column.as_str()) {
                        continue;
                    }
                    if let Some(value) = reader.get(&owner)? {
                        row.insert(column.clone(), value);
                    }
                }
            }
            out.push(row);
            continue;
        }
        // THE ID OF A ROW THAT IS NOT COMING BACK, for a table anything outside a file can name. A
        // declaration still there after the re-extraction is the SAME declaration, and keeping its id
        // is what stops every reference to it from another file having to be resolved again onto a new
        // one — which a partial run may not store.
        if let (Some(columns_for), Some(Value::String(id))) = (identity, row.get("id"))
            && let Some(key) = identity_of_row(&row, columns_for)
            && let Some(Value::String(owner)) = row.get("owner_file")
        {
            claims.insert(format!("{owner}\u{0}{key}"), id.clone());
        }
    }
    Ok((out, claims))
}

/// The tables node rebuilds whole, as the last run's spec names them.
fn rebuilt_of(db: &Connection, lang: &str) -> IndexSet<String> {
    let Some(text) = crate::rows::meta::get(db, &format!("spec:{lang}")) else { return IndexSet::new() };
    let Ok(spec) = serde_json::from_str::<Value>(&text) else { return IndexSet::new() };
    spec.get("rebuilt").and_then(Value::as_array).into_iter().flatten().filter_map(|t| t.as_str().map(String::from)).collect()
}

/// THE IDS A ROLLUP MADE, in the order it made them (rowid): every row of a table node rebuilds whole, and the rows
/// of a per-file table that belong to no file. Node hands them back to the rows the rollup makes again (`rollupId` in
/// `TsStore.mjs`), so a rollup rebuilt from the same rows keeps its ids. `projects` is re-pointed by node itself.
fn rollup_ids(db: &Connection, table: &str, columns: &[String], lang: &str, rebuilt: &IndexSet<String>) -> Result<Option<Vec<String>>> {
    if table == "projects" || !columns.iter().any(|c| c == "id") {
        return Ok(None);
    }
    let scope = if rebuilt.contains(table) {
        ""
    } else if columns.iter().any(|c| c == "owner_file") {
        " AND owner_file IS NULL"
    } else {
        return Ok(None);
    };
    let sql = format!("SELECT id FROM \"{}\" WHERE \"{HALF_COLUMN}\" = ?1 AND id IS NOT NULL{scope} ORDER BY rowid", schema::escape(table));
    let mut stmt = db.prepare(&sql)?;
    let ids: Vec<String> = stmt.query_map([lang], |r| r.get::<_, String>(0))?.flatten().collect();
    Ok((!ids.is_empty()).then_some(ids))
}

/// `{table: [column, ...]}` — which ids must survive a re-extraction, as the last full run measured it - or
/// `{table: [[column, ...], ...]}`, ALTERNATIVES: a row is named by the first it holds every column of (node's
/// `identityOf`).
fn identity_of(db: &Connection, lang: &str) -> IndexMap<String, Vec<Vec<String>>> {
    let mut out = IndexMap::new();
    let Some(text) = crate::rows::meta::get(db, &format!("spec:{lang}")) else { return out };
    let Ok(spec) = serde_json::from_str::<Value>(&text) else { return out };
    let Some(Value::Object(identity)) = spec.get("identity") else { return out };
    let names = |list: &Vec<Value>| -> Vec<String> { list.iter().filter_map(|c| c.as_str().map(String::from)).collect() };
    for (table, columns) in identity {
        let Value::Array(list) = columns else { continue };
        let keys: Vec<Vec<String>> = if list.iter().all(Value::is_array) {
            list.iter().filter_map(Value::as_array).map(names).collect()
        } else {
            vec![names(list)]
        };
        if !keys.is_empty() && keys.iter().all(|k| !k.is_empty()) {
            out.insert(table.clone(), keys);
        }
    }
    out
}

pub fn carry_over(
    db_path: &Path,
    affected: &[String],
    target: &Path,
    lang: &str,
) -> Result<Carried> {
    if !db_path.exists() {
        anyhow::bail!("there is no database to carry rows over from");
    }
    let keep: IndexSet<String> = affected.iter().cloned().collect();
    let db = Connection::open(db_path)
        .with_context(|| format!("the database could not be opened: {}", db_path.display()))?;
    let ids = affected_ids(&db, lang, &keep)?;
    let identity = identity_of(&db, lang);

    let file = File::create(target)
        .with_context(|| format!("the carry file could not be written: {}", target.display()))?;
    let mut writer = BufWriter::new(file);
    // ONE ROW A LINE, and never one document.
    //
    // This file is hundreds of MB on a real tree and node reads it back with `JSON.parse(readFileSync(..))` -
    // which is two strings that long against V8's hard ceiling of 536 870 888 characters. Close to
    // that, a little more workspace and the partial path stops working with `Invalid string length`,
    // which no heap setting moves. A line is a row, so nothing here is ever held whole.
    //
    // The shape is private between this and `TsMap.mjs`: `{"t": <table>, "r": <row>}` for every row,
    // then one `{"claims": {...}, "rollup": {...}}` at the end. Order does not matter - the reader collects both.

    let mut carried = Carried::default();
    let mut all_claims: IndexMap<String, Claims> = IndexMap::new();
    let rebuilt = rebuilt_of(&db, lang);
    let mut rollup = Map::new();
    for table in tables_with_half(&db)? {
        // A NODE TABLE goes out inside the rows it belongs to (`held`), never as a table.
        if super::cells::parent_of(&db, &table).is_some() {
            continue;
        }
        let columns = schema::existing_columns(&db, &table)?;
        if let Some(ids) = rollup_ids(&db, &table, &columns, lang, &rebuilt)? {
            rollup.insert(table.clone(), Value::Array(ids.into_iter().map(Value::String).collect()));
        }
        // A TABLE WITH NO `owner_file` is a derived one - node rebuilds those whole and would
        // throw these away, so they are never sent. `files` is the exception: a file row IS a
        // file, and node needs every one of them because `imports.resolved_file` names files
        // it never extracted. And `projects`, which node rebuilds but reads first: a file row it hands back
        // names its project by the id the last run gave it, and only these rows say which project that was.
        if !columns.iter().any(|c| c == "owner_file") && table != "files" && table != "projects" {
            continue;
        }
        let (rows, claims) = held(&db, &table, &columns, lang, &ids, identity.get(&table))?;
        if !claims.is_empty() {
            all_claims.insert(table.clone(), claims);
        }
        if rows.is_empty() {
            continue;
        }
        for row in &rows {
            let mut line = Map::new();
            line.insert("t".to_string(), Value::String(table.clone()));
            line.insert("r".to_string(), Value::Object(row.clone()));
            write_json(&mut writer, &Value::Object(line))?;
            writer.write_all(b"\n")?;
        }
        carried.tables += 1;
        carried.rows += rows.len();
    }

    let mut claims_of = Map::new();
    for (table, claims) in all_claims.iter() {
        let mut map = Map::new();
        for (key, id) in claims {
            map.insert(key.clone(), Value::String(id.clone()));
        }
        claims_of.insert(table.clone(), Value::Object(map));
    }
    let mut last = Map::new();
    last.insert("claims".to_string(), Value::Object(claims_of));
    last.insert("rollup".to_string(), Value::Object(rollup));
    write_json(&mut writer, &Value::Object(last))?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(carried)
}

/// Spelled the way `json.dump(..., ensure_ascii=False)` spells it, straight into the file.
fn write_json<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    let mut ser = serde_json::Serializer::with_formatter(writer, PythonSeparators);
    value.serialize(&mut ser)?;
    Ok(())
}

#[cfg(test)]
#[allow(non_snake_case)]
#[path = "carry_tests.rs"]
mod tests;
