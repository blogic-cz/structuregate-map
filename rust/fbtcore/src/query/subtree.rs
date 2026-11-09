//! `--subtree OWNER [--node N] [--table T]`: one node's whole subtree, printed as the tree it is.
//! `--ancestors OWNER --node N [--table T]`: the path from the root down to that node.
//! `--children OWNER [--node N] [--table T]`: the nodes that hang directly off it, each with its subtree's size.
//! `--siblings OWNER --node N [--table T]`: the children of its parent, the node itself marked among them.
//!
//! A STRUCTURE IS NODE ROWS (`rows::cells`), and `n` is a gapless preorder with each node's `last`, so a node's
//! subtree is the RANGE `n BETWEEN s.n AND s.last` of its owner and its ancestors are the nodes whose range holds
//! it, `a.n <= x.n AND x.n <= a.last`, and its children are the nodes of its range one level down, `parent = s.n`
//! inside `n BETWEEN s.n + 1 AND s.last`, and its siblings are its parent's children read the same way - each one read of the `(owner, n)` key, never a recursive walk over `parent`,
//! which on a large tree cost a minute for what the range reads at once. `OWNER` is the row id the structure
//! belongs to (`x:41`, `#5`); `--node` the `n` asked about (the root 0 when it is not given, except for
//! `--ancestors` and `--siblings`, which need it); `--table` narrows to one node table (`expressions__ast`) or to every node table of one
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

/// The columns every lens prints a node from, in `print_nodes` order.
const READ: &str = "SELECT n, depth, role, ord, node, value, last FROM";

/// Which way from the node a lens reads.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Way {
    /// The node and everything below it.
    Down,
    /// The root, every node between, and the node.
    Up,
    /// The nodes one level below it.
    Children,
    /// The nodes one level below its parent, it among them.
    Siblings,
}

/// Prints every tree found and says whether there was one: a node no table holds is a refused question.
pub(super) fn lens(db: &Connection, out: &mut String, way: Way, owner: &str, node: &str, table: &str, limit: usize, width: usize) -> Result<bool> {
    let flag = match way { Way::Down => "--subtree", Way::Up => "--ancestors", Way::Children => "--children", Way::Siblings => "--siblings" };
    let needs_node = matches!(way, Way::Up | Way::Siblings);
    if owner.is_empty() || (needs_node && node.is_empty()) {
        let node_arg = if needs_node { "--node 5" } else { "[--node 5]" };
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
        let (_, top) = query(db, &format!("SELECT last, depth, parent FROM \"{t}\" WHERE owner = ?1 AND n = ?2"), &[&key, &start])?;
        let Some([Value::Integer(last), Value::Integer(depth), up]) = top.first().map(|r| [r[0].clone(), r[1].clone(), r[2].clone()]) else { continue };
        found = true;
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
                let (total, nodes) = one_level(db, &t, &key, start, last, rows_limit)?;
                let _ = writeln!(out, "{child}  {owner}  n {start}  depth {depth}  ({total} child(ren))");
                (total, depth + 1, nodes)
            }
            Way::Siblings => {
                // THE ROOT HAS NO PARENT, so nothing stands beside it: said, and the node is still found.
                let Value::Integer(up) = up else {
                    let _ = writeln!(out, "{child}  {owner}  n {start}  depth {depth}  (the root: no siblings)");
                    continue;
                };
                let (_, held) = query(db, &format!("SELECT last FROM \"{t}\" WHERE owner = ?1 AND n = ?2"), &[&key, &up])?;
                let up_last = match held.first().map(|r| &r[0]) { Some(Value::Integer(l)) => *l, _ => last };
                let (total, nodes) = one_level(db, &t, &key, up, up_last, rows_limit)?;
                let _ = writeln!(out, "{child}  {owner}  n {start}  parent {up}  depth {depth}  ({total} node(s) under {up}, it among them)");
                (total, depth, nodes)
            }
        };
        let sizes = matches!(way, Way::Children | Way::Siblings);
        print_nodes(out, &nodes, base, width, sizes, if way == Way::Siblings { start } else { -1 });
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

/// The nodes one level below `top` (whose subtree ends at `last`) and how many there are, `--limit` of them read.
fn one_level(db: &Connection, t: &str, key: &Value, top: i64, last: i64, limit: i64) -> Result<(i64, Vec<Vec<Value>>)> {
    let below = format!("\"{t}\" WHERE owner = ?1 AND n BETWEEN ?2 + 1 AND ?3 AND parent = ?2");
    let (_, counted) = query(db, &format!("SELECT count(*) FROM {below}"), &[key, &top, &last])?;
    let total = match counted.first().map(|r| &r[0]) { Some(Value::Integer(c)) => *c, _ => 0 };
    let (_, nodes) = query(db, &format!("{READ} {below} ORDER BY n LIMIT ?4"), &[key, &top, &last, &limit])?;
    Ok((total, nodes))
}

/// One line per node of `(n, depth, role, ord, node, value, last)`, indented by its depth below `base`; `sizes`
/// adds how many nodes a node holds below it, and the node numbered `mark` says it is the one asked about.
fn print_nodes(out: &mut String, nodes: &[Vec<Value>], base: i64, width: usize, sizes: bool, mark: i64) {
    let pad = nodes.iter().map(|r| cell(&r[0]).len()).max().unwrap_or(1);
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
        let asked = if matches!(r[0], Value::Integer(n) if n == mark) { "  <- asked" } else { "" };
        let _ = writeln!(out, "{:>pad$}  {}{role}{gap}{}{value}{held}{asked}", cell(&r[0]), "  ".repeat(level), cell(&r[4]));
    }
}
