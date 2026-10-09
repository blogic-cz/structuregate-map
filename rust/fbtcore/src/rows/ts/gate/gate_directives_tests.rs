//! The tests of `gate_directives.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `gate_directives::tests`, so `super::*` is `gate_directives`.

use super::*;
use super::super::gate_values::{enum_index, member_enums};
use serde_json::Map;

/// The whole shape one directive is read out of: a class binding a constant in its
/// constructor, a base class comparing the field, an `@Input` setter whose config type
/// declares a member of the same enum, and a structural gate whose selector names it.
fn tables(over: Value) -> Map<String, Value> {
    let mut t: Map<String, Value> = serde_json::from_value(json!({
        "files": [{"id": "f:1", "path": "app/vis.directive.ts"}],
        "enums": [{"id": "e:1", "name": "Vendor", "file": "f:1",
                   "members": [{"name": "Alpha"}, {"name": "Other"}]}],
        "classes": [
            {"id": "c:sub", "name": "AlphaVisible", "file": "f:1", "line": 10,
             "end_line": 20, "extends": {"name": "VisibleBase", "file": "C:/work/fe/app/vis.directive.ts"}},
            {"id": "c:base", "name": "VisibleBase", "file": "f:1", "line": 30, "end_line": 60}
        ],
        "members": [
            {"id": "m:field", "class": "c:base", "name": "vendorType", "line": 31,
             "file": "f:1", "type": "Vendor",
             "type_ref": {"name": "Vendor", "file": "C:/work/fe/app/vis.directive.ts"}},
            {"id": "m:other", "class": "c:base", "name": "selected", "line": 32,
             "file": "f:1", "type": "Vendor",
             "type_ref": {"name": "Vendor", "file": "C:/work/fe/app/vis.directive.ts"}},
            {"id": "m:set", "class": "c:base", "name": "config", "kind": "setter",
             "line": 40, "file": "f:1", "params": [{"type": "VisConfig"}]}
        ],
        "interfaces": [{"id": "i:1", "name": "VisConfig"}],
        "type_members": [{"id": "tm:1", "owner": "i:1", "name": "vendorType",
                          "type": "Vendor",
                          "type_ref": {"name": "Vendor", "file": "C:/work/fe/app/vis.directive.ts"}}],
        "assignments": [{"class": "c:sub", "target_id": "m:field",
                         "value": {"$enum": "Vendor.Alpha"}}],
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 45, "ast":
             {"k": "Binary", "op": "===",
              "left": {"k": "Read", "target": {"name": "vendorType",
                       "file": "C:/work/fe/app/vis.directive.ts", "line": 31}},
              "right": {"k": "Read", "target": {"name": "selected",
                        "file": "C:/work/fe/app/vis.directive.ts", "line": 32}}}},
            {"id": "x:gate", "file": "f:1", "line": 1, "ast":
             {"k": "Map", "keys": [{"key": "vendorType"}]}}
        ],
        "selector_index": [{"selector": "[if-kind]", "class": "c:sub"}],
        "gates": [{"id": "g:1", "name": "if-kind", "gate_kind": "structural",
                   "expression": "x:gate"}]
    })).unwrap();
    // Any table the case overrides, replaced whole.
    if let Value::Object(map) = over {
        for (k, v) in map {
            t.insert(k, v);
        }
    }
    t
}

fn run(over: Value) -> (IndexMap<String, Vec<Value>>, DirectiveStats) {
    let tables = tables(over);
    let store = Store::from_payload(tables, "typescript");
    let idx = enum_index(&store);
    let mem = member_enums(&store, &idx);
    directive_restrictions(&store, &mem, &idx)
}

