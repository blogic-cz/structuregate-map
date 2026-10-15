//! A CALLEE'S OWN BRANCH, CARRIED THROUGH A CALL A FIELD HOLDS, WITH THE CALLER'S ARGUMENTS IN PLACE OF ITS PARAMETERS.
//!
//! `get text() { return this.desc.label(this.item.kind); }` over `label(kind) { if (warmHues.indexOf(kind) !== -1) {
//! return t('a'); } .. }` renders `a` only for the kinds that test passes. The branch tests a PARAMETER - no dimension
//! of its own - so the key read through the call carried no condition at all. Each such way now holds
//! `arg:<branch>@<site>` (`Returned::call_at`), whose condition is the branch's tree with every read of a parameter
//! replaced by the argument the call at `site` hands it (`this.item.kind`), resolved in the CALLER - so the gate readers
//! (`gate_lists`, an enum comparison, ..) read it as if the caller had written the test itself.
//!
//! ONLY THE CALL A FIELD OR GETTER HOLDS ITSELF, and only the callee's own `if`s: a branch of a callee it calls in turn
//! tests that callee's parameters, which the site's arguments do not name. A METHOD A TEMPLATE CALLS carries its callee's
//! branches too, as `arg:<branch>@<site>@<template expression>`: the method's own parameters are replaced by the
//! template's arguments at that call, so the condition reads the template's names (`row.kind`). A condition naming a parameter the call does not
//! hand (a default, a rest) is no link: the way keeps no condition, as before.

use super::gate_features::Conds;
use super::gate_values::EnumIndex;
use super::value_folds::ArgParams;
use super::key_branches::Branches;
use super::key_fields::{id_of, text};
use super::store::{Row, Store};
use indexmap::IndexSet;
use serde_json::Value;
use std::collections::HashMap;

