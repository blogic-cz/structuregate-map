//! The tests of `gate_features.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `gate_features::tests`, so `super::*` is the pass.
use super::*;
use serde_json::{json, Map};

fn run(tables: Value) -> GateFeatures {
    let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
    let store = Store::from_payload(map, "typescript");
    gate_features(&store, &["Svc.isOn".to_string()], "FlagCodes", &mut |_| {})
}

/// A check declaration, a property assigned from it, and a gate reading that property.
fn direct() -> Value {
    json!({
        "files": [{"id": "fl:1", "path": "app/svc.ts"}],
        "classes": [{"id": "c:svc", "name": "Svc", "file": "fl:1"},
                    {"id": "c:cmp", "name": "Cmp", "file": "fl:1"}],
        "interfaces": [],
        "type_members": [],
        "members": [{"id": "m:isOn", "name": "isOn", "class": "c:svc", "file": "fl:1", "line": 3},
                    {"id": "m:visible", "name": "visible", "class": "c:cmp", "file": "fl:1", "line": 9}],
        "assignments": [{"id": "a:1", "scope": "this", "target": "visible", "target_id": "m:visible",
                         "class": "c:cmp", "operator": "=",
                         "value": {"$call": true, "$target": {"file": "/repo/app/svc.ts"},
                                   "$member": "FlagCodes.ALPHA"}}],
        "bindings": [],
        "components": [{"id": "ng:1", "class": "c:cmp", "file": "fl:1"}],
        "expressions": [{"id": "x:1", "role": "expr", "file": "fl:1", "line": 20,
                         "ast": {"k": "Read", "name": "visible", "target": {"row": "m:visible"}}}],
        "gates": [{"id": "g:1", "name": "ngIf", "component": "ng:1", "expression": "x:1"}],
        "branches": [], "functions": [], "calls": [], "translations": []
    })
}

fn features_of(gf: &GateFeatures, gate: &str) -> Option<Vec<String>> {
    gf.map.get(gate).map(|(_, f)| f.clone())
}

#[test]
fn a_gate_reading_a_property_assigned_from_a_check_requires_that_feature() {
    let gf = run(direct());
    assert_eq!(features_of(&gf, "g:1"), Some(vec!["ALPHA".to_string()]));
    assert_eq!(gf.direct, 1);
}

#[test]
fn a_property_the_template_writes_is_not_evidence_of_anything() {
    // `[(value)]="visible"` desugars to an output binding whose handler is the write,
    // and that write lands in `bindings` alone - no pass below can see it.
    let mut tables = direct();
    tables["bindings"] = json!([{"kind": "output", "component": "ng:1",
                                 "source": "visible =$event"}]);
    assert!(run(tables).map.is_empty(), "the user fills it by clicking");
}

#[test]
fn a_comparison_in_the_binding_source_is_not_a_write() {
    // `!==` and `>=` split into three parts and are rejected by the count.
    let mut tables = direct();
    tables["bindings"] = json!([{"kind": "output", "component": "ng:1",
                                 "source": "visible !== $event"}]);
    assert_eq!(run(tables).direct, 1, "still attributed");
}

#[test]
fn a_second_write_that_proves_nothing_disqualifies_the_property() {
    // One property is set from the check by many panels and to a bare `true`
    // elsewhere - several gates would have demanded a feature the content renders
    // without.
    let mut tables = direct();
    let mut writes = tables["assignments"].as_array().unwrap().clone();
    writes.push(json!({"id": "a:2", "scope": "this", "target": "visible",
                       "target_id": "m:visible", "class": "c:cmp", "operator": "=",
                       "value": true}));
    tables["assignments"] = Value::Array(writes);
    assert!(run(tables).map.is_empty());
}

#[test]
fn two_writes_proving_different_features_intersect_to_nothing() {
    // The property is truthy under either one, so neither is necessary.
    let mut tables = direct();
    let mut writes = tables["assignments"].as_array().unwrap().clone();
    writes.push(json!({"id": "a:2", "scope": "this", "target": "visible",
                       "target_id": "m:visible", "class": "c:cmp", "operator": "=",
                       "value": {"$call": true, "$target": {"file": "/repo/app/svc.ts"},
                                 "$member": "FlagCodes.BETA"}}));
    tables["assignments"] = Value::Array(writes);
    assert!(run(tables).map.is_empty());
}

#[test]
fn a_write_is_filed_under_the_class_that_declares_the_property() {
    // Most of the property writes resolve to a member of a DIFFERENT class than the
    // one the statement sits in. Filed under the writer, the owner's "every write"
    // universe never sees them and the writer gets a key it does not own.
    let mut tables = direct();
    tables["assignments"] = json!([{"id": "a:1", "scope": "property", "target": "visible",
                                    "target_id": "m:visible", "class": "c:OTHER",
                                    "operator": "=",
                                    "value": {"$call": true,
                                              "$target": {"file": "/repo/app/svc.ts"},
                                              "$member": "FlagCodes.ALPHA"}}]);
    assert_eq!(features_of(&run(tables), "g:1"), Some(vec!["ALPHA".to_string()]));
}

#[test]
fn a_property_write_with_no_resolved_target_credits_no_class() {
    let mut tables = direct();
    tables["assignments"] = json!([{"id": "a:1", "scope": "property", "target": "visible",
                                    "class": "c:cmp", "operator": "=",
                                    "value": {"$call": true,
                                              "$target": {"file": "/repo/app/svc.ts"},
                                              "$member": "FlagCodes.ALPHA"}}]);
    assert!(run(tables).map.is_empty());
}

