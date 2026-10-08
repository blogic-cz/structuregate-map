//! A KEY THAT REACHES ITS TEMPLATE THROUGH A CLASS FIELD — the render site that is a
//! template NODE, reached one hop later than a carrier.
//!
//! A component may name its translation key in TypeScript and bind the FIELD:
//! `hintKey = 'profile.nameHint'` with
//! `<app-hint [text]="hintKey">`. Both halves are rows the map already
//! resolved, and the `literal` route already reaches the component — but at rank 1, so a
//! key that is ALSO written as a carrier somewhere else loses this site to the stronger
//! route, and `n_paths=1` with `truncated=0` then reports one path and hides the other.
//!
//! IT IS A RANK-4 ROUTE, NOT A UNION ACROSS RANKS. The site published here is a template
//! NODE with its own `gate_chain` — the same thing that makes `template` and
//! `template_string` tie — so it belongs at their rank. Loosening the rank comparison
//! instead would also admit the bare `literal` route, whose site is a `.ts` file that
//! merely CONTAINS the key and proves no render position at all. Admitting those would ADD
//! paths to the intersection `always_gates` is computed over, and dropping a real
//! condition from `always_gates` is the failure this schema exists to prevent.
//!
//! EVERY HOP IS AN ID JOIN, never a name match:
//!   * `members.value` / `assignments.value` — the literal the field holds, matched to a key
//!   * `assignments.target_id` — the field it is written to, resolved by the checker
//!   * `expressions.ast` → `Read.target.row` — the template's own read of THAT declaration
//!   * `template_nodes.gate_chain` — the site's real conditions

use super::astreads::{read_rows, unwrap};
use super::key_branches::{member_ways, value_keys, Branches};
use super::key_routes::FieldSite;
use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::Value;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub fields: usize,
    pub nodes: usize,
    pub sites: usize,
}

fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn id_of(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// Member id -> the translation keys its literal writes can hold, each with the WAYS it is
/// written in, for the fields whose keys are written from INSIDE the class that declares
/// them. A way is the chain of branches a write sits in — see `key_branches`.
///
/// BOTH KINDS OF WRITE COUNT. The initializer is on the `members` row and the branch
/// writes are `assignments` rows: the field behind the measured case is written several times
/// — the class-level default plus one in each arm of a method's switch —
/// and reading only one kind would publish a site for some of a field's keys and not the
/// others.
///
/// A FIELD WRITTEN FROM OUTSIDE ITS CLASS IS NOT A SITE FOR THE KEYS IT RECEIVES, and this
/// is the whole reason the census is per class. `DialogComponent.okLabel` is
/// assigned dozens of times from dozens of OTHER classes, carrying many distinct keys, and the modal binds
/// it at ONE node whose `gate_chain` is the modal shell's own `*ngIf`s. Pairing each of
/// those keys with that node published keys whose `always_gates` were the shell's
/// near-vacuous condition instead of the step that opens the modal — the real restriction
/// gone. The key travels IN as data, so the binding's position says nothing about which key
/// is in the field when it renders. Nearly all fields are written only from inside;
/// the few that are not are dropped whole, keys and all.
fn keys_by_member(store: &Store<'_>) -> IndexMap<String, IndexMap<String, Vec<Vec<String>>>> {
    let keys: super::key_branches::KeySet = store
        .table("translations")
        .iter()
        .filter_map(|t| text(t, "key"))
        .collect();

    let branches = Branches::new(store);
    let mut owner: IndexMap<String, Option<String>> = IndexMap::new();
    let mut out: IndexMap<String, IndexMap<String, Vec<Vec<String>>>> = IndexMap::new();
    let mut foreign: Vec<String> = Vec::new();

    // A VALUE IS READ FOR EVERY KEY IT CAN BE, a ternary's and a logical operator's arms
    // included, each with the choice that picks it - see `key_branches::value_keys`.
    let members = store.table("members");
    let mut getters: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in members.iter() {
        let Some(id) = id_of(m, "id").filter(|i| !i.is_empty()) else { continue };
        owner.insert(id.clone(), text(m, "class"));
        if text(m, "kind").as_deref() == Some("getter") {
            getters.insert(id.clone());
        }
        let mut found = Vec::new();
        value_keys(m.get("value").unwrap_or(&Value::Null), &[], &keys, &mut found);
        for (key, chain) in found {
            member_ways(out.entry(id.clone()).or_default(), &key, chain);
        }
    }

    let assignments = store.table("assignments");
    for a in assignments.iter() {
        let Some(target) = id_of(a, "target_id").filter(|t| !t.is_empty()) else { continue };
        let mut found = Vec::new();
        value_keys(a.get("value").unwrap_or(&Value::Null), &branches.chain_of(a), &keys, &mut found);
        if found.is_empty() || !owner.contains_key(&target) {
            continue;
        }
        let writer = text(a, "class");
        if writer.is_none() || writer != *owner.get(&target).expect("checked above") {
            foreign.push(target.clone());
        }
        for (key, chain) in found {
            member_ways(out.entry(target.clone()).or_default(), &key, chain);
        }
    }

    // A GETTER IS A FIELD WHOSE WRITES ARE ITS RETURNS: `get tip() { if (a) return 'k'; ... }`
    // binds at the node that reads `tip`, under the branch the return sits in. Only a getter
    // - a method's returns may test its parameters, and a parameter is not a dimension any
    // gate shares.
    for r in store.table("returns").iter() {
        let Some(member) = id_of(r, "member").filter(|m| getters.contains(m)) else { continue };
        let mut found = Vec::new();
        value_keys(r.get("value").unwrap_or(&Value::Null), &branches.chain_of(r), &keys, &mut found);
        for (key, chain) in found {
            member_ways(out.entry(member.clone()).or_default(), &key, chain);
        }
    }

    for member in foreign {
        out.shift_remove(&member);
    }
    out
}

/// The sites a field binding proves.
///
/// A node is reported once per key it can hold. The stats are returned for the closure
/// build to print, because a chain that silently stops matching looks exactly like a
/// frontend that stopped binding fields.
pub fn field_sites(store: &Store<'_>) -> (Vec<FieldSite>, Stats) {
    let by_member = keys_by_member(store);
    let mut out = Vec::new();
    let mut stats = Stats { fields: by_member.len(), nodes: 0, sites: 0 };
    if by_member.is_empty() {
        return (out, stats);
    }

    let nodes = store.table("template_nodes");
    let node_by_id: std::collections::HashMap<String, &Row> = nodes
        .iter()
        .filter_map(|n| id_of(n, "id").map(|id| (id, n)))
        .collect();

    for e in store.table("expressions").iter() {
        let Some(node_id) = id_of(e, "node") else { continue };
        let Some(n) = node_by_id.get(&node_id) else { continue };
        let Some(ast) = e.get("ast").filter(|a| !a.is_null()) else { continue };
        if matches!(ast, Value::String(s) if s.is_empty()) {
            continue;
        }

        let gates: Vec<String> = match n.get("gate_chain") {
            Some(Value::Array(items)) => items
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect(),
            _ => Vec::new(),
        };

        let mut hit = false;
        for row in read_rows(unwrap(ast)) {
            let Some(keys) = by_member.get(&row) else { continue };
            if keys.is_empty() {
                continue;
            }
            hit = true;
            for (key, ways) in keys {
                out.push(FieldSite {
                    key: key.clone(),
                    comp: text(n, "component"),
                    node: id_of(n, "id"),
                    gates: gates.clone(),
                    ways: ways.clone(),
                });
                stats.sites += 1;
            }
        }
        if hit {
            stats.nodes += 1;
        }
    }
    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn run(tables: Value) -> (Vec<FieldSite>, Stats) {
        let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
        let store = Store::from_payload(map, "typescript");
        field_sites(&store)
    }

    /// A field declared and written inside its own class, bound at one template node.
    fn own_class() -> Value {
        json!({
            "translations": [{"key": "a.tooltip"}],
            "members": [{"id": "m:1", "class": "c:1", "value": "a.tooltip"}],
            "template_nodes": [{"id": "n:1", "component": "ng:1", "gate_chain": ["g:1"]}],
            "expressions": [{"node": "n:1", "ast": {"k": "Read", "target": {"row": "m:1"}}}],
        })
    }

    #[test]
    fn a_field_bound_in_a_template_publishes_that_nodes_site() {
        let (sites, stats) = run(own_class());
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].key, "a.tooltip");
        assert_eq!(sites[0].comp.as_deref(), Some("ng:1"));
        assert_eq!(sites[0].node.as_deref(), Some("n:1"));
        assert_eq!(sites[0].gates, vec!["g:1".to_string()]);
        assert_eq!(stats, Stats { fields: 1, nodes: 1, sites: 1 });
    }

    #[test]
    fn both_kinds_of_write_count() {
        // The class-level default plus a branch write: reading only one kind would publish
        // a site for some of a field's keys and not the others.
        let mut tables = own_class();
        tables["translations"] = json!([{"key": "a.tooltip"}, {"key": "b.tooltip"}]);
        tables["assignments"] = json!([{"target_id": "m:1", "class": "c:1", "value": "b.tooltip"}]);
        let (sites, _) = run(tables);
        let keys: Vec<&str> = sites.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec!["a.tooltip", "b.tooltip"]);
    }

    #[test]
    fn a_field_written_from_another_class_is_dropped_whole() {
        // DialogComponent.okLabel is assigned dozens of times from other classes and
        // bound at ONE node. Pairing those keys with that node published keys whose
        // always_gates were the modal shell's near-vacuous condition instead of the step
        // that opens it - the real restriction gone.
        let mut tables = own_class();
        tables["assignments"] = json!([{"target_id": "m:1", "class": "c:OTHER", "value": "a.tooltip"}]);
        let (sites, stats) = run(tables);
        assert!(sites.is_empty(), "the field is not a site for keys that travel in as data");
        assert_eq!(stats.fields, 0);
    }

    #[test]
    fn an_assignment_with_no_class_at_all_also_drops_the_field() {
        let mut tables = own_class();
        tables["assignments"] = json!([{"target_id": "m:1", "value": "a.tooltip"}]);
        assert!(run(tables).0.is_empty());
    }

    #[test]
    fn a_same_named_field_on_another_class_cannot_contribute() {
        // The join is on the declaration row the checker resolved, never on a name.
        let mut tables = own_class();
        tables["expressions"] = json!([{"node": "n:1", "ast": {"k": "Read", "target": {"row": "m:999"}}}]);
        let (sites, stats) = run(tables);
        assert!(sites.is_empty());
        assert_eq!(stats.nodes, 0, "no node matched, and the stats say so");
    }

    #[test]
    fn a_value_that_is_not_a_declared_key_is_not_one() {
        let mut tables = own_class();
        tables["members"] = json!([{"id": "m:1", "class": "c:1", "value": "not.a.key"}]);
        assert!(run(tables).0.is_empty());
    }

    #[test]
    fn an_expression_on_no_node_proves_no_render_position() {
        let mut tables = own_class();
        tables["expressions"] = json!([{"ast": {"k": "Read", "target": {"row": "m:1"}}}]);
        assert!(run(tables).0.is_empty());
    }

    #[test]
    fn each_arm_of_a_ternary_write_is_a_key_under_its_own_polarity() {
        let mut tables = own_class();
        tables["translations"] = json!([{"key": "a.tooltip"}, {"key": "b.tooltip"}]);
        tables["members"] = json!([{"id": "m:1", "class": "c:1"}]);
        tables["assignments"] = json!([{"target_id": "m:1", "class": "c:1",
            "value": {"$cond_expr": "x:9", "$then": "a.tooltip", "$else": "b.tooltip"}}]);
        let (sites, _) = run(tables);
        let got: Vec<(String, Vec<Vec<String>>)> = sites.iter().map(|s| (s.key.clone(), s.ways.clone())).collect();
        assert_eq!(got, vec![("a.tooltip".to_string(), vec![vec!["x:9".to_string()]]),
                             ("b.tooltip".to_string(), vec![vec!["!x:9".to_string()]])]);
    }

    #[test]
    fn a_getters_returns_are_its_writes_and_a_methods_are_not() {
        let mut tables = own_class();
        tables["members"] = json!([{"id": "m:1", "class": "c:1", "kind": "getter"}]);
        tables["branches"] = json!([{"id": "br:1", "sense": "then"}]);
        tables["returns"] = json!([{"member": "m:1", "value": "a.tooltip", "branch": "br:1"}]);
        let (sites, _) = run(tables.clone());
        assert_eq!(sites[0].ways, vec![vec!["br:1".to_string()]]);
        tables["members"][0]["kind"] = json!("method");
        assert!(run(tables).0.is_empty());
    }

    #[test]
    fn a_node_holding_two_keys_reports_one_site_per_key() {
        let mut tables = own_class();
        tables["translations"] = json!([{"key": "a.tooltip"}, {"key": "b.tooltip"}]);
        tables["assignments"] = json!([{"target_id": "m:1", "class": "c:1", "value": "b.tooltip"}]);
        let (sites, stats) = run(tables);
        assert_eq!(stats.sites, 2);
        assert_eq!(stats.nodes, 1, "one node, however many keys it can hold");
        assert!(sites.iter().all(|s| s.node.as_deref() == Some("n:1")));
    }
}
