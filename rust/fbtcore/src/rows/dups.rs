//! `duplicate_types`: A TYPE NAME DECLARED IN MORE THAN ONE PLACE, one row per name, and whether the places
//! disagree. A tree may declare `ColorIDs` three times - one shared-library enum and two copies in a renderer's
//! sources - and each is a list somebody keeps in step by hand. The map says so and merges nothing: which
//! copy is the truth, and whether a copy may exist at all, is the consumer's rule.
//!
//! A PLACE IS A SYMBOL IN A PROJECT. A `partial` class is one place however many files it spans; the same
//! namespace and name compiled into two projects is two. A declaration with no symbol (a TypeScript enum) is
//! a place per declaration (a TypeScript `classes`, `interfaces` and `type_aliases` row, kind `class`, `interface`, `alias`; its enums too). An enum's places are compared by their members, name and value; any other type's are not.

use super::cells;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;

const KINDS: [&str; 7] = ["class", "struct", "record", "interface", "enum", "delegate", "alias"];

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
        // `members` is a list of `{name, value}`: its nodes, read back row by row.
        let mut list = Column::new(conn, "enums", "members")?;
        let sql = format!("SELECT file, name, {}, {} FROM enums e", owner_sql(conn, "enums", "e"), scalar_sql(conn, "enums", "e", "members"));
        let mut stmt = conn.prepare(&sql)?;
        let rows: Vec<(String, String, String, SqlValue)> =
            stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        for (file, name, owner, scalar) in rows {
            let held = list.value(&owner, &scalar)?;
            members.insert((file, name), canonical(&held));
        }
    }
    let mut add = |name: String, place: Place, project: String, file: String| {
        let key = if place.symbol.is_empty() { (format!("{}:{}", place.lang, place.at), file) } else { (place.symbol.clone(), project) };
        by_name.entry(name).or_default().entry(key).or_insert(place);
    };
    // THE TYPESCRIPT HALF'S `classes`, `interfaces` AND `type_aliases` HAVE NO `kind` (the table is the kind): a row of a
    // TypeScript file with none is a class / interface / alias. Reading `kind` alone listed only enums for it, since
    // every class was skipped and the other two tables were never read.
    let mut sources: Vec<(&str, &str)> = Vec::new();
    if has_columns(conn, "classes", &["file", "name", "line"]) {
        sources.push(("classes", "class"));
    }
    if has_columns(conn, "interfaces", &["file", "name", "line"]) {
        sources.push(("interfaces", "interface"));
    }
    if has_columns(conn, "type_aliases", &["file", "name", "line"]) {
        sources.push(("type_aliases", "alias"));
    }
    for (table, own) in sources {
        let kind_sql = if has_columns(conn, table, &["kind"]) { "c.kind" } else { "''" };
        let symbol = if has_columns(conn, table, &["symbol"]) { "c.symbol" } else { "''" };
        let project = if has_columns(conn, "files", &["project"]) { "f.project" } else { "''" };
        let sql = format!(
            "SELECT c.name, {kind_sql}, {symbol}, f.path, c.line, f.lang, {project}, c.file FROM {table} c JOIN files f ON f.id = c.file"
        );
        let mut stmt = conn.prepare(&sql)?;
        for row in stmt.query_map([], |r| {
            Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?, text(r, 7)?))
        })? {
            let (name, mut kind, symbol, path, line, lang, project, file) = row?;
            if kind.is_empty() && (lang == "typescript" || lang == "ts") {
                kind = own.to_string();
            }
            if !KINDS.contains(&kind.as_str()) || name.is_empty() {
                continue;
            }
            let held = if kind == "enum" { members.remove(&(file.clone(), name.clone())) } else { None };
            let place = Place { lang, kind: kind.clone(), symbol, at: format!("{path}:{line}"), members: held };
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

    // `symbols` and `at` are LISTS, so they are nodes (`duplicate_types__symbols`, `duplicate_types__at`): the table
    // and its node tables go together. Made up front, so a tree with no duplicate still has the table.
    cells::drop_children(conn, "duplicate_types").map_err(failed)?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS duplicate_types; CREATE TABLE duplicate_types (id, name, kinds, langs, places, members_differ, detail);",
    )?;
    let mut made: Vec<Value> = Vec::new();
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
        made.push(serde_json::json!({
            "id": format!("dt:{n}"),
            "name": name,
            "kinds": set(&|p| p.kind.clone()).join(","),
            "langs": set(&|p| p.lang.clone()).join(","),
            "places": places.len() as i64,
            "symbols": set(&|p| p.symbol.clone()),
            "at": set(&|p| p.at.clone()),
            "members_differ": differ,
            "detail": detail,
        }));
    }
    super::store::insert_rows(conn, "duplicate_types", &made).map_err(failed)?;
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
fn canonical(list: &Value) -> String {
    let parsed: &[Value] = list.as_array().map(Vec::as_slice).unwrap_or_default();
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

/// Whether a table holds every column - as a column of its own, or as the node table its structures went to.
pub(crate) fn has_columns(conn: &Connection, table: &str, wanted: &[&str]) -> bool {
    let held = super::schema::existing_columns(conn, table).unwrap_or_default();
    if held.is_empty() {
        return false;
    }
    let nodes = cells::columns_of(conn, table);
    wanted.iter().all(|w| held.iter().any(|c| c == w) || nodes.iter().any(|(c, _)| c == w))
}

// A LIST OR AN OBJECT IS NODES (`cells`): what the passes over finished rows read a column through. A column every
// row held a structure in has no column of its own at all, so each of these asks before it names one.

/// The node table a column's structures went to, when any row held one.
pub(crate) fn nodes_of(conn: &Connection, table: &str, column: &str) -> Option<String> {
    let child = cells::child(table, column);
    let made = cells::columns_of(conn, table).into_iter().any(|(c, t)| c == column && t == child);
    (made && !super::schema::existing_columns(conn, &child).unwrap_or_default().is_empty()).then_some(child)
}

/// A column as SQL selects it: `alias."column"`, or NULL where the table holds no scalar there.
pub(crate) fn scalar_sql(conn: &Connection, table: &str, alias: &str, column: &str) -> String {
    let held = super::schema::existing_columns(conn, table).unwrap_or_default();
    if held.iter().any(|c| c == column) { format!("{alias}.\"{}\"", super::schema::escape(column)) } else { "NULL".into() }
}

/// What a node row's `owner` is for a row of `table`, in SQL: its id, or `#<rowid>` where it has none (`store`).
pub(crate) fn owner_sql(conn: &Connection, table: &str, alias: &str) -> String {
    super::cells::owner_expr(conn, table, alias)
}

/// The ITEMS of a list column as a table: `owner`, `ord` and each of `members` (`value` for a scalar item, a member's
/// name for an object item - a node below it by role), NULL where no node carries it - a plain table, no JSON function.
pub(crate) fn items_sql(conn: &Connection, table: &str, column: &str, members: &[&str]) -> String {
    let Some(child) = nodes_of(conn, table, column) else {
        let nulls: Vec<String> = members.iter().map(|m| format!(", NULL AS \"{m}\"")).collect();
        return format!("(SELECT NULL AS owner, NULL AS ord{} WHERE 0)", nulls.concat());
    };
    let escaped = super::schema::escape(&child);
    // A scalar item is its own `value`; an object item's member is a node BELOW it, found by role (`cells`).
    let picked: Vec<String> = members
        .iter()
        .map(|m| {
            if *m == "value" {
                ", j.value AS \"value\"".to_string()
            } else {
                format!(", (SELECT x.value FROM \"{escaped}\" x WHERE x.owner = j.owner AND x.parent = j.n AND x.role = '{m}') AS \"{m}\"")
            }
        })
        .collect();
    format!("(SELECT j.owner AS owner, j.ord AS ord{} FROM \"{escaped}\" j WHERE j.parent = 0)", picked.concat())
}

/// One column read back row by row: a scalar as itself, a structure from its node table.
pub(crate) struct Column<'c> {
    reader: Option<cells::Reader<'c>>,
}