/// The condition of every `arg:<branch>@<site>[@<template expression>]` link in `links`.
///
/// AND, per link, the ONE callee parameter its condition reads (`param:<callee>#<index>`): the restriction is on that
/// parameter whatever each caller spelled, so `value_folds` keys it so. A condition reading none or several has no entry.
pub fn arg_conds<'l>(
    store: &Store<'_>,
    branches: &Branches,
    links: impl Iterator<Item = &'l String>,
    idx: &EnumIndex,
) -> (Conds, ArgParams) {
    let mut out = Conds::new();
    let mut found_params = ArgParams::new();
    let links: IndexSet<&String> = links.filter(|l| l.starts_with("arg:")).collect();
    if links.is_empty() {
        return (out, found_params);
    }
    let rows = super::gate_tsrows::TsRows::new(store);
    let expressions = store.table("expressions");
    let by_id: HashMap<String, &Row> = expressions.iter().filter_map(|e| Some((id_of(e, "id")?, e))).collect();
    let params = super::key_fields::callables(store).0;
    // The parameters of the callbacks a member holds: no dimension, as `Branches::conds` reads them.
    let mut inner: HashMap<String, IndexSet<String>> = HashMap::new();
    for f in store.table("functions").iter() {
        let (Some(parent), Some(id)) = (text(f, "parent"), id_of(f, "id")) else { continue };
        inner.entry(parent).or_default().extend(params.get(&id).into_iter().flatten().cloned());
    }
    // The member each site is the return or the assignment of: an `expressions` row carries none.
    let mut returned_by: HashMap<String, String> = HashMap::new();
    for table in ["returns", "assignments"] {
        returned_by.extend(store.table(table).iter().filter_map(|r| Some((id_of(r, "expression")?, id_of(r, "member")?))));
    }
    let locals = Locals::new(store, &rows, idx, &params);
    // member -> every target it assigns: a parameter written is not what its call handed.
    let mut writes: HashMap<String, Vec<String>> = HashMap::new();
    for a in store.table("assignments").iter() {
        if let (Some(member), Some(target)) = (id_of(a, "member"), text(a, "target")) {
            writes.entry(member).or_default().push(target);
        }
    }
    // member -> the names its body declares.
    let mut declared: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    for l in store.table("locals").iter() {
        if let (Some(member), Some(name)) = (id_of(l, "member"), text(l, "name")) {
            declared.entry(member).or_default().insert(name);
        }
    }
    let names = |m: &str| params.get(m).cloned().unwrap_or_default();
    let mut lifted: HashMap<String, Option<Value>> = HashMap::new();
    let ast = |id: &str| by_id.get(id).and_then(|r| r.get("ast")).filter(|a| !a.is_null()).map(|a| super::astreads::unwrap(a).clone());
    for link in links {
        let Some((branch, rest)) = link.strip_prefix("arg:").and_then(|r| r.split_once('@')) else { continue };
        let (site, tpl) = match rest.split_once('@') {
            Some((site, tpl)) => (site, Some(tpl)),
            None => (rest, None),
        };
        let Some((negated, tree, Some(callee))) = branches.condition(branch) else { continue };
        let Some(callee_names) = params.get(callee) else { continue };
        // THE HOPS, outermost first (#48): `E` is a call written as expression `E`, `^E` a call INSIDE `E` found by the
        // member it enters. Hop k's call enters the member that owns hop k+1's expression; the last enters `callee`.
        let hops: Vec<&str> = site.split('~').collect();
        let owner = |hop: &str| returned_by.get(hop.trim_start_matches('^')).cloned();
        let mut entered: Vec<String> = hops.iter().skip(1).filter_map(|h| owner(h)).collect();
        entered.push(callee.to_string());
        if entered.len() != hops.len() {
            continue;
        }
        let Some(cond) = ast(tree) else { continue };
        // THE CALLEE'S OWN `const` LOCALS are their values first: a plain chain, or a private accessor's one return.
        let cond = locals.inline(&cond, callee);
        let mut read = IndexSet::new();
        params_read(&cond, callee_names, &mut read);
        // THE CONDITION LIFTED OUT OF HOPS k.. is kept by that suffix (`lifted`), read in the member hop k is written in:
        // each call site only puts its own arguments in, however many links share what lies below it.
        let suffix = |k: usize| format!("{branch}@{}", hops[k..].join("~"));
        let (mut at, mut cond) = (hops.len(), Some(cond));
        if let Some(k) = (0..hops.len()).find(|k| lifted.contains_key(&suffix(*k))) {
            (at, cond) = (k, lifted[&suffix(k)].clone());
        }
        for k in (0..at).rev() {
            let next = cond.and_then(|c| {
                let into = &entered[k];
                let own = names(into);
                // A LOCAL NO VALUE STANDS FOR would be read as a name of the member above it.
                if reads_any(&c, declared.get(into)) {
                    return None;
                }
                // A PARAMETER THE MEMBER WRITES, or a property of one, is no longer what the call handed it.
                if own.iter().any(|n| writes.get(into).is_some_and(|ts| ts.iter().any(|t| t == n || t.starts_with(&format!("{n}.")) || t.starts_with(&format!("{n}["))))) {
                    return None;
                }
                let hop = hops[k];
                let found = match hop.strip_prefix('^') {
                    None => ast(hop).filter(|c| matches!(c.get("k").and_then(Value::as_str), Some("Call" | "SafeCall"))),
                    Some(inner) => ast(inner).and_then(|tree| {
                        let at = owner(hop);
                        let resolved = rows.resolve_in(&tree, &names(at.as_deref().unwrap_or_default()).into_iter().collect(), idx, at.as_deref());
                        calling(&resolved, into)
                    }),
                };
                let args = found?.get("args").and_then(Value::as_array).cloned().unwrap_or_default();
                // A SPREAD hands the parameters something no position names.
                if args.iter().any(|a| a.get("k").and_then(Value::as_str) == Some("Spread")) {
                    return None;
                }
                let mut missing = false;
                let c = substitute(&c, &own, &args, &mut missing);
                // THE MEMBER THE HOP IS WRITTEN IN: its `const` locals are in scope for the arguments it hands.
                (!missing).then(|| owner(hop).map_or(c.clone(), |m| locals.inline(&c, &m)))
            });
            lifted.insert(suffix(k), next.clone());
            cond = next;
        }
        let Some(replaced) = cond else { continue };
        let caller = owner(hops[0]);
        let none = IndexSet::new();
        let mut resolved = rows.resolve_in(&replaced, inner.get(callee).unwrap_or(&none), idx, caller.as_deref());
        // A METHOD THE TEMPLATE CALLS: its own parameters are the template's arguments at that call, or the condition names `r.kind`
        // (or a bare `k`) where the template reads `row.kind`.
        if let (Some(tpl), Some(caller)) = (tpl, caller.as_ref()) {
            let mut found = Vec::new();
            if let Some(t) = ast(tpl) {
                super::key_methods::calls(&t, &mut found);
            }
            let hand = found.into_iter().find(|(row, _)| row == caller).map(|(_, a)| a);
            if let (Some(hand), Some(own)) = (hand, params.get(caller)) {
                let mut missing = false;
                resolved = substitute(&resolved, own, &hand, &mut missing);
                if missing {
                    continue;
                }
            }
        }
        if read.len() == 1 {
            found_params.insert(link.clone(), format!("param:{callee}#{}", read.first().copied().unwrap_or(0)));
        }
        out.insert(link.clone(), (resolved, negated));
    }
    (out, found_params)
}

