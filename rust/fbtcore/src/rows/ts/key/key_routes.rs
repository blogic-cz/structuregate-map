//! WHERE A TRANSLATION KEY IS REACHED FROM, by the STRONGEST evidence available for it.
//!
//! The six routes are not equally strong, and taking only the narrowest one made thousands of
//! referenced keys look dead:
//!
//! | route             | what it is                                    |
//! |-------------------|-----------------------------------------------|
//! | `template`        | an `i18n_refs` carrier on a template node — a position, with real in-template gates |
//! | `template_string` | a string inside a template EXPRESSION — also a node, also gated |
//! | `field_binding`   | a template node binding a CLASS FIELD whose literal IS a key — also a node, also gated |
//! | `ts_call`         | a declared i18n-call method in TypeScript — no template position |
//! | `route_path`      | a key inside `{{ }}` in a string — a route path that IS a key |
//! | `literal`         | a string literal equal to a key — the file says it, nothing proves it is used AS one |

use super::key_branches::{literal_ways, value_keys, Branches};
use super::sourcescan::braced_names;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

/// THE FIRST THREE TIE. Ranking `template_string` BELOW a carrier lost render sites
/// rather than weak evidence: a key written as a carrier in one component and as a bound
/// string in another had the bound site MASKED, because the stronger route replaced the
/// site set instead of joining it. Checked against an independent scan of the same
/// source, the missing component sites were every one recoverable, no real
/// extraction gaps. The `route` column still names the STRONGEST evidence, and a carrier
/// still wins that label.
fn rank(route: &str) -> u8 {
    match route {
        "template" | "template_string" | "field_binding" => 4,
        // A PIPE TIES WITH `ts_call`, never above it: ranked with the template routes it REPLACED the
        // component a translation call sits in - a real site - and the key read as rendering only
        // where the pipe is applied. Keys lost the restriction their own component proves.
        "ts_call" | "route_path" | "pipe" => 2,
        "literal" => 1,
        _ => 0,
    }
}

/// A template node binding a class field whose literal is a key — see `key_fields`.
#[derive(Clone, Debug)]
pub struct FieldSite {
    pub key: String,
    pub comp: Option<String>,
    pub gates: Vec<String>,
    pub node: Option<String>,
    /// The branch chains the field is written under, one per write — see `key_branches`.
    pub ways: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct Route {
    /// The strongest evidence found for this key.
    pub route: String,
    /// Component -> one gate list PER REF. Keyed by component so the pairing survives:
    /// collected into one flat list the gates were unioned into every path of every
    /// component, and the intersection then picked up conditions from a site the path
    /// never passes through — `always_gates` claimed gates that do not always hold.
    pub sites: IndexMap<Option<String>, Vec<Vec<String>>>,
    pub refs: usize,
    pub nodes: IndexSet<String>,
}

struct Routes {
    inner: IndexMap<String, Route>,
}

impl Routes {
    fn new() -> Routes {
        Routes { inner: IndexMap::new() }
    }

    fn put(&mut self, key: &str, route: &str, comp: Option<String>, gates: Vec<String>, node: Option<&str>) {
        self.put_ways(key, route, comp, gates, &[], node);
    }