#[test]
fn a_local_write_never_attributes_to_a_property_of_the_same_name() {
    // `const enabled = await isOn(X)` attributed X to any PROPERTY called
    // `enabled` before the scope was checked.
    let mut tables = direct();
    tables["assignments"] = json!([{"id": "a:1", "scope": "local", "target": "visible",
                                    "target_id": "m:visible", "class": "c:cmp",
                                    "operator": "=",
                                    "value": {"$call": true,
                                              "$target": {"file": "/repo/app/svc.ts"},
                                              "$member": "FlagCodes.ALPHA"}}]);
    assert!(run(tables).map.is_empty());
}

#[test]
fn a_chain_of_ternaries_needs_a_fixed_point_and_not_one_pass() {
    // THREE LEVELS, because two do not distinguish the two behaviours. `first` comes
    // from the check, so the FIRST ternary pass already resolves `second`; `third`
    // hangs off `second` and can only be resolved once `second` is known, which is
    // the second pass. A gate reading `third` therefore answers differently.
    //
    // A WHOLE-DATABASE DIFFERENTIAL NEED NOT CONTAIN THIS SHAPE. Removing the loop can leave a
    // differential green, which is why the case is written out here -
    // and why the first version of it, with only two levels, proved nothing.
    let mut tables = direct();
    tables["members"] = json!([
        {"id": "m:isOn", "name": "isOn", "class": "c:svc", "file": "fl:1", "line": 3},
        {"id": "m:first", "name": "first", "class": "c:cmp", "file": "fl:1", "line": 9},
        {"id": "m:second", "name": "second", "class": "c:cmp", "file": "fl:1", "line": 10},
        {"id": "m:third", "name": "third", "class": "c:cmp", "file": "fl:1", "line": 11}
    ]);
    tables["assignments"] = json!([
        {"id": "a:1", "scope": "this", "target": "first", "target_id": "m:first",
         "class": "c:cmp", "operator": "=",
         "value": {"$call": true, "$target": {"file": "/repo/app/svc.ts"},
                   "$member": "FlagCodes.ALPHA"}},
        {"id": "a:2", "scope": "this", "target": "second", "target_id": "m:second",
         "class": "c:cmp", "operator": "=",
         "value": {"$cond_expr": "x:t2", "$then": "text", "$else": null}},
        {"id": "a:3", "scope": "this", "target": "third", "target_id": "m:third",
         "class": "c:cmp", "operator": "=",
         "value": {"$cond_expr": "x:t3", "$then": "text", "$else": null}}
    ]);
    tables["expressions"] = json!([
        {"id": "x:1", "role": "expr", "file": "fl:1", "line": 20,
         "ast": {"k": "Read", "name": "third", "target": {"row": "m:third"}}},
        {"id": "x:t2", "role": "ternary", "file": "fl:1", "line": 10,
         "ast": {"k": "Read", "name": "first", "target": {"row": "m:first"}}},
        {"id": "x:t3", "role": "ternary", "file": "fl:1", "line": 11,
         "ast": {"k": "Read", "name": "second", "target": {"row": "m:second"}}}
    ]);
    assert_eq!(
        features_of(&run(tables), "g:1"),
        Some(vec!["ALPHA".to_string()]),
        "the third link resolves only on the second pass"
    );
}

#[test]
fn a_ternary_whose_else_supplies_something_proves_nothing() {
    // The property is filled either way, so the condition says nothing about what
    // renders.
    let mut tables = direct();
    tables["members"] = json!([
        {"id": "m:isOn", "name": "isOn", "class": "c:svc", "file": "fl:1", "line": 3},
        {"id": "m:first", "name": "first", "class": "c:cmp", "file": "fl:1", "line": 9},
        {"id": "m:second", "name": "second", "class": "c:cmp", "file": "fl:1", "line": 10}
    ]);
    tables["assignments"] = json!([
        {"id": "a:1", "scope": "this", "target": "first", "target_id": "m:first",
         "class": "c:cmp", "operator": "=",
         "value": {"$call": true, "$target": {"file": "/repo/app/svc.ts"},
                   "$member": "FlagCodes.ALPHA"}},
        {"id": "a:2", "scope": "this", "target": "second", "target_id": "m:second",
         "class": "c:cmp", "operator": "=",
         "value": {"$cond_expr": "x:t", "$then": "text", "$else": "other text"}}
    ]);
    tables["expressions"] = json!([
        {"id": "x:1", "role": "expr", "file": "fl:1", "line": 20,
         "ast": {"k": "Read", "name": "second", "target": {"row": "m:second"}}},
        {"id": "x:t", "role": "ternary", "file": "fl:1", "line": 10,
         "ast": {"k": "Read", "name": "first", "target": {"row": "m:first"}}}
    ]);
    assert_eq!(features_of(&run(tables), "g:1"), None);
}

#[test]
fn a_negated_occurrence_is_not_a_grantable_gate() {
    let mut tables = direct();
    tables["expressions"] = json!([{"id": "x:1", "role": "expr", "file": "fl:1", "line": 20,
                                    "ast": {"k": "Not", "expr": {"k": "Read", "name": "visible",
                                            "target": {"row": "m:visible"}}}}]);
    assert!(run(tables).map.is_empty(), "renders when the feature is OFF");
}

#[test]
fn nothing_declared_is_said_out_loud_rather_than_answered_silently() {
    let map: Map<String, Value> = direct().as_object().cloned().expect("tables");
    let store = Store::from_payload(map, "typescript");
    let mut said = Vec::new();
    let gf = gate_features(&store, &[], "", &mut |n| said.push(n.to_string()));
    assert!(gf.map.is_empty());
    assert_eq!(said.len(), 1, "an empty table would read as a claim about the application");
}
