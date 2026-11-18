//! `ts_reads`: ONE ROW PER TYPESCRIPT READ THAT NAMES ITS DECLARATION, flat - a slim map's derived table (`slim.rs`).
//!
//! The rows are in the base already: every bound read in a code or template expression tree is a node with a `target` child
//! (`{name, file, line}`, the declaration's name line) and, from rows 134, its own `line` and `col`
//! (`skills/map-sqlite/typescript-refs.md`). Asking "who reads this member" of the trees is a six-way join over a node table of millions of
//! rows; a review map answers it from one indexed table instead.
//!
//! ONE ROW PER READ SITE: a read sits in every expression row whose tree holds it (a call's row and the condition around it), so the rows
//! are grouped by what they say - one expression row holding it named. On a map before rows 134 a site is its line, so two reads of one
//! declaration on one line are one row there.
//!
//! Columns: `expression` (an `expressions` row holding it), `kind` (`Read`, `SafeRead`, `Write`), `name`, `decl_file` (the declaration's `files` id,
//! NULL for a library's), `decl_path`, `decl_line`, `file` (the file the read is in: the code's own, a template's `.html` or an inline
//! template's `.ts`), `line`, `col` (the read's own place; on a map before rows 134 the expression's line, a template's its binding's, and no
//! column). Indexed on `(decl_file, decl_line)`.

use anyhow::{bail, Result};
use rusqlite::Connection;

/// Writes `ts_reads` into `main` from the attached base `b`; the rows written.
pub fn write(db: &Connection) -> Result<usize> {
    let has = |table: &str| -> Result<bool> {
        Ok(db.query_row("SELECT count(*) FROM b.sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get::<_, i64>(0))? > 0)
    };
    if !has("expressions__ast")? {
        bail!("ts_reads: the base map holds no expression trees (`expressions__ast`)");
    }
    // A TEMPLATE READ ON A MAP BEFORE ITS OWN PLACE: its binding's line, where the half wrote bindings.
    let (binding_join, binding_line) = if has("bindings")? {
        ("LEFT JOIN b.bindings bd ON bd.id = x.binding", "bd.line, ")
    } else {
        ("", "")
    };
    db.execute_batch("CREATE TABLE ts_reads (expression, kind, name, decl_file, decl_path, decl_line, file, line, col);")?;
    let rows = db.execute(
        &format!(
            "INSERT INTO ts_reads
             SELECT min(t.owner), k.value, nm.value, df.id, tf.value, tl.value, coalesce(nullif(x.file, ''), x.owner_file) AS read_file,
                    coalesce(rl.value, {binding_line}x.line) AS read_line, rc.value AS read_col
             FROM b.expressions__ast t
             JOIN b.expressions__ast nm ON nm.role = 'name' AND nm.parent = t.n AND nm.owner = t.owner
             LEFT JOIN b.expressions__ast tf ON tf.role = 'file' AND tf.parent = t.n AND tf.owner = t.owner
             LEFT JOIN b.expressions__ast tl ON tl.role = 'line' AND tl.parent = t.n AND tl.owner = t.owner
             LEFT JOIN b.expressions__ast k ON k.role = 'k' AND k.parent IS t.parent AND k.owner = t.owner
             LEFT JOIN b.expressions__ast rl ON rl.role = 'line' AND rl.parent IS t.parent AND rl.owner = t.owner
             LEFT JOIN b.expressions__ast rc ON rc.role = 'col' AND rc.parent IS t.parent AND rc.owner = t.owner
             JOIN b.expressions x ON x.id = t.owner
             {binding_join}
             LEFT JOIN b.files df ON df.path = tf.value AND df.half = 'typescript'
             WHERE t.role = 'target' AND x.half = 'typescript'
             GROUP BY k.value, nm.value, df.id, tf.value, tl.value, read_file, read_line, read_col
             ORDER BY read_file, read_line, read_col, nm.value"
        ),
        [],
    )?;
    db.execute_batch("CREATE INDEX ix_ts_reads_decl ON ts_reads (decl_file, decl_line); CREATE INDEX ix_ts_reads_file ON ts_reads (file, line);")?;
    Ok(rows)
}
