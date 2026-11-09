//! `--subtree OWNER [--node N] [--table T]`: one node's whole subtree, printed as the tree it is.
//!
//! A STRUCTURE IS NODE ROWS (`rows::cells`), and a node's subtree is the RANGE `n BETWEEN s.n AND s.last` of its
//! owner, so this is one read of the `(owner, n)` key - never a recursive walk over `parent`, which on a large
//! tree cost a minute for what the range reads at once. `OWNER` is the row id the structure belongs to (`x:41`,
//! `#5`); `--node` the `n` to start at, the root 0 when it is not given; `--table` narrows to one node table
//! (`expressions__ast`) or to every node table of one row table (`expressions`). Without it every node table
//! holding that node answers, one tree each.
//!
//! EACH LINE IS ONE NODE: its `n`, then its `role` (`[ord]` for a list item) and `node` indented by its depth below
//! the start, and a scalar's `value`. `--limit` counts nodes per tree; `--width` cuts a value as every lens does.

use super::{cell, clip, columns_of, escape, query};
use crate::rows::cells;
use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::Connection;
use std::fmt::Write as _;

/// Prints every tree found and says whether there was one: a node no table holds is a refused question.
pub(super) fn lens(db: &Connection, out: &mut String, owner: &str, node: &str, table: &str, limit: usize, width: usize) -> Result<bool> {
    if owner.is_empty() {
        let _ = writeln!(out, "--subtree needs the owner a structure belongs to: --subtree x:41 [--node 5] [--table expressions__ast]");
        return Ok(false);
    }
    let Ok(start) = (if node.is_empty() { Ok(0) } else { node.trim().parse::<i64>() }) else {
        let _ = writeln!(out, "--node takes a node's n, a number: {node}");
        return Ok(false);
    };
    let key = cells::owner_value(owner);
    let (_, registered) = query(db, &format!("SELECT tbl, child FROM {} ORDER BY child", cells::REGISTRY), &[])?;
    let mut found = false;
    for row in registered {
        let (Value::Text(parent), Value::Text(child)) = (&row[0], &row[1]) else { continue };
        if !table.is_empty() && table != parent && table != child {
            continue;
        }
        // A MAP WRITTEN BEFORE A SUBTREE WAS A RANGE has no `last`: said once, and the map rebuilds on its next run.
        if !columns_of(db, child).iter().any(|c| c == "last") {
            let _ = writeln!(out, "{child} has no `last` column: the map predates --subtree - rebuild it with the entry point");
            return Ok(false);
        }
        let t = escape(child);
        let (_, top) = query(db, &format!("SELECT last, depth FROM \"{t}\" WHERE owner = ?1 AND n = ?2"), &[&key, &start])?;
        let Some([Value::Integer(last), Value::Integer(depth)]) = top.first().map(|r| [r[0].clone(), r[1].clone()]) else { continue };
        found = true;
        let total = last - start + 1;
        let _ = writeln!(out, "{child}  {owner}  n {start}..{last}  ({total} node(s))");
        let (_, nodes) = query(
            db,
            &format!("SELECT n, depth, role, ord, node, value FROM \"{t}\" WHERE owner = ?1 AND n BETWEEN ?2 AND ?3 ORDER BY n LIMIT ?4"),
            &[&key, &start, &last, &i64::try_from(limit).unwrap_or(i64::MAX)],
        )?;
        let pad = last.to_string().len();
        for r in &nodes {
            let level = match r[1] { Value::Integer(d) => (d - depth).max(0) as usize, _ => 0 };
            let role = match (&r[2], &r[3]) {
                (Value::Null, Value::Integer(i)) => format!("[{i}]"),
                (Value::Null, _) => String::new(),
                (role, Value::Integer(i)) => format!("{}[{i}]", cell(role)),
                (role, _) => cell(role),
            };
            let value = match &r[5] { Value::Null => String::new(), v => format!(" {}", clip(&cell(v), width)) };
            let gap = if role.is_empty() { "" } else { " " };
            let _ = writeln!(out, "{:>pad$}  {}{role}{gap}{}{value}", cell(&r[0]), "  ".repeat(level), cell(&r[4]));
        }
        if (nodes.len() as i64) < total {
            let _ = writeln!(out, "... {} more node(s); raise --limit", total - nodes.len() as i64);
        }
    }
    if !found {
        let narrowed = if table.is_empty() { String::new() } else { format!(" of {table}") };
        let _ = writeln!(out, "no node {start} of {owner} in any node table{narrowed}");
    }
    Ok(found)
}
