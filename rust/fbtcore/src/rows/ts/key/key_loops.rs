//! A KEY HELD IN A LIST OF OBJECTS IS A SITE WHERE `*ngFor`'S LOOP VARIABLE READS ITS PROPERTY.
//!
//! `tips = [{ tooltip: 'demo.tips.alpha' }]` (or `tips = this.labels.tips()` over a method whose
//! every return is such a list) rendered by `<b *ngFor="let t of tips" [title]="t.tooltip">`: the
//! key lives in the field, but the template never reads the field at the node that shows it - it reads
//! `t.tooltip`, and `t` is a variable of the `ngForOf` gate's node.
//!
//! THE KEY IS A SITE ONLY WHERE THE LOOP VARIABLE'S PROPERTY IS READ, never at the loop. Treating the
//! whole field's read as the site would give the key the `*ngFor` node's `gate_chain`, which misses
//! every condition INSIDE the loop (an `*ngIf` on a child, a `@if` around the read), and dropping a
//! real condition from `always_gates` is the failure this schema exists to prevent. A property no
//! template reads through the variable (`other` in `{ tooltip, other }`) is no site, and a bare const
//! list of objects with no `*ngFor` read stays a `literal`.
//!
//! EVERY HOP IS AN ID JOIN: the `ngForOf` gate's expression reads the field by `Read.target.row`, the
//! gate's `node` declares the variable (`variables` entry whose value is `$implicit`), and the sites
//! are that node and its descendants by `parent` - NOT a descendant that declares the same name, which
//! shadows it. The elements are what the member's own value spells, or - when nothing initialises it but an
//! empty list - the union of every plain `=` its own class writes, each element under the branches of its
//! write (`elements`), a callee's own branches not carried; a member any other class writes is dropped, like
//! `key_fields` drops a field written from outside.
//!
//! A KEY'S WAYS CARRY THE ELEMENT THEY CAME FROM, `loop:<gate>#<index>` for each gate of the site's chain
//! that reads the loop's variable:
//! a directive's config over the item is read per element (`Loops::expand`), and the link makes the key
//! take that element's value instead of the union.

use super::astreads::{is_read, read_rows, unwrap};
use super::gate_values::EnumIndex;
use super::key_branches::{value_keys, Branches, KeySet};
use super::key_follow::Returned;
use super::key_fields::{id_of, returns_by_member, text};
use super::key_lists::{callee_lists, with_reader};
use super::key_routes::FieldSite;
use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// property -> key -> (element index, way), per loop.
type Elements = IndexMap<String, IndexMap<String, Vec<(usize, Vec<String>)>>>;

fn element_keys(items: &[Element], keys: &KeySet, i18n: &[String], reader: &Returned<'_, '_>) -> Elements {
    let mut out: Elements = IndexMap::new();
    for (i, item) in items.iter().enumerate() {
        let Some(object) = item.value.as_object() else { continue };
        for (prop, value) in object {
            let mut found = Vec::new();
            value_keys(value, &item.chain, keys, &mut found);
            // A TRANSLATE CALL AS THE PROPERTY'S VALUE holds its arguments' keys, as a method's return of one does.
            for arg in super::key_follow::translated(value, i18n).unwrap_or_default() {
                value_keys(arg, &item.chain, keys, &mut found);
            }
            // A CALL AS THE PROPERTY'S VALUE inside a list a callee returns is read as that callee's own return of it is -
            // a local arrow of the callee, a free function, a method - with the string it is handed spelled into its hole
            // (`label('alpha')` over `(n) => `demo.${n}``). No condition is carried: the call's branches test its parameters.
            if let Some(owner) = &item.owner {
                reader.in_member(value, owner, &mut found);
                found.iter_mut().for_each(|(_, chain)| chain.retain(|l| !l.starts_with("tpl:")));
            }
            for (key, mut chain) in found {
                if item.from_call {
                    chain.retain(|link| !link.starts_with("tpl:"));
                }
                let ways = out.entry(prop.clone()).or_default().entry(key).or_default();
                if !ways.iter().any(|(j, c)| *j == i && *c == chain) {
                    ways.push((i, chain));
                }
            }
        }
    }
    out
}

