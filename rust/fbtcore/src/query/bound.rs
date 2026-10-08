//! What the lenses follow through a BINDING rather than a spelling: an import alias to the def it binds
//! (`--find`), a read through an alias to the symbol it names (`--reads`), a settings key through the section
//! a local holds (`--key`). The python half resolves each of these as it parses - `imports.bind`, `binds`,
//! `string_literals.key`, `keys` - and a lens that matched only the name the code SPELLS answered "nothing"
//! for every one of them.
//!
//! Every column is asked for where present: a database another half wrote, or one written before the column
//! existed, answers as it did instead of failing the lens.

use super::{cell, columns_of, query, tables_of};
use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::Connection;

/// Where `--find` looks for the def a binding names, and the column holding its name there.
const DEFS: &[(&str, &str)] = &[("functions", "qualname"), ("classes", "qualname"), ("consts", "name")];

fn has(db: &Connection, table: &str, column: &str) -> bool {
    columns_of(db, table).iter().any(|c| c == column)
}

/// `--find` rows for the DEF an import binds under ANOTHER name: `from sets_a import both as _both`,
/// or `from ids_b import _both` re-importing that alias, is `sets_a.py`'s `both`. The import row
/// itself is found by name like any other; this is the row it stands for, as `calls.target_path` already
/// is for a call. An import under the def's own name adds nothing - `--find both` finds the def already.
pub(super) fn aliased(db: &Connection, like: &str) -> Result<Vec<Vec<Value>>> {
    if !has(db, "imports", "alias") || !has(db, "imports", "bind") {
        return Ok(Vec::new());
    }
    let (_, found) = query(
        db,
        "SELECT DISTINCT CASE WHEN alias <> '' THEN alias ELSE name END, bind FROM imports \
         WHERE (alias LIKE ?1 OR (alias = '' AND name LIKE ?1)) AND bind LIKE '%::%'",
        &[&like],
    )?;
    let known = tables_of(db);
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for pair in found {
        let (local, bind) = (cell(&pair[0]), cell(&pair[1]));
        let Some((path, qual)) = bind.split_once("::") else { continue };
        if qual == local {
            continue;
        }
        for (table, column) in DEFS {
            if !known.iter().any(|k| k == table) || !has(db, table, column) {
                continue;
            }
            // THE PATH AS EITHER SIDE SPELLS IT, by whole segments: a map of several roots keys `files.path` by its
            // root folder, and `bind` may name the file from another one - `--find _both` found no def on v1.5.13.
            let named = if has(db, table, "name") { format!("(t.\"{column}\" = ?4 OR t.name = ?4)") } else { format!("t.\"{column}\" = ?4") };
            let sql = format!(
                "SELECT ?1, t.\"{column}\" || ' (as ' || ?2 || ')', f.path, t.line FROM \"{table}\" t \
                 JOIN files f ON f.id = t.file WHERE (f.path = ?3 OR f.path LIKE '%/' || ?3 OR ?3 LIKE '%/' || f.path) AND {named}"
            );
            for row in query(db, &sql, &[table, &local, &path, &qual])?.1 {
                if !rows.contains(&row) {
                    rows.push(row);
                }
            }
        }
    }
    Ok(rows)
}

/// A JSON-list column with an element matching `element` (on `?2`), or false where the table has no such
/// column - still naming `?2`, since a statement handed a parameter it does not use is refused. Guarded by
/// `json_valid`, so a cell another half wrote as plain text is a miss rather than a failed query.
fn holds(db: &Connection, table: &str, alias: &str, column: &str, element: &str) -> String {
    if !has(db, table, column) {
        return "(?2 IS NULL)".to_string();
    }
    format!(
        "(CASE WHEN json_valid({alias}.\"{column}\") THEN EXISTS (SELECT 1 FROM json_each({alias}.\"{column}\") j \
         WHERE {element}) ELSE 0 END)"
    )
}

/// `--reads`: a row whose `binds` names a symbol LIKE `?2` - `OUT_DIR` after `from m import IMPORT_OUT_DIR
/// as OUT_DIR` binds `m.py::IMPORT_OUT_DIR`, so `--reads IMPORT_OUT_DIR` finds it. Matched on the
/// qualname after `::`, never the path, as `reads` is matched on the name.
pub(super) fn reads_bound(db: &Connection, table: &str) -> String {
    holds(db, table, "t", "binds", "substr(j.value, instr(j.value, '::') + 2) LIKE ?2")
}

/// `--key`: a literal whose dotted `key` is the one asked for - "url" read from a local holding `service`.
pub(super) fn key_spelled(db: &Connection) -> &'static str {
    if has(db, "string_literals", "key") { "s.key = ?1" } else { "0" }
}

/// `--key`: an assignment whose value looks the key up, by its `keys` - `SERVICE_URL = _svc.get("url")`.
pub(super) fn key_built(db: &Connection, table: &str, alias: &str) -> String {
    holds(db, table, alias, "keys", "j.value = ?2")
}
