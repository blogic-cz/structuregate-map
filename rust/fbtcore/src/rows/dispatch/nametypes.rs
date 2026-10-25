//! A CLASS CHOSEN BY A NAME STRING (#121) - `name_type_links`, the string counterpart of `enum_type_links`: under a declared
//! convention (`csconfig.rs`, `typesByName`) every call of `method` links the NAME its `parameter` is handed to the classes the
//! reflection dictionary holds under it (`enumtypes::candidates`/`selected`, the same matching). Rewritten whole after every half.
//!
//! WHERE THE NAME COMES FROM, `via`: `literal` - the argument folds to a constant (`Include("Footer", m)`); `property` - the
//! argument reads a property every constant of which is written in an object built in the tree (`new { PartName = "Header" }`
//! read back as `p.PartName`: the initializer slot's `arguments.member` is the property the argument's `symbol` names), one row
//! per value; `unresolved` - neither, a row with no name, so a call the map cannot follow is listed rather than silent. A name
//! no class answers is `kind = 'none'`. `--map-slice` follows a link from the call's file to the class's (`type_file`).

use super::csconfig::NameTypes;
use super::enumtypes::{candidates, classes, lines, selected, Class, Keying};
use crate::rows::dups::{has_columns, text};
use rusqlite::{params, Connection};

/// A symbol without any generic argument list, at any level (#124): `Demo.TemplateBase<System.Object>.Include<T>` is
/// `Demo.TemplateBase.Include` - a call of a generic class's method is stored CLOSED, the convention names it open or bare.
fn bare(symbol: &str) -> String {
    let (mut out, mut nesting) = (String::with_capacity(symbol.len()), 0usize);
    for c in symbol.chars() {
        match c {
            '<' => nesting += 1,
            '>' => nesting = nesting.saturating_sub(1),
            _ if nesting == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The classes a name selects: as `enumtypes::selected` matches a member's name - or, when none answers and the name is the
/// dictionary's OWN key of an override (#125, `Include("Acme.Footer", ..)`: an override is stored as `<prefix>.<Name>`), the one
/// override of `Name` whose prefix is `<prefix>`.
fn choose<'c>(candidates: &[&'c Class], value: &str, keying: &Keying<'_>) -> Vec<(&'static str, Option<String>, &'c Class)> {
    let plain = selected(candidates, value, keying);
    if !plain.is_empty() {
        return plain;
    }
    let Some((prefix, name)) = value.rsplit_once('.') else { return plain };
    let same = |a: &str| if keying.ignore_case { a.eq_ignore_ascii_case(prefix) } else { a == prefix };
    selected(candidates, name, keying).into_iter().filter(|(kind, p, _)| *kind == "override" && p.as_deref().is_some_and(same)).collect()
}

/// The table, rewritten whole; the notes on conventions that matched nothing.
pub fn write(conn: &Connection, specs: &[NameTypes]) -> rusqlite::Result<(usize, Vec<String>)> {
    crate::rows::cells::drop_children(conn, "name_type_links").map_err(|e| rusqlite::Error::InvalidParameterName(format!("{e:#}")))?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS name_type_links; CREATE TABLE name_type_links \
         (method, call, file, line, parameter, value, via, kind, prefix, type_symbol, type_file, type_line);",
    )?;
    let mut notes = Vec::new();
    let ready = has_columns(conn, "classes", &["file", "symbol", "bases"])
        && has_columns(conn, "calls", &["id", "file", "line", "symbol"])
        && has_columns(conn, "arguments", &["call", "name", "const", "const_kind", "symbol"]);
    if specs.is_empty() || !ready {
        return Ok((0, notes));
    }
    let classes = classes(conn)?;
    let lines = lines(conn)?;
    let slots = has_columns(conn, "arguments", &["object", "member"]);
    let mut add = conn.prepare("INSERT INTO name_type_links VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)")?;
    let mut written = 0;
    for spec in specs {
        let (candidates, note) = candidates(&classes, &spec.base, &spec.namespace, "typesByName");
        notes.extend(note);
        let keying = Keying { prefixes: &spec.prefixes, default: spec.default.as_deref(), ignore_case: spec.ignore_case };
        // GENERIC ARGUMENTS IGNORED ON BOTH SIDES (#124): a generic method's (`Render<T>`) and its class's - `TemplateBase<System.Object>.Include`
        // is how a call of `TemplateBase<TModel>.Include` is stored. Narrowed by the method's name in SQL, matched bare here.
        let wanted = bare(&spec.method);
        let name = wanted.rsplit('.').next().unwrap_or("").to_string();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.file, c.line, a.const, a.const_kind, a.symbol, c.symbol FROM calls c JOIN arguments a ON a.call = c.id AND a.name = ?2 \
             JOIN files f ON f.id = c.file WHERE f.lang = 'csharp' AND c.symbol LIKE '%' || ?1 || '%' ORDER BY c.file, c.line, c.id",
        )?;
        let calls: Vec<(String, String, String, String, String, String)> = stmt
            .query_map(params![name, spec.parameter], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?)))?
            .filter_map(|r| r.map(|(id, file, line, constant, kind, symbol, callee)| (bare(&callee) == wanted).then_some((id, file, line, constant, kind, symbol))).transpose())
            .collect::<rusqlite::Result<_>>()?;
        if calls.is_empty() {
            notes.push(format!("typesByName: no call of {} hands a `{}` argument", spec.method, spec.parameter));
        }
        for (call, file, line, constant, kind, symbol) in calls {
            let mut values: Vec<(Option<String>, &str)> = Vec::new();
            if !constant.is_empty() && matches!(kind.as_str(), "const" | "folded") {
                values.push((Some(constant), "literal"));
            } else if slots && !symbol.is_empty() {
                let mut written_in = conn.prepare(
                    "SELECT DISTINCT const FROM arguments WHERE member = ?1 AND object IS NOT NULL AND object <> '' AND const IS NOT NULL AND const <> '' ORDER BY const",
                )?;
                let found: Vec<String> = written_in.query_map([&symbol], |r| text(r, 0))?.collect::<rusqlite::Result<_>>()?;
                values.extend(found.into_iter().map(|v| (Some(v), "property")));
            }
            if values.is_empty() {
                values.push((None, "unresolved"));
            }
            for (value, via) in values {
                let mut any = false;
                for (kind, prefix, c) in value.as_deref().map(|v| choose(&candidates, v, &keying)).unwrap_or_default() {
                    add.execute(params![spec.method, call, file, line, spec.parameter, value, via, kind, prefix, c.symbol, c.file, lines.get(&c.symbol)])?;
                    written += 1;
                    any = true;
                }
                if !any {
                    add.execute(params![spec.method, call, file, line, spec.parameter, value, via, "none", None::<String>, None::<String>, None::<String>, None::<String>])?;
                    written += 1;
                }
            }
        }
    }
    Ok((written, notes))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_symbol_is_compared_without_any_generic_argument_list() {
        assert_eq!(super::bare("Demo.TemplateBase<System.Object>.Include"), "Demo.TemplateBase.Include");
        assert_eq!(super::bare("Demo.TemplateBase<TModel>.Include<Dictionary<string, int>>"), "Demo.TemplateBase.Include");
        assert_eq!(super::bare("Demo.Plain.Include"), "Demo.Plain.Include");
    }
}
