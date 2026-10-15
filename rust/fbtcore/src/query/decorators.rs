//! `--decorators [FRAGMENT]`: what decorates what, and what is decorated twice. A python-map lens, kept
//! beside `--dead` so `query.rs` holds the lenses every map shares.
//!
//! THE FRAGMENT ALSO MATCHES THE FILE, and a row carries what the decorator is CALLED WITH: "which routes
//! does app.py serve" is `--decorators app.py`, and the route of `@app.post("/items")` is in its `args`
//! column rather than behind a `--cat`. A database written before `args` existed shows it empty.

use super::{cell, columns_of, named, query, show};
use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::Connection;

pub(super) fn lens(db: &Connection, out: &mut String, fragment: &str, limit: usize, width: usize) -> Result<()> {
    let like = format!("%{fragment}%");
    // `args` is a list, so nodes: its items joined in order, or the cell itself where it was a scalar.
    let scalar = if columns_of(db, "decorators").iter().any(|c| c == "args") { "d.args" } else { "NULL" };
    let args = match crate::rows::dups::nodes_of(db, "decorators", "args") {
        Some(_) => format!(
            "coalesce({scalar}, (SELECT group_concat(j.value, ', ' ORDER BY j.ord) FROM {} j WHERE j.owner = {}))",
            crate::rows::dups::items_sql(db, "decorators", "args", &["value"]),
            crate::rows::dups::owner_sql(db, "decorators", "d")
        ),
        None => scalar.to_string(),
    };
    let sql = format!(
        "SELECT f.path, d.line, d.name, {args}, d.target, d.target_kind, \
         (SELECT count(*) FROM decorators o WHERE o.file = d.file AND o.target = d.target \
          AND o.name = d.name) n \
         FROM decorators d JOIN files f ON f.id = d.file \
         WHERE d.name LIKE ?1 OR d.target LIKE ?1 OR f.path LIKE ?1 ORDER BY n DESC, f.path, d.line"
    );
    let (_, found) = query(db, &sql, &[&like])?;
    let rows: Vec<Vec<Value>> = found
        .into_iter()
        .map(|r| {
            let repeated = matches!(r[6], Value::Integer(n) if n > 1);
            vec![
                r[0].clone(),
                r[1].clone(),
                r[2].clone(),
                Value::Text(given(&r[3])),
                r[4].clone(),
                r[5].clone(),
                Value::Text(if repeated { "REPEATED".to_string() } else { String::new() }),
            ]
        })
        .collect();
    show(
        out,
        &rows,
        &named(&["file", "line", "decorator", "args", "target", "kind", "repeat"]),
        limit,
        width,
    );
    Ok(())
}

/// The arguments as they are written, comma-separated - `"/items", response_model=Item` - joined from the list's
/// nodes by the query; a cell that is not a list is shown as it is.
fn given(raw: &Value) -> String {
    cell(raw)
}