/// One element of a loop's list: the object, the branches of the write that put it there, and whether it
/// came out of a call - a `tpl:` link in it names the callee's parameter, never anything the loop holds.
#[derive(Clone)]
pub struct Element {
    pub value: Value,
    chain: Vec<String>,
    from_call: bool,
    /// The member or function whose return the element came out of: where a call inside it resolves its locals.
    owner: Option<String>,
}

/// The elements a list VALUE spells, each under `chain`: an array literal's items, a `...` spread of a call
/// replaced by what its callee returns (`callee_lists`), a call by the same. None for a value that is no
/// list. A spread no callee resolves stays ONE item, as an unread item always was, so `Loops::expand`
/// refuses the list instead of reading it in part.
fn elements(value: &Value, rows: &super::gate_tsrows::TsRows, by_member: &HashMap<String, Vec<&Row>>, chain: &[String]) -> Option<Vec<Element>> {
    let of = |items: Vec<Value>, owner: String| items.into_iter().map(move |value| Element { value, chain: chain.to_vec(), from_call: true, owner: Some(owner.clone()) });
    match value {
        Value::Array(items) => Some(
            items
                .iter()
                .flat_map(|item| match callee_lists(item.get("$spread"), rows, by_member) {
                    Some((spread, owner)) => of(spread, owner).collect(),
                    None => vec![Element { value: item.clone(), chain: chain.to_vec(), from_call: false, owner: None }],
                })
                .collect(),
        ),
        other => callee_lists(Some(other), rows, by_member).map(|(items, owner)| of(items, owner).collect()),
    }
}

/// One `*ngFor` over a list of objects: its node, its variable and the list's ELEMENTS.
pub struct Loop {
    pub node: String,
    pub variable: String,
    pub items: Vec<Element>,
}

/// THE LOOPS OF A TREE, found once for the sites and for the config a directive inside one reads.
///
/// The elements are the member's own list, or the union of every write of its own class (`elements`),
/// a callee's branches not carried; a member any other class writes has none, like `key_fields` drops a
/// field written from outside.
pub struct Loops {
    pub list: Vec<Loop>,
    by_node: HashMap<String, usize>,
    parent: HashMap<String, String>,
    declared: HashMap<String, Vec<String>>,
}

