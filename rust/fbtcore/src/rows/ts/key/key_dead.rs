//! THE GATES NO WAY GETS PAST: `*ngIf="false"` and `@if (false)`.
//!
//! A literal `false` names no enum member, so no `gate_values` row can say what it permits, and
//! folded as a gate like any other it left the key behind it counted on a live way, unrestricted -
//! read as rendered for everyone when it renders for no one. It is decided here, from the parsed
//! expression, and the ways through it are dropped before anything is folded (`keyreach::ways_of`).
//!
//! ONLY A RENDER GATE. A `[class.x]="false"` is a gate row too, and it hides nothing.

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

/// Every render gate whose condition is the literal `false`.
pub fn dead_gates(store: &Store<'_>) -> IndexSet<String> {
    let mut asts: IndexSet<String> = IndexSet::new();
    for e in store.table("expressions").iter() {
        if let (Some(id), Some(ast)) = (str_of(e, "id"), e.get("ast"))
            && is_false(ast)
        {
            asts.insert(id.to_string());
        }
    }
    let mut out = IndexSet::new();
    if asts.is_empty() {
        return out;
    }
    for g in store.table("gates").iter() {
        let renders = matches!(
            (str_of(g, "gate_kind"), str_of(g, "name")),
            (Some("structural"), Some("ngIf")) | (Some("block"), Some("IfBlockBranch"))
        );
        if renders
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
}
