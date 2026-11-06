//! WHICH LABEL A FORM CONTROL IS SHOWN UNDER (#155): `control_labels`, one row per (control, label it finds, key).
//!
//! A control is what `control_options` calls one: a node binding a value (`key_options::value_bindings`). Its label is a
//! binding named in `labelInputs` (`structuregate.ts.json`, default `label` and `title`; an `[x]` input or a plain `x="..."`
//! attribute), looked for in this order and the first place that has one wins:
//!
//! - `own`  - the control's own node (`<checkbox [label]="'k'">`);
//! - `wrap` - the nearest ANCESTOR in the same template (`<form-row [title]="'k'"><text-input>`); a sibling's label is never
//!   one - the nearest element holding a label in the tree is how a hint got filed under the wrong field;
//! - `host` - nothing up to the template's root: then the HOST ELEMENT of every place the component is rendered
//!   (`renders.node`), and up from there in the host's template, one render edge at a time until a label is found. Every
//!   host is its own row, so a component rendered under two titles has two.
//!
//! The climb goes up through render edges only while it finds nothing, and each host element is climbed from once (a
//! visited set; a component rendered inside itself ends there). The KEY is what `key_node_ways` places through the
//! label binding (a literal, a field a key lands in, a pipe), or a plain attribute's text when a locale defines it; a label
//! the closure places no key through is still a row, `key` NULL, its `source` the binding's text.

use super::key_fields::{id_of, text};
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

/// One label binding: its id, the input it is bound to, and its text.
struct Label {
    binding: String,
    input: String,
    source: String,
    attribute: bool,
}

/// Where a climb found a label: the node holding it, how it was reached, and the last host element on the way.
struct Found<'a> {
    node: &'a str,
    via: &'static str,
    host: Option<&'a str>,
    up: usize,
}

/// The template facts the climb reads: each node's parent and component, the label bindings on each node, and the host
/// elements each component is rendered at.
struct Tree {
    parent: HashMap<String, Option<String>>,
    component: HashMap<String, String>,
    labels: HashMap<String, Vec<Label>>,
    hosts: HashMap<String, Vec<String>>,
}

impl Tree {
    fn new(store: &Store<'_>, names: &[String]) -> Tree {
        let mut parent = HashMap::new();
        let mut component = HashMap::new();
        for n in store.table("template_nodes").iter() {
            let Some(id) = id_of(n, "id") else { continue };
            if let Some(c) = text(n, "component") {
                component.insert(id.clone(), c);
            }
            parent.insert(id, id_of(n, "parent"));
        }
        let mut labels: HashMap<String, Vec<Label>> = HashMap::new();
        for b in store.table("bindings").iter() {
            let (Some(id), Some(node), Some(name)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) else { continue };
            let kind = text(b, "kind").unwrap_or_default();
            if !(kind == "input" || kind == "attribute") || !names.contains(&name) {
                continue;
            }
            let attribute = kind == "attribute";
            let source = if attribute { text(b, "value") } else { text(b, "source") }.unwrap_or_default();
            labels.entry(node).or_default().push(Label { binding: id, input: name, source, attribute });
        }
        let mut hosts: HashMap<String, Vec<String>> = HashMap::new();
        for r in store.table("renders").iter() {
            if let (Some(to), Some(node)) = (text(r, "to"), id_of(r, "node")) {
                let list = hosts.entry(to).or_default();
                if !list.contains(&node) {
                    list.push(node);
                }
            }
        }
        Tree { parent, component, labels, hosts }
    }