impl Loops {
    pub fn new(store: &Store<'_>) -> Loops {
        let mut out = Loops { list: Vec::new(), by_node: HashMap::new(), parent: HashMap::new(), declared: HashMap::new() };
        let gates = store.table("gates");
        if !gates.iter().any(|g| text(g, "name").as_deref() == Some("ngForOf")) {
            return out;
        }
        let nodes = store.table("template_nodes");
        for n in nodes.iter() {
            let Some(id) = id_of(n, "id") else { continue };
            if let Some(parent) = id_of(n, "parent") {
                out.parent.insert(id.clone(), parent);
            }
            if let Some(Value::Array(vs)) = n.get("variables") {
                let names = vs.iter().filter_map(|v| v.get("name").and_then(|x| x.as_str()).map(str::to_string)).collect();
                out.declared.insert(id, names);
            }
        }
        let expressions = store.table("expressions");
        let expr_by_id: HashMap<String, &Row> = expressions.iter().filter_map(|e| id_of(e, "id").map(|i| (i, e))).collect();
        let rows = super::gate_tsrows::TsRows::new(store);
        let returns = store.table("returns");
        let by_member = returns_by_member(&returns);
        let branches = Branches::new(store);
        let members = store.table("members");
        let mut owner: HashMap<String, Option<String>> = HashMap::new();
        let mut lists: HashMap<String, Vec<Element>> = HashMap::new();
        let mut members_valued: HashSet<String> = HashSet::new();
        for m in members.iter() {
            let Some(id) = id_of(m, "id").filter(|i| !i.is_empty()) else { continue };
            owner.insert(id.clone(), text(m, "class"));
            // AN EMPTY LIST INITIALISES NOTHING: `tips: T[] = []` filled in `ngOnInit` holds what that writes.
            match m.get("value") {
                None | Some(Value::Null) => {}
                Some(Value::Array(items)) if items.is_empty() => {}
                Some(value) => {
                    members_valued.insert(id.clone());
                    if let Some(list) = elements(value, &rows, &by_member, &[]) {
                        lists.insert(id, list);
                    }
                }
            }
        }
        // EVERY WRITE IN A METHOD OF ITS OWN CLASS adds its elements when nothing initialises the member:
        // `ngOnInit() { this.tips = [...] }`, one write per arm of an `if`, or `= this.build()` over a method
        // of several returns. An element keeps the branches of the write that put it there. A write that is
        // not a plain `=` of a list leaves the member unread - a list read in part would hand a directive
        // fewer members than it can hold.
        let mut written: HashMap<String, Vec<&Row>> = HashMap::new();
        let assignments = store.table("assignments");
        for a in assignments.iter() {
            let Some(target) = id_of(a, "target_id") else { continue };
            if let Some(own) = owner.get(&target) {
                let writer = text(a, "class");
                if writer.is_none() || writer != *own {
                    lists.remove(&target);
                    written.remove(&target);
                    owner.insert(target, Some(String::new()));
                    continue;
                }
                written.entry(target).or_default().push(a);
            }
        }
        for (target, writes) in written {
            if lists.contains_key(&target) || members_valued.contains(&target) {
                continue;
            }
            let mut all = Vec::new();
            for a in writes {
                let list = a.get("value")
                    .filter(|_| text(a, "operator").as_deref() == Some("="))
                    .and_then(|v| elements(v, &rows, &by_member, &branches.chain_of(a)));
                let Some(list) = list else {
                    all.clear();
                    break;
                };
                all.extend(list);
            }
            if !all.is_empty() {
                lists.insert(target, all);
            }
        }
        for g in gates.iter().filter(|g| text(g, "name").as_deref() == Some("ngForOf")) {
            let (Some(top), Some(expr)) = (id_of(g, "node"), id_of(g, "expression")) else { continue };
            let Some(ast) = expr_by_id.get(&expr).and_then(|e| e.get("ast")).filter(|a| !a.is_null()) else { continue };
            let Some(items) = read_rows(unwrap(ast)).iter().find_map(|r| lists.get(r)) else { continue };
            let variable = match nodes.iter().find(|n| id_of(n, "id").as_deref() == Some(top.as_str())).and_then(|n| n.get("variables")) {
                Some(Value::Array(vs)) => vs
                    .iter()
                    .find(|v| v.get("value").and_then(|x| x.as_str()) == Some("$implicit"))
                    .and_then(|v| v.get("name").and_then(|n| n.as_str())),
                _ => None,
            };
            let Some(variable) = variable else { continue };
            out.by_node.entry(top.clone()).or_insert(out.list.len());
            out.list.push(Loop { node: top, variable: variable.to_string(), items: items.clone() });
        }
        out
    }

    /// A CONFIG READING THE LOOP VARIABLE'S ENUM PROPERTY IS ONE COPY PER ELEMENT.
    ///
    /// `{ kinds: [item.kind] }` under `*ngFor="let item of tips"` holds each element's `kind` in
    /// turn, so the ast is copied once per element with `item.kind` replaced by that element's
    /// constant. Nothing is guessed: no enclosing loop, nothing read through the variable, an
    /// element without the property, a value that is not an enum member, or an enum NAME that
    /// is zero or several declarations (a shared name is never merged) is no expansion.
    pub fn expand(&self, node: &str, ast: &Value, idx: &EnumIndex) -> Option<Vec<Value>> {
        let mut at = self.parent.get(node)?;
        let lp = loop {
            if let Some(i) = self.by_node.get(at) {
                break &self.list[*i];
            }
            at = self.parent.get(at)?;
        };
        let mut between = self.parent.get(node)?;
        while between != &lp.node {
            if self.declared.get(between).is_some_and(|d| d.contains(&lp.variable)) {
                return None;
            }
            between = self.parent.get(between)?;
        }
        let mut hits = 0;
        let mut copies = Vec::new();
        for element in &lp.items {
            copies.push(substitute(ast, &lp.variable, &element.value, idx, &mut hits)?);
        }
        (hits > 0 && !copies.is_empty()).then_some(copies)
    }
}

