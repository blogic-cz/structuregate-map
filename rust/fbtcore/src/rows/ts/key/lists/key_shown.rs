//! HOW A CONTROL SHOWS EACH PROPERTY OF THE ELEMENTS IT IS OFFERED (#156): `control_options.shown_as` and `shown_node`.
//!
//! `<choice [(value)]="m.mode" [items]="modes">` is offered `{ Id, Caption, Help }` elements, and only the control's own
//! template says which property is the choice's text and which is help beside it. Read there, never from a property's name:
//! the control's node renders a component (`renders`), the input it is handed the list under is one of that component's
//! `@Input` members (`io`), and a loop of its template (an `*ngFor` or an `@for`) whose collection reads that member renders
//! each element as the loop variable. Every `<variable>.<property>` the loop's subtree SHOWS - an interpolation, or an input
//! binding that is no gate (`*ngIf="item.Help"` decides, it shows nothing) - is one place the property is rendered:
//!
//! - `hint`   - the input is named in `hintInputs` (a tooltip: default `title`, `matTooltip`), or the node or an ancestor of it
//!   in the template is an element `hintTags` names (default `mat-hint`);
//! - `option` - any other place. One place showing the property as an option makes it one.
//!
//! A control whose template is not in the tree (a library's), an input that is no `@Input` of it, and a property no loop
//! over that input shows, are NULL. `shown_node` is the template node the answer was read at.

use super::astreads::{read_rows, unwrap};
use super::key_fields::{id_of, text};
use super::key_options::Offered;
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// One loop of a template: the node declaring its item, the item's name, and the collection's expression tree.
pub(super) struct Head {
    pub node: String,
    pub variable: String,
    pub ast: Value,
}

/// EVERY LOOP OF EVERY TEMPLATE, two ways: an `ngForOf` gate on the `*ngFor` wrapper, its expression the collection; an `@for`
/// block, which is no gate - its collection is the `block_ForLoopBlock` expression on its own node. Either node declares the
/// item as its `$implicit` variable.
pub(super) fn loop_heads(store: &Store<'_>) -> Vec<Head> {
    let nodes = store.table("template_nodes");
    let mut implicit: HashMap<String, String> = HashMap::new();
    let mut blocks: HashSet<String> = HashSet::new();
    for n in nodes.iter() {
        let Some(id) = id_of(n, "id") else { continue };
        if text(n, "kind").as_deref() == Some("ForLoopBlock") {
            blocks.insert(id.clone());
        }
        let Some(Value::Array(vs)) = n.get("variables") else { continue };
        let item = vs.iter().find(|v| v.get("value").and_then(Value::as_str) == Some("$implicit"));
        if let Some(name) = item.and_then(|v| v.get("name")).and_then(Value::as_str) {
            implicit.insert(id, name.to_string());
        }
    }
    let expressions = store.table("expressions");
    let by_id: HashMap<String, &Row> = expressions.iter().filter_map(|e| id_of(e, "id").map(|i| (i, e))).collect();
    let gates = store.table("gates");
    let iterated: Vec<(String, String)> = gates
        .iter()
        .filter(|g| text(g, "name").as_deref() == Some("ngForOf"))
        .filter_map(|g| Some((id_of(g, "node")?, id_of(g, "expression")?)))
        .chain(expressions.iter().filter(|e| text(e, "kind").as_deref() == Some("block_ForLoopBlock")).filter_map(|e| {
            let node = id_of(e, "node").filter(|n| blocks.contains(n))?;
            Some((node, id_of(e, "id")?))
        }))
        .collect();
    iterated
        .into_iter()
        .filter_map(|(node, expr)| {
            let ast = by_id.get(&expr)?.get("ast").filter(|a| !a.is_null())?.clone();
            let variable = implicit.get(&node)?.clone();
            Some(Head { node, variable, ast })
        })
        .collect()
}

