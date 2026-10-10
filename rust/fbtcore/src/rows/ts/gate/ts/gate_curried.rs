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

use super::gate_lists::{enum_for, list_test, spelled, List};
use super::gate_predicates::{kind, local, only_return};
use super::gate_tsrows::TsRows;
use super::gate_values::{enum_of_type_ref, EnumIndex};
use super::astreads::unwrap;
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
        let Some((list, d)) = list_test(&tree) else { continue };
        if local(list) != Some(p) || local(d) != Some(q) {
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
