//! A PROPERTY OF A FIELD'S DECLARED TYPE, WRITTEN BY THE SELECTOR THE FIELD SELECTS (#117).
//!
//! `vm$: Observable<{ criteria: { yesNoOptions: Option[] } }>` written `this.vm$ = this.store.select((x) => sel.forId(id)(x))`
//! makes `[options]="vm.criteria.yesNoOptions"` (under `vm$ | async as vm`) a read of the TYPE literal's `shape_members` row,
//! a signature with no value - `shape_values` finds none. The value is in the selector: the field's initializer or a plain
//! `=` of it calls ngrx's `select` with a selector, and the selector's projector returns an object literal holding the same
//! path. Every step is a row the checker resolved:
//!
//!   * the selector - a `$fn` arrow (`$fn_id`) whose return is a `createSelector(..)` call, or a call of a selector FACTORY:
//!     the `calls` row of that callee in the arrow, its `callee_id` the factory's `functions` row, whose return is the
//!     `createSelector(..)` call (a factory returning a factory's call is followed the same way, each once);
//!   * the projector - the call's last argument, a `$fn` arrow whose returns are the object literals walked.
//!
//! The path from the field's type to the read property is the signature's parent signatures, as `shape_values` climbs an
//! object literal's properties. A projector returning no object, or no value at the path, gives nothing.
//!
//! A SIGNATURE IS ALSO WRITTEN WHERE A VALUE FILLS IT (`filled`): an object literal's property the checker types by it
//! (`shape_members.target`: `initialState: State = { list: [..] }`) and a plain `=` naming its place (`assignments.target_ref`
//! with its `line`: a reducer's `state.list = [..]`). Each is one more value of the read; no write's condition is carried.

use super::key_fields::{id_of, text};
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn at(r: &Row) -> Option<(i64, i64, i64)> {
    Some((r.get("line")?.as_i64()?, r.get("end_line")?.as_i64()?, r.get("col").and_then(Value::as_i64).unwrap_or(0)))
}

/// An evaluated call of ngrx's `name` (`select`, `createSelector`), by the declaration it resolved to.
fn ngrx(v: &Value, name: &str) -> bool {
    let target = v.get("$target");
    target.and_then(|t| t.get("name")).and_then(Value::as_str) == Some(name)
        && target.and_then(|t| t.get("file")).and_then(Value::as_str).is_some_and(|f| f.replace('\\', "/").contains("/@ngrx/store/"))
}

struct Rows<'s> {
    returns: HashMap<String, Vec<&'s Value>>,
    /// Function or member id -> its `calls` rows that resolved to a `functions` row: (callee text, callee id).
    calls: HashMap<String, Vec<(String, String)>>,
    functions: HashSet<String>,
}

impl Rows<'_> {
    /// The object literals a selector's projector returns, the selector being what `value` evaluates to inside `owner`.
    fn projected(&self, value: &Value, owner: &str, seen: &mut HashSet<String>, out: &mut Vec<Value>) {
        if let Some(arrow) = value.get("$fn_id").and_then(Value::as_str) {
            for r in self.returns.get(arrow).into_iter().flatten() {
                self.projected(r, arrow, seen, out);
            }
            return;
        }
        if ngrx(value, "createSelector") {
            let projector = value.get("$args").and_then(Value::as_array).and_then(|a| a.last()).and_then(|p| p.get("$fn_id")).and_then(Value::as_str);
            out.extend(projector.into_iter().flat_map(|p| self.returns.get(p).into_iter().flatten()).filter(|r| plain_object(r)).map(|r| (*r).clone()));
            return;
        }
        // A FACTORY'S CALL, CALLED: `sel.forId(id)(x)` is a `$call` whose text is the factory's call; that call's own row in
        // `owner` names the factory. `sel.forId(id)` alone is the factory's call too.
        let Some(call) = value.get("$call").and_then(Value::as_str) else { return };
        for (callee, factory) in self.calls.get(owner).into_iter().flatten() {
            let named = call == callee || call.strip_prefix(callee.as_str()).is_some_and(|rest| rest.starts_with('('));
            if named && self.functions.contains(factory) && seen.insert(factory.clone()) {
                for r in self.returns.get(factory).into_iter().flatten() {
                    self.projected(r, factory, seen, out);
                }
            }
        }
    }
}

