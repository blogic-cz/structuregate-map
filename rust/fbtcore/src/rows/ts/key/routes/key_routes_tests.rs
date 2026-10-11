//! The unit tests of `key_routes`.

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
    // A bare literal is weak evidence. When a real position turns up IN THE SAME COMPONENT, the literal's
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
fn a_stronger_route_in_one_component_keeps_the_components_a_weaker_route_reaches() {
    // Divergence #18: the label is the strongest route, the sites are every component's own.
    let tables = json!({
        "translations": [{"key": "a.b"}],
        "template_nodes": [{"id": "n:1", "component": "ng:1", "gate_chain": ["g:1"]}],
        "i18n_refs": [{"key": "a.b", "node": "n:1"}],
        "components": [{"id": "ng:2", "file": "f:9", "class": "c:2"}],
        "string_literals": [{"value": "a.b", "file": "f:9"}],
    });
    let routes = run(tables, &[]);
    assert_eq!(routes["a.b"].route, "template");
    assert_eq!(routes["a.b"].sites.len(), 2, "the literal's component stays");
    assert_eq!(routes["a.b"].refs, 2);
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
