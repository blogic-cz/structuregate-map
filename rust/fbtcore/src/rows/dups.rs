//! `duplicate_types`: A TYPE NAME DECLARED IN MORE THAN ONE PLACE, one row per name, and whether the places
//! disagree. A tree may declare `ColorIDs` three times - one shared-library enum and two copies in a renderer's
//! sources - and each is a list somebody keeps in step by hand. The map says so and merges nothing: which
//! copy is the truth, and whether a copy may exist at all, is the consumer's rule.
//!
//! A PLACE IS A SYMBOL IN A PROJECT. A `partial` class is one place however many files it spans; the same
//! namespace and name compiled into two projects is two. A declaration with no symbol (a TypeScript enum) is
//! a place per file. An enum's places are compared by their members, name and value; any other type's are not.

use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const KINDS: [&str; 6] = ["class", "struct", "record", "interface", "enum", "delegate"];

#[derive(Clone)]
struct Place {
    lang: String,
    kind: String,
    symbol: String,
    at: String,
    members: Option<String>,
}

/// The table rewritten whole. The number of names it holds.
pub fn write(conn: &Connection) -> rusqlite::Result<usize> {
    let mut by_name: BTreeMap<String, BTreeMap<(String, String), Place>> = BTreeMap::new();
    let mut members: BTreeMap<(String, String), String> = BTreeMap::new();
    let enums = has_columns(conn, "enums", &["file", "name", "members"]);
    if enums {
        let mut stmt = conn.prepare("SELECT file, name, members FROM enums")?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?)))? {
            let (file, name, list) = row?;
            members.insert((file, name), canonical(&list));
        }
    }
    let mut add = |name: String, place: Place, project: String, file: String| {
        let key = if place.symbol.is_empty() { (format!("{}:{}", place.lang, place.at), file) } else { (place.symbol.clone(), project) };
        by_name.entry(name).or_default().entry(key).or_insert(place);
    };
    if has_columns(conn, "classes", &["file", "name", "kind", "line"]) {
        let symbol = if has_columns(conn, "classes", &["symbol"]) { "c.symbol" } else { "''" };
        let project = if has_columns(conn, "files", &["project"]) { "f.project" } else { "''" };
        let sql = format!(
            "SELECT c.name, c.kind, {symbol}, f.path, c.line, f.lang, {project}, c.file FROM classes c JOIN files f ON f.id = c.file"
        );
        let mut stmt = conn.prepare(&sql)?;
        for row in stmt.query_map([], |r| {
            Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?, text(r, 7)?))
        })? {
            let (name, kind, symbol, path, line, lang, project, file) = row?;
            if !KINDS.contains(&kind.as_str()) || name.is_empty() {
                continue;
            }
            let held = members.remove(&(file.clone(), name.clone()));
            let place = Place { lang, kind: kind.clone(), symbol, at: format!("{path}:{line}"), members: held.filter(|_| kind == "enum") };
            add(name, place, project, file);
        }
    }
    // AN ENUM NO `classes` ROW NAMES - the TypeScript half keeps its enums apart.
    if enums && !members.is_empty() {
        let mut stmt = conn.prepare("SELECT e.file, e.name, f.path, e.line, f.lang FROM enums e JOIN files f ON f.id = e.file")?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?)))? {
            let (file, name, path, line, lang) = row?;
            let Some(list) = members.get(&(file.clone(), name.clone())) else { continue };
            let place = Place { lang, kind: "enum".into(), symbol: String::new(), at: format!("{path}:{line}"), members: Some(list.clone()) };
            add(name, place, String::new(), file);
        }
    }

    conn.execute_batch(
        "DROP TABLE IF EXISTS duplicate_types; CREATE TABLE duplicate_types (id, name, kinds, langs, places, symbols, at, members_differ, detail);",
    )?;
    let mut insert = conn.prepare("INSERT INTO duplicate_types VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?;
    let mut n = 0;
    for (name, places) in &by_name {
        if places.len() < 2 {
            continue;
        }
        let places: Vec<&Place> = places.values().collect();
        let set = |pick: &dyn Fn(&Place) -> String| -> Vec<String> {
            let mut out: Vec<String> = places.iter().map(|p| pick(p)).filter(|s| !s.is_empty()).collect();
            out.sort();
            out.dedup();
            out
        };
        let (differ, detail) = compared(&places);
        n += 1;
        insert.execute(rusqlite::params![
            format!("dt:{n}"),
            name,
            set(&|p| p.kind.clone()).join(","),
            set(&|p| p.lang.clone()).join(","),
            places.len() as i64,
            json!(set(&|p| p.symbol.clone())).to_string(),
            json!(set(&|p| p.at.clone())).to_string(),
            differ,
            detail,
        ])?;
    }
    conn.execute_batch("CREATE INDEX ix_duplicate_types_name ON duplicate_types(name);")?;
    Ok(n)
}

