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
//! NOTHING IS DECODED HERE. A column the half encoded is TEXT in SQLite, and turning it back
//! into a structure only to re-encode it one line later cost more than twice as long as
//! passing the text straight through — on a run whose whole purpose is to be shorter than a
//! rebuild. node decodes them on the way in; it has the spec that says which columns they
//! are, and it is the side that encoded them in the first place.

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
    identity: Option<&Vec<String>>,
) -> Result<(Vec<Map<String, Value>>, Claims)> {
    let by_owner = columns.iter().any(|c| c == "owner_file");
    let sql = format!(
        "SELECT * FROM \"{}\" WHERE \"{}\" = ?1",
        schema::escape(table),
        HALF_COLUMN
    );
    let mut stmt = db.prepare(&sql)?;
    let names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();

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
            if let Some(value) = cell(record.get::<_, Sql>(i)?) {
                row.insert(name.clone(), value);
            }
        }

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
            out.push(row);
            continue;
        }
        // THE ID OF A ROW THAT IS NOT COMING BACK, for a table anything outside a file can name. A
        // declaration still there after the re-extraction is the SAME declaration, and keeping its id
        // is what stops every reference to it from another file having to be resolved again onto a new
        // one — which a partial run may not store.
        if let (Some(columns_for), Some(Value::String(id))) = (identity, row.get("id"))
            && let Some(key) = identity_key(&row, columns_for)
            && let Some(Value::String(owner)) = row.get("owner_file")
        {
            claims.insert(format!("{owner}\u{0}{key}"), id.clone());
        }
    }
    Ok((out, claims))
}

