//! `--subtree OWNER [--node N] [--table T]`: one node's whole subtree, printed as the tree it is.
//! `--ancestors OWNER --node N [--table T]`: the path from the root down to that node.
//! `--children OWNER [--node N] [--table T]`: the nodes that hang directly off it, each with its subtree's size.
//! `--siblings OWNER --node N [--table T]`: the children of its parent, the node itself marked among them.
//! `--descendants-of-kind OWNER --kind K [--node N] [--table T]`: every object below it whose kind is K.
//! `--path-to OWNER --node N [--from F] [--table T]`: the way down from F (the root) to N, and its accessor path.
//! `--leaves OWNER [--node N] [--table T]`: the nodes of its subtree that hold nothing, each one's own `last`.
//! `--depth-range OWNER --depth A[-B] [--node N] [--table T]`: the nodes of its subtree A to B levels below it.
//!
//! A STRUCTURE IS NODE ROWS (`rows::cells`), and `n` is a gapless preorder with each node's `last`, so a node's
//! subtree is the RANGE `n BETWEEN s.n AND s.last` of its owner and its ancestors are the nodes whose range holds
//! it, `a.n <= x.n AND x.n <= a.last`, and its children are the nodes of its range one level down, `parent = s.n`
//! inside `n BETWEEN s.n + 1 AND s.last`, and its siblings are its parent's children read the same way, and its descendants of a kind are the `k`/`kind` members inside its range whose value is the
//! kind, each joined to its parent, the object it names - each one read of the `(owner, n)` key, never a recursive walk over `parent`,
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
    /// Every object below it whose kind is the one asked for.
    OfKind,
    /// The nodes from `--from` down to it, and the roles that lead there.
    PathTo,
    /// The nodes of its subtree that hold no node of their own.
    Leaves,
    /// The nodes of its subtree between two levels below it.
    Levels,
}

/// The member that names an object's KIND: `k` in the expression shapes, `kind` where a half names it so.
const KIND_ROLES: &str = "('k', 'kind')";

