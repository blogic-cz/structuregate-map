//! A KEY FOR EVERY ENTITY, THE SAME IN EVERY BUILD (#130): `entity_keys`, one row per (table, row id) - `tbl`, `id`, `lang`, `key`.
//!
//! A row id is build-local - a fresh map numbers rows again in path order, so one new file shifts every id after it - and a
//! consumer keeping its own data about an entity (a verdict per method, a note per column) needs to join on something that does
//! not move. The key is built only from what the source declares, and is unique in its table - one entity two tables hold (a C# enum
//! is a `classes` and an `enums` row, a property a `functions` and a `members` row) has one key in both:
//!
//! | entity | key |
//! |---|---|
//! | a file | `file:<path from its root>` |
//! | C# type, member, enum | `cs:<symbol>`; a method or constructor `cs:<symbol>(<parameter types>)`, so overloads differ |
//! | python, plain TypeScript | `py:<path>#<qualname>`, `ts:<path>#<qualname>` |
//! | Angular-half declaration | `ts:<path>#<Name>`, a member `ts:<path>#<Class>.<name>`; its component/directive/pipe/injectable/module row `ng:<path>#<Class>` |
//! | an enum member | the enum's key `.<Member>`, the row id `<enum id>#<ord>` (`enums__members`' item at `parent = 0`, that `ord`) |
//! | SQL object, column | `sql:<schema>.<name>`, `sql:<schema>.<table>.<column>`, `@<database>` when it has one |
//! | HTTP endpoint, call | `http:<VERB> <route>`, `httpcall:<VERB> <url>` |
//!
//! A KEY TWO ROWS OF ONE TABLE WOULD SHARE (a partial type, two declarations of one name) takes `@<path>` when it has no path in it, then `~<n>`
//! in (path, line, id) order. A row with no name of its own - an anonymous callback, an expression - has no key. EVERY LINK BY ROW
//! ID IS ONE JOIN AWAY: `JOIN entity_keys k ON k.id = sql_columns.object` gives the object's key, and a stored key finds its row
//! again with `WHERE key = ?` (and `tbl`, to pick which table's row). Rewritten whole after every half, like `sql_links`.

use crate::rows::dups::has_columns;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::path::Path;

mod source;
pub use source::source_state;

/// One keyed row before the duplicates are told apart.
struct Entry {
    tbl: &'static str,
    id: String,
    lang: String,
    key: String,
    path: String,
    line: i64,
}

/// The table, rewritten whole; how many rows it holds.
pub fn write(conn: &Connection, roots: &[String]) -> Result<usize> {
    conn.execute_batch("DROP TABLE IF EXISTS entity_keys; CREATE TABLE entity_keys (tbl, id, lang, key);")?;
    if !has_columns(conn, "files", &["id", "path", "lang"]) {
        return Ok(0);
    }
    let mut out: Vec<Entry> = Vec::new();
    let paths = files(conn, roots, &mut out)?;
    let path_of = |file: &str| paths.get(file).cloned().unwrap_or_default();
    let half = |table: &str| has_columns(conn, table, &["half"]);
    csharp(conn, &path_of, &mut out)?;
    named(conn, &path_of, &mut out, half)?;
    sql(conn, &path_of, &mut out)?;
    http(conn, &path_of, &mut out)?;
    distinct(&mut out);
    let mut add = conn.prepare("INSERT INTO entity_keys VALUES (?1, ?2, ?3, ?4)")?;
    for e in &out {
        add.execute(params![e.tbl, e.id, e.lang, e.key])?;
    }
    conn.execute_batch("CREATE INDEX ix_entity_keys_id ON entity_keys(id); CREATE UNIQUE INDEX ux_entity_keys_key ON entity_keys(key, tbl);")?;
    Ok(out.len())
}

/// Every file's key; file id -> its path from its root. A row holding an absolute `abs` (the TypeScript half's path is the
/// workspace's) is measured from the first root that holds it, so two machines agree.
fn files(conn: &Connection, roots: &[String], out: &mut Vec<Entry>) -> Result<HashMap<String, String>> {
    let abs = has_columns(conn, "files", &["abs"]);
    let sql = if abs { "SELECT id, path, lang, abs FROM files" } else { "SELECT id, path, lang, NULL FROM files" };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?)))?;
    let mut paths = HashMap::new();
    for row in rows {
        let (id, path, lang, abs) = row?;
        let from_root = abs.as_deref().and_then(|a| relative(a, roots));
        let Some(path) = from_root.or(path).filter(|p| !p.is_empty()) else { continue };
        out.push(Entry { tbl: "files", id: id.clone(), lang: lang.unwrap_or_default(), key: format!("file:{path}"), path: path.clone(), line: 0 });
        paths.insert(id, path);
    }
    Ok(paths)
}