/// The ast with every `<variable>.<property>` read replaced by the element's constant.
fn substitute(ast: &Value, variable: &str, element: &Value, idx: &EnumIndex, hits: &mut usize) -> Option<Value> {
    match ast {
        Value::Array(items) => items.iter().map(|i| substitute(i, variable, element, idx, hits)).collect::<Option<Vec<_>>>().map(Value::Array),
        Value::Object(object) => {
            if is_read(ast)
                && let Some(name) = object.get("name").and_then(|n| n.as_str())
                && object.get("target").is_none_or(|t| t.is_null() || property_of(t, name))
                && let Some(receiver) = object.get("receiver")
                && is_read(receiver)
                && receiver.get("name").and_then(|n| n.as_str()) == Some(variable)
                && receiver.get("target").is_none_or(|t| t.is_null())
                && receiver.get("receiver").and_then(|r| r.get("k")).and_then(|k| k.as_str()) == Some("Implicit")
            {
                *hits += 1;
                let (en, member) = element.get(name)?.get("$enum")?.as_str()?.rsplit_once('.')?;
                let mut found = idx.by_id.iter().filter(|(_, info)| info.name.as_deref() == Some(en));
                let (id, _) = found.next()?;
                if found.next().is_some() {
                    return None;
                }
                return Some(serde_json::json!({"k": "Read", "name": member, "receiver": {"k": "Implicit"}, "target": {"enum": id}}));
            }
            let mut out = serde_json::Map::new();
            for (k, v) in object {
                out.insert(k.clone(), substitute(v, variable, element, idx, hits)?);
            }
            Some(Value::Object(out))
        }
        other => Some(other.clone()),
    }
}

/// A `target` the reader stamped on `<variable>.<property>` because the list's element type is NAMED (an
/// interface, a `type` alias or a class, in any file): it is that type's declaration of the same property, so
/// the read is still the element's own value. A type literal written in place leaves no target at all.
fn property_of(target: &Value, name: &str) -> bool {
    target.get("name").and_then(Value::as_str) == Some(name)
}

/// A gate whose condition reads `<variable>.<property>` - the only gate a loop element can restrict.
fn reads_variable(ast: Option<&&Value>, variable: &str) -> bool {
    let mut props = Vec::new();
    if let Some(ast) = ast {
        property_reads(unwrap(ast), variable, &mut props);
    }
    !props.is_empty()
}

/// Every `<variable>.<property>` read in a tree, the receiver a bare `Read` of the variable with no
/// resolved `target`.
fn property_reads(ast: &Value, variable: &str, out: &mut Vec<String>) {
    match ast {
        Value::Array(items) => items.iter().for_each(|i| property_reads(i, variable, out)),
        Value::Object(object) => {
            if is_read(ast)
                && let Some(name) = object.get("name").and_then(|n| n.as_str())
                && let Some(receiver) = object.get("receiver")
                && is_read(receiver)
                && receiver.get("name").and_then(|n| n.as_str()) == Some(variable)
                && receiver.get("target").is_none_or(|t| t.is_null())
                && receiver.get("receiver").and_then(|r| r.get("k")).and_then(|k| k.as_str()) == Some("Implicit")
            {
                out.push(name.to_string());
            }
            object.values().for_each(|v| property_reads(v, variable, out));
        }
        _ => {}
    }
}

fn declares(node: &Row, variable: &str) -> bool {
    matches!(node.get("variables"), Some(Value::Array(vs))
        if vs.iter().any(|v| v.get("name").and_then(|n| n.as_str()) == Some(variable)))
}

