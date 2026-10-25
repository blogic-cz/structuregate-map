//! A CLASS CHOSEN BY AN ENUM MEMBER'S NAME (#113) - `enum_type_links`, joined from the C# rows under a DECLARED convention
//! (`csconfig.rs` beside it, `typesByEnumName`), rewritten whole after every half.
//!
//! The lookup itself - a dictionary filled from `Type.Name` by reflection, read with `member.ToString()` - is code no parse
//! tree gives a meaning to, so nothing links by name unless a convention says so. For each one: every member of the enum
//! (`enums.members`, by the enum's `symbol`) links to every class that DERIVES from `base` - its `classes.bases`, followed
//! through the classes in between, generic arguments dropped - whose namespace holds `namespace`, and whose name is the
//! member's (`ignoreCase`: as the dictionary's comparer says). A class whose LAST namespace segment is one of `prefixes`
//! is an `override` (`$"{prefix}.{name}"`), any other the `default` - or, with `default` given, a class whose namespace holds
//! that text is the default and every other an override keyed by its last segment. A member no class answers is a row with no type
//! (`kind = 'none'`), so a gap is listed rather than silent. `--map-slice` follows a link from a file that references the
//! member (`refs.symbol`) to the class's file.

use super::csconfig::EnumTypes;
use crate::rows::dups::{has_columns, owner_sql, scalar_sql, text, Column};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

pub(super) struct Class {
    pub(super) symbol: String,
    pub(super) file: String,
    bases: Vec<String>,
}

/// A base as a symbol: its generic arguments dropped (`Demo.Base<T>` is `Demo.Base`).
fn plain(base: &str) -> &str {
    base.split('<').next().unwrap_or(base).trim()
}

/// Every C# class with its bases, generic arguments dropped.
pub(super) fn classes(conn: &Connection) -> rusqlite::Result<Vec<Class>> {
    let mut out = Vec::new();
    let mut bases = Column::new(conn, "classes", "bases")?;
    let sql = format!(
        "SELECT c.symbol, c.file, {}, {} FROM classes c JOIN files f ON f.id = c.file WHERE f.lang = 'csharp'",
        owner_sql(conn, "classes", "c"),
        scalar_sql(conn, "classes", "c", "bases")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(String, String, String, SqlValue)> =
        stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    for (symbol, file, owner, held) in rows {
        let list = bases.strings(&owner, &held)?.iter().map(|b| plain(b).to_string()).collect();
        out.push(Class { symbol, file, bases: list });
    }
    Ok(out)
}

/// Each class's declaring line, by symbol.
pub(super) fn lines(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT symbol, line FROM classes")?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))?;
    rows.collect()
}

/// THE CLASSES A REFLECTION DICTIONARY HOLDS: those deriving from `base` - through the classes in between, a visited set,
/// never a depth - whose namespace holds `namespace`. With none, the note that says why (`what` names the convention).
pub(super) fn candidates<'c>(classes: &'c [Class], base: &str, namespace: &str, what: &str) -> (Vec<&'c Class>, Option<String>) {
    let mut derived: HashSet<String> = HashSet::new();
    // GENERIC ARGUMENTS DROPPED ON BOTH SIDES (#115): a generic class's symbol keeps its parameters (`Demo.ReportBase<TModel>`) where a
    // derived class's base names it closed (`Demo.ReportBase<System.Object>`) - compared as written, nothing below a generic base linked.
    let mut front = vec![plain(base).to_string()];
    while let Some(at) = front.pop() {
        for c in classes.iter().filter(|c| c.bases.iter().any(|b| *b == at)) {
            if derived.insert(c.symbol.clone()) {
                front.push(plain(&c.symbol).to_string());
            }
        }
    }
    let found: Vec<&Class> = classes
        .iter()
        .filter(|c| derived.contains(&c.symbol))
        .filter(|c| c.symbol.rsplit_once('.').is_some_and(|(ns, _)| ns.contains(namespace)))
        .collect();
    if !found.is_empty() {
        return (found, None);
    }
    // A BASE THE COMPILER COULD NOT BIND is stored as its bare written name (an error type): never matched to the configured one -
    // that would be a guess - but counted, so the note says to restore or build rather than leave the convention looking wrong.
    let simple = plain(base).rsplit('.').next().unwrap_or("");
    let unbound = classes.iter().filter(|c| c.bases.iter().any(|b| b == simple)).count();
    let why = if unbound > 0 { format!("; {unbound} class(es) name an UNRESOLVED base `{simple}` - restore or build the project so it binds") } else { String::new() };
    (found, Some(format!("{what}: no class derives from {base} in a namespace holding \"{namespace}\"{why}")))
}

