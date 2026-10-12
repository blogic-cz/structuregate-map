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
//! tests that callee's parameters, which the site's arguments do not name. A condition naming a parameter the call does not
//! hand (a default, a rest) is no link: the way keeps no condition, as before.

use super::gate_features::Conds;
use super::gate_values::EnumIndex;
use super::key_branches::Branches;
use super::key_fields::{id_of, text};
use super::store::{Row, Store};
use indexmap::IndexSet;
use serde_json::Value;
use std::collections::HashMap;

/// The condition of every `arg:<branch>@<site>` link in `links`.
pub fn arg_conds<'l>(store: &Store<'_>, branches: &Branches, links: impl Iterator<Item = &'l String>, idx: &EnumIndex) -> Conds {
    let mut out = Conds::new();
    let links: IndexSet<&String> = links.filter(|l| l.starts_with("arg:")).collect();
    if links.is_empty() {
        return out;
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
    let ast = |id: &str| by_id.get(id).and_then(|r| r.get("ast")).filter(|a| !a.is_null()).map(|a| super::astreads::unwrap(a).clone());
    for link in links {
        let Some((branch, site)) = link.strip_prefix("arg:").and_then(|r| r.split_once('@')) else { continue };
        let Some((negated, tree, Some(callee))) = branches.condition(branch) else { continue };
        let (Some(cond), Some(call)) = (ast(tree), ast(site)) else { continue };
        if !matches!(call.get("k").and_then(Value::as_str), Some("Call" | "SafeCall")) {
            continue;
        }
        let args = call.get("args").and_then(Value::as_array).cloned().unwrap_or_default();
        let Some(names) = params.get(callee) else { continue };
        let mut missing = false;
        let replaced = substitute(&cond, names, &args, &mut missing);
        if missing {
            continue;
        }
        let caller = by_id.get(site).and_then(|r| text(r, "member"));
        let none = IndexSet::new();
        let resolved = rows.resolve_in(&replaced, inner.get(callee).unwrap_or(&none), idx, caller.as_deref());
        out.insert(link.clone(), (resolved, negated));
    }
    out
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
