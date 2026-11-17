//! THE LINK KINDS OF AN API SPEC: a route against the endpoints the C# half routes (`http_endpoints`), a property name
//! against a DTO's properties (C# `members` - its own and every base class's - named by its `[JsonPropertyName]`,
//! `[JsonProperty]` or `[DataMember(Name = ..)]` when it has one; a TypeScript interface's or class's members), a column name against a SQL table's columns (`sql_columns`).
//! Read from rows the halves already stored - nothing here parses code.

use super::config::Link;
use super::links::Code;
use super::words::VERBS;
use crate::rows::dups::{has_columns, owner_sql, scalar_sql, text, Column};
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

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
            typed: String::new(),
        })
    })?;
    rows.collect()
}

/// The properties of one type: a C# class by its symbol or name (`members`, kind `property`), named by its JSON
/// attribute where it has one (`"by": "name"` keeps the C# name); a TypeScript interface, type alias or class by name.
pub fn properties(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    Ok(Properties::read(conn)?.codes(&link.target, &link.by))
}

/// Every type's properties in one read of each table, by every name a link may give the type: one link reads it for
/// one type, `--facts-suggest` for thousands - a query per type scanned `members` once per type. A C# class's bases
/// (`classes.bases`, type arguments left out) by its symbol, and the symbols a bare name stands for, so a class answers
/// with what it inherits too.
pub struct Properties(HashMap<String, Vec<Code>>, HashMap<String, Vec<String>>, HashMap<String, Vec<String>>);

/// The type a property's rows describe: an array's, a list's, a nullable's element, a map's value - `Demo.Item[]`,
/// `List<Demo.Item>`, `Demo.Item?` and `IDictionary<string, List<Demo.Item>>` are each `Demo.Item`.
pub fn element(typed: &str) -> String {
    let mut at = typed.trim().to_string();
    loop {
        let before = at.clone();
        at = at.trim_end_matches('?').trim_end_matches("[]").trim().to_string();
        if let (Some(open), true) = (at.find('<'), at.ends_with('>')) {
            let inner = &at[open + 1..at.len() - 1];
            let (mut nested, mut last) = (0usize, 0usize);
            for (i, c) in inner.char_indices() {
                match c {
                    '<' => nested += 1,
                    '>' => nested = nested.saturating_sub(1),
                    ',' if nested == 0 => last = i + 1,
                    _ => {}
                }
            }
            at = inner[last..].trim().to_string();
        }
        if at == before {
            return at;
        }
    }
}

/// A symbol without its type arguments: `Demo.Base<T>` is `Demo.Base`.
fn bare(symbol: &str) -> String {
    symbol.split('<').next().unwrap_or(symbol).to_string()
}

/// The JSON name an attribute's folded arguments give a property: `PropertyName=x` or `Name=x` wherever it stands,
/// else the first argument when it is positional (`[JsonPropertyName("x")]`, `[JsonProperty("x")]`); `DataMember`
/// names one only by `Name=`.
fn json_name(attribute: &str, arguments: &[String]) -> Option<String> {
    let named = |a: &String| a.split_once('=').filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_')).map(|(n, v)| (n.to_string(), v.to_string()));
    if let Some((_, v)) = arguments.iter().filter_map(named).find(|(n, _)| n == "PropertyName" || n == "Name") {
        return Some(v).filter(|v| !v.is_empty());
    }
    let first = arguments.first().filter(|a| named(a).is_none())?;
    (!attribute.contains("DataMember") && !first.is_empty()).then(|| first.clone())
}