/// `abs` from the first root that holds it, `/`-separated; None outside every root.
fn relative(abs: &str, roots: &[String]) -> Option<String> {
    let abs = abs.replace('\\', "/");
    roots.iter().find_map(|root| {
        let root = root.replace('\\', "/");
        let rest = abs.strip_prefix(root.trim_end_matches('/'))?.strip_prefix('/')?;
        Some(rest.to_string())
    })
}

/// Rows of one query as (id, key part, file, line), the key part never empty.
fn query(conn: &Connection, sql: &str) -> Result<Vec<(String, String, String, i64)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<i64>>(3)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, part, file, line) = row?;
        if let Some(part) = part.filter(|p| !p.is_empty()) {
            out.push((id, part, file.unwrap_or_default(), line.unwrap_or(0)));
        }
    }
    Ok(out)
}

/// C#: types, members and enums by symbol; a method or constructor by symbol and its parameter types in order.
fn csharp(conn: &Connection, path_of: &dyn Fn(&str) -> String, out: &mut Vec<Entry>) -> Result<()> {
    let cs = "JOIN files f ON f.id = t.file WHERE f.lang = 'csharp'";
    let mut push = |tbl: &'static str, rows: Vec<(String, String, String, i64)>| {
        for (id, part, file, line) in rows {
            out.push(Entry { tbl, id, lang: "csharp".into(), key: format!("cs:{part}"), path: path_of(&file), line });
        }
    };
    for tbl in ["classes", "members", "enums"] {
        if has_columns(conn, tbl, &["id", "symbol", "file", "line"]) {
            push(tbl, query(conn, &format!("SELECT t.id, t.symbol, t.file, t.line FROM {tbl} t {cs}"))?);
        }
    }
    if has_columns(conn, "functions", &["id", "symbol", "qualname", "kind", "file", "line"]) {
        // THE PARAMETER TYPES, BY THE FUNCTION'S PLACE: a `parameters` row names its function by file, qualname and line.
        let mut types: HashMap<(String, String, i64), Vec<(i64, String)>> = HashMap::new();
        if has_columns(conn, "parameters", &["file", "qualname", "line", "position", "type"]) {
            let mut stmt = conn.prepare("SELECT file, qualname, line, position, type FROM parameters")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?)))?;
            for row in rows {
                let (Some(file), Some(qualname), Some(line), position, kind) = row? else { continue };
                types.entry((file, qualname, line)).or_default().push((position.unwrap_or(0), kind.unwrap_or_default()));
            }
        }
        let mut stmt = conn.prepare(&format!("SELECT t.id, t.symbol, t.file, t.line, t.qualname, t.kind FROM functions t {cs}"))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?)))?;
        for row in rows {
            let (id, Some(symbol), file, line, qualname, kind) = row? else { continue };
            let (file, line) = (file.unwrap_or_default(), line.unwrap_or(0));
            let key = if matches!(kind.as_deref(), Some("method" | "constructor" | "local")) {
                let mut list = types.get(&(file.clone(), qualname.unwrap_or_default(), line)).cloned().unwrap_or_default();
                list.sort();
                format!("cs:{symbol}({})", list.into_iter().map(|(_, t)| t).collect::<Vec<_>>().join(","))
            } else {
                format!("cs:{symbol}")
            };
            out.push(Entry { tbl: "functions", id, lang: "csharp".into(), key, path: path_of(&file), line });
        }
    }
    enum_members(conn, "csharp", out)
}