/// Whether the places' members differ - NULL unless every place is an enum with members - and which sets there are.
fn compared(places: &[&Place]) -> (Option<i64>, String) {
    let lists: Vec<&String> = places.iter().filter_map(|p| p.members.as_ref()).collect();
    if lists.len() != places.len() {
        return (None, String::new());
    }
    let mut sets: BTreeMap<&String, Vec<&str>> = BTreeMap::new();
    for place in places {
        if let Some(list) = &place.members {
            sets.entry(list).or_default().push(place.at.as_str());
        }
    }
    let detail = sets.iter().map(|(list, at)| format!("{list} at {}", at.join(", "))).collect::<Vec<_>>().join("; ");
    (Some(i64::from(sets.len() > 1)), detail)
}

/// `[{name, value}]` as `{A=1, B=2}`, sorted by name, so two copies listed in a different order are one set.
fn canonical(list: &str) -> String {
    let parsed: Vec<Value> = serde_json::from_str(list).unwrap_or_default();
    let mut pairs: Vec<String> = parsed
        .iter()
        .map(|m| {
            let value = match &m["value"] {
                Value::String(s) => format!("'{s}'"),
                Value::Null => "?".into(),
                other => other.to_string(),
            };
            format!("{}={value}", m["name"].as_str().unwrap_or(""))
        })
        .collect();
    pairs.sort();
    format!("{{{}}}", pairs.join(", "))
}

fn has_columns(conn: &Connection, table: &str, wanted: &[&str]) -> bool {
    let held = super::schema::existing_columns(conn, table).unwrap_or_default();
    wanted.iter().all(|w| held.iter().any(|c| c == w))
}

fn text(row: &rusqlite::Row, i: usize) -> rusqlite::Result<String> {
    Ok(match row.get::<_, rusqlite::types::Value>(i)? {
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Integer(n) => n.to_string(),
        _ => String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(conn: &Connection) -> Vec<(String, i64, Option<i64>, String)> {
        conn.prepare("SELECT name, places, members_differ, detail FROM duplicate_types ORDER BY name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn a_name_in_two_projects_is_reported_and_a_partial_class_is_not() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE files (id, path, lang, project);
             INSERT INTO files VALUES ('f:1', 'a/Ids.cs', 'csharp', 'A'), ('f:2', 'b/Ids.cs', 'csharp', 'B'),
                                      ('f:3', 'a/P1.cs', 'csharp', 'A'), ('f:4', 'a/P2.cs', 'csharp', 'A'),
                                      ('f:5', 'web/ids.ts', 'typescript', '');
             CREATE TABLE classes (file, name, kind, line, symbol);
             INSERT INTO classes VALUES ('f:1', 'ColorIDs', 'enum', 3, 'X.ColorIDs'), ('f:2', 'ColorIDs', 'enum', 4, 'X.ColorIDs'),
                                        ('f:3', 'Partial', 'class', 1, 'X.Partial'), ('f:4', 'Partial', 'class', 1, 'X.Partial');
             CREATE TABLE enums (file, name, line, members);
             INSERT INTO enums VALUES ('f:1', 'ColorIDs', 3, '[{\"name\": \"Blue\", \"value\": 2}, {\"name\": \"Red\", \"value\": 1}]'),
                                      ('f:2', 'ColorIDs', 4, '[{\"name\": \"Red\", \"value\": 1}]'),
                                      ('f:5', 'ColorIDs', 1, '[{\"name\": \"Red\", \"value\": 1}, {\"name\": \"Blue\", \"value\": 2}]');",
        )
        .unwrap();
        assert_eq!(write(&conn).unwrap(), 1);
        let found = rows(&conn);
        assert_eq!(found.len(), 1);
        let (name, places, differ, detail) = &found[0];
        assert_eq!((name.as_str(), *places, *differ), ("ColorIDs", 3, Some(1)));
        // The TypeScript copy lists the same members in another order: one set with the first C# copy.
        assert_eq!(detail, "{Blue=2, Red=1} at a/Ids.cs:3, web/ids.ts:1; {Red=1} at b/Ids.cs:4");
    }
}
