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
//!
//! A CONTROL is also an input `x` beside an output `xChange` of ANY handler (`[value]="x" (valueChange)="onX($event)"`): it
//! shows a value and reports its change, so it is labelled like a two-way one, though `control_options` does not count it.
//!
//! A LABEL BOUND TO THE COMPONENT'S OWN `@Input` (`[title]="rowTitle"`) that places no key is read at every HOST: the binding
//! of that input on each element the component is rendered at, and up again while that is a bare read of the host's own
//! input (a visited set over bindings). Each key a host writes is a row; `key_binding` is the binding that placed it and
//! `key_host` its element (the label binding itself and NULL when the label places the key).

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
    /// The template's shape (`rowread::TemplateTree`), shared with the other passes.
    nodes: std::rc::Rc<crate::rows::ts::rowread::TemplateTree>,
    labels: HashMap<String, Vec<Label>>,
    hosts: HashMap<String, Vec<String>>,
    /// Node -> its input and attribute bindings (binding id, name, text, attribute?).
    bound: HashMap<String, Vec<(String, String, String, bool)>>,
    /// (component, member) -> the name an `@Input` member is bound under.
    inputs: HashMap<(String, String), String>,
}

impl Tree {
    fn new(store: &Store<'_>, names: &[String]) -> Tree {
        let nodes = crate::rows::ts::rowread::TemplateTree::of(store);
        let mut labels: HashMap<String, Vec<Label>> = HashMap::new();
        let mut bound: HashMap<String, Vec<(String, String, String, bool)>> = HashMap::new();
        for b in store.table("bindings").iter() {
            let (Some(id), Some(node), Some(name)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) else { continue };
            let kind = text(b, "kind").unwrap_or_default();
            if !(kind == "input" || kind == "attribute") {
                continue;
            }
            let attribute = kind == "attribute";
            let source = if attribute { text(b, "value") } else { text(b, "source") }.unwrap_or_default();
            bound.entry(node.clone()).or_default().push((id.clone(), name.clone(), source.clone(), attribute));
            if names.contains(&name) {
                labels.entry(node).or_default().push(Label { binding: id, input: name, source, attribute });
            }
        }
        // A COMPONENT'S `@Input`s, by the component id a template node names (`components.class` joins `io.class`).
        let class_of: HashMap<String, String> = store.table("components").iter()
            .filter_map(|c| Some((id_of(c, "class")?, id_of(c, "id")?))).collect();
        let mut inputs = HashMap::new();
        for io in store.table("io").iter().filter(|r| text(r, "kind").as_deref() == Some("input")) {
            let (Some(class), Some(member)) = (id_of(io, "class"), text(io, "member")) else { continue };
            let Some(component) = class_of.get(&class) else { continue };
            let name = text(io, "binding_name").unwrap_or_else(|| member.clone());
            inputs.insert((component.clone(), member), name);
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
        Tree { nodes, labels, hosts, bound, inputs }
    }

    /// The keys a label binding shows, with the binding that placed each: its own, else - when it is a bare read of its
    /// component's `@Input` - what each host binds that input to, up while a host's binding is one again.
    fn keys_of(&self, binding: &str, source: &str, attribute: bool, node: &str, k: &Keys<'_>) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, String, bool, String)> =
            VecDeque::from([(binding.to_string(), source.to_string(), attribute, node.to_string())]);
        while let Some((b, text, attribute, at)) = queue.pop_front() {
            if !seen.insert(b.clone()) {
                continue;
            }
            let placed = k.placed(&b, &text, attribute);
            if !placed.is_empty() {
                out.extend(placed.into_iter().map(|key| (key, b.clone())));
                continue;
            }
            // A BARE NAME of an `@Input` of the component this node is in: every host's binding of that input.
            if attribute || text.is_empty() || !text.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                continue;
            }
            let Some(component) = self.nodes.component.get(&at) else { continue };
            let Some(name) = self.inputs.get(&(component.clone(), text.clone())) else { continue };
            for h in self.hosts.get(component).into_iter().flatten() {
                for (hb, hname, htext, hattr) in self.bound.get(h).into_iter().flatten() {
                    if hname == name {
                        queue.push_back((hb.clone(), htext.clone(), *hattr, h.clone()));
                    }
                }
            }
        }
        out
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
                match self.nodes.parent.get(at).map(String::as_str) {
                    Some(p) => at = p,
                    // THE ROOT, WITH NO LABEL: up one render edge, from every host element of the component.
                    None => {
                        let Some(component) = self.nodes.component.get(at) else { break };
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

/// What the closure placed through each binding (`key_node_ways`), and the keys a locale defines.
struct Keys<'a> {
    by_binding: HashMap<String, Vec<String>>,
    defined: &'a HashSet<String>,
}

impl Keys<'_> {
    /// The keys one binding places: its `key_node_ways` rows, or a plain attribute's text a locale defines.
    fn placed(&self, binding: &str, text: &str, attribute: bool) -> Vec<String> {
        match self.by_binding.get(binding) {
            Some(keys) => keys.clone(),
            None if attribute && self.defined.contains(text) => vec![text.to_string()],
            None => Vec::new(),
        }
    }
}

/// THE ROWS, over the `key_node_ways` rows the closure just derived.
pub fn rows(store: &Store<'_>, ways: &[Row]) -> Vec<Row> {
    let controls = controls(store);
    if controls.is_empty() {
        return Vec::new();
    }
    // THE NAMES NODE RESOLVED (`TsSetup/TsSpec.mjs`: `labelInputs`, else `label` and `title`); none names no label.
    let names = store.option_list("label_inputs");
    if names.is_empty() {
        return Vec::new();
    }
    let tree = Tree::new(store, &names);
    let mut by_binding: HashMap<String, Vec<String>> = HashMap::new();
    for w in ways {
        if let (Some(b), Some(k)) = (text(w, "binding"), text(w, "key")) {
            let list = by_binding.entry(b).or_default();
            if !list.contains(&k) {
                list.push(k);
            }
        }
    }
    let defined: HashSet<String> = store.table("translations").iter().filter_map(|t| text(t, "key")).collect();
    let keys = Keys { by_binding, defined: &defined };
    let node_of: HashMap<&str, &str> = tree.bound.iter().flat_map(|(n, bs)| bs.iter().map(move |b| (b.0.as_str(), n.as_str()))).collect();
    let mut sorted: Vec<(&String, &String)> = controls.iter().collect();
    sorted.sort_by(|a, b| node_order(a.0).cmp(&node_order(b.0)).then(a.0.cmp(b.0)));
    let mut out = Vec::new();
    let mut written: HashSet<(String, String, Option<String>, String, String)> = HashSet::new();
    for (node, value_binding) in sorted {
        for found in tree.climb(node) {
            for label in tree.labels.get(found.node).into_iter().flatten() {
                let mut placed: Vec<(Option<String>, Option<String>)> = tree.keys_of(&label.binding, &label.source, label.attribute, found.node, &keys)
                    .into_iter().map(|(k, b)| (Some(k), Some(b))).collect();
                if placed.is_empty() {
                    placed.push((None, None));
                }
                for (key, key_binding) in placed {
                    let host = found.host.map(str::to_string);
                    let unique = (node.clone(), label.binding.clone(), host.clone(), key.clone().unwrap_or_default(), key_binding.clone().unwrap_or_default());
                    if !written.insert(unique) {
                        continue;
                    }
                    // THE HOST WHOSE BINDING PLACED THE KEY, when it is not the label's own.
                    let key_host = key_binding.as_deref().filter(|b| *b != label.binding).and_then(|b| node_of.get(b)).map(|n| n.to_string());
                    let mut row = Row::new();
                    let mut put = |k: &str, v: Value| row.insert(k.to_string(), v);
                    let opt = |s: Option<String>| s.map(Value::String).unwrap_or(Value::Null);
                    put("component", opt(tree.nodes.component.get(node).cloned()));
                    put("node", Value::String(node.clone()));
                    put("value_binding", Value::String(value_binding.clone()));
                    put("via", Value::String(found.via.to_string()));
                    put("label_node", Value::String(found.node.to_string()));
                    put("label_component", opt(tree.nodes.component.get(found.node).cloned()));
                    put("label_binding", Value::String(label.binding.clone()));
                    put("input", Value::String(label.input.clone()));
                    put("source", Value::String(label.source.clone()));
                    put("key", opt(key));
                    put("key_binding", opt(key_binding));
                    put("key_host", opt(key_host));
                    put("host_node", opt(host));
                    put("up", Value::from(found.up as i64));
                    out.push(row);
                }
            }
        }
    }
    out
}

/// Node id -> its value binding: what `key_options::value_bindings` calls a control, and an input `x` beside an output
/// `xChange` of any handler - one-way, yet a control that shows a value and reports its change.
fn controls(store: &Store<'_>) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = super::key_options::value_bindings(store).iter().map(|(n, (b, _))| (n.clone(), b.clone())).collect();
    let bindings = store.table("bindings");
    let outputs: HashSet<(String, String)> = bindings.iter().filter(|b| text(b, "kind").as_deref() == Some("output"))
        .filter_map(|b| Some((text(b, "node")?, text(b, "name")?))).collect();
    for b in bindings.iter().filter(|b| text(b, "kind").as_deref() == Some("input")) {
        let (Some(id), Some(node), Some(name)) = (id_of(b, "id"), text(b, "node"), text(b, "name")) else { continue };
        if outputs.contains(&(node.clone(), format!("{name}Change"))) {
            out.entry(node).or_insert(id);
        }
    }
    out
}

/// A node id's number (`n:12` is 12), so rows come out in the order the nodes were made.
fn node_order(id: &str) -> u64 {
    id.rsplit(':').next().and_then(|n| n.parse().ok()).unwrap_or(u64::MAX)
}
