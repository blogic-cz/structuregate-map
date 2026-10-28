//! ONE VARIABLE UNDER TWO SPELLINGS OF ONE CLASS (#117): `this.vehicle.Type` and `this.model.Vehicle.Type` are one
//! dimension when the class PROVES both names read the same chain - and a way testing it twice then intersects.
//!
//! `fold_enums` unions an enum's differently spelled dimensions on a way, since two spellings may be two variables: a
//! way under `vehicle.Type not_in [A]` and `model.Vehicle.Type in [A]` permitted nearly everything, where the two
//! contradict and the way renders for nobody. A name is replaced by the chain it reads when the class shows one of:
//!
//!   * a GETTER of one return under no branch or case, a plain `this.` chain (`get model() { return this.ctx.input; }`);
//!   * a PROPERTY or SETTER written exactly once - a plain `=` of a plain `this.` chain, in the class's constructor or
//!     `ngOnInit`, under no branch or case, before any `return` - never an `@Input`, never a two-way template write;
//!     a setter's backing field is the setter's value when the setter only copies its one parameter into it and the
//!     getter of its name returns that field.
//!
//! The pairs are `value_folds::spellings`' (`name`, `chain`), added to a way for every component on its path - its
//! class and every base it extends, the nearest declaration of a name first. Nothing is renamed: the rows keep their
//! texts, and `unify_present` gives the spellings present on a way the intersection of their sets.

use super::store::{Row, Store};
use crate::rows::{IndexMap, IndexSet};
use serde_json::Value;
use std::sync::Arc;

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// `this.a.b` -> `a.b`, only when every link is a plain name.
fn this_chain(text: &str) -> Option<String> {
    let rest = text.trim().strip_prefix("this.")?;
    rest.split('.')
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
        .then(|| rest.to_string())
}

fn plain(r: &Row) -> bool {
    ["branch", "case", "loop"].iter().all(|f| cell(r, f).is_none())
}

