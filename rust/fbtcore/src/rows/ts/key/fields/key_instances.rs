//! THE PROPERTIES OF A CLASS INSTANCE A LIST HOLDS, for a loop that reads them (`key_loops`).
//!
//! `buttons.push(new PayButton(this.t))` puts an INSTANCE in the list, and the child's `*ngFor="let b of buttons"` reads
//! `b.text` and `b.tooltip.text(row)`: what those hold is what the CLASS says - the member's own initializer, and every
//! `this.<member> = ..` its constructors (and its bases') write. The class is the one the `new` names, so no subtype is
//! guessed (`key_classes` joins a receiver's declared type to the subtypes a `new` builds; here the `new` is the element).
//!
//! A PROPERTY IS A DOTTED PATH: an object literal written to `tooltip` is `tooltip.text`, a function-valued property the
//! keys that function returns (`this.tipText` - an arrow property or a method - or an arrow written in place), a translate
//! call written straight to a member its arguments' keys. No condition of the callee is carried: its branches test
//! its own parameters, which no gate shares.

use super::key_branches::value_keys;
use super::key_follow::{arg_keys, translated, Returned};
use super::store::Row;
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

type Found = Vec<(String, Vec<String>)>;

/// Property path -> the keys it holds, for one `new X(..)` element.
pub(super) fn properties(new: &Value, reader: &Returned<'_, '_>) -> IndexMap<String, Found> {
    let classes = reader.classes;
    let mut out: IndexMap<String, Found> = IndexMap::new();
    let Some(class) = classes.class_built(new) else { return out };
    let (inst, chain) = (classes.inst(), classes.chain(class));
    let mut seen = HashSet::new();
    for owner in &chain {
        for name in inst.names.get(owner).into_iter().flatten().filter(|n| seen.insert((*n).clone())) {
            let Some(member) = classes.member_of(class, name) else { continue };
            let mut found = Found::new();
            // A METHOD OR GETTER the template calls or reads: its returns.
            reader.enter_member(member, &mut found);
            add(&mut out, name, found);
            let written = chain.iter().flat_map(|k| inst.written.get(&(k.clone(), name.clone())).into_iter().flatten());
            for value in inst.initial.get(member).into_iter().chain(written) {
                leaf(name, value, class, reader, &mut out);
            }
        }
    }
    out
}

fn add(out: &mut IndexMap<String, Found>, path: &str, mut found: Found) {
    found.iter_mut().for_each(|(_, chain)| chain.retain(|l| !l.starts_with("tpl:")));
    if !found.is_empty() {
        out.entry(path.to_string()).or_default().extend(found);
    }
}

/// `x` of a value that is exactly `this.x`.
fn this_member(value: &Value) -> Option<&str> {
    let name = value.get("$expr")?.as_str()?.strip_prefix("this.")?;
    (value.get("$kind")?.as_str()? == "PropertyAccessExpression" && !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')).then_some(name)
}

/// The keys a value written to a property holds: a key it spells, a translate call's arguments, the returns of a function
/// it is (`$fn_id`) or names (`this.tipText`), and - for an object literal - each property under its own path.
///
/// EVERY LEVEL of a nested literal, in source order: a stack, not a depth - an object four levels down once
/// held keys no path ever named.
fn leaf(path: &str, value: &Value, class: &str, reader: &Returned<'_, '_>, out: &mut IndexMap<String, Found>) {
    let mut pending = vec![(path.to_string(), value)];
    while let Some((path, value)) = pending.pop() {
        if let Some(object) = value.as_object().filter(|o| o.keys().all(|k| !k.starts_with('$'))) {
            pending.extend(object.iter().rev().map(|(k, v)| (format!("{path}.{k}"), v)));
            continue;
        }
        value_leaf(&path, value, class, reader, out);
    }
}

/// One property's value that is not an object literal of its own.
fn value_leaf(path: &str, value: &Value, class: &str, reader: &Returned<'_, '_>, out: &mut IndexMap<String, Found>) {
    let mut found = Found::new();
    spelled(value, reader, &mut found);
    if let Some(function) = value.get("$fn_id").and_then(Value::as_str) {
        reader.enter_member(function, &mut found);
    }
    if let (Some(name), classes) = (this_member(value), reader.classes) {
        if let Some(member) = classes.member_of(class, name) {
            reader.enter_member(member, &mut found);
            let inst = classes.inst();
            let written = classes.chain(class).into_iter().flat_map(|k| inst.written.get(&(k, name.to_string())).into_iter().flatten().collect::<Vec<_>>());
            inst.initial.get(member).into_iter().chain(written).for_each(|v| spelled(v, reader, &mut found));
        }
    }
    add(out, path, found);
}

fn spelled(value: &Value, reader: &Returned<'_, '_>, found: &mut Found) {
    value_keys(value, &[], reader.keys, found);
    for arg in translated(value, reader.i18n).unwrap_or_default() {
        arg_keys(arg, &[], reader.keys, found);
    }
}

/// THE PUSHES ON THE BARE LOCAL AN ASSIGNMENT HANDS A FIELD (`const bs = []; bs.push(new X(..)); this.list = bs;`), or a
/// return hands back (`return bs;`, `key_lists::ListSource`): the row's value is the local's initializer, so the local is
/// named by the row's `source`. The local and the assignment
/// sit in one body - a callback (`.then(() => { this.list = bs; })`) is part of the member it is written in.
pub(super) fn local_pushes<'a>(assignment: &Row, pushes: &super::key_pushes::Pushes<'a>, parent: &HashMap<String, String>) -> Vec<&'a Row> {
    let Some(name) = super::key_fields::text(assignment, "source").filter(|s| s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')) else { return Vec::new() };
    let root = |member: &str| {
        let mut at = member.to_string();
        for _ in 0..32 {
            match parent.get(&at) {
                Some(up) => at = up.clone(),
                None => break,
            }
        }
        at
    };
    let Some(home) = super::key_fields::id_of(assignment, "member").map(|m| root(&m)) else { return Vec::new() };
    let mut found: Vec<&Row> = pushes.local.iter().filter(|((m, n), _)| *n == name && root(m) == home).flat_map(|(_, rows)| rows.iter().copied()).collect();
    found.sort_by_key(|r| super::key_fields::id_of(r, "id"));
    found
}