    /// ONE POSITION, AS MANY WAYS AS IT WAS WRITTEN IN: the site's own gates joined to each
    /// chain in turn. No chain at all is the one way with no condition of its own.
    fn put_ways(
        &mut self,
        key: &str,
        route: &str,
        comp: Option<String>,
        gates: Vec<String>,
        ways: &[Vec<String>],
        node: Option<&str>,
    ) {
        if key.is_empty() {
            return;
        }
        let incoming = rank(route);
        let existing = self.inner.get(key).map(|hit| rank(&hit.route));

        match existing {
            // A STRONGER ROUTE REPLACES what is there, sites and all: the weaker evidence
            // is not part of this key's answer any more.
            None => {
                self.inner.insert(key.to_string(), Route { route: route.to_string(), ..Default::default() });
            }
            Some(held) if incoming > held => {
                // `insert` on an existing key keeps its position, exactly as rebinding a
                // python dict entry does.
                self.inner.insert(key.to_string(), Route { route: route.to_string(), ..Default::default() });
            }
            Some(held) if incoming < held => return,
            Some(_) => {}
        }

        let hit = self.inner.get_mut(key).expect("just inserted or already present");

        // ONE NODE IS ONE POSITION, and the two render routes overlap almost completely —
        // nearly all keyed `i18n_refs` rows have a `template_strings` row on the
        // SAME node, because a carrier's key is also a string sitting in that carrier's
        // expression. Joined without this, every carrier ref would count twice and
        // `n_refs`/`n_paths` would double for no new way in.
        if let Some(node) = node.filter(|n| !n.is_empty()) {
            if hit.nodes.contains(node) {
                return;
            }
            hit.nodes.insert(node.to_string());
        }

        hit.refs += 1;
        let site = hit.sites.entry(comp).or_default();
        if ways.is_empty() {
            site.push(gates);
            return;
        }
        for way in ways {
            let mut one = gates.clone();
            one.extend(way.iter().filter(|b| !gates.contains(b)).cloned());
            site.push(one);
        }
    }
}

/// A cell that is a non-empty string, or nothing. Python's `if comp else None` treats an
/// empty string as absent, and so does this.
fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// A cell that is PRESENT and not null, whatever its type — python's `is None` test.
fn present(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// A gate chain, which is a list of gate ids or nothing at all.
fn gate_chain(row: &Row) -> Vec<String> {
    match row.get("gate_chain") {
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

pub fn key_routes(
    store: &Store<'_>,
    lit_gates: &std::collections::HashMap<String, Vec<String>>,
    fields: &[FieldSite],
) -> IndexMap<String, Route> {
    let mut routes = Routes::new();

    let nodes = store.table("template_nodes");
    let node_by_id: std::collections::HashMap<String, &Row> = nodes
        .iter()
        .filter_map(|n| present(n, "id").map(|id| (id, n)))
        .collect();

    let translations = store.table("translations");
    let keys: super::key_branches::KeySet = translations
        .iter()
        .filter_map(|t| text(t, "key"))
        .collect();

    // 1. A carrier on a template node.
    for r in store.table("i18n_refs").iter() {
        let (Some(key), Some(node)) = (text(r, "key"), present(r, "node")) else {
            continue;
        };
        let Some(n) = node_by_id.get(&node) else { continue };
        routes.put(&key, "template", text(n, "component"), gate_chain(n), Some(&node));
    }

    // 2. A string inside a template expression, on the same kind of node.
    for s in store.table("template_strings").iter() {
        let Some(node) = present(s, "node") else { continue };
        let Some(value) = text(s, "value") else { continue };
        if !keys.contains(&value) {
            continue;
        }
        let Some(n) = node_by_id.get(&node) else { continue };
        routes.put(&value, "template_string", text(n, "component"), gate_chain(n), Some(&node));
    }

    let components = store.table("components");
    let mut components_by_file: IndexMap<Option<String>, Vec<&Row>> = IndexMap::new();
    for c in components.iter() {
        components_by_file.entry(text(c, "file")).or_default().push(c);
    }

    // 3. A declared i18n call in TypeScript: no template position, so no gates - but the
    //    call runs only under the branches it sits in, and a getter returning
    //    `translate('k')` inside an `if` produces `k` only when that `if` holds. The ref names
    //    its own `calls` row, which names its branch and case.
    let branches = Branches::new(store);
    let calls = store.table("calls");
    let call_by_id: std::collections::HashMap<String, &Row> =
        calls.iter().filter_map(|c| present(c, "id").map(|id| (id, c))).collect();
    for r in store.table("i18n_refs").iter() {
        let Some(key) = text(r, "key") else { continue };
        if text(r, "source").as_deref() != Some("ts") {
            continue;
        }
        // A KEY CHOSEN IN THE ARGUMENT (`translate(c ? 'a' : 'b')`) takes its arm's condition on
        // top of the call's, exactly as a value does - see `key_branches::value_keys`.
        let mut ways: Vec<Vec<String>> = Vec::new();
        if let Some(call) = present(r, "call").and_then(|c| call_by_id.get(&c)) {
            let chain = branches.chain_of(call);
            let mut found = Vec::new();
            for arg in call.get("args").and_then(|a| a.as_array()).into_iter().flatten() {
                value_keys(arg, &chain, &keys, &mut found);
            }
            ways.extend(found.into_iter().filter(|(k, _)| *k == key).map(|(_, c)| c));
            if ways.is_empty() {
                ways.push(chain);
            }
        }
        if let Some(list) = components_by_file.get(&text(r, "file")) {
            for c in list {
                routes.put_ways(&key, "ts_call", text(c, "id"), Vec::new(), &ways, None);
            }
        }
    }

    // 4a. A key a PIPE's `transform` produces renders wherever a template applies that pipe:
    //     each such node is a position, under its own gates and the key's chain inside the
    //     transform. The pipe's file declares no component, so no other route gives it a site.
    //     Ranked with `ts_call` - see `rank`.
    let lit_ways = literal_ways(store, &branches);
    let mut pipe_of_file: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for p in store.table("pipes").iter() {
        if let (Some(file), Some(name)) = (text(p, "file"), text(p, "pipe_name")) {
            pipe_of_file.entry(file).or_default().insert(name);
        }
    }
    let mut uses: IndexMap<String, Vec<(&Row, String)>> = IndexMap::new();
    let expressions = store.table("expressions");
    for e in expressions.iter() {
        let Some(n) = present(e, "node").and_then(|x| node_by_id.get(&x).copied()) else { continue };
        for name in e.get("pipes").and_then(|p| p.as_array()).into_iter().flatten().filter_map(|v| v.as_str()) {
            uses.entry(name.to_string()).or_default().push((n, present(n, "id").unwrap_or_default()));
        }
    }
    for (pair, ways) in &lit_ways {
        let Some((key, file)) = pair.rsplit_once(' ') else { continue };
        for name in pipe_of_file.get(file).into_iter().flatten() {
            for (n, id) in uses.get(name).into_iter().flatten() {
                routes.put_ways(key, "pipe", text(n, "component"), gate_chain(n), ways, Some(id));
            }
        }
    }

    // 4. A field binding is a position, one hop past the literal.
    for f in fields {
        routes.put_ways(&f.key, "field_binding", f.comp.clone(), f.gates.clone(), &f.ways, f.node.as_deref());
    }

    // 5. A bare literal equal to a key. DISTINCT over (key, component): a key written
    //    twice in one file is one literal site.
    let literals = store.table("string_literals");
    let mut seen_literal: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in literals.iter() {
        let Some(value) = text(s, "value") else { continue };
        if !keys.contains(&value) {
            continue;
        }
        let file = text(s, "file");
        let ids: Vec<Option<String>> = match components_by_file.get(&file) {
            Some(list) if !list.is_empty() => list.iter().map(|c| text(c, "id")).collect(),
            _ => vec![None],
        };
        for cid in ids {
            let pair = format!("{} {}", value, cid.clone().unwrap_or_else(|| "None".to_string()));
            if !seen_literal.insert(pair.clone()) {
                continue;
            }
            // NOT an empty gate list: a `.ts` literal has no template node, but the
            // property its value flows into is read by one, and that gate is the only
            // condition standing between the key and the caller.
            let gates = lit_gates.get(&pair).cloned().unwrap_or_default();
            // THE BRANCHES THE LITERAL IS ASSIGNED UNDER, one way per place it sits in the file.
            let empty = Vec::new();
            let ways = file.as_ref().and_then(|f| lit_ways.get(&format!("{value} {f}"))).unwrap_or(&empty);
            routes.put_ways(&value, "literal", cid, gates, ways, None);
        }
    }
    // 5b. A key only a TEMPLATE LITERAL builds (`menu.${name}.label`) is the same
    //     evidence as a literal of it, in the file the template sits in.
    let literal_at: std::collections::HashSet<String> = literals
        .iter()
        .filter_map(|s| Some(format!("{} {}", text(s, "value")?, text(s, "file")?)))
        .collect();
    for (pair, ways) in &lit_ways {
        if literal_at.contains(pair) {
            continue;
        }
        let Some((key, file)) = pair.rsplit_once(' ') else { continue };
        let ids: Vec<Option<String>> = match components_by_file.get(&Some(file.to_string())) {
            Some(list) if !list.is_empty() => list.iter().map(|c| text(c, "id")).collect(),
            _ => vec![None],
        };
        for cid in ids {
            if seen_literal.insert(format!("{key} {}", cid.clone().unwrap_or_else(|| "None".to_string()))) {
                routes.put_ways(key, "literal", cid, Vec::new(), ways, None);
            }
        }
    }

    // 6. A route path MAY BE a translation key, by the convention `{{routes.auth.login}}`
    //    as the whole `path`. Those keys live in no template and in no bare literal, so
    //    all of them read as unreferenced until this join exists.
    for s in literals.iter() {
        let Some(value) = text(s, "value") else { continue };
        if !value.contains("{{") {
            continue;
        }
        let empty = Vec::new();
        let comps = components_by_file.get(&text(s, "file")).unwrap_or(&empty);
        for name in braced_names(&value) {
            if !keys.contains(&name) {
                continue;
            }
            if comps.is_empty() {
                routes.put(&name, "route_path", None, Vec::new(), None);
            }
            for c in comps {
                routes.put(&name, "route_path", text(c, "id"), Vec::new(), None);
            }
        }
    }

    let mut component_by_class: IndexMap<Option<String>, Option<String>> = IndexMap::new();
    for c in components.iter() {
        component_by_class.entry(text(c, "class")).or_insert_with(|| text(c, "id"));
    }
    for r in store.table("routes").iter() {
        let Some(path) = text(r, "path") else { continue };
        if !path.contains("{{") {
            continue;
        }
        for name in braced_names(&path) {
            if !keys.contains(&name) {
                continue;
            }
            let c = match present(r, "component_id") {
                Some(id) => component_by_class.get(&Some(id)).cloned().flatten(),
                None => None,
            };
            routes.put(&name, "route_path", c, Vec::new(), None);
        }
    }

    routes.inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};
    use std::collections::HashMap;

    fn store_of(tables: serde_json::Value) -> Map<String, Value> {
        tables.as_object().cloned().expect("an object of tables")
    }

    fn run(tables: serde_json::Value, fields: &[FieldSite]) -> IndexMap<String, Route> {
        let map = store_of(tables);
        let store = Store::from_payload(map, "typescript");
        key_routes(&store, &HashMap::new(), fields)
    }

    /// One key, reached as a carrier on a node inside component `ng:1`.
    fn carrier_tables() -> serde_json::Value {
        json!({
            "translations": [{"key": "a.b"}],
            "template_nodes": [{"id": "n:1", "component": "ng:1", "gate_chain": ["g:1"]}],
            "i18n_refs": [{"key": "a.b", "node": "n:1"}],
            "components": [{"id": "ng:1", "file": "f:1", "class": "c:1"}],
        })
    }

    #[test]
    fn a_carrier_on_a_node_is_a_template_route_carrying_that_nodes_gates() {
        let routes = run(carrier_tables(), &[]);
        let hit = &routes["a.b"];
        assert_eq!(hit.route, "template");
        assert_eq!(hit.refs, 1);
        assert_eq!(hit.sites[&Some("ng:1".to_string())], vec![vec!["g:1".to_string()]]);
    }

    #[test]
    fn one_node_is_one_position_however_many_routes_find_it() {
        // Nearly all keyed `i18n_refs` rows have a `template_strings` row on the
        // SAME node, because a carrier's key is also a string in that carrier's
        // expression. Counted twice, `n_refs` and `n_paths` double for no new way in.
        let mut tables = carrier_tables();
        tables["template_strings"] = json!([{"node": "n:1", "value": "a.b"}]);
        let routes = run(tables, &[]);
        assert_eq!(routes["a.b"].refs, 1, "the same node must not be counted twice");
        assert_eq!(routes["a.b"].sites[&Some("ng:1".to_string())].len(), 1);
    }

    #[test]
    fn a_string_on_a_different_node_is_a_second_way_in() {
        let mut tables = carrier_tables();
        tables["template_nodes"] = json!([
            {"id": "n:1", "component": "ng:1", "gate_chain": ["g:1"]},
            {"id": "n:2", "component": "ng:1", "gate_chain": ["g:2"]},
        ]);
        tables["template_strings"] = json!([{"node": "n:2", "value": "a.b"}]);
        let routes = run(tables, &[]);
        assert_eq!(routes["a.b"].refs, 2);
        assert_eq!(
            routes["a.b"].sites[&Some("ng:1".to_string())],
            vec![vec!["g:1".to_string()], vec!["g:2".to_string()]]
        );
    }

    #[test]
    fn the_three_render_routes_tie_so_neither_masks_the_other() {
        // Ranking `template_string` below a carrier lost component sites across many
        // keys: the stronger route REPLACED the site set instead of joining it.
        let mut tables = carrier_tables();
        tables["template_nodes"] = json!([
            {"id": "n:1", "component": "ng:1", "gate_chain": ["g:1"]},
            {"id": "n:2", "component": "ng:2", "gate_chain": ["g:2"]},
        ]);
        tables["template_strings"] = json!([{"node": "n:2", "value": "a.b"}]);
        let routes = run(tables, &[]);
        assert_eq!(routes["a.b"].route, "template", "a carrier still wins the LABEL");
        assert_eq!(routes["a.b"].sites.len(), 2, "but both components keep their site");
    }

    #[test]
    fn a_stronger_route_replaces_a_weaker_ones_sites_entirely() {
        // A bare literal is weak evidence. When a real position turns up, the literal's
        // site is not part of the answer any more.
        let tables = json!({
            "translations": [{"key": "a.b"}],
            "template_nodes": [{"id": "n:1", "component": "ng:1", "gate_chain": ["g:1"]}],
            "i18n_refs": [{"key": "a.b", "node": "n:1"}],
            "components": [{"id": "ng:1", "file": "f:9", "class": "c:1"}],
            "string_literals": [{"value": "a.b", "file": "f:9"}],
        });
        let routes = run(tables, &[]);
        assert_eq!(routes["a.b"].route, "template");
        assert_eq!(routes["a.b"].refs, 1, "the literal did not add a ref");
    }

    #[test]
    fn a_weaker_route_arriving_later_changes_nothing() {
        let mut tables = carrier_tables();
        // A ts_call ranks 2, below the carrier's 4.
        tables["i18n_refs"] = json!([
            {"key": "a.b", "node": "n:1"},
            {"key": "a.b", "source": "ts", "file": "f:1"},
        ]);
        let routes = run(tables, &[]);
        assert_eq!(routes["a.b"].route, "template");
        assert_eq!(routes["a.b"].refs, 1);
    }

    #[test]
    fn a_field_binding_ties_with_the_template_routes() {
        let fields = [FieldSite {
            key: "a.b".to_string(),
            comp: Some("ng:2".to_string()),
            gates: vec!["g:7".to_string()],
            node: Some("n:9".to_string()),
            ways: Vec::new(),
        }];
        let routes = run(carrier_tables(), &fields);
        assert_eq!(routes["a.b"].sites.len(), 2, "the field site joins rather than masks");
        assert_eq!(routes["a.b"].refs, 2);
    }

    #[test]
    fn a_key_no_translation_defines_is_not_reached_by_a_bare_string() {
        // `template_strings` and `string_literals` only count when the value IS a key.
        let tables = json!({
            "translations": [{"key": "a.b"}],
            "template_nodes": [{"id": "n:1", "component": "ng:1", "gate_chain": []}],
            "template_strings": [{"node": "n:1", "value": "not.a.key"}],
            "components": [{"id": "ng:1", "file": "f:1", "class": "c:1"}],
        });
        assert!(run(tables, &[]).is_empty());
    }

    #[test]
    fn a_route_path_is_read_out_of_its_braces() {
        // `{{routes.auth.login}}` is the whole path, and those keys live in no template
        // and no bare literal — all of them read as unreferenced without this join.
        let tables = json!({
            "translations": [{"key": "routes.auth.login"}],
            "components": [],
            "routes": [{"path": "{{routes.auth.login}}", "component_id": null}],
        });
        let routes = run(tables, &[]);
        assert_eq!(routes["routes.auth.login"].route, "route_path");
        assert!(routes["routes.auth.login"].sites.contains_key(&None));
    }

    #[test]
    fn one_literal_per_key_and_component_however_often_the_file_repeats_it() {
        let tables = json!({
            "translations": [{"key": "a.b"}],
            "components": [{"id": "ng:1", "file": "f:1", "class": "c:1"}],
            "string_literals": [{"value": "a.b", "file": "f:1"}, {"value": "a.b", "file": "f:1"}],
        });
        let routes = run(tables, &[]);
        assert_eq!(routes["a.b"].refs, 1, "a key written twice in one file is one site");
    }

    #[test]
    fn a_literal_in_a_file_no_component_owns_still_reaches_the_key() {
        let tables = json!({
            "translations": [{"key": "a.b"}],
            "components": [],
            "string_literals": [{"value": "a.b", "file": "f:7"}],
        });
        let routes = run(tables, &[]);
        assert_eq!(routes["a.b"].route, "literal");
        assert!(routes["a.b"].sites.contains_key(&None), "no component is a site of its own");
    }
}
