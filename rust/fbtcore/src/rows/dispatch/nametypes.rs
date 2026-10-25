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
use super::enumtypes::{candidates, classes, lines, selected, Keying};
use crate::rows::dups::{has_columns, text};
use rusqlite::{params, Connection};

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
        // A GENERIC METHOD'S SYMBOL carries its arguments (`Render<T>`): compared without them.
        let mut stmt = conn.prepare(
            "SELECT c.id, c.file, c.line, a.const, a.const_kind, a.symbol FROM calls c JOIN arguments a ON a.call = c.id AND a.name = ?2 \
             JOIN files f ON f.id = c.file WHERE f.lang = 'csharp' AND (c.symbol = ?1 OR c.symbol LIKE ?1 || '<%') ORDER BY c.file, c.line, c.id",
        )?;
        let calls: Vec<(String, String, String, String, String, String)> = stmt
            .query_map(params![spec.method, spec.parameter], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?)))?
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
                for (kind, prefix, c) in value.as_deref().map(|v| selected(&candidates, v, &keying)).unwrap_or_default() {
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