/// The positions of the callee's parameters `ast` reads bare, by the test `substitute` replaces them with.
fn params_read(ast: &Value, names: &[String], out: &mut IndexSet<usize>) {
    match ast {
        Value::Array(xs) => xs.iter().for_each(|x| params_read(x, names, out)),
        Value::Object(o) => {
            let bare = o.get("k").and_then(Value::as_str) == Some("Read")
                && o.get("receiver").and_then(|r| r.get("k")).and_then(Value::as_str) == Some("Implicit")
                && o.get("target").is_none_or(Value::is_null);
            if let Some(i) = o.get("name").and_then(Value::as_str).filter(|_| bare).and_then(|n| names.iter().position(|p| p == n)) {
                out.insert(i);
                return;
            }
            let shadowed: Vec<String> = match o.get("params").and_then(Value::as_array) {
                Some(ps) if o.get("k").and_then(Value::as_str) == Some("Fn") => ps.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                _ => Vec::new(),
            };
            let kept: Vec<String> = names.iter().map(|n| if shadowed.contains(n) { String::new() } else { n.clone() }).collect();
            o.values().for_each(|v| params_read(v, &kept, out));
        }
        _ => {}
    }
}

/// `ast` reads one of `names` bare (no receiver, nothing it resolved to).
fn reads_any(ast: &Value, names: Option<&std::collections::HashSet<String>>) -> bool {
    let Some(names) = names else { return false };
    match ast {
        Value::Array(xs) => xs.iter().any(|x| reads_any(x, Some(names))),
        Value::Object(o) => {
            let bare = o.get("k").and_then(Value::as_str) == Some("Read")
                && o.get("receiver").and_then(|r| r.get("k")).and_then(Value::as_str) == Some("Implicit")
                && o.get("target").is_none_or(Value::is_null);
            (bare && o.get("name").and_then(Value::as_str).is_some_and(|n| names.contains(n))) || o.values().any(|x| reads_any(x, Some(names)))
        }
        _ => false,
    }
}

