//! THE LINK KINDS OF AN API SPEC: a route against the endpoints the C# half routes (`http_endpoints`), a property name
//! against a DTO's properties (C# `members`, the name its `[JsonPropertyName]`/`[JsonProperty]` gives it when it has
//! one; a TypeScript interface's or class's members), a column name against a SQL table's columns (`sql_columns`).
//! Read from rows the halves already stored - nothing here parses code.

use super::config::Link;
use super::links::Code;
use super::words::VERBS;
use crate::rows::dups::{has_columns, items_sql, text};
use rusqlite::Connection;

/// Every endpoint, `VERB /route` as its template spells it - of one controller when the link names one.
pub fn endpoints(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    if !has_columns(conn, "http_endpoints", &["verb", "template", "cls"]) {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT id, file, line, verb, template, cls, method FROM http_endpoints \
         WHERE ?1 = '' OR cls = ?1 OR cls LIKE '%.' || ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([&link.target], |r| {
        let template = text(r, 4)?;
        Ok(Code {
            target: text(r, 0)?,
            file: text(r, 1)?,
            line: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            value: format!("{} /{}", text(r, 3)?, template.trim_start_matches(['~', '/'])),
            name: format!("{}.{}", text(r, 5)?, text(r, 6)?),
            condition: String::new(),
        })
    })?;
    rows.collect()
}

/// The properties of one type: a C# class by its symbol or name (`members`, kind `property`), named by its JSON
/// attribute where it has one (`"by": "name"` keeps the C# name); a TypeScript interface, type alias or class by name.
pub fn properties(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    let mut out = Vec::new();
    if has_columns(conn, "members", &["kind", "cls", "class_symbol", "name"]) {
        let json = if has_columns(conn, "decorators", &["file", "cls", "target", "name", "arg_values"]) {
            let args = items_sql(conn, "decorators", "arg_values", &["value"]);
            let owner = crate::rows::dups::owner_sql(conn, "decorators", "d");
            format!(
                "(SELECT a.value FROM decorators d JOIN {args} a ON a.owner = {owner} AND a.ord = 0 \
                 WHERE d.file = m.file AND d.cls = m.cls AND d.target = m.name \
                   AND (d.name LIKE '%JsonPropertyName%' OR d.name LIKE '%JsonProperty' OR d.name LIKE '%JsonPropertyAttribute') LIMIT 1)"
            )
        } else {
            "NULL".to_string()
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT m.id, m.file, m.line, m.name, {json} FROM members m \
             WHERE m.kind = 'property' AND (m.class_symbol = ?1 OR m.cls = ?1) ORDER BY m.id"
        ))?;
        let rows = stmt.query_map([&link.target], |r| {
            let (name, json) = (text(r, 3)?, text(r, 4)?);
            let value = if json.is_empty() || link.by == "name" { name.clone() } else { json };
            Ok(Code { target: text(r, 0)?, file: text(r, 1)?, line: r.get::<_, Option<i64>>(2)?.unwrap_or(0), value, name, condition: String::new() })
        })?;
        out.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    // TYPESCRIPT: an interface's or an alias's members are `type_members` by `owner`, a class's `members` by `class`.
    let mut typescript = Vec::new();
    if has_columns(conn, "type_members", &["owner", "name"]) {
        let owners: Vec<&str> = ["interfaces", "type_aliases"].into_iter().filter(|t| has_columns(conn, t, &["id", "name"])).collect();
        if !owners.is_empty() {
            let named: Vec<String> = owners.iter().map(|t| format!("SELECT id FROM {t} WHERE name = ?1")).collect();
            typescript.push(format!("SELECT t.id, t.file, t.line, t.name FROM type_members t WHERE t.owner IN ({})", named.join(" UNION ")));
        }
    }
    if has_columns(conn, "members", &["class", "kind", "name"]) && has_columns(conn, "classes", &["id", "name"]) {
        typescript.push("SELECT m.id, m.file, m.line, m.name FROM members m WHERE m.kind = 'property' AND m.class IN (SELECT id FROM classes WHERE name = ?1)".into());
    }
    for sql in typescript {
        let mut stmt = conn.prepare(&format!("{sql} ORDER BY 1"))?;
        let rows = stmt.query_map([&link.target], |r| {
            let name = text(r, 3)?;
            Ok(Code { target: text(r, 0)?, file: text(r, 1)?, line: r.get::<_, Option<i64>>(2)?.unwrap_or(0), value: name.clone(), name, condition: String::new() })
        })?;
        out.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    Ok(out)
}

/// The columns of a SQL table, `Schema.Table` (`dbo` when no schema is given).
pub fn columns(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    if !has_columns(conn, "sql_columns", &["object", "name"]) || !has_columns(conn, "sql_objects", &["id", "schema", "name"]) {
        return Ok(vec![]);
    }
    let (schema, name) = schema_and_name(&link.target);
    let place = if has_columns(conn, "sql_columns", &["file", "line"]) { "c.file, c.line" } else { "o.id, 0" };
    let mut stmt = conn.prepare(&format!(
        "SELECT c.id, {place}, c.name FROM sql_columns c JOIN sql_objects o ON o.id = c.object \
         WHERE lower(CASE coalesce(o.schema, '') WHEN '' THEN 'dbo' ELSE o.schema END) = ?1 AND lower(o.name) = ?2 ORDER BY c.id"
    ))?;
    let rows = stmt.query_map([&schema, &name], |r| {
        let name = text(r, 3)?;
        Ok(Code { target: text(r, 0)?, file: text(r, 1)?, line: r.get::<_, Option<i64>>(2)?.unwrap_or(0), value: name.clone(), name, condition: String::new() })
    })?;
    rows.collect()
}

/// `[dbo].[Items]` as (`dbo`, `items`): lowercase, brackets off, `dbo` when no schema is written.
pub fn schema_and_name(target: &str) -> (String, String) {
    let clean = |s: &str| s.trim().trim_matches(['[', ']', '"']).to_lowercase();
    match target.rsplit_once('.') {
        Some((s, n)) => (clean(s), clean(n)),
        None => ("dbo".to_string(), clean(target)),
    }
}

/// A route as its verb (None when it names none, or for `ANY`) and its segments: lowercase, a parameter - `{id}`,
/// `{id:int}`, `:id` - one `{}`, an optional one `{}?`. A query string is no part of it.
pub fn route_parts(route: &str) -> (Option<String>, Vec<String>) {
    let route = route.trim();
    let (verb, path) = match route.split_once(' ') {
        Some((v, p)) if VERBS.contains(&v.to_uppercase().as_str()) || v.eq_ignore_ascii_case("ANY") => (Some(v.to_uppercase()), p.trim()),
        _ => (None, route),
    };
    let path = super::words::without_query(path).to_lowercase();
    let segments = path
        .split('/')
        .filter(|s| !s.is_empty() && *s != "~")
        .map(|s| match (s.contains('{'), s.starts_with(':')) {
            (true, _) if s.ends_with("?}") => "{}?".to_string(),
            (true, _) | (_, true) => "{}".to_string(),
            _ => s.to_string(),
        })
        .collect();
    (verb.filter(|v| v != "ANY"), segments)
}

/// A documented route against an endpoint's: the same segments - an optional one there or not, a parameter there any
/// one segment - and the same verb when both name one.
pub fn same_endpoint(code: &str, fact: &str) -> bool {
    let ((code_verb, code_path), (fact_verb, fact_path)) = (route_parts(code), route_parts(fact));
    if let (Some(a), Some(b)) = (&code_verb, &fact_verb)
        && a != b
    {
        return false;
    }
    let fact_path: Vec<String> = fact_path.into_iter().map(|s| if s == "{}?" { "{}".into() } else { s }).collect();
    // A PARAMETER THERE takes any one segment here: a spec writes `/api/items/{id}` or an example, `/api/items/42`.
    variants(&code_path).iter().any(|v| v.len() == fact_path.len() && v.iter().zip(&fact_path).all(|(c, f)| c == "{}" || c == f))
}

/// The segment lists a route answers to: with and without each optional segment.
fn variants(route: &[String]) -> Vec<Vec<String>> {
    let mut all: Vec<Vec<String>> = vec![vec![]];
    for seg in route {
        if seg == "{}?" {
            let mut with = all.clone();
            with.iter_mut().for_each(|v| v.push("{}".to_string()));
            all.extend(with);
        } else {
            all.iter_mut().for_each(|v| v.push(seg.clone()));
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_documented_route_matches_an_endpoint_by_segments_and_verb() {
        assert!(same_endpoint("GET /api/Items/{id:int}", "GET /api/items/{itemId}"));
        assert!(same_endpoint("GET /api/items/{id}", "/api/items/:id"));
        assert!(same_endpoint("ANY /api/items", "DELETE /api/items/"));
        assert!(!same_endpoint("GET /api/items/{id}", "POST /api/items/{id}"));
        assert!(!same_endpoint("GET /api/items/{id}", "GET /api/items"));
        assert!(same_endpoint("GET /api/items/{id?}", "GET /api/items"));
        assert!(same_endpoint("GET /api/items/{id?}", "GET /api/items/7x?q=1"));
    }
}
