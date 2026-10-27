//! WHICH OPTION LABELS A FORM CONTROL OFFERS (#117): `control_options`, one row per (control, input, property, key).
//!
//! A select or a radio list is handed its options through an input beside its value binding -
//! `<radio-list [items]="hues" [(value)]="model.hue">` - and every hop of how the labels got there is a row of its own: the
//! const table, the helper that rebuilds it into items, the field the result lands in, the binding. This joins them: the
//! node is a CONTROL when it binds a value - a two-way `[(x)]` (an input `x` and an output `xChange` over the same
//! expression), or `ngModel`, `formControl`, `formControlName` in any form - and each other input bound there whose
//! expression is a LIST this closure can read (`key_inputs::bound_list`: a field's list, a getter's or a method's returns,
//! a property of a field) offers every key an element's property holds (`key_loops::element_keys`) - a literal, a translate
//! call, a call read as its callee returns, a const table a helper rebuilt (`key_lists::ListSource::table_lists`). A list whose
//! elements hold a function (validation rules, #133) offers nothing.
//!
//! THE ROUTE IS IDS: `input_binding` and `value_binding` are `bindings` rows, `holder` the member (or the function) the
//! list is read from, `callee` the member or function whose return built its elements when every one came from the same.
//! A child component's own `*ngFor` over the list, if the tree has one, is still where `key_reach` places the key; this
//! table says what the control is OFFERED, whether its template is in the tree or a library's.

use super::key_branches::{Branches, KeySet};
use super::key_fields::{id_of, text};
use super::key_lists::{with_reader, ListSource};
use super::key_loops::{element_keys, Element};
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::HashMap;

/// The names a control's value is bound under in any form, besides a two-way `[(x)]`.
const VALUE_NAMES: &[&str] = &["ngModel", "formControl", "formControlName"];

/// One list a control is handed: the control's node, the input and its binding, the value binding beside it, the row the
/// list is read from, and its elements.
pub struct Offered {
    pub node: String,
    pub component: Option<String>,
    pub input: String,
    pub input_binding: String,
    pub value: String,
    pub value_binding: String,
    pub holder: String,
    pub items: Vec<Element>,
}

/// Binding id -> its expression's tree: the last row of a binding that has one, as `key_inputs` reads an input's.
fn binding_trees<'s>(expressions: &'s [Row]) -> HashMap<String, &'s Value> {
    let mut trees = HashMap::new();
    for e in expressions {
        // THE BINDING FIRST: a tree is decoded when it is read and then kept, and asking every row for its tree decoded the
        // whole table's - most of them the trees of no binding.
        let Some(binding) = text(e, "binding") else { continue };
        if let Some(ast) = e.get("ast").filter(|a| !a.is_null()) {
            trees.insert(binding, ast);
        }
    }
    trees
}

/// AN OUTPUT THAT STORES `$event` IN A PLACE, as the read of that place (#118): `[(value)]="m.x"` is compiled by one Angular
/// as the output `m.x = $event` (a `Write`) and by a later one as `m.x` itself - so the pair is the same place, never the
/// same text. A plain `=` of `$event` alone; anything else is None.
fn stored_place(tree: &Value) -> Option<Value> {
    let o = tree.as_object()?;
    let read = match o.get("k")?.as_str()? {
        "Write" => "Read",
        "KeyedWrite" => "KeyedRead",
        _ => return None,
    };
    let value = o.get("value")?;
    let event = value.get("k").and_then(Value::as_str) == Some("Read") && value.get("name").and_then(Value::as_str) == Some("$event")
        && value.get("receiver").and_then(|r| r.get("k")).and_then(Value::as_str) == Some("Implicit");
    if !event || o.contains_key("op") {
        return None;
    }
    let mut place = o.clone();
    place.remove("value");
    place.insert("k".to_string(), Value::String(read.to_string()));
    Some(Value::Object(place))
}

