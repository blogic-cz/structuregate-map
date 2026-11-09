//! `--subtree OWNER [--node N] [--table T]`: one node's whole subtree, printed as the tree it is.
//! `--ancestors OWNER --node N [--table T]`: the path from the root down to that node.
//! `--children OWNER [--node N] [--table T]`: the nodes that hang directly off it, each with its subtree's size.
//!
//! A STRUCTURE IS NODE ROWS (`rows::cells`), and `n` is a gapless preorder with each node's `last`, so a node's
//! subtree is the RANGE `n BETWEEN s.n AND s.last` of its owner and its ancestors are the nodes whose range holds
//! it, `a.n <= x.n AND x.n <= a.last`, and its children are the nodes of its range one level down, `parent = s.n`
//! inside `n BETWEEN s.n + 1 AND s.last` - each one read of the `(owner, n)` key, never a recursive walk over `parent`,
//! which on a large tree cost a minute for what the range reads at once. `OWNER` is the row id the structure
//! belongs to (`x:41`, `#5`); `--node` the `n` asked about (the root 0 when it is not given, except for
//! `--ancestors`, which needs it); `--table` narrows to one node table (`expressions__ast`) or to every node table of one
//! row table (`expressions`). Without it every node table holding that node answers, one tree each.
//!
//! EACH LINE IS ONE NODE: its `n`, then its `role` (`[ord]` for a list item) and `node` indented by its depth -
//! below the start for a subtree, below the root for ancestors - and a scalar's `value`; a child that holds nodes
//! of its own says how many (`+N below`), which is what decides whether to ask `--subtree` of it. `--limit` counts nodes per
//! tree; `--width` cuts a value as every lens does.

use super::{cell, clip, columns_of, escape, query};
use crate::rows::cells;
use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::Connection;
use std::fmt::Write as _;

/// Which way from the node a lens reads.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Way {
    /// The node and everything below it.
    Down,
    /// The root, every node between, and the node.
    Up,
    /// The nodes one level below it.
    Children,
}

/// Prints every tree found and says whether there was one: a node no table holds is a refused question.
pub(super) fn lens(db: &Connection, out: &mut String, way: Way, owner: &str, node: &str, table: &str, limit: usize, width: usize) -> Result<bool> {
    let flag = match way { Way::Down => "--subtree", Way::Up => "--ancestors", Way::Children => "--children" };
    if owner.is_empty() || (way == Way::Up && node.is_empty()) {
        let node_arg = if way == Way::Up { "--node 5" } else { "[--node 5]" };
        let _ = writeln!(out, "{flag} needs the owner a structure belongs to: {flag} x:41 {node_arg} [--table expressions__ast]");
        return Ok(false);
    }
    let Ok(start) = (if node.is_empty() { Ok(0) } else { node.trim().parse::<i64>() }) else {
        let _ = writeln!(out, "--node takes a node's n, a number: {node}");
        return Ok(false);
    };
    let key = cells::owner_value(owner);
    let rows_limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let (_, registered) = query(db, &format!("SELECT tbl, child FROM {} ORDER BY child", cells::REGISTRY), &[])?;
    let mut found = false;
    for row in registered {
        let (Value::Text(parent), Value::Text(child)) = (&row[0], &row[1]) else { continue };
        if !table.is_empty() && table != parent && table != child {
            continue;
        }
        // A MAP WRITTEN BEFORE A SUBTREE WAS A RANGE has no `last`: said once, and the map rebuilds on its next run.
        if !columns_of(db, child).iter().any(|c| c == "last") {
            let _ = writeln!(out, "{child} has no `last` column: the map predates {flag} - rebuild it with the entry point");
            return Ok(false);
        }
        let t = escape(child);
        let (_, top) = query(db, &format!("SELECT last, depth FROM \"{t}\" WHERE owner = ?1 AND n = ?2"), &[&key, &start])?;
        let Some([Value::Integer(last), Value::Integer(depth)]) = top.first().map(|r| [r[0].clone(), r[1].clone()]) else { continue };
        found = true;
        const READ: &str = "SELECT n, depth, role, ord, node, value, last FROM";
        let (total, base, nodes) = match way {
            Way::Down => {
                let _ = writeln!(out, "{child}  {owner}  n {start}..{last}  ({} node(s))", last - start + 1);
                let sql = format!("{READ} \"{t}\" WHERE owner = ?1 AND n BETWEEN ?2 AND ?3 ORDER BY n LIMIT ?4");
                (last - start + 1, depth, query(db, &sql, &[&key, &start, &last, &rows_limit])?.1)
            }
            Way::Up => {
                let _ = writeln!(out, "{child}  {owner}  n {start}  depth {depth}  ({} node(s) from the root)", depth + 1);
                let sql = format!("{READ} \"{t}\" WHERE owner = ?1 AND n <= ?2 AND last >= ?2 ORDER BY n LIMIT ?3");
                (depth + 1, 0, query(db, &sql, &[&key, &start, &rows_limit])?.1)
            }
            Way::Children => {
                let below = format!("\"{t}\" WHERE owner = ?1 AND n BETWEEN ?2 + 1 AND ?3 AND parent = ?2");
                let (_, counted) = query(db, &format!("SELECT count(*) FROM {below}"), &[&key, &start, &last])?;
                let total = match counted.first().map(|r| &r[0]) { Some(Value::Integer(c)) => *c, _ => 0 };
                let _ = writeln!(out, "{child}  {owner}  n {start}  depth {depth}  ({total} child(ren))");
                let sql = format!("{READ} {below} ORDER BY n LIMIT ?4");
                (total, depth + 1, query(db, &sql, &[&key, &start, &last, &rows_limit])?.1)
            }
        };
        print_nodes(out, &nodes, base, last.max(start).to_string().len(), width, way == Way::Children);
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

/// One line per node of `(n, depth, role, ord, node, value, last)`, indented by its depth below `base`; `sizes`
/// adds how many nodes a node holds below it.
fn print_nodes(out: &mut String, nodes: &[Vec<Value>], base: i64, pad: usize, width: usize, sizes: bool) {
    for r in nodes {
        let level = match r[1] { Value::Integer(d) => (d - base).max(0) as usize, _ => 0 };
        let role = match (&r[2], &r[3]) {
            (Value::Null, Value::Integer(i)) => format!("[{i}]"),
            (Value::Null, _) => String::new(),
            (role, Value::Integer(i)) => format!("{}[{i}]", cell(role)),
            (role, _) => cell(role),
        };
        let value = match &r[5] { Value::Null => String::new(), v => format!(" {}", clip(&cell(v), width)) };
        let gap = if role.is_empty() { "" } else { " " };
        // HOW MUCH HANGS BELOW, said only where a list shows one level: a subtree and a path show it already.
        let held = match (sizes, &r[0], r.get(6)) {
            (true, Value::Integer(n), Some(Value::Integer(last))) if last > n => format!("  +{} below", last - n),
            _ => String::new(),
        };
        let _ = writeln!(out, "{:>pad$}  {}{role}{gap}{}{value}{held}", cell(&r[0]), "  ".repeat(level), cell(&r[4]));
    }
}
