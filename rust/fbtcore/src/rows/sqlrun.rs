//! SQL AGAINST THE MAP ITSELF, for a pass that derives rows from rows the halves already stored.
//!
//! The link between C# and the SQL projects (`src/Map/Sql/SqlLinks.cs`) is a fact about TWO halves'
//! rows, so it can only be worked out once both are in the database - and the database is only reachable
//! through this library. The caller runs a script (the writes) and then, optionally, one query whose rows
//! come back as JSON. Nothing is parsed here: the script is the caller's, and so is what it means.

use rusqlite::types::Value;
use rusqlite::Connection;
use serde_json::{json, Value as Json};
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{in_string, out_string};

/// Run `script` (may be empty), then `query` (may be empty), against the database at `db`.
///
/// Returns `{"columns": [...], "rows": [[...]]}`, or `{"error": "..."}`. The caller frees the string
/// with `fbt_free`.
///
/// # Safety
/// All three pointers must be NUL-terminated UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fbt_sql_run(
    db: *const c_char,
    script: *const c_char,
    query: *const c_char,
) -> *mut c_char {
    let db = unsafe { in_string(db) }.unwrap_or_default();
    let script = unsafe { in_string(script) }.unwrap_or_default();
    let query = unsafe { in_string(query) }.unwrap_or_default();
    let outcome = catch_unwind(AssertUnwindSafe(|| run(&db, &script, &query)));
    let answer = match outcome {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => json!({ "error": e }),
        Err(_) => json!({ "error": "the SQL pass panicked" }),
    };
    out_string(answer.to_string())
}

fn run(db: &str, script: &str, query: &str) -> Result<Json, String> {
    let conn = Connection::open(db).map_err(|e| format!("the database could not be opened - {e}"))?;
    if !script.trim().is_empty() {
        conn.execute_batch(script).map_err(|e| format!("the script failed - {e}"))?;
    }
    if query.trim().is_empty() {
        return Ok(json!({ "columns": [], "rows": [] }));
    }
    let mut stmt = conn.prepare(query).map_err(|e| format!("the query failed - {e}"))?;
    let columns: Vec<String> = stmt.column_names().iter().map(|c| (*c).to_string()).collect();
    let width = columns.len();
    let mut rows = stmt.query([]).map_err(|e| format!("the query failed - {e}"))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| format!("the query failed - {e}"))? {
        let mut cells = Vec::with_capacity(width);
        for i in 0..width {
            let value: Value = row.get(i).map_err(|e| format!("a cell could not be read - {e}"))?;
            cells.push(match value {
                Value::Null => Json::Null,
                Value::Integer(n) => json!(n),
                Value::Real(f) => json!(f),
                Value::Text(t) => json!(t),
                Value::Blob(b) => json!(format!("{b:?}")),
            });
        }
        out.push(Json::Array(cells));
    }
    Ok(json!({ "columns": columns, "rows": out }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_writes_and_the_query_reads_it_back() {
        let db = crate::rows::TempDb::new("sqlrun", "run");
        let path = db.to_string_lossy().to_string();
        let answer = run(&path, "CREATE TABLE t (a, b); INSERT INTO t VALUES ('x', 1);", "SELECT a, b FROM t").unwrap();
        assert_eq!(answer["columns"], json!(["a", "b"]));
        assert_eq!(answer["rows"], json!([["x", 1]]));
        assert!(run(&path, "NOT SQL", "").is_err(), "a failed script is an error, not an empty answer");
    }
}