/// Every half that names its declarations by file and name: python and plain TypeScript by `qualname`, the Angular half
/// (`half = 'typescript'`) by name - a member under its class's - and its framework rows by their class.
fn named(conn: &Connection, path_of: &dyn Fn(&str) -> String, out: &mut Vec<Entry>, half: impl Fn(&str) -> bool) -> Result<()> {
    for (lang, prefix) in [("python", "py"), ("ts", "ts")] {
        for tbl in ["classes", "functions"] {
            if !has_columns(conn, tbl, &["id", "qualname", "file", "line"]) {
                continue;
            }
            for (id, part, file, line) in query(conn, &format!("SELECT t.id, t.qualname, t.file, t.line FROM {tbl} t JOIN files f ON f.id = t.file WHERE f.lang = '{lang}'"))? {
                let path = path_of(&file);
                out.push(Entry { tbl, id, lang: lang.into(), key: format!("{prefix}:{path}#{part}"), path, line });
            }
        }
    }
    let ng = |tbl: &str| has_columns(conn, tbl, &["id", "name", "file"]) && half(tbl);
    let mut push = |tbl: &'static str, prefix: &str, rows: Vec<(String, String, String, i64)>| {
        for (id, part, file, line) in rows {
            let path = path_of(&file);
            out.push(Entry { tbl, id, lang: "typescript".into(), key: format!("{prefix}:{path}#{part}"), path, line });
        }
    };
    for tbl in ["classes", "enums", "interfaces", "type_aliases", "consts"] {
        if ng(tbl) {
            push(tbl, "ts", query(conn, &format!("SELECT id, name, file, line FROM {tbl} WHERE half = 'typescript' AND (owner_file IS NULL OR owner_file = file)"))?);
        }
    }
    // A FUNCTION OF ITS OWN NAME, not one inside another (`parent`) nor a callback (`inline`): those have no name to keep.
    if ng("functions") && has_columns(conn, "functions", &["parent", "inline"]) {
        push("functions", "ts", query(conn, "SELECT id, name, file, line FROM functions WHERE half = 'typescript' AND (parent IS NULL OR parent = '') AND (inline IS NULL OR inline = 0) AND (owner_file IS NULL OR owner_file = file)")?);
    }
    if ng("members") && has_columns(conn, "members", &["class"]) && ng("classes") {
        push("members", "ts", query(conn, "SELECT m.id, c.name || '.' || m.name, m.file, m.line FROM members m JOIN classes c ON c.id = m.class WHERE m.half = 'typescript' AND (m.owner_file IS NULL OR m.owner_file = m.file)")?);
    }
    for tbl in ["components", "directives", "pipes", "injectables", "ng_modules"] {
        if ng(tbl) && has_columns(conn, tbl, &["class"]) && ng("classes") {
            push(tbl, "ng", query(conn, &format!("SELECT t.id, c.name, t.file, c.line FROM {tbl} t JOIN classes c ON c.id = t.class WHERE t.half = 'typescript'"))?);
        }
    }
    enum_members(conn, "typescript", out)
}

/// Every member of every enum the half's `lang` wrote: the enum's key, `.`, the member - its row id `<enum id>#<ord>`.
fn enum_members(conn: &Connection, lang: &str, out: &mut Vec<Entry>) -> Result<()> {
    if !has_columns(conn, "enums__members", &["owner", "n", "parent", "ord", "role", "value"]) {
        return Ok(());
    }
    let keys: HashMap<String, (String, String)> = out.iter().filter(|e| e.tbl == "enums" && e.lang == lang).map(|e| (e.id.clone(), (e.key.clone(), e.path.clone()))).collect();
    if keys.is_empty() {
        return Ok(());
    }
    let mut stmt = conn.prepare("SELECT i.owner, i.ord, nm.value FROM enums__members i JOIN enums__members nm ON nm.owner = i.owner AND nm.parent = i.n AND nm.role = 'name' WHERE i.parent = 0")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<String>>(2)?)))?;
    for row in rows {
        let (owner, ord, Some(name)) = row? else { continue };
        let Some((key, path)) = keys.get(&owner) else { continue };
        let ord = ord.unwrap_or(0);
        out.push(Entry { tbl: "enums__members", id: format!("{owner}#{ord}"), lang: lang.into(), key: format!("{key}.{name}"), path: path.clone(), line: ord });
    }
    Ok(())
}

/// SQL objects and their columns, by schema and name, in their database.
fn sql(conn: &Connection, path_of: &dyn Fn(&str) -> String, out: &mut Vec<Entry>) -> Result<()> {
    if !has_columns(conn, "sql_objects", &["id", "database", "schema", "name", "file", "line"]) {
        return Ok(());
    }
    let at = |db: &Option<String>| db.as_deref().filter(|d| !d.is_empty()).map(|d| format!("@{d}")).unwrap_or_default();
    let mut stmt = conn.prepare("SELECT id, database, schema, name, file, line FROM sql_objects")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<i64>>(5)?)))?;
    let mut objects: HashMap<String, (String, String, String)> = HashMap::new();
    for row in rows {
        let (id, database, schema, Some(name), file, line) = row? else { continue };
        let qualified = match schema.filter(|s| !s.is_empty()) { Some(s) => format!("{s}.{name}"), None => name };
        let path = path_of(&file.unwrap_or_default());
        out.push(Entry { tbl: "sql_objects", id: id.clone(), lang: "sql".into(), key: format!("sql:{qualified}{}", at(&database)), path: path.clone(), line: line.unwrap_or(0) });
        objects.insert(id, (qualified, at(&database), path));
    }
    if has_columns(conn, "sql_columns", &["id", "object", "name", "line"]) {
        let mut stmt = conn.prepare("SELECT id, object, name, line FROM sql_columns")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<i64>>(3)?)))?;
        for row in rows {
            let (id, Some(object), Some(name), line) = row? else { continue };
            let Some((qualified, database, path)) = objects.get(&object) else { continue };
            out.push(Entry { tbl: "sql_columns", id, lang: "sql".into(), key: format!("sql:{qualified}.{name}{database}"), path: path.clone(), line: line.unwrap_or(0) });
        }
    }
    Ok(())
}

