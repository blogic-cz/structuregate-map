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

struct Class {
    symbol: String,
    file: String,
    bases: Vec<String>,
}

/// A base as a symbol: its generic arguments dropped (`Demo.Base<T>` is `Demo.Base`).
fn plain(base: &str) -> &str {
    base.split('<').next().unwrap_or(base).trim()
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
    let mut classes: Vec<Class> = Vec::new();
    {
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
            classes.push(Class { symbol, file, bases: list });
        }
    }
    let lines: HashMap<String, String> = {
        let mut stmt = conn.prepare("SELECT symbol, line FROM classes")?;
        let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
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
        // THE CLASSES DERIVING FROM `base`, through the classes in between - a visited set, never a depth.
        let mut derived: HashSet<String> = HashSet::new();
        let mut front = vec![spec.base.clone()];
        while let Some(at) = front.pop() {
            for c in classes.iter().filter(|c| c.bases.iter().any(|b| *b == at)) {
                if derived.insert(c.symbol.clone()) {
                    front.push(c.symbol.clone());
                }
            }
        }
        let candidates: Vec<&Class> = classes
            .iter()
            .filter(|c| derived.contains(&c.symbol))
            .filter(|c| c.symbol.rsplit_once('.').is_some_and(|(ns, _)| ns.contains(&spec.namespace)))
            .collect();
        if candidates.is_empty() {
            notes.push(format!("typesByEnumName: no class derives from {} in a namespace holding \"{}\"", spec.base, spec.namespace));
        }
        for name in &names {
            let member_symbol = format!("{}.{name}", spec.enum_symbol);
            let same = |n: &str| if spec.ignore_case { n.eq_ignore_ascii_case(name) } else { n == name };
            let mut any = false;
            for c in candidates.iter().filter(|c| c.symbol.rsplit('.').next().is_some_and(same)) {
                let segment = c.symbol.rsplit('.').nth(1).unwrap_or("");
                // `default` GIVEN: a class whose namespace holds it is the default, any other an override by its last segment.
                // Otherwise an override is a class whose last segment `prefixes` lists.
                let prefix: Option<&str> = match &spec.default {
                    Some(text) => (!c.symbol.rsplit_once('.').is_some_and(|(ns, _)| ns.contains(text.as_str()))).then_some(segment),
                    None => spec.prefixes.iter().find(|p| if spec.ignore_case { p.eq_ignore_ascii_case(segment) } else { *p == segment }).map(String::as_str),
                };
                let kind = if prefix.is_some() { "override" } else { "default" };
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
