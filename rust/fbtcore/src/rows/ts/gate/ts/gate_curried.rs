//! A PREDICATE A FACTORY BUILDS — `const isAlpha = makeCheck([Grade.Alpha])` over
//! `makeCheck = (ids: Grade[]) => (grade: Grade) => ids.includes(grade)` — as the membership test
//! its factory spells, with the list the factory was handed.
//!
//! The factory is found by the call's evaluated `$target` (name and file, one function of that name
//! in that file), never by the name alone, and it must be what `gate_lists` already demands of a
//! predicate: ONE unconditional return, here an arrow, whose one return is a membership test of
//! the OUTER parameter's list by the INNER parameter. The list is the call's argument, spelled out
//! in constants (`$args`); anything else - a variable, a spread, two parameters, a second return -
//! is not read, and the call restricts nothing, as before.
//!
//! A REST PARAMETER (`(...rest) => rest.some((x) => ids.includes(x))`) is the same test: the call site
//! hands exactly one argument (`Lists::restriction`), so `rest` is that one value. So are `ids.includes(rest[0])`
//! and `rest.m(ids)` over an `Array.prototype.m = function (o) { return this.some((x) => o.includes(x)); }`,
//! joined by the call's `target` to the `prototype-method` row the extractor files for the assignment.

use super::gate_lists::{enum_for, list_test, spelled, List};
use super::gate_predicates::{kind, literal_is, local, method_call, only_return};
use super::gate_tsrows::TsRows;
use super::gate_values::{enum_of_type_ref, EnumIndex};
use super::astreads::{is_read, unwrap};
use super::store::Store;
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

/// Row id of every member or `const` holding such a call -> the enum and members it tests in.
pub fn curried(
    store: &Store<'_>,
    idx: &EnumIndex,
    rows: &TsRows,
    bound: &IndexMap<String, IndexSet<String>>,
) -> IndexMap<String, (String, Vec<String>)> {
    // (factory, list the call handed it) by the row the call initialises.
    let mut built: IndexMap<String, (String, Value)> = IndexMap::new();
    for table in ["members", "consts"] {
        for r in store.table(table).iter() {
            let (Some(Value::String(id)), Some(value)) = (r.get("id"), r.get("value")) else { continue };
            let (Some(target), Some(Value::Array(args))) = (value.get("$target"), value.get("$args")) else { continue };
            let ([list], Some(factory)) = (args.as_slice(), rows.function_named(target)) else { continue };
            built.insert(id.clone(), (factory, list.clone()));
        }
    }
    let mut out = IndexMap::new();
    if built.is_empty() {
        return out;
    }
    // THE FACTORY'S ONE UNCONDITIONAL RETURN, the arrow it hands back.
    let mut returns: IndexMap<&str, Vec<Option<&str>>> = IndexMap::new();
    let all = store.table("returns");
    for r in all.iter() {
        let Some(member) = r.get("member").and_then(Value::as_str).filter(|m| built.values().any(|(f, _)| f == m)) else { continue };
        let plain = ["branch", "case"].iter().all(|f| r.get(f).is_none_or(Value::is_null));
        returns.entry(member).or_default().push(r.get("expression").and_then(Value::as_str).filter(|_| plain));
    }
    let wanted: IndexMap<&str, &str> = returns.iter()
        .filter_map(|(m, xs)| match xs.as_slice() { [Some(x)] => Some((*x, *m)), _ => None })
        .collect();
    let none = IndexSet::new();
    let protos = some_methods(store);
    let mut params: IndexMap<&str, &Value> = IndexMap::new();
    let functions = store.table("functions");
    for f in functions.iter() {
        if let (Some(id), Some(Value::Array(ps))) = (f.get("id").and_then(Value::as_str), f.get("params")) {
            if let [one] = ps.as_slice() {
                params.insert(id, one);
            }
        }
    }
    for e in store.table("expressions").iter() {
        let Some(factory) = e.get("id").and_then(Value::as_str).and_then(|x| wanted.get(x)) else { continue };
        let (Some(ast), Some(outer)) = (e.get("ast"), params.get(factory)) else { continue };
        let inner = unwrap(ast);
        let ([q], Some(body)) = (
            inner.get("params").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default(),
            only_return(inner).filter(|_| kind(inner) == Some("Fn")),
        ) else { continue };
        let (Some(p), Some(q)) = (outer.get("name").and_then(Value::as_str), q.as_str()) else { continue };
        let tree = rows.resolve_in(body, bound.get(*factory).unwrap_or(&none), idx, Some(factory));
        let rest = inner.get("rest").and_then(Value::as_str) == Some(q);
        let tested = if rest {
            some_of_rest(&tree, q).or_else(|| first_of_rest(&tree, q)).or_else(|| proto_of_rest(&tree, q, &protos))
        } else {
            list_test(&tree).filter(|(_, d)| local(d) == Some(q))
        };
        let Some((list, _)) = tested else { continue };
        if local(list) != Some(p) {
            continue;
        }
        let element = outer.get("type_ref").and_then(|t| t.get("element"));
        for (row, (_, handed)) in built.iter().filter(|(_, (f, _))| f == factory) {
            let Some((named, members)) = spelled(Some(handed)) else { continue };
            let enum_id = element.and_then(|e| enum_of_type_ref(idx, &named, Some(e)));
            let held = List { named, enum_id, members };
            let Some(en) = enum_for(&held, None, idx) else { continue };
            let domain = idx.by_id.get(&en).map(|i| &i.domain);
            if domain.is_some_and(|d| held.members.iter().all(|m| d.contains(m))) {
                out.insert(row.clone(), (en, held.members));
            }
        }
    }
    out
}