fn plain_object(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.keys().all(|k| !k.starts_with('$')))
}

/// Each wanted signature's values, over the selectors of the field whose declared type holds it.
pub(super) fn selected_values(store: &Store<'_>, wanted: &HashSet<String>) -> HashMap<String, Vec<Value>> {
    let members = store.table("shape_members");
    let shapes = store.table("shapes");
    let shape_by_id: HashMap<String, &Row> = shapes.iter().filter_map(|s| Some((id_of(s, "id")?, s))).collect();
    let signature = |m: &&Row| text(m, "kind").as_deref() == Some("signature");
    let mut ending: HashMap<(String, i64), Vec<&Row>> = HashMap::new();
    for m in members.iter().filter(signature) {
        if let (Some(file), Some((_, end, _))) = (text(m, "file"), at(m)) {
            ending.entry((file, end)).or_default().push(m);
        }
    }
    // THE PATH AND THE ROOT TYPE LITERAL of each wanted signature: its parent signatures, as `shape_values` climbs.
    let mut roots: Vec<(String, Vec<String>, String, (i64, i64, i64))> = Vec::new();
    for m in members.iter().filter(signature) {
        let Some(id) = id_of(m, "id").filter(|i| wanted.contains(i)) else { continue };
        let Some(name) = text(m, "name") else { continue };
        let mut path = vec![name];
        let mut shape = id_of(m, "shape").and_then(|s| shape_by_id.get(&s).copied());
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(s) = shape {
            let (Some(sid), Some(file), Some(sp)) = (id_of(s, "id"), text(s, "file"), at(s)) else { break };
            if !seen.insert(sid.clone()) {
                break;
            }
            let parent = ending.get(&(file.clone(), sp.1)).into_iter().flatten()
                .filter_map(|p| Some((p, at(p)?)))
                .filter(|(p, pa)| (pa.0, pa.2) < (sp.0, sp.2) && id_of(p, "shape").as_deref() != Some(sid.as_str()))
                .max_by_key(|(_, pa)| (pa.0, pa.2))
                .map(|(p, _)| p);
            match parent {
                Some(p) => {
                    let Some(pname) = text(p, "name") else { break };
                    path.push(pname);
                    shape = id_of(p, "shape").and_then(|x| shape_by_id.get(&x).copied());
                }
                None => {
                    path.reverse();
                    roots.push((id, path, file, sp));
                    break;
                }
            }
        }
    }
    if roots.is_empty() {
        return HashMap::new();
    }

    // THE FIELD WHOSE DECLARATION HOLDS THE ROOT, and what it is written with: its initializer and every plain `=` of it.
    let fields = store.table("members");
    let mut written: HashMap<String, Vec<(Value, String)>> = HashMap::new();
    for m in fields.iter() {
        if let (Some(id), Some(v)) = (id_of(m, "id"), m.get("value").filter(|v| v.is_object())) {
            written.entry(id.clone()).or_default().push((v.clone(), id));
        }
    }
    for a in store.table("assignments").iter() {
        let (Some(target), Some(v)) = (id_of(a, "target_id"), a.get("value").filter(|v| v.is_object())) else { continue };
        if text(a, "operator").as_deref() == Some("=")
            && let Some(member) = text(a, "member")
        {
            written.entry(target).or_default().push((v.clone(), member));
        }
    }
    let mut rows = Rows { returns: HashMap::new(), calls: HashMap::new(), functions: HashSet::new() };
    let returns = store.table("returns");
    for r in returns.iter() {
        if let (Some(m), Some(v)) = (text(r, "member"), r.get("value").filter(|v| v.is_object())) {
            rows.returns.entry(m).or_default().push(v);
        }
    }
    let functions = store.table("functions");
    rows.functions.extend(functions.iter().filter_map(|f| id_of(f, "id")));
    for c in store.table("calls").iter() {
        if let (Some(m), Some(callee), Some(to)) = (text(c, "member"), text(c, "callee"), id_of(c, "callee_id")) {
            rows.calls.entry(m).or_default().push((callee, to));
        }
    }

    let mut out = HashMap::new();
    for (id, path, file, root) in roots {
        let holder = fields.iter()
            .filter(|f| text(f, "file").as_deref() == Some(file.as_str()) && text(f, "kind").as_deref() == Some("property"))
            .filter_map(|f| Some((f, at(f)?)))
            .filter(|(_, fa)| fa.0 <= root.0 && fa.1 >= root.1)
            .max_by_key(|(_, fa)| (fa.0, -fa.1))
            .and_then(|(f, _)| id_of(f, "id"));
        let Some(field) = holder else { continue };
        let mut objects = Vec::new();
        for (v, owner) in written.get(&field).into_iter().flatten() {
            if !ngrx(v, "select") {
                continue;
            }
            let Some(selector) = v.get("$args").and_then(Value::as_array).and_then(|a| a.first()) else { continue };
            rows.projected(selector, owner, &mut HashSet::new(), &mut objects);
        }
        let found: Vec<Value> = objects.iter()
            .filter_map(|o| path.iter().try_fold(o, |at, name| at.as_object().filter(|_| plain_object(at))?.get(name)))
            .cloned()
            .collect();
        if !found.is_empty() {
            out.insert(id, found);
        }
    }
    out
}

