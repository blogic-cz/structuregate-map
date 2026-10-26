//! WHAT A PARTIAL RUN NEEDS TO LEAVE A PROJECT'S PROGRAM UNBUILT (`TsDecls/TsWalk.mjs`): which project owns each file, which
//! TypeScript files a program reached (`reachable`), and the notes building a program writes - the unresolved imports, kept
//! with the program that first noted them (`diagnostics.program`) - so a project whose files nothing re-reads is not built
//! again, and what its program would have said is said again as the last run said it, in its place.
//!
//! `notes` is None when the database cannot say which program noted what (a map from before the column, or one with no such note,
//! where the column was never made): nothing is then skipped, since a skipped program's notes would be lost.

use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Map, Value};
use std::path::Path;

#[derive(Default)]
pub struct Skip {
    /// path -> the name of the project whose program owns it.
    pub owners: Map<String, Value>,
    /// The absolute path of every `.ts` file a program reached.
    pub reached: Vec<Value>,
    pub notes: Option<Vec<Value>>,
}

pub fn recorded(db: &Path, half: &str) -> Skip {
    let mut out = Skip::default();
    let Ok(conn) = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY) else { return out };
    let files = conn.prepare(
        "SELECT f.path, p.name, f.abs, f.reachable, f.ext FROM files f LEFT JOIN projects p ON p.id = f.project WHERE f.half = ?1",
    );
    if let Ok(mut stmt) = files {
        let rows = stmt.query_map([half], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?))
        });
        for (path, project, abs, reachable, ext) in rows.into_iter().flatten().flatten() {
            if let Some(project) = project {
                out.owners.insert(path, Value::String(project));
            }
            if let (Some(abs), Some(1), Some("ts")) = (abs, reachable, ext.as_deref()) {
                out.reached.push(Value::String(abs));
            }
        }
    }
    let notes = conn.prepare(
        "SELECT kind, module, \"from\", case_insensitive_match, program FROM diagnostics \
         WHERE half = ?1 AND kind IN ('unresolved_import', 'unresolved_node_builtin') AND program IS NOT NULL ORDER BY rowid",
    );
    if let Ok(mut stmt) = notes {
        let rows = stmt.query_map([half], |r| {
            Ok(json!({
                "kind": r.get::<_, String>(0)?,
                "module": r.get::<_, Option<String>>(1)?,
                "from": r.get::<_, Option<String>>(2)?,
                "case_insensitive_match": r.get::<_, Option<i64>>(3)?,
                "program": r.get::<_, String>(4)?,
            }))
        });
        if let Ok(rows) = rows {
            out.notes = Some(rows.flatten().collect());
        }
    }
    out
}
