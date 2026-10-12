//! A LOOP ITEM HANDED TO A METHOD IS READ WHERE THE METHOD READS ITS PARAMETER.
//!
//! `<b *ngFor="let b of buttons" [title]="tipOf(b, row)">` over `tipOf(button, row) { return button.tooltip.text(row); }`
//! renders what `b.tooltip.text` holds, exactly as `[title]="b.tooltip.text(row)"` would: the template hands the element
//! over, and the method reads its property. The paths are the property reads the extractor recorded (`$reads`) on the
//! method's own rows that start at the parameter - `button.tooltip.text` is `tooltip.text` - so no source text is parsed.
//! The site is the node that CALLS the method (`key_loops`), with that node's `gate_chain`.

use super::key_fields::id_of;
use super::store::Store;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// (method id, parameter position) -> the property paths the method reads off that parameter.
pub(super) type Handed = HashMap<(String, usize), Vec<String>>;

pub(super) fn handed(store: &Store<'_>, params: &HashMap<String, Vec<String>>) -> Handed {
    fn reads(v: &Value, out: &mut HashSet<String>) {
        match v {
            Value::Object(o) => {
                o.get("$reads").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).for_each(|r| {
                    out.insert(r.to_string());
                });
                o.values().for_each(|x| reads(x, out));
            }
            Value::Array(xs) => xs.iter().for_each(|x| reads(x, out)),
            _ => {}
        }
    }
    let mut by_member: HashMap<String, HashSet<String>> = HashMap::new();
    for table in ["returns", "locals", "assignments", "calls"] {
        for r in store.table(table).iter() {
            let Some(member) = id_of(r, "member").filter(|m| params.contains_key(m)) else { continue };
            let found = by_member.entry(member).or_default();
            r.get("value").into_iter().chain(r.get("args")).for_each(|v| reads(v, found));
        }
    }
    let mut out = Handed::new();
    for (member, found) in by_member {
        for (i, param) in params[&member].iter().enumerate() {
            let prefix = format!("{param}.");
            let mut paths: Vec<String> = found
                .iter()
                .filter_map(|r| r.strip_prefix(&prefix))
                .filter(|p| p.split('.').all(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')))
                .map(str::to_string)
                .collect();
            if !paths.is_empty() {
                paths.sort();
                out.insert((member.clone(), i), paths);
            }
        }
    }
    out
}

/// The positions at which a template call hands over the loop variable itself (a bare read no declaration resolved), and
/// the method the call's receiver resolves to.
pub(super) fn handed_at(call: &Value, is_variable: impl Fn(&Value) -> bool) -> Option<(String, Vec<usize>)> {
    if !matches!(call.get("k").and_then(Value::as_str), Some("Call" | "SafeCall")) {
        return None;
    }
    let method = call.get("receiver")?.get("target")?.get("row")?.as_str()?.to_string();
    let at: Vec<usize> = call.get("args")?.as_array()?.iter().enumerate().filter(|(_, a)| is_variable(a)).map(|(i, _)| i).collect();
    (!at.is_empty()).then_some((method, at))
}