fn rows_of(out: &IndexMap<String, Vec<Value>>) -> Vec<(String, String, String)> {
    out.get("g:1")
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    (
                        r.get("dim").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        r.get("op").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        r.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_constant_is_in_the_class_and_the_comparison_in_its_BASE() {
    // The template holds only an attribute and an object literal, so a resolver that
    // reads the gate expression alone finds nothing. Several classes are written this way
    // over dozens of occurrences and every one resolved to NOTHING - which a consumer folding
    // `always_values` reads as "unrestricted".
    let (out, stats) = run(json!({}));
    assert_eq!(rows_of(&out), vec![("selected".into(), "in".into(), "Alpha".into())]);
    assert_eq!(stats.resolved, 1);
    assert_eq!(stats.unknown, 0);
    assert_eq!(stats.gates, 1);
}

#[test]
fn a_bound_constant_no_INPUT_can_carry_restricts_nothing_at_all() {
    // Not uncertainty - PROOF of irrelevance. Reading it as a restriction attributed
    // keys to a kind they do not belong to, keys that then came out gated on two
    // kinds at once, which is unsatisfiable. The separating test is DECLARED: the
    // input's type must declare a member of the bound enum.
    let (out, stats) = run(json!({
        "type_members": [{"id": "tm:1", "owner": "i:1", "name": "unrelated"}]
    }));
    assert!(out.is_empty(), "no row, not an unknown row");
    assert_eq!(stats.gates, 0);
    assert_eq!(stats.inert, 1, "and it is counted, so the silence is visible");
}

#[test]
fn a_bound_constant_no_COMPARISON_reads_restricts_nothing_at_all() {
    let (out, stats) = run(json!({
        "expressions": [{"id": "x:gate", "file": "f:1", "line": 1, "ast":
                         {"k": "Map", "keys": [{"key": "vendorType"}]}}]
    }));
    assert!(out.is_empty());
    assert_eq!(stats.inert, 1);
}

#[test]
fn an_occurrence_that_supplies_ANOTHER_key_can_take_the_escape_so_it_is_unknown() {
    // The base renders unconditionally when an override flag is set, BEFORE the
    // comparison is reached. The map reads what the CALL SITE supplies rather than
    // modelling which branch dominates which.
    let (out, stats) = run(json!({
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 45, "ast":
             {"k": "Binary", "op": "===",
              "left": {"k": "Read", "target": {"name": "vendorType",
                       "file": "C:/work/fe/app/vis.directive.ts", "line": 31}},
              "right": {"k": "Read", "target": {"name": "selected",
                        "file": "C:/work/fe/app/vis.directive.ts", "line": 32}}}},
            {"id": "x:gate", "file": "f:1", "line": 1, "ast":
             {"k": "Map", "keys": [{"key": "vendorType"}, {"key": "forced"}]}}
        ]
    }));
    assert_eq!(rows_of(&out), vec![("selected".into(), "unknown".into(), "".into())]);
    assert_eq!(stats.unknown, 1);
    assert_eq!(stats.resolved, 0);
}

#[test]
fn an_input_that_is_not_an_object_literal_at_all_is_unknown_and_never_absent() {
    // The honesty rule of the table it writes into.
    let (out, _) = run(json!({
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 45, "ast":
             {"k": "Binary", "op": "===",
              "left": {"k": "Read", "target": {"name": "vendorType",
                       "file": "C:/work/fe/app/vis.directive.ts", "line": 31}},
              "right": {"k": "Read", "target": {"name": "selected",
                        "file": "C:/work/fe/app/vis.directive.ts", "line": 32}}}},
            {"id": "x:gate", "file": "f:1", "line": 1, "ast":
             {"k": "Read", "target": {"name": "someVar"}}}
        ]
    }));
    assert_eq!(rows_of(&out), vec![("selected".into(), "unknown".into(), "".into())]);
}

#[test]
fn a_constant_bound_under_an_if_no_longer_proves_one_value_reaches_the_comparison() {
    let (out, _) = run(json!({
        "assignments": [{"class": "c:sub", "target_id": "m:field", "branch": "b:1",
                         "value": {"$enum": "Vendor.Alpha"}}]
    }));
    assert_eq!(rows_of(&out), vec![("selected".into(), "unknown".into(), "".into())]);
}

#[test]
fn two_constants_on_one_field_leave_the_class_unable_to_prove_either() {
    let (out, _) = run(json!({
        "assignments": [
            {"class": "c:sub", "target_id": "m:field", "value": {"$enum": "Vendor.Alpha"}},
            {"class": "c:sub", "target_id": "m:field", "value": {"$enum": "Vendor.Other"}}
        ]
    }));
    assert_eq!(rows_of(&out), vec![("selected".into(), "unknown".into(), "".into())]);
}

#[test]
fn the_NEAREST_binder_wins_because_that_is_what_the_instance_holds() {
    // The base constructor runs first and the subclass overwrites it, so an ancestor's
    // constant is not a second value on the same field.
    let (out, _) = run(json!({
        "assignments": [
            {"class": "c:sub", "target_id": "m:field", "value": {"$enum": "Vendor.Alpha"}},
            {"class": "c:base", "target_id": "m:field", "value": {"$enum": "Vendor.Other"}}
        ]
    }));
    assert_eq!(rows_of(&out), vec![("selected".into(), "in".into(), "Alpha".into())]);
}

#[test]
fn a_nearer_class_assigning_something_UNREADABLE_closes_the_field() {
    // The ancestor's constant no longer survives: the instance holds whatever the
    // subclass wrote, and the map cannot read it.
    let (out, _) = run(json!({
        "assignments": [
            {"class": "c:sub", "target_id": "m:field", "value": {"k": "Read"}},
            {"class": "c:base", "target_id": "m:field", "value": {"$enum": "Vendor.Other"}}
        ]
    }));
    assert!(out.is_empty(), "no binding survives, so nothing is claimed");
}