/// How the dictionary keys its classes: `prefixes`, the last namespace segments of the overrides, or `default`, the namespace
/// text a default class holds; and its comparer.
pub(super) struct Keying<'k> {
    pub(super) prefixes: &'k [String],
    pub(super) default: Option<&'k str>,
    pub(super) ignore_case: bool,
}

/// The classes `name` selects: each one's kind (`default`, `override`) and an override's prefix. A class whose LAST namespace
/// segment is one of `prefixes` is an `override` (`$"{prefix}.{name}"`), any other the `default` - or, with `default` given, a
/// class whose namespace holds that text is the default and every other an override keyed by its last segment.
pub(super) fn selected<'c>(candidates: &[&'c Class], name: &str, keying: &Keying<'_>) -> Vec<(&'static str, Option<String>, &'c Class)> {
    let same = |a: &str, b: &str| if keying.ignore_case { a.eq_ignore_ascii_case(b) } else { a == b };
    let mut out = Vec::new();
    for c in candidates.iter().filter(|c| c.symbol.rsplit('.').next().is_some_and(|n| same(n, name))) {
        let segment = c.symbol.rsplit('.').nth(1).unwrap_or("");
        let prefix: Option<&str> = match keying.default {
            Some(text) => (!c.symbol.rsplit_once('.').is_some_and(|(ns, _)| ns.contains(text))).then_some(segment),
            None => keying.prefixes.iter().find(|p| same(p, segment)).map(String::as_str),
        };
        out.push((if prefix.is_some() { "override" } else { "default" }, prefix.map(str::to_string), *c));
    }
    out
}

/// The table, rewritten whole; the notes on conventions that matched nothing.
pub fn write(conn: &Connection, specs: &[EnumTypes]) -> rusqlite::Result<(usize, Vec<String>)> {
    crate::rows::cells::drop_children(conn, "enum_type_links").map_err(|e| rusqlite::Error::InvalidParameterName(format!("{e:#}")))?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS enum_type_links; CREATE TABLE enum_type_links (enum_symbol, member, member_symbol, kind, prefix, type_symbol, type_file, type_line);",
    )?;
    let mut notes = Vec::new();
    if specs.is_empty() || !has_columns(conn, "classes", &["file", "symbol", "bases"]) || !has_columns(conn, "enums", &["symbol", "members"]) {
        return Ok((0, notes));
    }
    let classes = classes(conn)?;
    let lines = lines(conn)?;
    let mut members = Column::new(conn, "enums", "members")?;
    let mut add = conn.prepare("INSERT INTO enum_type_links VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?;
    let mut written = 0;
    for spec in specs {
        let sql = format!("SELECT {}, {} FROM enums e WHERE e.symbol = ?1", owner_sql(conn, "enums", "e"), scalar_sql(conn, "enums", "e", "members"));
        let found: Option<(String, SqlValue)> = conn.query_row(&sql, [&spec.enum_symbol], |r| Ok((text(r, 0)?, r.get(1)?))).ok();
        let Some((owner, held)) = found else {
            notes.push(format!("typesByEnumName: no enum {} in the map", spec.enum_symbol));
            continue;
        };
        let names: Vec<String> = members.value(&owner, &held)?.as_array().into_iter().flatten().filter_map(|m| m["name"].as_str().map(str::to_string)).collect();
        let (candidates, note) = candidates(&classes, &spec.base, &spec.namespace, "typesByEnumName");
        notes.extend(note);
        let keying = Keying { prefixes: &spec.prefixes, default: spec.default.as_deref(), ignore_case: spec.ignore_case };
        for name in &names {
            let member_symbol = format!("{}.{name}", spec.enum_symbol);
            let mut any = false;
            for (kind, prefix, c) in selected(&candidates, name, &keying) {
                add.execute(params![spec.enum_symbol, name, member_symbol, kind, prefix, c.symbol, c.file, lines.get(&c.symbol)])?;
                written += 1;
                any = true;
            }
            if !any {
                add.execute(params![spec.enum_symbol, name, member_symbol, "none", None::<String>, None::<String>, None::<String>, None::<String>])?;
                written += 1;
            }
        }
    }
    Ok((written, notes))
}
