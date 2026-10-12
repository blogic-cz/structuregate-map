//! A LIST HANDED TO A CHILD'S `@Input` is the child's list: `<demo-child [items]="list">` over `list = [{ label: t('k') }]`
//! (or a call returning one) is what the child's `*ngFor="let i of items"` loops over, so `i.label` renders the key in the
//! CHILD's template and the child's render paths carry it through the parent.
//!
//! ONLY WHEN EVERY RENDER EDGE INTO THE CHILD BINDS THE INPUT TO A BARE READ OF A FIELD WHOSE LIST IS KNOWN, and the child
//! writes the input nowhere: one parent that binds something else (a call, a filter, nothing) leaves the input unread - a
//! list read in part would publish a key only some edges show. The elements are the union of the parents' own, each parent
//! field once; a parent's branches stay on its elements, the edge's conditions are the render path's.

use super::astreads::{is_read, read_rows, unwrap};
use super::gate_input_flags::{input_alias, written_members};
use super::key_fields::{id_of, text};
use super::key_loops::Element;
use super::store::Store;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// `@Input` member id -> the elements of the lists every parent binds to it.
pub(super) fn input_lists(store: &Store<'_>, lists: &HashMap<String, Vec<Element>>) -> HashMap<String, Vec<Element>> {
    let written = written_members(store);
    // class -> (input alias -> member id), for the inputs nothing initialises or writes
    let mut inputs: HashMap<String, HashMap<String, String>> = HashMap::new();
    for m in store.table("members").iter() {
        let (Some(id), Some(class)) = (id_of(m, "id"), text(m, "class")) else { continue };
        if lists.contains_key(&id) || written.contains(&id) || m.get("value").is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty())) {
            continue;
        }
        if let Some(alias) = input_alias(m) {
            inputs.entry(class).or_default().insert(alias, id);
        }
    }
    let mut out = HashMap::new();
    if inputs.is_empty() {
        return out;
    }
    let expressions = store.table("expressions");
    let tree_of: HashMap<String, &Value> = expressions.iter().filter_map(|e| Some((text(e, "binding")?, e.get("ast").filter(|a| !a.is_null())?))).collect();
    let mut bound: HashMap<(String, String), Vec<&Value>> = HashMap::new();
    for b in store.table("bindings").iter().filter(|b| text(b, "kind").as_deref() == Some("input")) {
        if let (Some(id), Some(node), Some(name)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) {
            bound.entry((node, name)).or_default().extend(tree_of.get(&id).copied());
        }
    }
    // member id -> the parent fields bound so far, or None once an edge binds anything else
    let mut parents: HashMap<String, Option<Vec<String>>> = HashMap::new();
    for r in store.table("renders").iter() {
        let (Some(class), Some(node)) = (text(r, "to_class"), text(r, "node")) else { continue };
        for (alias, member) in inputs.get(&class).into_iter().flatten() {
            let field = match bound.get(&(node.clone(), alias.clone())).map(Vec::as_slice) {
                Some([tree]) if is_read(unwrap(tree)) => read_rows(unwrap(tree)).into_iter().next().filter(|row| lists.contains_key(row)),
                _ => None,
            };
            let slot = parents.entry(member.clone()).or_insert_with(|| Some(Vec::new()));
            match (slot.as_mut(), field) {
                (Some(fields), Some(row)) => fields.push(row),
                _ => *slot = None,
            }
        }
    }
    for (member, fields) in parents {
        let mut seen = HashSet::new();
        let items: Vec<Element> = fields.into_iter().flatten().filter(|f| seen.insert(f.clone())).flat_map(|f| lists[&f].clone()).collect();
        if !items.is_empty() {
            out.insert(member, items);
        }
    }
    out
}