#[test]
fn a_negated_comparison_keeps_its_polarity_because_the_domain_is_finite() {
    let (out, _) = run(json!({
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 45, "ast":
             {"k": "Binary", "op": "!==",
              "left": {"k": "Read", "target": {"name": "vendorType",
                       "file": "C:/work/fe/app/vis.directive.ts", "line": 31}},
              "right": {"k": "Read", "target": {"name": "selected",
                        "file": "C:/work/fe/app/vis.directive.ts", "line": 32}}}},
            {"id": "x:gate", "file": "f:1", "line": 1, "ast":
             {"k": "Map", "keys": [{"key": "vendorType"}]}}
        ]
    }));
    assert_eq!(rows_of(&out), vec![("selected".into(), "not_in".into(), "Alpha".into())]);
}

#[test]
fn a_comparison_outside_the_declaring_classes_lines_is_not_its_comparison() {
    let (out, _) = run(json!({
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 900, "ast":
             {"k": "Binary", "op": "===",
              "left": {"k": "Read", "target": {"name": "vendorType",
                       "file": "C:/work/fe/app/vis.directive.ts", "line": 31}},
              "right": {"k": "Read", "target": {"name": "selected",
                        "file": "C:/work/fe/app/vis.directive.ts", "line": 32}}}},
            {"id": "x:gate", "file": "f:1", "line": 1, "ast":
             {"k": "Map", "keys": [{"key": "vendorType"}]}}
        ]
    }));
    assert!(out.is_empty());
}

#[test]
fn a_class_hierarchy_that_names_a_LOOP_terminates_rather_than_hanging() {
    // `extends` is data, and data can name a loop.
    let (out, _) = run(json!({
        "classes": [
            {"id": "c:sub", "name": "AlphaVisible", "file": "f:1", "line": 10,
             "end_line": 20, "extends": {"name": "VisibleBase", "file": "C:/work/fe/app/vis.directive.ts"}},
            {"id": "c:base", "name": "VisibleBase", "file": "f:1", "line": 30, "end_line": 60,
             "extends": {"name": "AlphaVisible", "file": "C:/work/fe/app/vis.directive.ts"}}
        ]
    }));
    assert_eq!(rows_of(&out), vec![("selected".into(), "in".into(), "Alpha".into())]);
}

#[test]
fn a_selector_naming_SEVERAL_classes_is_not_decided_by_whichever_was_indexed_last() {
    // The join this replaces produced one row per class.
    // BOTH ORDERS, because keeping only one of them decides the gate by whichever class
    // happened to be indexed last - and a test that puts the contributing class last
    // cannot tell the two apart.
    for order in [json!(["c:nothing", "c:sub"]), json!(["c:sub", "c:nothing"])] {
        let list: Vec<Value> = order
            .as_array()
            .unwrap()
            .iter()
            .map(|c| json!({"selector": "[if-kind]", "class": c}))
            .collect();
        let (out, stats) = run(json!({"selector_index": list}));
        assert_eq!(rows_of(&out), vec![("selected".into(), "in".into(), "Alpha".into())],
                   "order {order}");
        assert_eq!(stats.gates, 1, "order {order}");
    }
}

#[test]
fn only_a_STRUCTURAL_gate_is_read_this_way() {
    let (out, _) = run(json!({
        "gates": [{"id": "g:1", "name": "if-kind", "gate_kind": "binding",
                   "expression": "x:gate"}]
    }));
    assert!(out.is_empty());
}

#[test]
fn a_target_with_a_row_but_no_FILE_resolves_to_nothing_because_the_guard_comes_first() {
    let idx = MemberIdx { by_line_name: IndexMap::new(), name_of: IndexMap::new(), plain: IndexSet::new() };
    assert_eq!(member_at(&idx, Some(&json!({"name": "x", "row": "m:1"}))), None);
    assert_eq!(
        member_at(&idx, Some(&json!({"name": "x", "file": "a.ts", "row": "m:1"}))),
        Some("m:1".to_string())
    );
}

#[test]
fn the_file_match_is_BY_SUFFIX_because_one_side_is_absolute() {
    // `type_ref` and AST targets name the file absolutely while `files.path` is
    // FE-relative.
    let tables = tables(json!({}));
    let store = Store::from_payload(tables, "typescript");
    let idx = member_index(&store);
    let target = json!({"name": "vendorType", "line": 31,
                        "file": "C:/work/fe/app/vis.directive.ts"});
    assert_eq!(member_at(&idx, Some(&target)), Some("m:field".to_string()));
}

#[test]
fn an_input_that_is_not_a_Map_gives_None_and_an_empty_one_gives_an_empty_list() {
    // Which are different answers: `None` means "not an object literal at all".
    assert_eq!(literal_keys_of(Some(&json!({"k": "Read"}))), None);
    assert_eq!(literal_keys_of(None), None);
    assert_eq!(literal_keys_of(Some(&json!({"k": "Map", "keys": []}))), Some(vec![]));
}