/// Prints every tree found and says whether there was one: a node no table holds is a refused question.
pub(super) fn lens(db: &Connection, out: &mut String, way: Way, ask: &super::Ask, limit: usize, width: usize) -> Result<bool> {
    let (owner, node, table, kind) = (ask.arg.as_str(), ask.node.as_str(), ask.table.as_str(), ask.kind.trim());
    let flag = match way {
        Way::Down => "--subtree",
        Way::Up => "--ancestors",
        Way::Children => "--children",
        Way::Siblings => "--siblings",
        Way::OfKind => "--descendants-of-kind",
        Way::PathTo => "--path-to",
        Way::Leaves => "--leaves",
        Way::Levels => "--depth-range",
    };
    // THE LEVELS ARE COUNTED BELOW THE NODE: `--depth 1` is its children, `--depth 0-2` it and two levels down.
    let levels = match ask.depth.trim().split_once('-') {
        None => ask.depth.trim().parse::<u32>().map(|one| (one, one)),
        Some((a, b)) => a.trim().parse::<u32>().and_then(|a| b.trim().parse::<u32>().map(|b| (a, b))),
    };
    let (near, far) = match (way, levels) {
        (Way::Levels, Ok((a, b))) if a <= b => (i64::from(a), i64::from(b)),
        (Way::Levels, _) => {
            let _ = writeln!(out, "{flag} needs --depth A or A-B, the levels below the node (A <= B): {flag} x:41 --depth 1-2 [--node 5] [--table expressions__ast]");
            return Ok(false);
        }
        _ => (0, 0),
    };
    if way == Way::OfKind && (owner.is_empty() || kind.is_empty()) {
        let _ = writeln!(out, "{flag} needs the owner and the kind: {flag} x:41 --kind Call [--node 5] [--table expressions__ast]");
        return Ok(false);
    }
    let needs_node = matches!(way, Way::Up | Way::Siblings | Way::PathTo);
    if owner.is_empty() || (needs_node && node.is_empty()) {
        let node_arg = if needs_node { "--node 5" } else { "[--node 5]" };
        let _ = writeln!(out, "{flag} needs the owner a structure belongs to: {flag} x:41 {node_arg} [--table expressions__ast]");
        return Ok(false);
    }
    let Ok(start) = (if node.is_empty() { Ok(0) } else { node.trim().parse::<i64>() }) else {
        let _ = writeln!(out, "--node takes a node's n, a number: {node}");
        return Ok(false);
    };
    let Ok(from) = (if ask.from.is_empty() { Ok(0) } else { ask.from.trim().parse::<i64>() }) else {
        let _ = writeln!(out, "--from takes a node's n, a number: {}", ask.from);
        return Ok(false);
    };
    let key = cells::owner_value(owner);
    let rows_limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let (_, registered) = query(db, &format!("SELECT tbl, child FROM {} ORDER BY child", cells::REGISTRY), &[])?;
    let (mut found, mut refused) = (false, false);
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
            Way::OfKind => {
                // A KIND IS A MEMBER, so the range is read for the members and each joins its object, `k.parent`.
                let named = format!(
                    "\"{t}\" k JOIN \"{t}\" o ON o.owner = k.owner AND o.n = k.parent \
                     WHERE k.owner = ?1 AND k.n BETWEEN ?2 + 1 AND ?3 AND k.role IN {KIND_ROLES} AND k.value = ?4 AND o.n > ?2"
                );
                let (_, counted) = query(db, &format!("SELECT count(*) FROM {named}"), &[&key, &start, &last, &kind])?;
                let total = match counted.first().map(|r| &r[0]) { Some(Value::Integer(c)) => *c, _ => 0 };
                let _ = writeln!(out, "{child}  {owner}  n {start}..{last}  ({total} node(s) of kind {kind})");
                let sql = format!("SELECT o.n, o.depth, o.role, o.ord, o.node, k.value, o.last FROM {named} ORDER BY o.n LIMIT ?5");
                (total, depth + 1, query(db, &sql, &[&key, &start, &last, &kind, &rows_limit])?.1)
            }
            Way::Leaves => {
                // A LEAF ENDS WHERE IT STARTS: `last = n`, a scalar or an empty list or object.
                let leaves = format!("\"{t}\" WHERE owner = ?1 AND n BETWEEN ?2 AND ?3 AND last = n");
                let (_, counted) = query(db, &format!("SELECT count(*) FROM {leaves}"), &[&key, &start, &last])?;
                let total = match counted.first().map(|r| &r[0]) { Some(Value::Integer(c)) => *c, _ => 0 };
                let _ = writeln!(out, "{child}  {owner}  n {start}..{last}  ({total} leaf node(s) of {})", last - start + 1);
                (total, depth, query(db, &format!("{READ} {leaves} ORDER BY n LIMIT ?4"), &[&key, &start, &last, &rows_limit])?.1)
            }
            Way::Levels => {
                let band = format!("\"{t}\" WHERE owner = ?1 AND n BETWEEN ?2 AND ?3 AND depth BETWEEN ?4 AND ?5");
                let (lo, hi) = (depth + near, depth + far);
                let (_, counted) = query(db, &format!("SELECT count(*) FROM {band}"), &[&key, &start, &last, &lo, &hi])?;
                let total = match counted.first().map(|r| &r[0]) { Some(Value::Integer(c)) => *c, _ => 0 };
                let _ = writeln!(out, "{child}  {owner}  n {start}..{last}  levels {near}..{far} below it  ({total} node(s))");
                let sql = format!("{READ} {band} ORDER BY n LIMIT ?6");
                (total, depth, query(db, &sql, &[&key, &start, &last, &lo, &hi, &rows_limit])?.1)
            }
            Way::PathTo => {
                // THE WAY DOWN IS THE ANCESTORS OF N THAT START AT OR BELOW F: F must hold N, or there is no way.
                let (_, held) = query(db, &format!("SELECT last, depth FROM \"{t}\" WHERE owner = ?1 AND n = ?2"), &[&key, &from])?;
                let Some([Value::Integer(from_last), Value::Integer(from_depth)]) = held.first().map(|r| [r[0].clone(), r[1].clone()]) else {
                    let _ = writeln!(out, "{child}  {owner}  no node {from} to start from");
                    refused = true;
                    continue;
                };
                if start < from || start > from_last {
                    let _ = writeln!(out, "{child}  {owner}  n {start} is not below n {from} (n {from}..{from_last})");
                    refused = true;
                    continue;
                }
                let sql = format!("{READ} \"{t}\" WHERE owner = ?1 AND n BETWEEN ?2 AND ?3 AND last >= ?3 ORDER BY n");
                let mut nodes = query(db, &sql, &[&key, &from, &start])?.1;
                // A LIST ITEM IS `[ord]` AFTER ITS LIST, whose role it repeats: `c[1].k`, never `c.c[1].k`.
                let steps: Vec<String> = nodes.iter().skip(1).map(|r| label(if matches!(r[3], Value::Integer(_)) { &Value::Null } else { &r[2] }, &r[3])).collect();
                let path = steps.iter().fold(String::new(), |acc, s| match (acc.is_empty(), s.starts_with('[')) {
                    (true, _) | (false, true) => acc + s,
                    (false, false) => acc + "." + s,
                });
                let path = if path.is_empty() { "(the start itself)".to_string() } else { path };
                let total = nodes.len() as i64;
                let _ = writeln!(out, "{child}  {owner}  n {from} to {start}  ({total} node(s))  path {path}");
                nodes.truncate(limit);
                (total, from_depth, nodes)
            }
        };
        let sizes = matches!(way, Way::Children | Way::Siblings | Way::OfKind | Way::Levels);
        print_nodes(out, &nodes, base, width, sizes, if way == Way::Siblings { start } else { -1 });
        if (nodes.len() as i64) < total {
            let _ = writeln!(out, "... {} more node(s); raise --limit", total - nodes.len() as i64);
        }
    }
    if !found {
        let narrowed = if table.is_empty() { String::new() } else { format!(" of {table}") };
        let _ = writeln!(out, "no node {start} of {owner} in any node table{narrowed}");
    }
    Ok(found && !refused)
}

/// A node's place in its parent: its `role`, `[ord]` for a list item, both for a named list's item.
fn label(role: &Value, ord: &Value) -> String {
    match (role, ord) {
        (Value::Null, Value::Integer(i)) => format!("[{i}]"),
        (Value::Null, _) => String::new(),
        (role, Value::Integer(i)) => format!("{}[{i}]", cell(role)),
        (role, _) => cell(role),
    }
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
        let role = label(&r[2], &r[3]);
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
