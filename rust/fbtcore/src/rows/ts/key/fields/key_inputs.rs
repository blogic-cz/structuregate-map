//! A LIST HANDED TO A CHILD'S `@Input` is the child's list: `<demo-child [items]="list">` over `list = [{ label: t('k') }]`
//! (or a call returning one) is what the child's `*ngFor="let i of items"` loops over, so `i.label` renders the key in the
//! CHILD's template and the child's render paths carry it through the parent.
//!
//! EACH RENDER EDGE INTO THE CHILD HANDS ITS OWN LIST, when the child writes the input nowhere: a bare read of a field
//! whose list is known, or a read of a getter or a call of a method every return of which is a list (`[items]="build()"`,
//! `[items]="all"` over `get all() { return [..]; }`). AN ELEMENT CARRIES THE EDGE IT CAME THROUGH (`edge:<render>`), and
//! a way holds it only on a path through that edge (`keyreach::ways_of`): a key one parent's list holds renders under that
//! parent alone, and a parent that binds something else (a filter, a property path, nothing) adds nothing - it no longer
//! voids what the other parents bind. A parent's branches stay on its elements, the edge's conditions are the render path's.
//!
//! AN INPUT SETTER COPYING ITS PARAMETER INTO A FIELD (`@Input() set items(b) { this._items = b; }`) hands the list to that
//! field and to the getter of the input's name, which is what the child's `*ngFor="let i of items"` reads - when that copy
//! is the field's only write.

use super::astreads::{is_read, read_rows, unwrap};
use super::gate_input_flags::{input_alias, written_members};
use super::key_fields::{id_of, text};
use super::key_branches::Branches;
use super::key_lists::ListSource;
use super::key_loops::Element;
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::HashMap;

/// `@Input` member id (or the field and getter an input setter fills) -> the elements of the lists every parent binds to it.
pub(super) fn input_lists(store: &Store<'_>, lists: &HashMap<String, Vec<Element>>, src: &ListSource<'_, '_>, branches: &Branches) -> HashMap<String, Vec<Element>> {
    let written = written_members(store);
    let members = store.table("members");
    let assignments = store.table("assignments");
    // field id -> how many writes it has, and setter id -> the fields it copies its parameter into
    let mut writes: HashMap<String, usize> = HashMap::new();
    let mut copies: HashMap<String, Vec<String>> = HashMap::new();
    for a in assignments.iter() {
        let Some(target) = id_of(a, "target_id") else { continue };
        *writes.entry(target.clone()).or_default() += 1;
        let Some(member) = id_of(a, "member") else { continue };
        let param = src.params.get(&member).and_then(|p| p.first());
        let bare = a.get("value").and_then(|v| v.get("$expr")).and_then(Value::as_str);
        if param.is_some() && bare == param.map(String::as_str) && text(a, "operator").as_deref() == Some("=") && text(a, "scope").as_deref() == Some("this") {
            copies.entry(member).or_default().push(target);
        }
    }
    // class -> (input alias -> the member ids the bound list lands in), for the inputs nothing initialises or writes
    let mut inputs: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    for m in members.iter() {
        let (Some(id), Some(class)) = (id_of(m, "id"), text(m, "class")) else { continue };
        let Some(alias) = input_alias(m) else { continue };
        if text(m, "kind").as_deref() == Some("setter") {
            let Some([field]) = copies.get(&id).map(Vec::as_slice) else { continue };
            if lists.contains_key(field) || writes.get(field) != Some(&1) {
                continue;
            }
            let name = text(m, "name");
            let getter = members.iter().find(|g| text(g, "kind").as_deref() == Some("getter") && text(g, "class").as_ref() == Some(&class) && text(g, "name") == name);
            let lands = std::iter::once(field.clone()).chain(getter.and_then(|g| id_of(g, "id"))).collect();
            inputs.entry(class).or_default().insert(alias, lands);
            continue;
        }
        if lists.contains_key(&id) || written.contains(&id) || m.get("value").is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty())) {
            continue;
        }
        inputs.entry(class).or_default().insert(alias, vec![id]);
    }
    if inputs.is_empty() {
        return HashMap::new();
    }
    let expressions = store.table("expressions");
    // BY ROW, the tree read only for an INPUT's binding - the last row of the binding that has one. A map of every
    // binding to its tree decoded every template expression in the store.
    let mut rows_of: HashMap<String, Vec<&Row>> = HashMap::new();
    for e in expressions.iter() {
        if let Some(binding) = text(e, "binding") {
            rows_of.entry(binding).or_default().push(e);
        }
    }
    let tree_of = |id: &str| rows_of.get(id)?.iter().rev().find_map(|e| e.get("ast").filter(|a| !a.is_null()));
    let mut bound: HashMap<(String, String), Vec<&Value>> = HashMap::new();
    for b in store.table("bindings").iter().filter(|b| text(b, "kind").as_deref() == Some("input")) {
        if let (Some(id), Some(node), Some(name)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) {
            bound.entry((node, name)).or_default().extend(tree_of(&id));
        }
    }
    let paths = path_lists(store, branches);
    // landing ids -> every element a render edge's parent binds, tied to that edge
    let mut out: HashMap<String, Vec<Element>> = HashMap::new();
    for r in store.table("renders").iter() {
        let (Some(class), Some(node), Some(edge)) = (text(r, "to_class"), text(r, "node"), id_of(r, "id")) else { continue };
        for (alias, lands) in inputs.get(&class).into_iter().flatten() {
            let Some([tree]) = bound.get(&(node.clone(), alias.clone())).map(Vec::as_slice) else { continue };
            let Some((_, items)) = bound_list(unwrap(tree), lists, src, &paths) else { continue };
            let items: Vec<Element> = items.into_iter().map(|e| e.through_edge(&edge)).collect();
            for id in lands {
                out.entry(id.clone()).or_default().extend(items.iter().cloned());
            }
        }
    }
    out
}

