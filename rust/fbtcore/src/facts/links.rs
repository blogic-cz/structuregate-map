//! WHAT A LIST OF FACTS IS CHECKED AGAINST: the code values a declared link names, read from rows the halves
//! already stored - a seeded table's column (`sql_seeds`), an enum's members (`enums`, either half), the
//! constants a class declares (`consts`), the classes deriving from a type (`classes.bases`). Nothing here parses.

use super::config::Link;
use rusqlite::Connection;
use serde_json::Value;

/// One value the code holds: the row it is, where, and what it is called there.
#[derive(Debug, Clone, Default)]
pub struct Code {
    pub target: String,
    pub file: String,
    pub line: i64,
    pub value: String,
    pub name: String,
    pub condition: String,
}

/// The code values of one link, or why there are none to read.
pub fn codes(conn: &Connection, link: &Link) -> Result<Vec<Code>, String> {
    let found = match link.kind.as_str() {
        "seed" => seed(conn, link),
        "enum" => members(conn, link),
        "consts" => consts(conn, link),
        "class" => derived(conn, link),
        other => return Err(format!("no link kind `{other}`")),
    }
    .map_err(|e| format!("{} `{}` could not be read - {e}", link.kind, link.target))?;
    if found.is_empty() {
        return Err(format!("{} `{}` holds no value in this map", link.kind, link.target));
    }
    Ok(found)
}

/// Two values the same: equal text, or equal numbers (`42` and `42.0`, a doc's `042` and the code's `42`).
pub fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a == b {
        return true;
    }
    matches!((a.parse::<f64>(), b.parse::<f64>()), (Ok(x), Ok(y)) if x == y)
}

fn seed(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    let (schema, name) = match link.target.rsplit_once('.') {
        Some((s, n)) => (s.trim_matches(['[', ']']).to_lowercase(), n.trim_matches(['[', ']']).to_lowercase()),
        None => ("dbo".to_string(), link.target.trim_matches(['[', ']']).to_lowercase()),
    };
    let mut stmt = conn.prepare(
        "SELECT s.id, s.file, s.line, s.condition, \
                (SELECT json_extract(s.row_values, '$[' || c.key || ']') FROM json_each(s.columns) c WHERE lower(c.value) = lower(?3)) \
         FROM sql_seeds s WHERE s.status <> 'temp' AND lower(CASE s.schema WHEN '' THEN 'dbo' ELSE s.schema END) = ?1 \
           AND lower(s.name) = ?2 ORDER BY s.id",
    )?;
    let rows = stmt.query_map([&schema, &name, &link.column], |r| {
        Ok(Code { target: text(r, 0)?, file: text(r, 1)?, line: r.get::<_, Option<i64>>(2)?.unwrap_or(0), condition: text(r, 3)?, value: text(r, 4)?, name: String::new() })
    })?;
    // A ROW THAT DOES NOT SET THE COLUMN has no value to check, and is not a value missing from the doc.
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter(|c| !c.value.is_empty()).collect())
}

/// The members of the enum the link names: by its symbol, or by its name when that names exactly one.
fn members(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    let symbol = has(conn, "enums", "symbol");
    let pick = |by: &str| -> rusqlite::Result<Vec<(String, String, i64, String)>> {
        let mut stmt = conn.prepare(&format!("SELECT id, file, line, members FROM enums WHERE {by} = ?1 ORDER BY id"))?;
        stmt.query_map([&link.target], |r| Ok((text(r, 0)?, text(r, 1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), text(r, 3)?)))?
            .collect()
    };
    let mut found = if symbol { pick("symbol")? } else { Vec::new() };
    if found.is_empty() {
        found = pick("name")?;
    }
    if found.len() > 1 {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "{} enums are named so - give its symbol (duplicate_types lists them)",
            found.len()
        )));
    }
    let mut out = Vec::new();
    for (id, file, line, list) in found {
        for member in serde_json::from_str::<Vec<Value>>(&list).unwrap_or_default() {
            let name = member["name"].as_str().unwrap_or("").to_string();
            let value = match &member["value"] {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            out.push(Code { target: id.clone(), file: file.clone(), line, value: if link.by == "name" { name.clone() } else { value }, name, condition: String::new() });
        }
    }
    Ok(out)
}

/// The constants a class declares: the class by its symbol (or name), its constants by that class and its files.
fn consts(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    let by = if has(conn, "classes", "symbol") { "(symbol = ?1 OR (symbol = '' AND name = ?1) OR (symbol IS NULL AND name = ?1))" } else { "name = ?1" };
    let mut stmt = conn.prepare(&format!(
        "SELECT k.id, k.file, k.line, k.name, k.value FROM consts k \
         JOIN (SELECT DISTINCT file, name FROM classes WHERE {by}) c ON c.file = k.file AND c.name = k.cls ORDER BY k.id"
    ))?;
    let rows = stmt.query_map([&link.target], |r| {
        let (name, value) = (text(r, 3)?, text(r, 4)?);
        Ok(Code { target: text(r, 0)?, file: text(r, 1)?, line: r.get::<_, Option<i64>>(2)?.unwrap_or(0), value: if link.by == "name" { name.clone() } else { value }, name, condition: String::new() })
    })?;
    rows.collect()
}

/// The classes that list the link's type among their bases, by their name.
fn derived(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.file, c.line, c.name, coalesce(c.symbol, '') FROM classes c, json_each(c.bases) b \
         WHERE b.value = ?1 OR b.value LIKE '%.' || ?1 ORDER BY c.id",
    )?;
    let rows = stmt.query_map([&link.target], |r| {
        let name = text(r, 3)?;
        Ok(Code { target: text(r, 0)?, file: text(r, 1)?, line: r.get::<_, Option<i64>>(2)?.unwrap_or(0), value: name.clone(), name: text(r, 4)?, condition: String::new() })
    })?;
    rows.collect()
}

fn has(conn: &Connection, table: &str, column: &str) -> bool {
    crate::rows::schema::existing_columns(conn, table).unwrap_or_default().iter().any(|c| c == column)
}

fn text(row: &rusqlite::Row, i: usize) -> rusqlite::Result<String> {
    Ok(match row.get::<_, rusqlite::types::Value>(i)? {
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Integer(n) => n.to_string(),
        rusqlite::types::Value::Real(f) => f.to_string(),
        _ => String::new(),
    })
}
