//! WHAT A VALUELESS STRUCTURAL DIRECTIVE RENDERS UNDER - read off its own code (divergence #130).
//!
//! `<div *demoShow>` has no expression, so its gate (#128) states nothing of its own: the condition is in the directive,
//! `if (this.env.isAlpha()) { this.vc.createEmbeddedView(this.tpl); } else { this.vc.clear(); }`. That `if` is a `branches`
//! row whose tree the readers already resolve for a getter (`gate_props`), so the gate is handed that tree - negated where
//! the view is created in the `else` (`*hideWhen`: render unless the test holds) - and `gate_values` reads it as it reads
//! `*ngIf="env.isAlpha()"`.
//!
//! ONLY WHAT EVERY RENDER SITE AGREES ON: each `createEmbeddedView` in the directive's class sits under the same top-level
//! branch, the same side of it. A site under no branch renders unconditionally, a nested one is not read, and two different
//! conditions are not merged - each leaves the gate as it was, a condition with no value.

use super::astreads::unwrap;
use super::gate_tsrows::TsRows;
use super::gate_values::EnumIndex;
use super::store::{Row, Store};
use crate::rows::{IndexMap, IndexSet};
use serde_json::{json, Value};

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Gate id -> the condition its directive creates the view under, resolved, for every valueless gate that has one.
pub fn render_conds(store: &Store<'_>, idx: &EnumIndex) -> IndexMap<String, Value> {
    let mut out = IndexMap::default();
    let gates = store.table("gates");
    let valueless: Vec<(String, String)> = gates.iter()
        .filter(|g| cell(g, "expression").is_none() && cell(g, "gate_kind").as_deref() == Some("structural"))
        .filter_map(|g| Some((cell(g, "id")?, cell(g, "node")?)))
        .collect();
    if valueless.is_empty() {
        return out;
    }
    let class_of: IndexMap<String, String> = store.table("directives").iter().filter_map(|d| Some((cell(d, "id")?, cell(d, "class")?))).collect();
    let nodes = store.table("template_nodes");
    let node_by_id: IndexMap<String, &Row> = nodes.iter().filter_map(|n| Some((cell(n, "id")?, n))).collect();
    let members = store.table("members");
    let class_of_member: IndexMap<String, String> = members.iter().filter_map(|m| Some((cell(m, "id")?, cell(m, "class")?))).collect();
    let branches = store.table("branches");
    let branch_by_id: IndexMap<String, &Row> = branches.iter().filter_map(|b| Some((cell(b, "id")?, b))).collect();
    // class -> every render site: (its member, its branch).
    let mut sites: IndexMap<String, Vec<(String, Option<String>)>> = IndexMap::default();
    for c in store.table("calls").iter() {
        if cell(c, "method").as_deref() != Some("createEmbeddedView") {
            continue;
        }
        let Some(member) = cell(c, "member") else { continue };
        if let Some(class) = class_of_member.get(&member) {
            sites.entry(class.clone()).or_default().push((member, cell(c, "branch")));
        }
    }
    let expressions = store.table("expressions");
    let expr_by_id: IndexMap<String, &Row> = expressions.iter().filter_map(|e| Some((cell(e, "id")?, e))).collect();
    let rows = TsRows::of(store);
    let none: IndexSet<String> = IndexSet::default();
    for (gate, node) in valueless {
        let Some(n) = node_by_id.get(&node) else { continue };
        let classes: Vec<&String> = n.get("directives").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|d| d.get("id").and_then(Value::as_str)).filter_map(|id| class_of.get(id)).collect();
        let [class] = classes.as_slice() else { continue };
        let Some(found) = sites.get(*class) else { continue };
        // (condition tree, sense, member) every site agrees on
        let mut agreed: Option<(String, String, String)> = None;
        let mut ok = true;
        for (member, branch) in found {
            let b = branch.as_ref().and_then(|b| branch_by_id.get(b));
            let read = b.filter(|b| cell(b, "parent").is_none()).and_then(|b| Some((cell(b, "condition_expr")?, cell(b, "sense")?, member.clone())));
            match (read, &agreed) {
                (Some(r), None) => agreed = Some(r),
                (Some(r), Some(a)) if r.0 == a.0 && r.1 == a.1 => {}
                _ => ok = false,
            }
        }
        let (true, Some((expr, sense, member))) = (ok, agreed) else { continue };
        if sense != "then" && sense != "else" {
            continue;
        }
        let Some(ast) = expr_by_id.get(&expr).and_then(|e| e.get("ast")).filter(|a| !a.is_null()) else { continue };
        let resolved = rows.resolve_in(unwrap(ast), &none, idx, Some(&member));
        out.insert(gate, if sense == "else" { json!({"k": "Not", "expr": resolved}) } else { resolved });
    }
    out
}
