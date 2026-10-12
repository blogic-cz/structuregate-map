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

use super::key_arms::template_arms;
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
    /// Per component: the rank of the route its sites came from, and what they cost (`put_ways`).
    held: IndexMap<Option<String>, Held>,
}

#[derive(Clone, Debug, Default)]
struct Held {
    rank: u8,
    refs: usize,
    nodes: Vec<String>,
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
        let hit = self.inner.entry(key.to_string()).or_default();
        // THE ROUTE LABEL IS THE STRONGEST PRESENT; THE SITES ARE EVERY COMPONENT'S OWN. A stronger route used to
        // REPLACE the key's sites whole, so one component's field binding hid every other component that reached the
        // same key by a `literal` or a `ts_call`, and the key read as reaching one component of five. Now a COMPONENT
        // keeps the sites of the strongest route that reaches IT (signed-off divergence #18): a weaker route's site in a
        // component a stronger one already reaches is dropped, so its empty gate list never dilutes the position's own
        // gates in `always_gates`, and a component only a weaker route reaches stays.
        if incoming > rank(&hit.route) || hit.route.is_empty() {
            hit.route = route.to_string();
        }
        let held = hit.held.entry(comp.clone()).or_default();
        if held.rank > incoming {
            return;
        }
        if held.rank < incoming {
            for node in held.nodes.drain(..) {
                hit.nodes.shift_remove(&node);
            }
            hit.refs -= held.refs;
            held.refs = 0;
            held.rank = incoming;
            hit.sites.shift_remove(&comp);
        }

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
            held.nodes.push(node.to_string());
        }

        hit.refs += 1;
        held.refs += 1;
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

    // A KEY THE EXPRESSION CHOOSES holds the arm it sits in, one way per literal of it - see
    // `key_arms::template_arms`. No literal of it found is the one way with no arm.
    let expressions = store.table("expressions");
    let ast_of: std::collections::HashMap<String, &Value> = expressions
        .iter()
        .filter_map(|e| Some((present(e, "id")?, e.get("ast")?)))
        .collect();
    let arm_ways = |row: &Row, key: &str| -> Vec<Vec<String>> {
        let Some((expr, ast)) = present(row, "expression").and_then(|x| ast_of.get(&x).map(|a| (x, *a))) else {
            return Vec::new();
        };
        let mut ways: Vec<Vec<String>> = Vec::new();
        for (_, chain) in template_arms(&expr, ast).0.into_iter().filter(|(v, _)| v == key) {
            if !ways.contains(&chain) {
                ways.push(chain);
            }
        }
        ways
    };

    // 1. A carrier on a template node.
    for r in store.table("i18n_refs").iter() {
        let (Some(key), Some(node)) = (text(r, "key"), present(r, "node")) else {
            continue;
        };
        let Some(n) = node_by_id.get(&node) else { continue };
        routes.put_ways(&key, "template", text(n, "component"), gate_chain(n), &arm_ways(r, &key), Some(&node));
    }

    // 2. A string inside a template expression, on the same kind of node.
    for s in store.table("template_strings").iter() {
        let Some(node) = present(s, "node") else { continue };
        let Some(value) = text(s, "value") else { continue };
        if !keys.contains(&value) {
            continue;
        }
        let Some(n) = node_by_id.get(&node) else { continue };
        routes.put_ways(&value, "template_string", text(n, "component"), gate_chain(n), &arm_ways(s, &value), Some(&node));
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
        if text(r, "source").as_deref() != Some("ts") {
            continue;
        }
        // A DICTIONARY CALL (`i18nDictionaryCalls`) CARRIES A PREFIX AND NO KEY: it hands back every locale key under
        // `<prefix>.`, so each of them is reached at this call exactly as a single-key call would reach it - the same
        // component sites, the call's own branch chain. Expanded here, against the locale keys, so a key added later
        // needs no re-extraction. A prefix no key sits under reaches nothing.
        let under: Vec<String> = match (text(r, "key"), text(r, "prefix")) {
            (Some(key), _) => vec![key],
            (None, Some(prefix)) => keys.starting_with(&format!("{prefix}.")).to_vec(),
            (None, None) => continue,
        };
        for key in under {
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
#[path = "key_routes_tests.rs"]
mod tests;