/// The one `Call` in `tree` whose callee resolved to `member` (`target.row`), None for none or several.
fn calling(tree: &Value, member: &str) -> Option<Value> {
    fn walk(v: &Value, member: &str, out: &mut Vec<Value>) {
        match v {
            Value::Object(o) => {
                let call = matches!(o.get("k").and_then(Value::as_str), Some("Call" | "SafeCall"));
                let row = o.get("receiver").and_then(|r| r.get("target")).and_then(|t| t.get("row")).and_then(Value::as_str);
                if call && row == Some(member) {
                    out.push(v.clone());
                }
                o.values().for_each(|x| walk(x, member, out));
            }
            Value::Array(xs) => xs.iter().for_each(|x| walk(x, member, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(tree, member, &mut found);
    match found.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// Per member, the `const` locals a condition may be read through, each as the tree it stands for:
///
/// - a bare read chain (`const k = this.item.kind`) - the extractor writes no tree for such a value, its `$expr` states it;
/// - A PRIVATE ACCESSOR'S CALL (#55, `const kind = this.kindOf(model)`): a `private` method whose one return, under no
///   branch, is a read chain rooted at one of its parameters (`return model.kind;`) is that chain with the call's
///   arguments in place - nothing else can run there, and nothing outside the class can replace it.
///
/// A name declared twice in one member, or destructured, is none of them.
struct Locals {
    by_member: HashMap<String, (Vec<String>, Vec<Value>)>,
}

impl Locals {
    fn new(store: &Store<'_>, rows: &super::gate_tsrows::TsRows, idx: &EnumIndex, params: &HashMap<String, Vec<String>>) -> Locals {
        let expressions = store.table("expressions");
        let by_id: HashMap<String, &Row> = expressions.iter().filter_map(|e| Some((id_of(e, "id")?, e))).collect();
        let members = store.table("members");
        let private: std::collections::HashSet<String> = members.iter()
            .filter(|m| text(m, "visibility").as_deref() == Some("private") && text(m, "kind").as_deref() == Some("method"))
            .filter_map(|m| id_of(m, "id")).collect();
        let returns = store.table("returns");
        let by_member = super::key_fields::returns_by_member(&returns);
        // The one plain return of a private method, when it is a read chain rooted at a parameter.
        let accessor = |method: &str| -> Option<Value> {
            if !private.contains(method) {
                return None;
            }
            let [r] = by_member.get(method)?.as_slice() else { return None };
            if text(r, "branch").is_some() || text(r, "case").is_some() {
                return None;
            }
            let chain = chain_tree(r.get("value")?.get("$expr")?.as_str()?)?;
            let mut root = &chain;
            while let Some(inner) = root.get("receiver").filter(|x| x.get("k").and_then(Value::as_str) == Some("Read")) {
                root = inner;
            }
            let name = root.get("name").and_then(Value::as_str)?;
            params.get(method)?.iter().any(|p| p == name).then_some(chain)
        };
        let mut seen: HashMap<(String, String), Option<Value>> = HashMap::new();
        for l in store.table("locals").iter() {
            let (Some(member), Some(name)) = (id_of(l, "member"), text(l, "name")) else { continue };
            let single = text(l, "declared").as_deref() == Some("const")
                && l.get("destructured").is_none_or(|d| d == &Value::Bool(false) || d == &Value::from(0));
            let tree = match l.get("expression").filter(|x| !x.is_null()).and_then(Value::as_str) {
                None => l.get("value").and_then(|v| v.get("$expr")).and_then(Value::as_str).filter(|_| single).and_then(chain_tree),
                // PEEKED, and only for a single `const`: the tree is read once into the chain kept.
                Some(x) => by_id.get(x).filter(|_| single).and_then(|e| e.peek("ast", |ast| ast.and_then(|ast| {
                    let own = params.get(&member).cloned().unwrap_or_default();
                    let call = rows.resolve_in(super::astreads::unwrap(ast), &own.into_iter().collect(), idx, Some(&member));
                    let callee = call.get("receiver").filter(|r| r.get("receiver").and_then(|t| t.get("k")).and_then(Value::as_str) == Some("This"))?
                        .get("target")?.get("row")?.as_str()?;
                    let chain = accessor(callee)?;
                    let args = call.get("args")?.as_array()?;
                    if args.iter().any(|a| a.get("k").and_then(Value::as_str) == Some("Spread")) {
                        return None;
                    }
                    let mut missing = false;
                    let out = substitute(&chain, params.get(callee)?, args, &mut missing);
                    (!missing).then_some(out)
                }))),
            };
            let key = (member, name);
            let twice = seen.contains_key(&key);
            seen.insert(key, if twice { None } else { tree });
        }
        let mut out: HashMap<String, (Vec<String>, Vec<Value>)> = HashMap::new();
        for ((member, name), tree) in seen {
            if let Some(tree) = tree {
                let slot = out.entry(member).or_default();
                slot.0.push(name);
                slot.1.push(tree);
            }
        }
        Locals { by_member: out }
    }

    /// `ast` with every bare read of one of `member`'s locals replaced by what it stands for.
    fn inline(&self, ast: &Value, member: &str) -> Value {
        match self.by_member.get(member) {
            Some((names, trees)) => substitute(ast, names, trees, &mut false),
            None => ast.clone(),
        }
    }
}

/// The `Read` tree of `this.a.b` or `a.b`, None unless every link is a plain name.
fn chain_tree(text: &str) -> Option<Value> {
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$');
    let mut parts = text.split('.');
    let first = parts.next()?;
    let mut tree = match first {
        "this" => serde_json::json!({"k": "This"}),
        name if plain(name) => serde_json::json!({"k": "Read", "name": name, "receiver": {"k": "Implicit"}}),
        _ => return None,
    };
    for part in parts {
        if !plain(part) {
            return None;
        }
        tree = serde_json::json!({"k": "Read", "name": part, "receiver": tree});
    }
    (tree.get("k").and_then(Value::as_str) == Some("Read")).then_some(tree)
}

/// `ast` with every bare read of a parameter replaced by the argument at its position - not inside a callback whose own
/// parameter shadows the name. A parameter the call hands nothing for sets `missing`.
fn substitute(ast: &Value, names: &[String], args: &[Value], missing: &mut bool) -> Value {
    match ast {
        Value::Array(xs) => Value::Array(xs.iter().map(|x| substitute(x, names, args, missing)).collect()),
        Value::Object(o) => {
            let bare = o.get("k").and_then(Value::as_str) == Some("Read")
                && o.get("receiver").and_then(|r| r.get("k")).and_then(Value::as_str) == Some("Implicit")
                && o.get("target").is_none_or(Value::is_null);
            if let Some(i) = o.get("name").and_then(Value::as_str).filter(|_| bare).and_then(|n| names.iter().position(|p| p == n)) {
                return match args.get(i) {
                    Some(arg) => arg.clone(),
                    None => {
                        *missing = true;
                        ast.clone()
                    }
                };
            }
            // A CALLBACK'S OWN PARAMETER SHADOWS the method's of the same name.
            let shadowed: Vec<String> = match o.get("params").and_then(Value::as_array) {
                Some(ps) if o.get("k").and_then(Value::as_str) == Some("Fn") => ps.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                _ => Vec::new(),
            };
            let kept: Vec<String> = names.iter().map(|n| if shadowed.contains(n) { String::new() } else { n.clone() }).collect();
            Value::Object(o.iter().map(|(k, v)| (k.clone(), substitute(v, &kept, args, missing))).collect())
        }
        other => other.clone(),
    }
}