impl Properties {
    pub fn read(conn: &Connection) -> rusqlite::Result<Properties> {
        let mut out: HashMap<String, Vec<Code>> = HashMap::new();
        if has_columns(conn, "members", &["kind", "cls", "class_symbol", "name"]) {
            // THE JSON NAME of each (file, class, member), the first an attribute gives it.
            let mut json: HashMap<(String, String, String), String> = HashMap::new();
            if has_columns(conn, "decorators", &["file", "cls", "target", "name", "arg_values"]) {
                let mut args = Column::new(conn, "decorators", "arg_values")?;
                let mut stmt = conn.prepare(&format!(
                    "SELECT d.file, d.cls, d.target, d.name, {}, {} FROM decorators d \
                     WHERE d.name LIKE '%JsonPropertyName%' OR d.name LIKE '%JsonProperty' OR d.name LIKE '%JsonPropertyAttribute' \
                     OR d.name LIKE '%DataMember' OR d.name LIKE '%DataMemberAttribute' ORDER BY d.rowid",
                    owner_sql(conn, "decorators", "d"),
                    scalar_sql(conn, "decorators", "d", "arg_values")
                ))?;
                let rows: Vec<_> = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, r.get::<_, SqlValue>(5)?)))?.collect::<rusqlite::Result<_>>()?;
                for (file, cls, target, name, owner, cell) in rows {
                    if let Some(value) = json_name(&name, &args.strings(&owner, &cell)?) {
                        json.entry((file, cls, target)).or_insert(value);
                    }
                }
            }
            let mut stmt = conn.prepare(&format!(
                "SELECT m.id, m.file, m.line, m.name, m.cls, m.class_symbol, {} FROM members m WHERE m.kind = 'property' ORDER BY m.id",
                scalar_sql(conn, "members", "m", "type")
            ))?;
            for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?)))? {
                let (id, file, line, name, cls, symbol, typed) = row?;
                let value = json.get(&(file.clone(), cls.clone(), name.clone())).filter(|j| !j.is_empty()).cloned().unwrap_or_else(|| name.clone());
                let code = Code { target: id, file, line, value, name, condition: String::new(), typed };
                // A C# TYPE IS ASKED FOR by its symbol (with or without its type parameters) or by its bare name.
                let mut keys = vec![symbol.clone(), bare(&symbol), cls];
                keys.dedup();
                for key in keys.into_iter().filter(|k| !k.is_empty()) {
                    out.entry(key).or_default().push(code.clone());
                }
            }
        }
        // TYPESCRIPT: an interface's or an alias's members are `type_members` by `owner`, a class's `members` by `class`.
        let mut typescript = Vec::new();
        if has_columns(conn, "type_members", &["owner", "name"]) {
            let owners: Vec<&str> = ["interfaces", "type_aliases"].into_iter().filter(|t| has_columns(conn, t, &["id", "name"])).collect();
            if !owners.is_empty() {
                let named: Vec<String> = owners.iter().map(|t| format!("SELECT id, name FROM {t}")).collect();
                typescript.push(format!("SELECT t.id, t.file, t.line, t.name, o.name, {} FROM type_members t JOIN ({}) o ON o.id = t.owner", scalar_sql(conn, "type_members", "t", "type"), named.join(" UNION ")));
            }
        }
        if has_columns(conn, "members", &["class", "kind", "name"]) && has_columns(conn, "classes", &["id", "name"]) {
            typescript.push(format!(
                "SELECT m.id, m.file, m.line, m.name, c.name, {} FROM members m JOIN (SELECT DISTINCT id, name FROM classes) c ON c.id = m.class WHERE m.kind = 'property'",
                scalar_sql(conn, "members", "m", "type")
            ));
        }
        for sql in typescript {
            let mut stmt = conn.prepare(&format!("{sql} ORDER BY 1"))?;
            for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), text(r, 3)?, text(r, 4)?, text(r, 5)?)))? {
                let (id, file, line, name, owner, typed) = row?;
                out.entry(owner).or_default().push(Code { target: id, file, line, value: name.clone(), name, condition: String::new(), typed });
            }
        }
        // EACH C# CLASS'S BASES, and the symbols each bare name stands for.
        let (mut bases, mut symbols): (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) = (HashMap::new(), HashMap::new());
        if has_columns(conn, "classes", &["name", "symbol", "bases"]) {
            let mut list = Column::new(conn, "classes", "bases")?;
            let sql = format!("SELECT k.name, k.symbol, {}, {} FROM classes k", owner_sql(conn, "classes", "k"), scalar_sql(conn, "classes", "k", "bases"));
            let mut stmt = conn.prepare(&sql)?;
            let rows: Vec<_> = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, r.get::<_, SqlValue>(3)?)))?.collect::<rusqlite::Result<_>>()?;
            for (name, symbol, owner, cell) in rows {
                if symbol.is_empty() {
                    continue;
                }
                bases.entry(bare(&symbol)).or_default().extend(list.strings(&owner, &cell)?.iter().map(|b| bare(b)));
                symbols.entry(name).or_default().push(bare(&symbol));
            }
        }
        Ok(Properties(out, bases, symbols))
    }

    /// The properties of the type `target` names, then those it inherits that it does not declare again, base by base
    /// (a visited set ends bases that name each other); `"name"` names each by its C# name, not its JSON one.
    pub fn codes(&self, target: &str, by: &str) -> Vec<Code> {
        let mut out = self.0.get(target).cloned().unwrap_or_default();
        let mut named: HashSet<String> = out.iter().map(|c| c.name.clone()).collect();
        let mut seen: HashSet<String> = HashSet::new();
        let mut todo: Vec<String> = self.2.get(target).cloned().unwrap_or_else(|| vec![bare(target)]);
        while let Some(at) = todo.pop() {
            if !seen.insert(at.clone()) {
                continue;
            }
            for base in self.1.get(&at).into_iter().flatten() {
                for code in self.0.get(base).into_iter().flatten() {
                    if named.insert(code.name.clone()) {
                        out.push(code.clone());
                    }
                }
                todo.push(base.clone());
            }
        }
        if by == "name" {
            out.iter_mut().for_each(|c| c.value = c.name.clone());
        }
        out
    }
}

/// The columns of a SQL table, `Schema.Table` (`dbo` when no schema is given).
pub fn columns(conn: &Connection, link: &Link) -> rusqlite::Result<Vec<Code>> {
    Ok(Columns::read(conn)?.codes(&link.target))
}

/// Every SQL table's columns in one read, by (schema, table) lowercase.
pub struct Columns(HashMap<(String, String), Vec<Code>>);

impl Columns {
    pub fn read(conn: &Connection) -> rusqlite::Result<Columns> {
        let mut out: HashMap<(String, String), Vec<Code>> = HashMap::new();
        if !has_columns(conn, "sql_columns", &["object", "name"]) || !has_columns(conn, "sql_objects", &["id", "schema", "name"]) {
            return Ok(Columns(out));
        }
        let place = if has_columns(conn, "sql_columns", &["file", "line"]) { "c.file, c.line" } else { "o.id, 0" };
        let mut stmt = conn.prepare(&format!(
            "SELECT c.id, {place}, c.name, lower(CASE coalesce(o.schema, '') WHEN '' THEN 'dbo' ELSE o.schema END), lower(o.name) \
             FROM sql_columns c JOIN sql_objects o ON o.id = c.object ORDER BY c.id"
        ))?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), text(r, 3)?, text(r, 4)?, text(r, 5)?)))? {
            let (id, file, line, name, schema, table) = row?;
            out.entry((schema, table)).or_default().push(Code { target: id, file, line, value: name.clone(), name, condition: String::new(), typed: String::new() });
        }
        Ok(Columns(out))
    }

    pub fn codes(&self, target: &str) -> Vec<Code> {
        self.0.get(&schema_and_name(target)).cloned().unwrap_or_default()
    }
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
