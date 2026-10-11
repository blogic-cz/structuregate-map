//! A KEY THAT REACHES ITS TEMPLATE THROUGH A METHOD THE TEMPLATE CALLS - the call-site half of `key_fields`.
//!
//! `{{ label() }}` over `label() { return this.svc.single(); }` renders what `single` returns, exactly as the
//! getter `{{ label }}` does. A METHOD IS A SITE ONLY WHERE A TEMPLATE CALLS IT: one no template calls stays a
//! `literal` or a `ts_call`, since nothing says where its result renders. The join is the id the checker put on the
//! call's receiver (`Call.receiver.target.row`), never the name.
//!
//! EACH RETURN IS READ LIKE A GETTER'S (`reader.in_member`), with two differences:
//!   * a method's branches may test its PARAMETERS, which no template gate shares: a method WITH parameters
//!     carries no branch (and no `tpl:` link, which would name the parameter), one without carries them as a getter.
//!   * the call hands ARGUMENTS: a string literal, or a ternary of them, is bound into the holes naming the
//!     parameter as `key_follow::bind_args` does (`tip('alpha')` over `` `demo.tip.${n}` `` spells `demo.tip.alpha`
//!     alone, a call at another node its own keys). Any other argument leaves the hole open, as the callee reads it.

use super::astreads::{is_read, unwrap};
use super::key_branches::{member_ways, value_keys, Branches};
use super::key_fields::{id_of, text};
use super::key_follow::{bind_args, spellings, Returned};
use super::key_routes::FieldSite;
use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::Value;

/// A node's `gate_chain` as text.
pub(super) fn gates_of(n: &Row) -> Vec<String> {
    match n.get("gate_chain") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A template argument as the extractor's own statement rows spell one: a string, or a ternary of them.
fn arg_value(a: &Value) -> Value {
    match a.get("k").and_then(Value::as_str) {
        Some("Literal") => match a.get("v") {
            Some(Value::String(s)) => Value::String(s.clone()),
            _ => Value::Null,
        },
        Some("Cond") => {
            let (then, other) = (a.get("then").map(arg_value), a.get("else").map(arg_value));
            serde_json::json!({"$cond_expr": "template", "$then": then, "$else": other})
        }
        _ => Value::Null,
    }
}

/// Every call in a tree whose receiver is a read the checker resolved: (declaration row, arguments).
fn calls(v: &Value, out: &mut Vec<(String, Vec<Value>)>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| calls(i, out)),
        Value::Object(o) => {
            if matches!(o.get("k").and_then(Value::as_str), Some("Call" | "SafeCall")) {
                let row = o.get("receiver").filter(|r| is_read(r)).and_then(|r| r.get("target")?.get("row")?.as_str());
                if let Some(row) = row {
                    let args = o.get("args").and_then(Value::as_array).map(|a| a.iter().map(arg_value).collect());
                    out.push((row.to_string(), args.unwrap_or_default()));
                }
            }
            o.values().for_each(|x| calls(x, out));
        }
        _ => {}
    }
}

/// The keys one call of `method` hands its node, each with the ways it is returned in.
fn method_keys(method: &str, args: &[Value], reader: &Returned<'_, '_>, branches: &Branches) -> IndexMap<String, Vec<Vec<String>>> {
    let no_params = reader.params.get(method).is_none_or(|p| p.is_empty());
    let params = reader.params.get(method).map(Vec::as_slice).unwrap_or_default();
    let mut ways = IndexMap::new();
    for combo in spellings(args) {
        for r in reader.every.get(method).into_iter().flatten() {
            let Some(ret) = r.get("value") else { continue };
            let value = bind_args(ret, params, &combo);
            let base = if no_params { branches.chain_of(r) } else { Vec::new() };
            let mut found = Vec::new();
            value_keys(&value, &base, reader.keys, &mut found);
            let mut reached = Vec::new();
            reader.in_member(&value, method, &mut reached);
            for (key, chain) in reached {
                let mut way = base.clone();
                way.extend(chain.into_iter().filter(|link| !link.starts_with("tpl:")));
                found.push((key, way));
            }
            for (key, mut chain) in found {
                if !no_params {
                    chain.retain(|link| !link.starts_with("tpl:"));
                }
                member_ways(&mut ways, &key, chain);
            }
        }
    }
    ways
}

/// The sites the template's calls of a method prove, and the nodes that hold one.
pub(super) fn method_sites(store: &Store<'_>) -> (Vec<FieldSite>, usize) {
    let methods: std::collections::HashSet<String> = store
        .table("members")
        .iter()
        .filter(|m| text(m, "kind").as_deref() == Some("method"))
        .filter_map(|m| id_of(m, "id"))
        .collect();
    let mut out = Vec::new();
    if methods.is_empty() {
        return (out, 0);
    }
    let nodes = store.table("template_nodes");
    let node_by_id: std::collections::HashMap<String, &Row> = nodes.iter().filter_map(|n| id_of(n, "id").map(|id| (id, n))).collect();
    let branches = Branches::new(store);
    let mut hit_nodes = std::collections::HashSet::new();
    super::key_lists::with_reader(store, |reader| {
        for e in store.table("expressions").iter() {
            let Some(node_id) = id_of(e, "node") else { continue };
            let Some(n) = node_by_id.get(&node_id) else { continue };
            let Some(ast) = e.get("ast").filter(|a| !a.is_null()) else { continue };
            let mut found = Vec::new();
            calls(unwrap(ast), &mut found);
            let mut done = std::collections::HashSet::new();
            for (method, args) in found.into_iter().filter(|(m, _)| methods.contains(m)) {
                if !done.insert(format!("{method} {}", Value::from(args.clone()))) {
                    continue;
                }
                for (key, ways) in method_keys(&method, &args, reader, &branches) {
                    hit_nodes.insert(node_id.clone());
                    out.push(FieldSite { key, comp: text(n, "component"), node: Some(node_id.clone()), gates: gates_of(n), ways });
                }
            }
        }
    });
    (out, hit_nodes.len())
}
