//! A STATEMENT'S FINGERPRINT: the same query whatever its literals. The tree is hashed in preorder - each node's
//! depth, kind and value - with every literal's value left out and an IN list of nothing but literals counted as
//! one, so `Status = 1 AND Id IN (10, 11)` and `Status = 7 AND Id IN (42)` share it and `Amount + 1` does not
//! share `Amount`'s. One function over the dialect-free node kinds, so T-SQL and PostgreSQL agree.

use super::Node;
use std::collections::HashMap;

pub fn of(nodes: &[Node]) -> String {
    let at: HashMap<i64, usize> = nodes.iter().enumerate().map(|(i, node)| (node.n, i)).collect();
    let parent = |i: usize| nodes[i].parent.and_then(|p| at.get(&p).copied());
    // A LITERAL IN AN IN LIST OF LITERALS AFTER THE FIRST is no part of the shape: the list is one value.
    let mut dropped = vec![false; nodes.len()];
    for (i, node) in nodes.iter().enumerate() {
        if node.kind != "in" {
            continue;
        }
        let items: Vec<usize> = (0..nodes.len()).filter(|&c| parent(c) == Some(i)).skip(1).collect();
        if !items.is_empty() && items.iter().all(|&c| nodes[c].kind == "literal") {
            for &c in items.iter().skip(1) {
                dropped[c] = true;
            }
        }
    }
    // PREORDER: a parent is always before its children, so one pass gives every depth.
    let mut depth = vec![0usize; nodes.len()];
    let mut hash = blake3::Hasher::new();
    for (i, node) in nodes.iter().enumerate() {
        if let Some(p) = parent(i) {
            depth[i] = depth[p] + 1;
            dropped[i] |= dropped[p];
        }
        if dropped[i] {
            continue;
        }
        let value = if node.kind == "literal" { "" } else { node.value.as_str() };
        hash.update(format!("{}\t{}\t{}\n", depth[i], node.kind, value).as_bytes());
    }
    hash.finalize().to_hex()[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::super::Node;

    fn node(n: i64, parent: Option<i64>, kind: &str, value: &str) -> Node {
        Node { n, parent, kind: kind.into(), value: value.into(), line: 1 }
    }

    /// `x IN (<literals>)` under a select root.
    fn listed(values: &[&str]) -> Vec<Node> {
        let mut nodes = vec![node(0, None, "select", ""), node(1, Some(0), "in", ""), node(2, Some(1), "column", "x")];
        for (i, v) in values.iter().enumerate() {
            nodes.push(node(3 + i as i64, Some(1), "literal", v));
        }
        nodes
    }

    #[test]
    fn literals_and_list_lengths_do_not_move_it() {
        assert_eq!(super::of(&listed(&["10", "11"])), super::of(&listed(&["42"])));
    }

    #[test]
    fn a_name_or_a_kind_moves_it() {
        let mut other = listed(&["10"]);
        other[2].value = "y".into();
        assert_ne!(super::of(&listed(&["10"])), super::of(&other));
        let mut param = listed(&["10", "11"]);
        param[3].kind = "param".into();
        assert_ne!(super::of(&listed(&["10", "11"])), super::of(&param));
    }
}