/// The sites the loops of `*ngFor` over a list of objects prove, and how many nodes they sit on.
///
/// EACH WAY OF A KEY FROM ELEMENT i CARRIES `loop:<gate>#i` FOR EVERY GATE OF THE SITE'S CHAIN THAT READS
/// THE LOOP'S VARIABLE - any other gate has no per-element value, and its link only cluttered `always_branches`: a
/// directive's config read per element (`gate_config`) publishes element i's value under that link, so
/// the key is permitted by ITS element's value and not by the union of all of them.
pub fn loop_sites(store: &Store<'_>) -> (Vec<FieldSite>, usize) {
    let mut out = Vec::new();
    let mut nodes_hit = 0;
    let loops = Loops::new(store);
    if loops.list.is_empty() {
        return (out, nodes_hit);
    }
    let i18n = store.option_list("i18n_calls");
    let keyset: KeySet = store.table("translations").iter().filter_map(|t| text(t, "key")).collect();
    let nodes = store.table("template_nodes");
    let node_by_id: HashMap<String, &Row> = nodes.iter().filter_map(|n| id_of(n, "id").map(|i| (i, n))).collect();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for n in nodes.iter() {
        if let (Some(id), Some(parent)) = (id_of(n, "id"), id_of(n, "parent")) {
            children.entry(parent).or_default().push(id);
        }
    }
    let expressions = store.table("expressions");
    let mut exprs_on: HashMap<String, Vec<&Row>> = HashMap::new();
    let mut expr_by_id: HashMap<String, &Row> = HashMap::new();
    for e in expressions.iter() {
        if let Some(node) = id_of(e, "node") {
            exprs_on.entry(node).or_default().push(e);
        }
        if let Some(id) = id_of(e, "id") {
            expr_by_id.insert(id, e);
        }
    }
    // Gate id -> its condition, so a link is laid only where the gate reads the loop's variable.
    let gate_ast: HashMap<String, &Value> = store.table("gates").iter()
        .filter_map(|g| Some((id_of(g, "id")?, expr_by_id.get(&id_of(g, "expression")?)?.get("ast")?)))
        .collect();

    let per_loop: Vec<Elements> = with_reader(store, |reader| loops.list.iter().map(|lp| element_keys(&lp.items, &keyset, &i18n, reader)).collect());
    for (lp, elements) in loops.list.iter().zip(per_loop) {
        if elements.is_empty() {
            continue;
        }
        let (top, variable) = (&lp.node, lp.variable.as_str());
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut stack = vec![top.clone()];
        while let Some(id) = stack.pop() {
            let Some(n) = node_by_id.get(&id) else { continue };
            let mut hit = false;
            for e in exprs_on.get(&id).into_iter().flatten() {
                let Some(ast) = e.get("ast").filter(|a| !a.is_null()) else { continue };
                let mut props = Vec::new();
                property_reads(unwrap(ast), variable, &mut props);
                for prop in props {
                    let Some(keys) = elements.get(&prop) else { continue };
                    if !seen.insert((id.clone(), prop.clone())) {
                        continue;
                    }
                    hit = true;
                    let gates: Vec<String> = match n.get("gate_chain") {
                        Some(Value::Array(items)) => items
                            .iter()
                            .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
                            .collect(),
                        _ => Vec::new(),
                    };
                    for (key, ways) in keys {
                        let ways = ways
                            .iter()
                            .map(|(i, chain)| {
                                let mut way = chain.clone();
                                way.extend(gates.iter().filter(|g| reads_variable(gate_ast.get(*g), variable))
                                    .map(|g| format!("loop:{g}#{i}")));
                                way
                            })
                            .collect();
                        out.push(FieldSite { key: key.clone(), comp: text(n, "component"), node: Some(id.clone()), gates: gates.clone(), ways });
                    }
                }
            }
            if hit {
                nodes_hit += 1;
            }
            for child in children.get(&id).into_iter().flatten() {
                if node_by_id.get(child).is_some_and(|c| declares(c, variable)) {
                    continue;
                }
                stack.push(child.clone());
            }
        }
    }
    (out, nodes_hit)
}