/// HTTP endpoints by verb and route; the calls by verb and url.
fn http(conn: &Connection, path_of: &dyn Fn(&str) -> String, out: &mut Vec<Entry>) -> Result<()> {
    for (tbl, prefix, what) in [("http_endpoints", "http", "route"), ("http_calls", "httpcall", "url")] {
        if !has_columns(conn, tbl, &["id", "verb", what, "file", "line"]) {
            continue;
        }
        let sql = format!("SELECT id, upper(coalesce(verb, '')) || ' ' || {what}, file, line FROM {tbl} WHERE {what} IS NOT NULL AND {what} <> ''");
        for (id, part, file, line) in query(conn, &sql)? {
            out.push(Entry { tbl, id, lang: String::new(), key: format!("{prefix}:{}", part.trim()), path: path_of(&file), line });
        }
    }
    Ok(())
}

/// Every key unique in its table: a shared one takes `@<path>` (when no path is in it), then `~<n>` by (path, line, id).
fn distinct(out: &mut [Entry]) {
    for round in 0..2 {
        let mut groups: HashMap<(&'static str, String), Vec<usize>> = HashMap::new();
        for (i, e) in out.iter().enumerate() {
            groups.entry((e.tbl, e.key.clone())).or_default().push(i);
        }
        for ((_, key), mut members) in groups.into_iter().filter(|(_, m)| m.len() > 1) {
            members.sort_by(|&a, &b| (&out[a].path, out[a].line, id_order(&out[a].id)).cmp(&(&out[b].path, out[b].line, id_order(&out[b].id))));
            for (n, i) in members.into_iter().enumerate() {
                let e = &mut out[i];
                if round == 0 && !key.contains('#') && !key.starts_with("file:") && !e.path.is_empty() {
                    e.key = format!("{key}@{}", e.path);
                } else {
                    e.key = format!("{key}~{}", n + 1);
                }
            }
        }
    }
}

/// A row id's number, so `fn:9` sorts before `fn:10`.
fn id_order(id: &str) -> (String, u64) {
    let (prefix, n) = id.rsplit_once(':').unwrap_or((id, ""));
    (prefix.to_string(), n.parse().unwrap_or(0))
}

/// The roots a map was built from, for `write` - also used by `source_state`.
pub fn roots_of(roots: &[(String, String)]) -> Vec<String> {
    roots.iter().map(|(_, root)| std::path::absolute(Path::new(root)).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| root.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, path: &str, line: i64, id: &str) -> Entry {
        Entry { tbl: "classes", id: id.into(), lang: "csharp".into(), key: key.into(), path: path.into(), line }
    }

    #[test]
    fn a_shared_key_takes_its_path_then_its_place() {
        let mut out = vec![entry("cs:Demo.A", "b.cs", 1, "c:2"), entry("cs:Demo.A", "a.cs", 1, "c:1"), entry("cs:Demo.B", "a.cs", 3, "c:3"),
            entry("ts:a.ts#X", "a.ts", 9, "c:10"), entry("ts:a.ts#X", "a.ts", 2, "c:9")];
        out.push(Entry { tbl: "enums", ..entry("cs:Demo.B", "a.cs", 3, "e:1") });
        distinct(&mut out);
        let keys: Vec<&str> = out.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["cs:Demo.A@b.cs", "cs:Demo.A@a.cs", "cs:Demo.B", "ts:a.ts#X~2", "ts:a.ts#X~1", "cs:Demo.B"], "one entity two tables hold keeps one key");
    }

    #[test]
    fn a_path_is_measured_from_the_root_that_holds_it() {
        let roots = vec!["/w/app".to_string(), "/w/db".to_string()];
        assert_eq!(relative("/w/db/Sales/T.sql", &roots).as_deref(), Some("Sales/T.sql"));
        assert_eq!(relative("/elsewhere/x.ts", &roots), None);
    }
}
