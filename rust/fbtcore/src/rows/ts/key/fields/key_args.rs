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
    let plain = plain_locals(store);
    let ast = |id: &str| by_id.get(id).and_then(|r| r.get("ast")).filter(|a| !a.is_null()).map(|a| super::astreads::unwrap(a).clone());
    for link in links {
        let Some((branch, rest)) = link.strip_prefix("arg:").and_then(|r| r.split_once('@')) else { continue };
        let (site, tpl) = match rest.split_once('@') {
            Some((site, tpl)) => (site, Some(tpl)),
            None => (rest, None),
        };
        let Some((negated, tree, Some(callee))) = branches.condition(branch) else { continue };
        let (Some(cond), Some(call)) = (ast(tree), ast(site)) else { continue };
        if !matches!(call.get("k").and_then(Value::as_str), Some("Call" | "SafeCall")) {
            continue;
        }
        let args = call.get("args").and_then(Value::as_array).cloned().unwrap_or_default();
        let Some(names) = params.get(callee) else { continue };
        let mut read = IndexSet::new();
        params_read(&cond, names, &mut read);
        let mut missing = false;
        let replaced = substitute(&cond, names, &args, &mut missing);
        if missing {
            continue;
        }
        // THE CALLER IS THE MEMBER THE SITE IS A RETURN OF: its `const` locals are in scope for the arguments it hands.
        let caller = returned_by.get(site);
        // A PLAIN `const` LOCAL THE CALLER HANDS ON (`const k = this.item.kind; f(k)`) is the chain it holds: left a
        // local, the condition named `k`, which no gate of the template shares.
        let replaced = match caller.and_then(|c| plain.get(c)) {
            Some((names, trees)) => substitute(&replaced, names, trees, &mut false),
            None => replaced,
        };
        let none = IndexSet::new();
        let mut resolved = rows.resolve_in(&replaced, inner.get(callee).unwrap_or(&none), idx, caller.map(String::as_str));
        // A METHOD THE TEMPLATE CALLS: its own parameters are the template's arguments at that call, or the condition names `r.kind`
        // (or a bare `k`) where the template reads `row.kind`.
        if let (Some(tpl), Some(caller)) = (tpl, caller) {
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

/// Per member, its `const` locals that hold a bare read chain (`this.item.kind`), each as the tree of that chain. The
/// extractor writes no tree for such a value - its `$expr` text states it in full - so the tree is built from the text,
/// and only when every link is a plain name. A name declared twice in one member, or destructured, is none of them.
fn plain_locals(store: &Store<'_>) -> HashMap<String, (Vec<String>, Vec<Value>)> {
    let mut seen: HashMap<(String, String), Option<Value>> = HashMap::new();
    for l in store.table("locals").iter() {
        let (Some(member), Some(name)) = (id_of(l, "member"), text(l, "name")) else { continue };
        let plain = text(l, "declared").as_deref() == Some("const")
            && l.get("destructured").is_none_or(|d| d == &Value::Bool(false) || d == &Value::from(0))
            && l.get("expression").is_none_or(Value::is_null);
        let chain = l.get("value").and_then(|v| v.get("$expr")).and_then(Value::as_str).filter(|_| plain).and_then(chain_tree);
        let key = (member, name);
        let twice = seen.contains_key(&key);
        seen.insert(key, if twice { None } else { chain });
    }
    let mut out: HashMap<String, (Vec<String>, Vec<Value>)> = HashMap::new();
    for ((member, name), tree) in seen {
        if let Some(tree) = tree {
            let slot = out.entry(member).or_default();
            slot.0.push(name);
            slot.1.push(tree);
        }
    }
    out
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