/// `{table: [column, ...]}` — which ids must survive a re-extraction, as the last full run measured it.
fn identity_of(db: &Connection, lang: &str) -> IndexMap<String, Vec<String>> {
    let mut out = IndexMap::new();
    let Ok(text) = db.query_row(
        "SELECT value FROM _meta WHERE key = ?1",
        [format!("spec:{lang}")],
        |r| r.get::<_, String>(0),
    ) else {
        return out;
    };
    let Ok(spec) = serde_json::from_str::<Value>(&text) else { return out };
    let Some(Value::Object(identity)) = spec.get("identity") else { return out };
    for (table, columns) in identity {
        if let Value::Array(list) = columns {
            let names: Vec<String> =
                list.iter().filter_map(|c| c.as_str().map(String::from)).collect();
            if !names.is_empty() {
                out.insert(table.clone(), names);
            }
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
    // then one `{"claims": {...}}` at the end. Order does not matter - the reader collects both.

    let mut carried = Carried::default();
    let mut all_claims: IndexMap<String, Claims> = IndexMap::new();
    for table in tables_with_half(&db)? {
        let columns = schema::existing_columns(&db, &table)?;
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
mod tests {
    use super::*;
    use crate::rows::{apply, Batch};
    use serde_json::json;

    fn tmp(name: &str) -> crate::rows::TempDb {
        crate::rows::TempDb::new("carry", name)
    }

    /// Two files, rows that hang off each by `owner_file`, and one derived table that hangs
    /// off nothing.
    fn seed(db: &Path) {
        let mut b = Batch {
            all: true,
            first: true,
            final_: true,
            lang: "typescript".to_string(),
            half: Some("typescript".to_string()),
            ..Default::default()
        };
        b.shas.insert("a.ts".to_string(), json!("sha-a"));
        b.shas.insert("b.ts".to_string(), json!("sha-b"));
        b.tables.insert("files".to_string(), json!([
            {"id": "f:1", "path": "a.ts", "sha": "sha-a"},
            {"id": "f:2", "path": "b.ts", "sha": "sha-b"}
        ]));
        b.tables.insert("bindings".to_string(), json!([
            {"id": "b:1", "owner_file": "f:1", "name": "one", "note": null,
             "gates_json": "[\"g:1\", \"g:2\"]"},
            {"id": "b:2", "owner_file": "f:2", "name": "two", "note": null}
        ]));
        // No `owner_file`: a rollup node rebuilds whole.
        b.tables.insert("key_reach".to_string(), json!([
            {"key": "menu.home", "route": "template"}
        ]));
        apply(db, ".", &b).unwrap();
    }

    fn carry(db: &Path, affected: &[&str]) -> (Carried, Value) {
        let target = db.with_extension("carry.json");
        let list: Vec<String> = affected.iter().map(|s| s.to_string()).collect();
        let out = carry_over(db, &list, &target, "typescript").unwrap();
        (out, collected(&target))
    }

    /// THE FILE AS ITS READER PUTS IT BACK TOGETHER. It is one row a line now - see `carry_over` -
    /// so a test that parsed it as one document would be asserting the shape of the wire rather
    /// than the rows on it. This is what `TsMap.mjs` builds from the same lines.
    fn collected(target: &Path) -> Value {
        let text = std::fs::read_to_string(target).unwrap();
        let mut tables: Map<String, Value> = Map::new();
        let mut claims = Value::Object(Map::new());
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let one: Value = serde_json::from_str(line).unwrap();
            if let Some(found) = one.get("claims") {
                claims = found.clone();
                continue;
            }
            let table = one["t"].as_str().unwrap().to_string();
            let entry = tables.entry(table).or_insert_with(|| Value::Array(Vec::new()));
            entry.as_array_mut().unwrap().push(one["r"].clone());
        }
        let mut out = Map::new();
        out.insert("tables".to_string(), Value::Object(tables));
        out.insert("claims".to_string(), claims);
        Value::Object(out)
    }

    #[test]
    fn the_rows_of_a_file_being_re_extracted_are_not_sent_because_they_are_about_to_be_replaced() {
        let db = tmp("replaced");
        seed(&db);
        let (out, payload) = carry(&db, &["a.ts"]);
        let bindings = payload["tables"]["bindings"].as_array().unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0]["id"], json!("b:2"));
        // ...BUT THE FILE ROW ITSELF IS, because the file is the same file.
        let files = payload["tables"]["files"].as_array().unwrap();
        assert_eq!(files.len(), 2, "a re-extracted file keeps its identity");
        assert_eq!(files[0]["id"], json!("f:1"));
        assert_eq!(out.rows, 3);
    }


    /// What the last full run measured about which ids must survive, stored where it stores it.
    fn with_identity(db: &Path, identity: Value) {
        let conn = Connection::open(db).unwrap();
        conn.execute("DELETE FROM _meta WHERE key = 'spec:typescript'", []).unwrap();
        conn.execute(
            "INSERT INTO _meta (key, value) VALUES ('spec:typescript', ?1)",
            [serde_json::json!({"identity": identity}).to_string()],
        )
        .unwrap();
    }

    #[test]
    fn a_declaration_that_is_still_there_keeps_the_id_it_already_had() {
        // Minting it a fresh number makes every reference to it from another file dangle, the
        // reference passes resolve them onto the new number, and those are per-file rows a partial run
        // may not rewrite. Measured on a large workspace: dozens of `calls`, `expressions` and
        // `assignments`, every one the same fact under a different number.
        let db = tmp("claims");
        seed(&db);
        with_identity(&db, json!({"bindings": ["name"]}));
        let (_, payload) = carry(&db, &["a.ts"]);

        let claims = payload["claims"]["bindings"].as_object().unwrap();
        assert_eq!(claims.len(), 1, "one row is being replaced, so one id is kept");
        assert_eq!(claims["f:1\u{0}one"], json!("b:1"));
    }

    #[test]
    fn a_row_that_belongs_to_no_file_is_derived_and_is_never_sent() {
        // The same rule as the table-level one, a level down. Carrying these put a copy beside each
        // freshly derived one, naming the same edge through a different call.
        let db = tmp("ownerless");
        let mut b = Batch {
            all: true,
            first: true,
            final_: true,
            lang: "typescript".to_string(),
            half: Some("typescript".to_string()),
            ..Default::default()
        };
        b.shas.insert("a.ts".to_string(), json!("sha-a"));
        b.tables.insert("files".to_string(), json!([{"id": "f:1", "path": "a.ts", "sha": "sha-a"}]));
        b.tables.insert("renders".to_string(), json!([
            {"id": "rd:1", "owner_file": "f:1", "to_class": "c:1"},
            {"id": "rd:2", "to_class": "c:2", "call": "cl:9"}
        ]));
        apply(&db, ".", &b).unwrap();

        let (out, payload) = carry(&db, &[]);
        let rows = payload["tables"]["renders"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "the one a rollup derives is left to be derived again");
        assert_eq!(rows[0]["id"], json!("rd:1"));
        assert_eq!(out.rows, 2, "it and the file row");
    }

    #[test]
    fn a_table_nothing_outside_a_file_names_hands_back_no_id_at_all() {
        // The measurement says which tables those are; a table absent from it is one whose rows are
        // named only from inside their own file, and a fresh id for one of those breaks nothing.
        let db = tmp("noclaims");
        seed(&db);
        with_identity(&db, json!({}));
        let (_, payload) = carry(&db, &["a.ts"]);
        assert!(payload["claims"].as_object().unwrap().is_empty());
    }

    #[test]
    fn a_row_whose_key_is_not_whole_is_left_unclaimable_rather_than_matched_on_less() {
        // A key that is not the whole key would match a row that merely looks alike, and the reference
        // it then kept would name the wrong declaration.
        let db = tmp("partialkey");
        seed(&db);
        with_identity(&db, json!({"bindings": ["name", "note"]}));
        let (_, payload) = carry(&db, &["a.ts"]);
        assert!(payload["claims"].as_object().unwrap().is_empty(),
                "`note` is null on that row, so it has no key");
    }

    #[test]
    fn a_table_with_no_owner_file_is_derived_and_is_never_sent() {
        // node rebuilds those whole and would throw them away. `files` is the exception.
        let db = tmp("derived");
        seed(&db);
        let (_, payload) = carry(&db, &[]);
        let tables = payload["tables"].as_object().unwrap();
        assert!(tables.contains_key("files"), "a file row IS a file");
        assert!(tables.contains_key("bindings"));
        assert!(!tables.contains_key("key_reach"), "derived, so rebuilt whole");
    }

    #[test]
    fn nothing_affected_carries_everything_that_is_not_derived() {
        let db = tmp("nothing");
        seed(&db);
        let (out, payload) = carry(&db, &[]);
        assert_eq!(payload["tables"]["files"].as_array().unwrap().len(), 2);
        assert_eq!(payload["tables"]["bindings"].as_array().unwrap().len(), 2);
        assert_eq!((out.rows, out.tables), (4, 2));
    }

    #[test]
    fn an_encoded_column_is_passed_through_as_TEXT_and_never_decoded() {
        // Turning it back into a structure only to re-encode it one line later cost more
        // than twice as long, on a run whose whole purpose is to be shorter than a rebuild.
        let db = tmp("text");
        seed(&db);
        let (_, payload) = carry(&db, &["a.ts"]);
        let row = &payload["tables"]["bindings"][0];
        assert_eq!(row["name"], json!("two"));
        let db2 = tmp("text2");
        seed(&db2);
        let (_, all) = carry(&db2, &[]);
        let first = all["tables"]["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == json!("b:1"))
            .unwrap();
        assert_eq!(first["gates_json"], json!("[\"g:1\", \"g:2\"]"), "still a string");
    }

    #[test]
    fn a_null_column_is_left_out_rather_than_carried_as_null() {
        let db = tmp("null");
        seed(&db);
        let (_, payload) = carry(&db, &[]);
        let row = payload["tables"]["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == json!("b:2"))
            .unwrap()
            .as_object()
            .unwrap();
        assert!(!row.contains_key("note"));
    }

    #[test]
    fn the_half_column_never_goes_out_because_the_reader_has_no_use_for_it() {
        let db = tmp("half");
        seed(&db);
        let (_, payload) = carry(&db, &[]);
        for table in payload["tables"].as_object().unwrap().values() {
            for row in table.as_array().unwrap() {
                assert!(!row.as_object().unwrap().contains_key(HALF_COLUMN));
            }
        }
    }

    #[test]
    fn carrying_from_a_database_that_is_not_there_says_so_rather_than_writing_an_empty_map() {
        // An empty carry file reads as "this map has nothing else in it", which is the one
        // answer a partial run must never be given.
        let missing = std::env::temp_dir().join("fbt-carry-nothing-at-all.db");
        let _ = std::fs::remove_file(&missing);
        let target = std::env::temp_dir().join("fbt-carry-nothing-at-all.json");
        let _ = std::fs::remove_file(&target);
        let err = carry_over(&missing, &[], &target, "typescript").unwrap_err();
        assert!(format!("{err:#}").contains("no database"));
        assert!(!target.exists(), "and it writes nothing");
    }

    #[test]
    fn another_halfs_rows_are_not_this_halfs_to_carry() {
        let db = tmp("other");
        let mut b = Batch {
            all: true,
            first: true,
            final_: true,
            lang: "csharp".to_string(),
            half: None,
            ..Default::default()
        };
        b.shas.insert("a.cs".to_string(), json!("sha-c"));
        b.tables.insert("files".to_string(), json!([{"id": "f:9", "path": "a.cs", "sha": "sha-c"}]));
        apply(&db, ".", &b).unwrap();
        seed(&db);

        let (_, payload) = carry(&db, &[]);
        let paths: Vec<&str> = payload["tables"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["a.ts", "b.ts"]);
    }

    #[test]
    fn the_file_is_spelled_the_way_python_spells_it() {
        // Same separators as python's `json.dump`, so two carry files can be compared byte for
        // byte rather than only after a parse.
        //
        // ONE ROW A LINE, and the last line the claims - see `carry_over` for why it is no longer
        // one document. The reader is `TsMap.mjs`, and nothing else reads this file.
        let db = tmp("spelling");
        seed(&db);
        let target = db.with_extension("carry.json");
        carry_over(&db, &[], &target, "typescript").unwrap();
        let text = std::fs::read_to_string(&target).unwrap();
        let mut lines = text.lines();
        let first = lines.next().unwrap();
        assert!(first.starts_with("{\"t\": \"bindings\", \"r\": {"), "{first}");
        assert!(first.ends_with('}'), "a line is one row and nothing else: {first}");
        assert!(text.contains("\"id\": \"b:1\""));
        assert!(!text.contains("\"id\":\"b:1\""));
        let last = text.lines().next_back().unwrap();
        assert!(last.starts_with("{\"claims\": {"), "the claims come last: {last}");
    }
}