impl<'c> Column<'c> {
    pub(crate) fn new(conn: &'c Connection, table: &str, column: &str) -> rusqlite::Result<Column<'c>> {
        let reader = match nodes_of(conn, table, column) {
            Some(child) => Some(cells::Reader::new(conn, &child).map_err(failed)?),
            None => None,
        };
        Ok(Column { reader })
    }

    /// The cell of the row `owner` is, whose own column held `scalar`.
    pub(crate) fn value(&mut self, owner: &str, scalar: &SqlValue) -> rusqlite::Result<Value> {
        if !matches!(scalar, SqlValue::Null) {
            return Ok(match scalar {
                SqlValue::Integer(i) => Value::from(*i),
                SqlValue::Real(f) => Value::from(*f),
                SqlValue::Text(t) => Value::String(t.clone()),
                SqlValue::Blob(b) => Value::String(String::from_utf8_lossy(b).into_owned()),
                SqlValue::Null => Value::Null,
            });
        }
        match self.reader.as_mut() {
            Some(reader) => Ok(reader.get(owner).map_err(failed)?.unwrap_or(Value::Null)),
            None => Ok(Value::Null),
        }
    }

    /// The cell's items as strings - a list of names, the way `bases` and `reads` are held. Anything else is none.
    pub(crate) fn strings(&mut self, owner: &str, scalar: &SqlValue) -> rusqlite::Result<Vec<String>> {
        Ok(match self.value(owner, scalar)? {
            Value::Array(items) => items.iter().map(|i| i.as_str().unwrap_or("").to_string()).collect(),
            _ => Vec::new(),
        })
    }
}

fn failed(e: anyhow::Error) -> rusqlite::Error {
    match e.downcast::<rusqlite::Error>() {
        Ok(e) => e,
        Err(e) => rusqlite::Error::InvalidParameterName(format!("{e:#}")),
    }
}

pub(crate) fn text(row: &rusqlite::Row, i: usize) -> rusqlite::Result<String> {
    Ok(match row.get::<_, rusqlite::types::Value>(i)? {
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Integer(n) => n.to_string(),
        _ => String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
",
        )
        .unwrap();
        // `members` as every half stores it: nodes, through the store.
        let enums = [
            json!({"id": "en:1", "file": "f:1", "name": "ColorIDs", "line": 3, "members": [{"name": "Blue", "value": 2}, {"name": "Red", "value": 1}]}),
            json!({"id": "en:2", "file": "f:2", "name": "ColorIDs", "line": 4, "members": [{"name": "Red", "value": 1}]}),
            json!({"id": "en:3", "file": "f:5", "name": "ColorIDs", "line": 1, "members": [{"name": "Red", "value": 1}, {"name": "Blue", "value": 2}]}),
        ];
        crate::rows::store::insert_rows(&conn, "enums", &enums).unwrap();
        assert_eq!(write(&conn).unwrap(), 1);
        let found = rows(&conn);
        assert_eq!(found.len(), 1);
        let (name, places, differ, detail) = &found[0];
        assert_eq!((name.as_str(), *places, *differ), ("ColorIDs", 3, Some(1)));
        // The TypeScript copy lists the same members in another order: one set with the first C# copy.
        assert_eq!(detail, "{Blue=2, Red=1} at a/Ids.cs:3, web/ids.ts:1; {Red=1} at b/Ids.cs:4");
        // THE PLACES ARE NODES, never a JSON text: one item per place, in order.
        let at: Vec<String> = conn
            .prepare("SELECT j.value FROM duplicate_types d JOIN duplicate_types__at j ON j.owner = d.id AND j.parent = 0 ORDER BY j.ord")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(at, vec!["a/Ids.cs:3", "b/Ids.cs:4", "web/ids.ts:1"]);
        assert!(!has_columns(&conn, "duplicate_types", &["nope"]) && has_columns(&conn, "duplicate_types", &["at", "symbols"]));
        // Rewritten whole: a second run leaves one copy of every node, not two.
        write(&conn).unwrap();
        let nodes: i64 = conn.query_row("SELECT count(*) FROM duplicate_types__at", [], |r| r.get(0)).unwrap();
        assert_eq!(nodes, 4);
    }

    #[test]
    fn typescript_classes_interfaces_and_aliases_are_places_and_a_lone_one_is_not() {
        let conn = Connection::open_in_memory().unwrap();
        // The Angular half's tables: no `kind`, no `symbol`.
        conn.execute_batch(
            "CREATE TABLE files (id, path, lang, project);
             INSERT INTO files VALUES ('f:1', 'a/x.ts', 'typescript', ''), ('f:2', 'b/x.ts', 'typescript', '');
             CREATE TABLE classes (id, file, name, line);
             INSERT INTO classes VALUES ('c:1', 'f:1', 'Alpha', 1), ('c:2', 'f:2', 'Alpha', 2), ('c:3', 'f:1', 'Solo', 5);
             CREATE TABLE interfaces (id, file, name, line);
             INSERT INTO interfaces VALUES ('i:1', 'f:1', 'Shape', 7), ('i:2', 'f:2', 'Shape', 9);
             CREATE TABLE type_aliases (id, file, name, line);
             INSERT INTO type_aliases VALUES ('t:1', 'f:1', 'Id', 3), ('t:2', 'f:2', 'Id', 4);",
        )
        .unwrap();
        assert_eq!(write(&conn).unwrap(), 3);
        let kinds: Vec<(String, String)> = conn
            .prepare("SELECT name, kinds FROM duplicate_types ORDER BY name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            kinds,
            vec![("Alpha".into(), "class".into()), ("Id".into(), "alias".into()), ("Shape".into(), "interface".into())]
        );
    }
}