/// What fills each wanted row's PLACE: the literal properties typed by it (their ids, valued by `shape_values`) and the
/// values of the plain `=` writes naming it.
pub(super) fn filled(store: &Store<'_>, wanted: &HashSet<String>) -> HashMap<String, (Vec<String>, Vec<Value>)> {
    // A REF NAMES ITS FILE BY PATH (`assignments.target_ref`) or absolute (`shape_members.target`, joined on `files.abs`).
    let mut paths: HashMap<String, String> = HashMap::new();
    for f in store.table("files").iter() {
        let Some(id) = id_of(f, "id") else { continue };
        for column in ["path", "abs"] {
            if let Some(p) = text(f, column) {
                paths.insert(p.replace('\\', "/"), id.clone());
            }
        }
    }
    // A REF'S PLACE: (file row id, line, name).
    let place = |r: &Value| -> Option<(String, i64, String)> {
        let file = paths.get(&r.get("file")?.as_str()?.replace('\\', "/"))?.clone();
        Some((file, r.get("line")?.as_i64()?, r.get("name")?.as_str()?.to_string()))
    };
    let members = store.table("shape_members");
    let mut at_place: HashMap<(String, i64, String), String> = HashMap::new();
    for m in members.iter() {
        if let Some(id) = id_of(m, "id").filter(|i| wanted.contains(i))
            && let (Some(file), Some(line), Some(name)) = (text(m, "file"), m.get("line").and_then(Value::as_i64), text(m, "name"))
        {
            at_place.insert((file, line, name), id);
        }
    }
    let mut out: HashMap<String, (Vec<String>, Vec<Value>)> = HashMap::new();
    if at_place.is_empty() {
        return out;
    }
    for m in members.iter() {
        let Some(w) = m.get("target").and_then(&place).and_then(|p| at_place.get(&p)) else { continue };
        if let Some(id) = id_of(m, "id") {
            out.entry(w.clone()).or_default().0.push(id);
        }
    }
    for a in store.table("assignments").iter() {
        if text(a, "operator").as_deref() != Some("=") {
            continue;
        }
        let Some(w) = a.get("target_ref").and_then(&place).and_then(|p| at_place.get(&p)) else { continue };
        if let Some(v) = a.get("value").filter(|v| !v.is_null()) {
            out.entry(w.clone()).or_default().1.push(v.clone());
        }
    }
    out
}

/// ONE VALUE, or every value a list: their elements together. Anything else is no answer.
pub(super) fn one_value(found: Vec<Value>) -> Option<Value> {
    match found.len() {
        0 => None,
        1 => found.into_iter().next(),
        _ if found.iter().all(Value::is_array) => Some(Value::Array(found.into_iter().flat_map(|v| match v {
            Value::Array(xs) => xs,
            _ => Vec::new(),
        }).collect())),
        _ => None,
    }
}