/// (control node, input) -> property -> (`option` or `hint`, the node it was read at).
pub(super) type Shown = HashMap<(String, String), HashMap<String, (&'static str, String)>>;

/// How each control of `options` shows each property of its elements, read from its own template.
pub(super) fn shown(store: &Store<'_>, options: &[Offered]) -> Shown {
    let mut out = Shown::new();
    let wanted = handed_members(store, options);
    if wanted.is_empty() {
        return out;
    }
    let (tags, inputs) = (store.option_list("hint_tags"), store.option_list("hint_inputs"));
    let nodes = store.table("template_nodes");
    let mut parent: HashMap<String, String> = HashMap::new();
    let mut tag: HashMap<String, String> = HashMap::new();
    let mut children: HashMap<String, Vec<&Row>> = HashMap::new();
    for n in nodes.iter() {
        let Some(id) = id_of(n, "id") else { continue };
        if let Some(t) = text(n, "tag") {
            tag.insert(id.clone(), t);
        }
        if let Some(p) = id_of(n, "parent") {
            children.entry(p.clone()).or_default().push(n);
            parent.insert(id, p);
        }
    }
    // WHAT A NODE SHOWS: its interpolations and its input bindings, never a gate's condition - nor the binding of a gate that reads
    // its condition from a row of its own (`[hidden]`'s negation), matched by its node and name.
    let gates = store.table("gates");
    let gated: HashSet<String> = gates.iter().filter_map(|g| id_of(g, "expression")).collect();
    let gate_inputs: HashSet<(String, String)> = gates.iter().filter_map(|g| Some((id_of(g, "node")?, text(g, "name")?))).collect();
    let expressions = store.table("expressions");
    let mut shows: HashMap<String, Vec<(Option<String>, &Value)>> = HashMap::new();
    for e in expressions.iter() {
        let kind = text(e, "kind").unwrap_or_default();
        let (Some(id), Some(node)) = (id_of(e, "id"), id_of(e, "node")) else { continue };
        let gate_input = kind == "input" && text(e, "name").is_some_and(|n| gate_inputs.contains(&(node.clone(), n)));
        if !(kind == "interpolation" || kind == "input") || gated.contains(&id) || gate_input {
            continue;
        }
        if let Some(ast) = e.get("ast").filter(|a| !a.is_null()) {
            shows.entry(node).or_default().push((text(e, "name"), ast));
        }
    }
    // A NODE UNDER A HINT ELEMENT: itself or any ancestor tagged as one (a visited set; a parent chain is a tree).
    let under_hint = |start: &str| {
        let mut seen = HashSet::new();
        let mut at = Some(start.to_string());
        while let Some(id) = at {
            if !seen.insert(id.clone()) {
                return false;
            }
            if tag.get(&id).is_some_and(|t| tags.contains(t)) {
                return true;
            }
            at = parent.get(&id).cloned();
        }
        false
    };
    for head in loop_heads(store) {
        let owners: Vec<&(String, String)> = read_rows(unwrap(&head.ast)).iter().filter_map(|r| wanted.get(r)).flatten().collect();
        if owners.is_empty() {
            continue;
        }
        let mut found: HashMap<String, (&'static str, String)> = HashMap::new();
        let mut stack = vec![head.node.clone()];
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            for (name, ast) in shows.get(&id).into_iter().flatten() {
                let mut props = Vec::new();
                super::key_loops::property_reads(unwrap(ast), &head.variable, &Default::default(), &mut props);
                for prop in props {
                    let hint = name.as_ref().is_some_and(|n| inputs.contains(n)) || under_hint(&id);
                    let as_ = if hint { "hint" } else { "option" };
                    let slot = found.entry(prop).or_insert((as_, id.clone()));
                    if slot.0 == "hint" && as_ == "option" {
                        *slot = (as_, id.clone());
                    }
                }
            }
            // A CHILD DECLARING THE SAME NAME shadows the item below it.
            for child in children.get(&id).into_iter().flatten().rev() {
                if !super::key_loops::declares(child, &head.variable)
                    && let Some(c) = id_of(child, "id")
                {
                    stack.push(c);
                }
            }
        }
        for owner in owners {
            let props = out.entry(owner.clone()).or_default();
            for (prop, (as_, node)) in &found {
                let slot = props.entry(prop.clone()).or_insert((*as_, node.clone()));
                if slot.0 == "hint" && *as_ == "option" {
                    *slot = (*as_, node.clone());
                }
            }
        }
    }
    out
}

/// Member row id -> each (control node, input) whose input is that `@Input` of a component the control's node renders.
fn handed_members(store: &Store<'_>, options: &[Offered]) -> HashMap<String, Vec<(String, String)>> {
    let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if options.is_empty() {
        return out;
    }
    let class_of: HashMap<String, String> = store.table("components").iter().filter_map(|c| Some((id_of(c, "id")?, id_of(c, "class")?))).collect();
    let mut renders: HashMap<String, Vec<String>> = HashMap::new();
    for r in store.table("renders").iter() {
        if let (Some(node), Some(to)) = (id_of(r, "node"), text(r, "to")) {
            renders.entry(node).or_default().push(to);
        }
    }
    let mut member_of: HashMap<(String, String), String> = HashMap::new();
    for io in store.table("io").iter().filter(|r| text(r, "kind").as_deref() == Some("input")) {
        let (Some(class), Some(member)) = (id_of(io, "class"), text(io, "member")) else { continue };
        let name = text(io, "binding_name").unwrap_or_else(|| member.clone());
        member_of.insert((class, name), member);
    }
    let members: HashMap<(String, String), String> =
        store.table("members").iter().filter_map(|m| Some(((id_of(m, "class")?, text(m, "name")?), id_of(m, "id")?))).collect();
    for o in options {
        for component in renders.get(&o.node).into_iter().flatten() {
            let Some(class) = class_of.get(component) else { continue };
            let Some(member) = member_of.get(&(class.clone(), o.input.clone())) else { continue };
            let Some(id) = members.get(&(class.clone(), member.clone())) else { continue };
            let list = out.entry(id.clone()).or_default();
            let owner = (o.node.clone(), o.input.clone());
            if !list.contains(&owner) {
                list.push(owner);
            }
        }
    }
    out
}