    /// Every label the control `node` is shown under: its own, else the nearest ancestor's, else each host's.
    fn climb<'a>(&'a self, node: &'a str) -> Vec<Found<'a>> {
        let mut out = Vec::new();
        // A HOST ELEMENT IS CLIMBED FROM ONCE: inside one template the parents end at its root, so only a render edge can
        // come back round (a component rendered inside itself).
        let mut queued: HashSet<&str> = HashSet::new();
        // (node to start from, how it was reached, the host element it came through, render edges climbed), in order.
        let mut queue: VecDeque<(&str, &'static str, Option<&str>, usize)> = VecDeque::from([(node, "own", None, 0)]);
        while let Some((start, via, host, up)) = queue.pop_front() {
            let mut at = start;
            loop {
                if self.labels.contains_key(at) {
                    let how = if at == start { via } else if up == 0 { "wrap" } else { "host" };
                    out.push(Found { node: at, via: how, host, up });
                    break;
                }
                match self.parent.get(at).and_then(|p| p.as_deref()) {
                    Some(p) => at = p,
                    // THE ROOT, WITH NO LABEL: up one render edge, from every host element of the component.
                    None => {
                        let Some(component) = self.component.get(at) else { break };
                        for h in self.hosts.get(component).into_iter().flatten() {
                            if queued.insert(h.as_str()) {
                                queue.push_back((h.as_str(), "host", Some(h.as_str()), up + 1));
                            }
                        }
                        break;
                    }
                }
            }
        }
        out
    }
}

/// THE ROWS, over the `key_node_ways` rows the closure just derived.
pub fn rows(store: &Store<'_>, ways: &[Row]) -> Vec<Row> {
    let controls = super::key_options::value_bindings(store);
    if controls.is_empty() {
        return Vec::new();
    }
    // THE NAMES NODE RESOLVED (`TsSetup/TsSpec.mjs`: `labelInputs`, else `label` and `title`); none names no label.
    let names = store.option_list("label_inputs");
    if names.is_empty() {
        return Vec::new();
    }
    let tree = Tree::new(store, &names);
    let mut keys_of: HashMap<String, Vec<String>> = HashMap::new();
    for w in ways {
        if let (Some(b), Some(k)) = (text(w, "binding"), text(w, "key")) {
            let list = keys_of.entry(b).or_default();
            if !list.contains(&k) {
                list.push(k);
            }
        }
    }
    let defined: HashSet<String> = store.table("translations").iter().filter_map(|t| text(t, "key")).collect();
    let mut sorted: Vec<(&String, &(String, String))> = controls.iter().collect();
    sorted.sort_by(|a, b| node_order(a.0).cmp(&node_order(b.0)).then(a.0.cmp(b.0)));
    let mut out = Vec::new();
    let mut written: HashSet<(String, String, Option<String>, String)> = HashSet::new();
    for (node, (value_binding, _)) in sorted {
        for found in tree.climb(node) {
            for label in tree.labels.get(found.node).into_iter().flatten() {
                let mut keys: Vec<Option<String>> = keys_of.get(&label.binding).into_iter().flatten().cloned().map(Some).collect();
                if keys.is_empty() && label.attribute && defined.contains(&label.source) {
                    keys.push(Some(label.source.clone()));
                }
                if keys.is_empty() {
                    keys.push(None);
                }
                for key in keys {
                    let host = found.host.map(str::to_string);
                    if !written.insert((node.clone(), label.binding.clone(), host.clone(), key.clone().unwrap_or_default())) {
                        continue;
                    }
                    let mut row = Row::new();
                    let mut put = |k: &str, v: Value| row.insert(k.to_string(), v);
                    let opt = |s: Option<String>| s.map(Value::String).unwrap_or(Value::Null);
                    put("component", opt(tree.component.get(node).cloned()));
                    put("node", Value::String(node.clone()));
                    put("value_binding", Value::String(value_binding.clone()));
                    put("via", Value::String(found.via.to_string()));
                    put("label_node", Value::String(found.node.to_string()));
                    put("label_component", opt(tree.component.get(found.node).cloned()));
                    put("label_binding", Value::String(label.binding.clone()));
                    put("input", Value::String(label.input.clone()));
                    put("source", Value::String(label.source.clone()));
                    put("key", opt(key));
                    put("host_node", opt(host));
                    put("up", Value::from(found.up as i64));
                    out.push(row);
                }
            }
        }
    }
    out
}

/// A node id's number (`n:12` is 12), so rows come out in the order the nodes were made.
fn node_order(id: &str) -> u64 {
    id.rsplit(':').next().and_then(|n| n.parse().ok()).unwrap_or(u64::MAX)
}