/// The list a bound expression is, and the row it comes from: a bare read of a field with a known list, or of a getter -
/// or a call of a method - whose every return is a list.
pub(super) fn bound_list(tree: &Value, lists: &HashMap<String, Vec<Element>>, src: &ListSource<'_, '_>, paths: &Paths) -> Option<(String, Vec<Element>)> {
    // `cfg.list`: a property of one of the class's own fields, no declaration of its own (`path_lists`).
    let holder = tree.get("receiver").filter(|r| is_read(r) && r.get("receiver").and_then(|i| i.get("k")).and_then(Value::as_str) == Some("Implicit"));
    if let (true, Some(row), Some(name)) = (is_read(tree), holder.and_then(|r| r.get("target")?.get("row")?.as_str()), tree.get("name").and_then(Value::as_str)) {
        if let Some(found) = paths.get(&(row.to_string(), name.to_string())) {
            return found.clone().map(|items| (format!("{row}.{name}"), items));
        }
    }
    let row = if is_read(tree) {
        read_rows(tree).into_iter().next()?
    } else if matches!(tree.get("k").and_then(Value::as_str), Some("Call" | "SafeCall")) {
        let receiver = tree.get("receiver").filter(|r| is_read(r))?;
        receiver.get("target")?.get("row")?.as_str()?.to_string()
    } else {
        return None;
    };
    if let Some(items) = lists.get(&row) {
        return Some((row, items.clone()));
    }
    let items = src.member_lists(&row)?.into_iter().map(|v| Element::returned(v, &row)).collect();
    Some((row, items))
}

/// (field id, property) -> the elements of the list that property holds, None once a write is no list.
pub(super) type Paths = HashMap<(String, String), Option<Vec<Element>>>;

/// A LIST HELD IN A PROPERTY OF A FIELD (`cfg = { list: [..] }`, `this.cfg.list = [..]`): the initializer's array and every
/// plain `=` of an array the field's own class writes to it (`holder`), each under its write's branches. A write of
/// anything else leaves the property unread - a list read in part would hand the child fewer elements than it can hold.
pub(super) fn path_lists(store: &Store<'_>, branches: &Branches) -> Paths {
    let mut out = Paths::new();
    let members = store.table("members");
    let mut by_name: HashMap<(String, String), String> = HashMap::new();
    for m in members.iter() {
        let (Some(id), Some(class), Some(name)) = (id_of(m, "id"), text(m, "class"), text(m, "name")) else { continue };
        if let Some(object) = m.get("value").and_then(Value::as_object).filter(|o| o.keys().all(|k| !k.starts_with('$'))) {
            for (k, v) in object {
                let items = v.as_array().map(|xs| xs.iter().map(|x| Element::written(x.clone(), Vec::new())).collect());
                out.insert((id.clone(), k.clone()), items);
            }
        }
        by_name.insert((class, name), id);
    }
    // A FIELD WRITTEN WHOLE (`this.cfg = { list: [..] }`) hands each property its own array; written as anything but an object
    // literal, every property of it is unread.
    let fields: std::collections::HashSet<String> = out.keys().map(|(f, _)| f.clone()).collect();
    for a in store.table("assignments").iter().filter(|a| text(a, "scope").as_deref() == Some("this")) {
        let Some(field) = id_of(a, "target_id").filter(|f| fields.contains(f)) else { continue };
        let object = a.get("value").and_then(Value::as_object).filter(|o| o.keys().all(|k| !k.starts_with('$')) && text(a, "operator").as_deref() == Some("="));
        for ((_, prop), slot) in out.iter_mut().filter(|((f, _), _)| *f == field) {
            match (slot.as_mut(), object.map(|o| o.get(prop).and_then(Value::as_array))) {
                (Some(items), Some(Some(xs))) => items.extend(xs.iter().map(|x| Element::written(x.clone(), branches.chain_of(a)))),
                _ => *slot = None,
            }
        }
    }
    for a in store.table("assignments").iter().filter(|a| text(a, "scope").as_deref() == Some("property")) {
        let (Some(class), Some(holder), Some(name)) = (text(a, "class"), text(a, "holder"), text(a, "target")) else { continue };
        let Some(field) = by_name.get(&(class, holder)) else { continue };
        let list = a.get("value").and_then(Value::as_array).filter(|_| text(a, "operator").as_deref() == Some("="));
        let slot = out.entry((field.clone(), name)).or_insert_with(|| Some(Vec::new()));
        match (slot.as_mut(), list) {
            (Some(items), Some(xs)) => items.extend(xs.iter().map(|x| Element::written(x.clone(), branches.chain_of(a)))),
            _ => *slot = None,
        }
    }
    out
}
