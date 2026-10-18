//! THE GATES NO WAY GETS PAST: `*ngIf="false"` and `@if (false)`.
//!
//! A literal `false` names no enum member, so no `gate_values` row can say what it permits, and
//! folded as a gate like any other it left the key behind it counted on a live way, unrestricted -
//! read as rendered for everyone when it renders for no one. It is decided here, from the parsed
//! expression, and the ways through it are dropped before anything is folded (`keyreach::ways_of`).
//!
//! ONLY A RENDER GATE. A `[class.x]="false"` is a gate row too, and it hides nothing. A STYLE gate whose value hides
//! on every side (`style_hides.when = always`: `[style.display]="'none'"`, a ternary of two hiding literals) is the
//! other one: its element is never shown, so it is read exactly as `*ngIf="false"` (#54).

use super::store::{Row, Store};
use indexmap::IndexSet;
use serde_json::Value;

fn str_of<'a>(row: &'a Row, name: &str) -> Option<&'a str> {
    row.get(name).and_then(|v| v.as_str())
}

/// Whether an expression's tree is the literal `false`, the `Source` wrapper looked through.
fn is_false(ast: &Value) -> bool {
    match ast.get("k").and_then(|k| k.as_str()) {
        Some("Source") => ast.get("ast").is_some_and(is_false),
        Some("Literal") => ast.get("v") == Some(&Value::Bool(false)),
        _ => false,
    }
}

/// A GATE WHOSE ELEMENT DOES NOT RENDER WHILE ITS CONDITION IS FALSE: `*ngIf` and an `@if` branch. `[hidden]` and a
/// class a stylesheet proves hiding are the other two, read through their negated condition (`key_bound`).
pub fn renders_at_false(kind: Option<&str>, name: Option<&str>) -> bool {
    matches!((kind, name), (Some("structural"), Some("ngIf")) | (Some("block"), Some("IfBlockBranch")))
}

/// Every render gate whose condition is the literal `false`.
pub fn dead_gates(store: &Store<'_>) -> IndexSet<String> {
    // ONLY A RENDER GATE'S EXPRESSION IS ASKED. Asked of every expression, this scan rebuilt every tree of the
    // store from its node rows to answer for the few hundred a render gate names - seconds on a large workspace.
    let gates = store.table("gates");
    let render = |g: &Row| renders_at_false(str_of(g, "gate_kind"), str_of(g, "name"));
    let wanted: IndexSet<&str> = gates.iter().filter(|g| render(g)).filter_map(|g| str_of(g, "expression")).collect();
    // A style gate never shown is dead by its `style_hides` row (written before the closure, `cssmap/style.rs`).
    let mut out: IndexSet<String> = store.table("style_hides").iter()
        .filter(|h| str_of(h, "when") == Some("always"))
        .filter_map(|h| str_of(h, "gate").map(String::from))
        .collect();
    if wanted.is_empty() {
        return out;
    }
    let mut asts: IndexSet<String> = IndexSet::new();
    for e in store.table("expressions").iter() {
        // PEEKED, not kept: the tree is asked whether it is the literal, and nothing later reads it again.
        if let Some(id) = str_of(e, "id")
            && wanted.contains(id)
            && e.peek("ast", |ast| ast.is_some_and(is_false))
        {
            asts.insert(id.to_string());
        }
    }
    for g in gates.iter() {
        if render(g)
            && str_of(g, "expression").is_some_and(|x| asts.contains(x))
            && let Some(id) = str_of(g, "id")
        {
            out.insert(id.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn dead_of(gates: Value) -> Vec<String> {
        let tables = json!({
            "expressions": [
                {"id": "x:f", "ast": {"k": "Source", "src": "false", "ast": {"k": "Literal", "v": false}}},
                {"id": "x:t", "ast": {"k": "Source", "src": "true", "ast": {"k": "Literal", "v": true}}},
                {"id": "x:r", "ast": {"k": "Source", "src": "shown", "ast": {"k": "Read", "name": "shown"}}}
            ],
            "gates": gates
        });
        let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
        let store = Store::from_payload(map, "typescript");
        dead_gates(&store).into_iter().collect()
    }

    #[test]
    fn a_literal_false_ngif_and_if_block_are_dead_and_nothing_else_is() {
        let dead = dead_of(json!([
            {"id": "g:if", "gate_kind": "structural", "name": "ngIf", "expression": "x:f"},
            {"id": "g:block", "gate_kind": "block", "name": "IfBlockBranch", "expression": "x:f"},
            {"id": "g:true", "gate_kind": "structural", "name": "ngIf", "expression": "x:t"},
            {"id": "g:read", "gate_kind": "structural", "name": "ngIf", "expression": "x:r"}
        ]));
        assert_eq!(dead, vec!["g:if", "g:block"]);
    }

    #[test]
    fn a_class_binding_to_false_hides_nothing() {
        let dead = dead_of(json!([
            {"id": "g:cls", "gate_kind": "class", "name": "is-changed", "expression": "x:f"},
            {"id": "g:case", "gate_kind": "structural", "name": "ngSwitchCase", "expression": "x:f"}
        ]));
        assert!(dead.is_empty());
    }

    #[test]
    fn a_style_gate_that_always_hides_is_dead_and_one_that_hides_on_a_side_is_not() {
        let tables = json!({
            "expressions": [{"id": "x:r", "ast": {"k": "Read", "name": "shown"}}],
            "gates": [{"id": "g:if", "gate_kind": "structural", "name": "ngIf", "expression": "x:r"}],
            "style_hides": [
                {"id": "cssy:1", "gate": "g:never", "when": "always"},
                {"id": "cssy:2", "gate": "g:side", "when": "then"},
                {"id": "cssy:3", "gate": null, "when": "always"}
            ]
        });
        let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
        let store = Store::from_payload(map, "typescript");
        assert_eq!(dead_gates(&store).into_iter().collect::<Vec<_>>(), vec!["g:never"]);
    }
}