/// `rest.some((x) => <list test of x>)`, as the (list, element) the callback tests.
fn some_of_rest<'a>(tree: &'a Value, rest: &str) -> Option<(&'a Value, &'a Value)> {
    let (over, cb) = method_call(tree, "some")?;
    let [x] = cb.get("params")?.as_array()?.as_slice() else { return None };
    let (list, d) = list_test(only_return(cb)?)?;
    (local(over) == Some(rest) && local(d) == x.as_str()).then_some((list, d))
}

/// `ids.includes(rest[0])`: the call site hands one argument, so `rest[0]` is it.
fn first_of_rest<'a>(tree: &'a Value, rest: &str) -> Option<(&'a Value, &'a Value)> {
    let (list, d) = list_test(tree)?;
    let first = kind(d) == Some("KeyedRead")
        && d.get("receiver").is_some_and(|r| local(r) == Some(rest))
        && d.get("key").is_some_and(|k| literal_is(k, 0));
    first.then_some((list, d))
}

/// `rest.overlaps(ids)` over a `prototype-method` whose one return is `this.some((x) => P.includes(x))` (or turned round).
fn proto_of_rest<'a>(tree: &'a Value, rest: &str, protos: &IndexSet<String>) -> Option<(&'a Value, &'a Value)> {
    if kind(tree) != Some("Call") {
        return None;
    }
    let callee = tree.get("receiver")?;
    let [list] = tree.get("args")?.as_array()?.as_slice() else { return None };
    let over = callee.get("receiver")?;
    (is_read(callee) && local(over) == Some(rest) && protos.contains(&target_key(callee.get("target")?)?))
        .then_some((list, over))
}

fn target_key(t: &Value) -> Option<String> {
    Some(format!("{} {} {}", t.get("name")?.as_str()?, t.get("file")?.as_str()?.replace('\\', "/"), t.get("line")?.as_i64()?))
}

/// The `target` of every `prototype-method` whose body PROVES "some element of `this` is in my one parameter" -
/// `this.some((x) => P.includes(x))`, or `P.some((x) => this.includes(x))`, the same test turned round. A `this`
/// parameter is a type, never an argument. A method two definitions give two bodies is read only when EVERY
/// one proves it: which assignment runs last is not in the source.
fn some_methods(store: &Store<'_>) -> IndexSet<String> {
    let mut found: IndexMap<String, (String, Option<String>)> = IndexMap::new(); // fn id -> (target key, the one param)
    for f in store.table("functions").iter() {
        if f.get("form").and_then(Value::as_str) != Some("prototype-method") {
            continue;
        }
        let (Some(id), Some(key)) = (f.get("id").and_then(Value::as_str), f.get("target").and_then(target_key)) else {
            continue;
        };
        let params: Vec<&str> = f.get("params").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|p| p.get("name").and_then(Value::as_str))
            .filter(|n| *n != "this")
            .collect();
        let one = match params.as_slice() { [p] => Some(p.to_string()), _ => None };
        found.insert(id.to_string(), (key, one));
    }
    let mut returns: IndexMap<&str, Vec<Option<String>>> = IndexMap::new();
    let all = store.table("returns");
    for r in all.iter() {
        let Some(member) = r.get("member").and_then(Value::as_str).filter(|m| found.contains_key(*m)) else { continue };
        let plain = ["branch", "case"].iter().all(|f| r.get(f).is_none_or(Value::is_null));
        returns.entry(member).or_default().push(r.get("expression").and_then(Value::as_str).filter(|_| plain).map(str::to_string));
    }
    let one: IndexMap<String, &str> = returns.iter()
        .filter_map(|(m, xs)| match xs.as_slice() { [Some(x)] => Some((x.clone(), *m)), _ => None })
        .collect();
    let mut proves: IndexSet<&str> = IndexSet::new();
    for e in store.table("expressions").iter() {
        let Some(fid) = e.get("id").and_then(Value::as_str).and_then(|x| one.get(x)) else { continue };
        let Some(p) = found[*fid].1.as_deref() else { continue };
        let Some(ast) = e.get("ast").map(unwrap) else { continue };
        let Some((over, cb)) = method_call(ast, "some") else { continue };
        let ([x], Some(body)) = (cb.get("params").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default(), only_return(cb)) else { continue };
        let Some((l, d)) = list_test(body) else { continue };
        let this_in_p = kind(over) == Some("This") && local(l) == Some(p);
        let p_in_this = local(over) == Some(p) && kind(l) == Some("This");
        if (this_in_p || p_in_this) && local(d) == x.as_str() {
            proves.insert(*fid);
        }
    }
    let mut every: IndexMap<&str, bool> = IndexMap::new();
    for (id, (key, _)) in &found {
        *every.entry(key.as_str()).or_insert(true) &= proves.contains(id.as_str());
    }
    every.into_iter().filter(|(_, ok)| *ok).map(|(k, _)| k.to_string()).collect()
}
