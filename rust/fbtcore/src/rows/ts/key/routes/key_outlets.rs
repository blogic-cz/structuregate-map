//! A KEY INSIDE AN `<ng-template #ref>` RENDERS WHERE ITS OUTLETS RENDER IT (#51, divergence #36).
//!
//! The template's nodes sit at the top of the template tree, so their `gate_chain` holds none of the
//! conditions an outlet stands under: `<ng-container *ngIf="c" [ngTemplateOutlet]="ref">` rendered a key no
//! gate restricted. When EVERY read of `ref` in its template is an outlet binding - `[ngTemplateOutlet]`,
//! `*ngTemplateOutlet`, `*ngIf="..; then ref"` - each outlet's chain is a WAY in to the contents. A ref read
//! anywhere else (an `else`, which renders under the NEGATION a gate list cannot hold; an event handler; a
//! child's input; a `@ViewChild`/`@ContentChild` query) renders where the map cannot follow, so its contents
//! keep their own chain, as before. Every fact read here is a row node already writes.

use super::store::{Row, Store};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The inputs of Angular's own `NgTemplateOutlet` and `NgIf` that RENDER a template handed to them.
const OUTLET_INPUTS: [&str; 2] = ["ngTemplateOutlet", "ngIfThen"];
/// The gate rows an outlet node carries for HANDING a template over: no condition, so no part of a way.
const HANDOVER: [&str; 5] = ["ngTemplateOutlet", "ngTemplateOutletContext", "ngTemplateOutletInjector", "ngIfThen", "ngIfElse"];
/// The decorators that hand a template to the class - by its ref name, or by type (then every template of it).
const QUERIES: [&str; 4] = ["ViewChild", "ViewChildren", "ContentChild", "ContentChildren"];

fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) { Some(Value::String(s)) if !s.is_empty() => Some(s.clone()), _ => None }
}
fn strings(row: &Row, field: &str) -> Vec<String> {
    match row.get(field) {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}
/// `references` / `variables`: `[{name, value}]`.
fn named(row: &Row, field: &str) -> Vec<(String, String)> {
    match row.get(field) {
        Some(Value::Array(items)) => items.iter().filter_map(|v| Some((
            v.get("name")?.as_str()?.to_string(),
            v.get("value").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        ))).collect(),
        _ => Vec::new(),
    }
}
/// The expression is the bare read of `name` and nothing else - `cond ? a : b` is no outlet of either.
fn bare_read(ast: Option<&Value>, name: &str) -> bool {
    let Some(ast) = ast else { return false };
    let inner = ast.get("ast").unwrap_or(ast);
    inner.get("k").and_then(|k| k.as_str()) == Some("Read")
        && inner.get("name").and_then(|n| n.as_str()) == Some(name)
        && inner.get("receiver").and_then(|r| r.get("k")).and_then(|k| k.as_str()) == Some("Implicit")
}

enum Under { No, Cycle, Ways(Vec<Vec<String>>) }

#[derive(Default)]
pub struct Outlets {
    parent: HashMap<String, String>,
    chain: HashMap<String, Vec<String>>,
    /// An outlet-only `ng-template` node -> the nodes whose binding renders it, in row order.
    outlets: HashMap<String, Vec<String>>,
    /// Node -> the hand-over gate ids it carries.
    handover: HashMap<String, HashSet<String>>,
}

impl Outlets {
    pub fn new(store: &Store<'_>) -> Outlets {
        let mut o = Outlets::default();
        let mut declared: HashMap<(String, String), usize> = HashMap::new();
        let mut candidate: HashMap<(String, String), String> = HashMap::new();
        for n in store.table("template_nodes").iter() {
            let (Some(id), Some(tpl)) = (text(n, "id"), text(n, "template")) else { continue };
            if let Some(p) = text(n, "parent") { o.parent.insert(id.clone(), p); }
            let chain = strings(n, "gate_chain");
            if !chain.is_empty() { o.chain.insert(id.clone(), chain); }
            let is_tpl = text(n, "kind").as_deref() == Some("Template") && text(n, "tag").as_deref() == Some("ng-template");
            for (name, _) in named(n, "variables") { *declared.entry((tpl.clone(), name)).or_default() += 1; }
            for (name, value) in named(n, "references") {
                *declared.entry((tpl.clone(), name.clone())).or_default() += 1;
                if is_tpl && value.is_empty() { candidate.insert((tpl.clone(), name), id.clone()); }
            }
        }
        // A NAME DECLARED TWICE IN ONE TEMPLATE (two embedded views, a `let-` of the same name) names no one template.
        candidate.retain(|k, _| declared.get(k) == Some(&1));
        if candidate.is_empty() { return o; }
        let class_of: HashMap<String, String> = store.table("templates").iter()
            .filter_map(|t| Some((text(t, "id")?, text(t, "class")?))).collect();
        let mut queried: HashSet<(String, String)> = HashSet::new();
        let mut by_type: HashSet<String> = HashSet::new();
        for m in store.table("members").iter() {
            let Some(class) = text(m, "class") else { continue };
            for d in m.get("decorators").and_then(|d| d.as_array()).into_iter().flatten() {
                if !d.get("name").and_then(|x| x.as_str()).is_some_and(|x| QUERIES.contains(&x)) { continue; }
                match d.get("args").and_then(|a| a.get(0)).and_then(|a| a.as_str()) {
                    Some(name) => { queried.insert((class.clone(), name.to_string())); }
                    None => { by_type.insert(class.clone()); }
                }
            }
        }
        let mut refused: HashSet<(String, String)> = candidate.keys()
            .filter(|(tpl, name)| class_of.get(tpl).is_some_and(|c| by_type.contains(c) || queried.contains(&(c.clone(), name.clone()))))
            .cloned().collect();
        let mut found: HashMap<(String, String), Vec<String>> = HashMap::new();
        for e in store.table("expressions").iter() {
            let Some(tpl) = text(e, "template") else { continue };
            for name in strings(e, "identifiers") {
                let k = (tpl.clone(), name);
                if !candidate.contains_key(&k) || refused.contains(&k) { continue; }
                let outlet = matches!(text(e, "kind").as_deref(), Some("input" | "template_attr"))
                    && text(e, "name").is_some_and(|n| OUTLET_INPUTS.contains(&n.as_str()))
                    && bare_read(e.get("ast"), &k.1);
                match text(e, "node").filter(|_| outlet) {
                    Some(node) => found.entry(k).or_default().push(node),
                    None => { refused.insert(k); }
                }
            }
        }
        for (k, nodes) in found {
            if !refused.contains(&k) && let Some(t) = candidate.get(&k) { o.outlets.insert(t.clone(), nodes); }
        }
        for g in store.table("gates").iter() {
            if let (Some(id), Some(node), Some(name)) = (text(g, "id"), text(g, "node"), text(g, "name"))
                && HANDOVER.contains(&name.as_str())
            {
                o.handover.entry(node).or_default().insert(id);
            }
        }
        o
    }

    /// None: `node` sits under no outlet-only `ng-template`, and `gates` (its own chain) stands as before.
    /// Some: one full gate list per way an outlet renders it on.
    pub fn rewrite(&self, node: &str, gates: &[String]) -> Option<Vec<Vec<String>>> {
        match self.under(node, gates, &mut Vec::new()) { Under::Ways(w) => Some(w), Under::No | Under::Cycle => None }
    }

    fn under(&self, node: &str, gates: &[String], stack: &mut Vec<String>) -> Under {
        if self.outlets.is_empty() { return Under::No; }
        let mut cur = Some(node.to_string());
        let mut seen = HashSet::new();
        let tpl = loop {
            let Some(id) = cur else { return Under::No };
            if !seen.insert(id.clone()) { return Under::No; }
            if self.outlets.contains_key(&id) { break id; }
            cur = self.parent.get(&id).cloned();
        };
        // AN OUTLET INSIDE THE TEMPLATE IT RENDERS (a recursive tree) is no way: it only adds gates to an outer one.
        if stack.contains(&tpl) { return Under::Cycle; }
        let empty = Vec::new();
        let above = self.parent.get(&tpl).and_then(|p| self.chain.get(p)).unwrap_or(&empty);
        let own = gates.strip_prefix(above.as_slice()).unwrap_or(gates);
        stack.push(tpl.clone());
        let mut ways: Vec<Vec<String>> = Vec::new();
        for o in &self.outlets[&tpl] {
            let chain = self.chain.get(o).cloned().unwrap_or_default();
            let bases = match self.under(o, &chain, stack) { Under::No => vec![chain], Under::Cycle => continue, Under::Ways(w) => w };
            let drop = self.handover.get(o);
            for base in bases {
                let mut way: Vec<String> = base.into_iter().filter(|g| !drop.is_some_and(|d| d.contains(g))).collect();
                for g in own { if !way.contains(g) { way.push(g.clone()); } }
                if !ways.contains(&way) { ways.push(way); }
            }
        }
        stack.pop();
        if ways.is_empty() { Under::Cycle } else { Under::Ways(ways) }
    }
}