/// Node id -> its value binding (binding id, its source): the input of a two-way pair, or a binding `VALUE_NAMES` names.
pub(super) fn value_bindings(store: &Store<'_>) -> HashMap<String, (String, String)> {
    let bindings = store.table("bindings");
    let expressions = store.table("expressions");
    let trees = binding_trees(&expressions);
    let tree = |id: &str| trees.get(id).map(|t| super::astreads::unwrap(t));
    let mut outputs: HashMap<(String, String), (String, Option<Value>)> = HashMap::new();
    for b in bindings.iter().filter(|b| text(b, "kind").as_deref() == Some("output")) {
        if let (Some(id), Some(node), Some(name)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) {
            outputs.insert((node, name), (text(b, "source").unwrap_or_default(), tree(&id).and_then(stored_place)));
        }
    }
    let mut out: HashMap<String, (String, String)> = HashMap::new();
    for b in bindings.iter() {
        let (Some(id), Some(node), Some(name)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) else { continue };
        // A PLAIN ATTRIBUTE (`formControlName="tier"`) has no expression: its text is its `value`.
        let source = text(b, "source").filter(|t| !t.is_empty()).or_else(|| text(b, "value")).unwrap_or_default();
        let paired = outputs.get(&(node.clone(), format!("{name}Change"))).is_some_and(|(text, place)| {
            *text == source || place.as_ref().is_some_and(|p| tree(&id) == Some(p))
        });
        let two_way = text(b, "kind").as_deref() == Some("input") && paired;
        if two_way || VALUE_NAMES.contains(&name.as_str()) {
            out.entry(node).or_insert((id, source));
        }
    }
    out
}

/// Every list bound to an input of a control, beside its value binding.
pub(super) fn offered(store: &Store<'_>, controls: &HashMap<String, (String, String)>, lists: &HashMap<String, Vec<Element>>,
    src: &ListSource<'_, '_>, branches: &Branches) -> Vec<Offered> {
    if controls.is_empty() {
        return Vec::new();
    }
    let expressions = store.table("expressions");
    let trees = binding_trees(&expressions);
    let paths = super::key_inputs::path_lists(store, branches, src);
    let mut out = Vec::new();
    for b in store.table("bindings").iter().filter(|b| text(b, "kind").as_deref() == Some("input")) {
        let (Some(id), Some(node), Some(input)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) else { continue };
        let Some((value_binding, value)) = controls.get(&node) else { continue };
        if *value_binding == id {
            continue;
        }
        let Some(tree) = trees.get(&id) else { continue };
        let Some((holder, items)) = super::key_inputs::bound_list(super::astreads::unwrap(tree), lists, src, &paths) else { continue };
        // A LIST OF RULES IS NO LIST OF CHOICES (#133): an element holding a function (`{ Message: 'k', Validate: (v) => .. }`) is something the
        // control runs, not a value it offers - a choice is data.
        if items.is_empty() || items.iter().any(|e| holds_function(&e.value)) {
            continue;
        }
        out.push(Offered {
            node, component: text(b, "component"), input, input_binding: id, value: value.clone(), value_binding: value_binding.clone(), holder, items,
        });
    }
    out
}

/// Whether an element is an object one property of which is a function (an arrow, a function expression: `$fn`).
fn holds_function(element: &Value) -> bool {
    element.as_object().is_some_and(|o| o.values().any(|v| v.get("$fn").is_some()))
}

/// THE ROWS: one per (control, input, property, key) - the keys an element's property holds, in the order the elements and
/// their properties are written.
pub fn rows(store: &Store<'_>) -> Vec<Row> {
    let loops = store.shared("loops", super::key_loops::Loops::new);
    if loops.options.is_empty() {
        return Vec::new();
    }
    let i18n = store.option_list("i18n_calls");
    let keys: KeySet = store.table("translations").iter().filter_map(|t| text(t, "key")).collect();
    let per_list = with_reader(store, |reader| loops.options.iter().map(|o| element_keys(&o.items, &keys, &i18n, reader)).collect::<Vec<_>>());
    let mut out = Vec::new();
    for (o, elements) in loops.options.iter().zip(per_list) {
        // ONE CALLEE when every element came out of the same return; elements written in place, or from two, name none.
        let owners: Vec<Option<&str>> = o.items.iter().map(Element::owner).collect();
        let callee = owners.first().copied().flatten().filter(|first| owners.iter().all(|w| *w == Some(*first)));
        for (property, found) in &elements {
            for key in found.keys() {
                let mut row = Row::new();
                let mut put = |k: &str, v: Value| row.insert(k.to_string(), v);
                put("component", o.component.clone().map(Value::String).unwrap_or(Value::Null));
                put("node", Value::String(o.node.clone()));
                put("value", Value::String(o.value.clone()));
                put("value_binding", Value::String(o.value_binding.clone()));
                put("input", Value::String(o.input.clone()));
                put("input_binding", Value::String(o.input_binding.clone()));
                put("holder", Value::String(o.holder.clone()));
                put("callee", callee.map(|c| Value::String(c.to_string())).unwrap_or(Value::Null));
                put("property", Value::String(property.clone()));
                put("key", Value::String(key.clone()));
                out.push(row);
            }
        }
    }
    out
}