/// Component id -> the (name, chain) pairs its class chain proves.
pub fn defined(store: &Store<'_>) -> IndexMap<String, Arc<Vec<(String, String)>>> {
    let line = |r: &Row| r.get("line").and_then(Value::as_i64).unwrap_or(i64::MAX);
    let mut returns: IndexMap<String, Vec<&Row>> = IndexMap::default();
    let all_returns = store.table("returns");
    for r in all_returns.iter() {
        if let Some(m) = cell(r, "member") {
            returns.entry(m).or_default().push(r);
        }
    }
    let mut writes: IndexMap<String, Vec<&Row>> = IndexMap::default();
    let assignments = store.table("assignments");
    for a in assignments.iter() {
        if let Some(t) = cell(a, "target_id") {
            writes.entry(t).or_default().push(a);
        }
    }
    let two_way = super::gate_features::template_written(store);
    let members = store.table("members");
    let mut lifecycle: IndexMap<String, String> = IndexMap::default();
    // class -> name -> the member rows of that name (a getter and a setter share one).
    let mut by_class: IndexMap<String, IndexMap<String, Vec<&Row>>> = IndexMap::default();
    for m in members.iter() {
        let (Some(id), Some(class), Some(name)) = (cell(m, "id"), cell(m, "class"), cell(m, "name")) else { continue };
        if cell(m, "kind").as_deref() == Some("constructor") || name == "ngOnInit" {
            lifecycle.insert(id, class.clone());
        }
        by_class.entry(class).or_default().entry(name).or_default().push(m);
    }
    let input = |m: &Row| m.get("decorators").and_then(Value::as_array).into_iter().flatten()
        .any(|d| d.get("name").and_then(Value::as_str) == Some("Input"));
    // THE ONE WRITE THAT RAN BEFORE THE VIEW: in its class's constructor or `ngOnInit`, plain, no `return` before it.
    let once = |id: &str, class: &str| -> Option<&Row> {
        let [a] = writes.get(id)?.as_slice() else { return None };
        let member = cell(a, "member")?;
        (cell(a, "operator").as_deref() == Some("=")
            && plain(a)
            && lifecycle.get(&member).is_some_and(|c| c == class)
            && returns.get(&member).into_iter().flatten().all(|r| line(r) > line(a)))
            .then_some(*a)
    };
    let mut own: IndexMap<String, Vec<(String, String)>> = IndexMap::default();
    for (class, names) in &by_class {
        let mut pairs = Vec::new();
        for (name, rows) in names {
            if rows.iter().any(|m| input(m)) || two_way.contains(&format!("{class} {name}")) {
                continue;
            }
            let kind_of = |k: &str| rows.iter().find(|m| cell(m, "kind").as_deref() == Some(k));
            let chain = match (kind_of("getter"), kind_of("setter"), kind_of("property")) {
                // A SETTER WRITTEN ONCE whose getter returns the field the setter copies its parameter into.
                (Some(g), Some(s), None) => {
                    let (Some(gid), Some(sid)) = (cell(g, "id"), cell(s, "id")) else { continue };
                    let [r] = returns.get(&gid).map(Vec::as_slice).unwrap_or_default() else { continue };
                    let Some(field) = cell(r, "source").and_then(|t| this_chain(&t)).filter(|f| !f.contains('.') && plain(r)) else { continue };
                    let param = s.get("params").and_then(Value::as_array).and_then(|p| match p.as_slice() {
                        [one] => one.get("name").and_then(Value::as_str).map(str::to_string),
                        _ => None,
                    });
                    let copies = names.get(&field).and_then(|f| f.first()).and_then(|f| cell(f, "id"))
                        .and_then(|fid| writes.get(&fid))
                        .is_some_and(|ws| ws.iter().all(|w| cell(w, "member").as_deref() == Some(sid.as_str())
                            && cell(w, "operator").as_deref() == Some("=") && param.is_some() && cell(w, "source") == param));
                    if !copies { continue }
                    once(&sid, class).and_then(|a| this_chain(&cell(a, "source")?))
                }
                (Some(g), None, None) => {
                    let Some(gid) = cell(g, "id") else { continue };
                    match returns.get(&gid).map(Vec::as_slice).unwrap_or_default() {
                        [r] if plain(r) => cell(r, "source").and_then(|t| this_chain(&t)),
                        _ => None,
                    }
                }
                (None, None, Some(p)) => cell(p, "id").and_then(|id| once(&id, class)).and_then(|a| this_chain(&cell(a, "source")?)),
                _ => None,
            };
            if let Some(chain) = chain.filter(|c| c != name && !c.starts_with(&format!("{name}."))) {
                pairs.push((name.clone(), chain));
            }
        }
        if !pairs.is_empty() {
            own.insert(class.clone(), pairs);
        }
    }
    let mut base: IndexMap<String, String> = IndexMap::default();
    for c in store.table("classes").iter() {
        if let (Some(id), Some(b)) = (cell(c, "id"), c.get("extends").and_then(|e| e.get("id")).and_then(Value::as_str)) {
            base.insert(id, b.to_string());
        }
    }
    let mut out = IndexMap::default();
    for comp in store.table("components").iter() {
        let (Some(id), Some(class)) = (cell(comp, "id"), cell(comp, "class")) else { continue };
        // THE NEAREST DECLARATION OF A NAME WINS: a subclass declaring it again (any kind) hides the base's.
        let mut seen: IndexSet<String> = IndexSet::default();
        let mut pairs = Vec::new();
        let mut at = Some(class);
        let mut visited: IndexSet<String> = IndexSet::default();
        while let Some(c) = at {
            if !visited.insert(c.clone()) {
                break;
            }
            for (name, chain) in own.get(&c).into_iter().flatten() {
                if !seen.contains(name) {
                    pairs.push((name.clone(), chain.clone()));
                }
            }
            seen.extend(by_class.get(&c).into_iter().flat_map(|n| n.keys().cloned()));
            at = base.get(&c).cloned();
        }
        if !pairs.is_empty() {
            out.insert(id, Arc::new(pairs));
        }
    }
    out
}
