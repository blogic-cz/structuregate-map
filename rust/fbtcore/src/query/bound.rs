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

/// A list column with an item `j` matching `element` (on `?2`), or false where the table has no such list - still
/// naming `?2`, since a statement handed a parameter it does not use is refused. A list is NODES (`rows::cells`), so
/// a cell another half wrote as plain text is in no node table and is a miss, as before.
fn holds(db: &Connection, table: &str, alias: &str, column: &str, element: &str) -> String {
    if crate::rows::dups::nodes_of(db, table, column).is_none() {
        return "(?2 IS NULL)".to_string();
    }
    format!(
        "EXISTS (SELECT 1 FROM {} j WHERE j.owner = {} AND {element})",
        crate::rows::dups::items_sql(db, table, column, &["value"]),
        crate::rows::dups::owner_sql(db, table, alias)
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

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::json;

    /// A python-shaped map whose lists - `reads`, `binds`, `args` - are NODES, the way the store writes them.
    fn map() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        let put = |table: &str, rows: Vec<serde_json::Value>| crate::rows::store::insert_rows(&db, table, &rows).unwrap();
        put("files", vec![json!({"id": "f:1", "path": "app.py", "module": "app"})]);
        put("functions", vec![
            json!({"id": "fn:1", "file": "f:1", "name": "spelled", "qualname": "spelled", "line": 1, "launched": 0, "cls": "", "func": ""}),
            json!({"id": "fn:2", "file": "f:1", "name": "bound_one", "qualname": "bound_one", "line": 2, "launched": 0, "cls": "", "func": ""}),
            json!({"id": "fn:3", "file": "f:1", "name": "orphan", "qualname": "orphan", "line": 3, "launched": 0, "cls": "", "func": ""}),
        ]);
        put("calls", vec![json!({"id": "c:1", "file": "f:1", "line": 9, "callee": "print", "target_path": "", "target_name": ""})]);
        // `spelled` is read by name and bound to nothing; `alias` is bound to `bound_one`.
        put("expressions", vec![json!({"id": "e:1", "file": "f:1", "line": 9, "func": "", "source": "spelled(alias)",
                                       "reads": ["spelled", "alias"], "binds": ["", "app.py::bound_one"]})]);
        put("decorators", vec![json!({"id": "d:1", "file": "f:1", "line": 4, "name": "app.get", "target": "orphan",
                                      "target_kind": "function", "args": ["\"/items\"", "response_model=Item"]})]);
        put("string_literals", vec![json!({"id": "s:1", "file": "f:1", "line": 6, "func": "", "value": "other", "key": ""})]);
        put("consts", vec![json!({"id": "k:1", "file": "f:1", "line": 7, "name": "PLAIN", "cls": "", "func": "", "source": "1", "reads": [], "binds": []})]);
        put("assignments", vec![json!({"id": "a:1", "file": "f:1", "line": 5, "func": "", "target": "URL", "source": "cfg.get(k)",
                                       "reads": [], "binds": [], "keys": ["service.url"]})]);
        db
    }

    #[test]
    fn the_lenses_read_a_list_from_its_nodes() {
        let db = map();
        // `--dead`: `spelled` is kept by its unbound read, `bound_one` by the bind at the same position, and only
        // `orphan` is left - which `@app.get` then hands to an object, so nothing is.
        let mut out = String::new();
        super::super::dead::lens(&db, &mut out, "", 50, 0).unwrap();
        assert!(!out.contains("spelled") && !out.contains("bound_one") && !out.contains("orphan"), "{out}");
        // `--reads`: by an item LIKE the name, and by what an item binds.
        for name in ["spelled", "bound_one"] {
            let mut out = String::new();
            super::super::lenses::lens_reads(&db, &mut out, name, 50, 0).unwrap();
            assert!(out.contains("expressions") && out.contains("app.py"), "{name}: {out}");
        }
        // `--decorators`: the arguments joined in order.
        let mut out = String::new();
        super::super::decorators::lens(&db, &mut out, "app.py", 50, 0).unwrap();
        assert!(out.contains("\"/items\", response_model=Item"), "{out}");
        // `--key`: an assignment found by the key it looks up.
        let mut out = String::new();
        super::super::lenses::lens_key(&db, &mut out, "service.url", 50, 0).unwrap();
        assert!(out.contains("URL"), "{out}");
    }

    #[test]
    fn a_def_nothing_reads_is_dead_whatever_the_lists_hold() {
        let db = map();
        db.execute("DELETE FROM decorators", []).unwrap();
        let mut out = String::new();
        super::super::dead::lens(&db, &mut out, "", 50, 0).unwrap();
        assert!(out.contains("orphan") && !out.contains("spelled") && !out.contains("bound_one"), "{out}");
    }
}
