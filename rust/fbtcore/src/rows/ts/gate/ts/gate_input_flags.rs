//! AN `@Input` FLAG IS WHAT ITS PARENT BINDS.
//!
//! `@Input() isWide = false;` read by the child's own `*ngIf="isCompact || isWide"` says
//! nothing in the child: the value is the parent's `[isWide]="isWide"`, and the parent's
//! field carries the comparison (`gate_props` reads it from there). So the child's flag is the
//! bound expression - the parent's tree, whose reads name the parent's rows - when the proof is
//! whole:
//!   * EVERY element the child renders at binds the input (a usage that does not leaves it at its
//!     initializer, which the binding says nothing of), and
//!   * every such binding is the SAME resolved tree, and
//!   * the child never writes the flag itself.
//!
//! Then the flag is that expression in both polarities - Angular sets the input before the child's
//! view is first checked, and again whenever it changes. Anything less and the flag is left as it
//! was: no definition, so a gate over it states nothing more than it did.

use super::astreads::unwrap;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// The `@Input` alias of a member, or nothing when it is no input.
pub fn input_alias(m: &Row) -> Option<String> {
    let decorators = m.get("decorators").and_then(|d| d.as_array())?;
    let d = decorators.iter().find(|d| d.get("name").and_then(|n| n.as_str()) == Some("Input"))?;
    // `@Input('alias')`, or `@Input({alias: 'x'})`; anything else is the member's own name.
    let arg = d.get("args").and_then(|a| a.get(0));
    let alias = arg.and_then(|a| a.as_str().map(str::to_string).or_else(|| a.get("alias")?.as_str().map(str::to_string)));
    alias.or_else(|| cell(m, "name"))
}

/// Input property id -> the one tree every usage binds it to.
pub fn bound_inputs(store: &Store<'_>, written: &IndexSet<String>) -> IndexMap<String, Value> {
    // class -> every node it is rendered at.
    let mut usages: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for r in store.table("renders").iter() {
        if let (Some(class), Some(node)) = (cell(r, "to_class"), cell(r, "node")) {
            usages.entry(class).or_default().insert(node);
        }
    }
    if usages.is_empty() {
        return IndexMap::new();
    }
    let expressions = store.table("expressions");
    let mut tree_of_binding: IndexMap<String, &Row> = IndexMap::new();
    for e in expressions.iter() {
        if let Some(b) = cell(e, "binding") {
            tree_of_binding.insert(b, e);
        }
    }
    // (node, input name) -> the binding trees on it.
    let mut bound: IndexMap<(String, String), Vec<Value>> = IndexMap::new();
    for b in store.table("bindings").iter() {
        if cell(b, "kind").as_deref() != Some("input") {
            continue;
        }
        let (Some(id), Some(node), Some(name)) = (cell(b, "id"), cell(b, "node"), cell(b, "name")) else { continue };
        let tree = tree_of_binding.get(&id).and_then(|e| e.get("ast")).filter(|a| !a.is_null()).map(|a| unwrap(a).clone());
        bound.entry((node, name)).or_default().push(tree.unwrap_or(Value::Null));
    }
    let mut out = IndexMap::new();
    for m in store.table("members").iter() {
        let (Some(id), Some(class)) = (cell(m, "id"), cell(m, "class")) else { continue };
        if cell(m, "kind").as_deref() != Some("property") || written.contains(&id) {
            continue;
        }
        let (Some(alias), Some(nodes)) = (input_alias(m), usages.get(&class)) else { continue };
        let mut one: Option<Value> = None;
        let agreed = nodes.iter().all(|node| match bound.get(&(node.clone(), alias.clone())).map(Vec::as_slice) {
            Some([tree]) if !tree.is_null() && one.as_ref().is_none_or(|held| held == tree) => {
                one = Some(tree.clone());
                true
            }
            _ => false,
        });
        if let (true, Some(tree)) = (agreed, one) {
            out.insert(id, tree);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store(over: Value) -> Store<'static> {
        let mut t = json!({
            "members": [{"id": "m:c", "class": "c:child", "name": "isPlus", "kind": "property",
                         "decorators": [{"name": "Input", "args": []}]}],
            "renders": [{"node": "n:1", "to_class": "c:child"}],
            "bindings": [{"id": "b:1", "node": "n:1", "name": "isPlus", "kind": "input"}],
            "expressions": [{"id": "x:1", "binding": "b:1",
                             "ast": {"k": "Source", "ast": {"k": "Read", "name": "isPlus", "target": {"row": "m:p"}}}}],
        });
        for (k, v) in over.as_object().cloned().unwrap_or_default() {
            t[k] = v;
        }
        Store::from_payload(t.as_object().cloned().unwrap(), "typescript")
    }

    #[test]
    fn an_input_every_usage_binds_alike_is_the_bound_tree() {
        let out = bound_inputs(&store(json!({})), &IndexSet::new());
        assert_eq!(out["m:c"], json!({"k": "Read", "name": "isPlus", "target": {"row": "m:p"}}));
    }

    #[test]
    fn a_usage_that_binds_nothing_two_trees_or_a_write_of_its_own_leave_it_alone() {
        let unbound = store(json!({"renders": [{"node": "n:1", "to_class": "c:child"}, {"node": "n:2", "to_class": "c:child"}]}));
        assert!(bound_inputs(&unbound, &IndexSet::new()).is_empty(), "n:2 leaves the initializer");
        let differ = store(json!({
            "renders": [{"node": "n:1", "to_class": "c:child"}, {"node": "n:2", "to_class": "c:child"}],
            "bindings": [{"id": "b:1", "node": "n:1", "name": "isPlus", "kind": "input"},
                         {"id": "b:2", "node": "n:2", "name": "isPlus", "kind": "input"}],
            "expressions": [{"id": "x:1", "binding": "b:1", "ast": {"k": "Read", "name": "a"}},
                            {"id": "x:2", "binding": "b:2", "ast": {"k": "Read", "name": "b"}}],
        }));
        assert!(bound_inputs(&differ, &IndexSet::new()).is_empty(), "two parents disagree");
        let written: IndexSet<String> = ["m:c".to_string()].into_iter().collect();
        assert!(bound_inputs(&store(json!({})), &written).is_empty());
    }
}
