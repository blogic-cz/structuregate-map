//! The tests of `gate_config.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `gate_config::tests`, so `super::*` is `gate_config`.

use super::*;
use super::super::gate_values::{enum_index, member_enums};
use serde_json::{json, Map};

/// One enum, one config interface declaring two members of it, one directive class
/// taking that interface as an input, and one gate bound to the directive.
fn workspace(class_body: Value, gate_ast: Value) -> Value {
    json!({
        "files": [{"id": "fl:1", "path": "app/kinds.ts"}],
        "enums": [{"id": "e:1", "name": "VendorIDs", "file": "fl:1",
                   "members": [{"name": "Beta"}, {"name": "Alpha"}, {"name": "Gamma"}]}],
        "interfaces": [{"id": "if:1", "name": "ShowConfig", "file": "fl:1"}],
        "type_members": [
            {"id": "tm:good", "owner": "if:1", "name": "allowIDs",
             "type_ref": {"element": {"name": "VendorIDs", "file": "/repo/app/kinds.ts"}}},
            {"id": "tm:bad", "owner": "if:1", "name": "blockIDs",
             "type_ref": {"element": {"name": "VendorIDs", "file": "/repo/app/kinds.ts"}}}
        ],
        "classes": [{"id": "c:1", "name": "VisibilityDirective", "file": "fl:1",
                     "line": 1, "end_line": 99}],
        "members": [
            {"id": "m:in", "class": "c:1", "name": "input",
             "type_ref": {"name": "ShowConfig", "file": "/repo/app/kinds.ts"}},
            {"id": "m:alias", "class": "c:1", "name": "allowIDsAlias",
             "type": "typeof VendorIDs",
             "type_ref": {"name": "VendorIDs", "file": "/repo/app/kinds.ts"}}
        ],
        "selector_index": [{"selector": "[showConfig]", "class": "c:1"}],
        "gates": [{"id": "g:1", "name": "showConfig", "component": "ng:1",
                   "expression": "x:gate"}],
        "expressions": [
            {"id": "x:gate", "file": "fl:9", "line": 5, "ast": gate_ast},
            {"id": "x:body", "file": "fl:1", "line": 20, "ast": class_body}
        ],
        "locals": [], "assignments": [], "returns": []
    })
}

/// `input.<member>.some((m) => m === x)`, optionally under a `!`.
fn membership(member: &str, negated: bool) -> Value {
    let call = json!({
        "k": "Call",
        "receiver": {"k": "Read", "name": "some",
                     "receiver": {"k": "Read", "name": member,
                                  "receiver": {"k": "Read", "name": "input"}}},
        "args": [{"k": "Binary", "op": "===",
                  "left": {"k": "Read", "name": "m"},
                  "right": {"k": "Read", "name": "x"}}]
    });
    if negated { json!({"k": "Not", "expr": call}) } else { call }
}

/// `{allowIDs: [alias.Beta, alias.Alpha]}`
fn literal(member: &str, values: &[&str]) -> Value {
    let items: Vec<Value> = values
        .iter()
        .map(|v| json!({"k": "Read", "name": v,
                        "receiver": {"k": "Read", "name": "alias",
                                     "target": {"row": "m:alias"}}}))
        .collect();
    json!({"k": "Map", "keys": [{"key": member}],
           "values": [{"k": "Array", "items": items}]})
}

fn run(tables: Value) -> (IndexMap<String, Vec<Value>>, ConfigStats) {
    let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
    let store = Store::from_payload(map, "typescript");
    let idx = enum_index(&store);
    let mem = member_enums(&store, &idx);
    config_restrictions(&store, &mem, &idx)
}

fn rows_of(out: &IndexMap<String, Vec<Value>>) -> Vec<(String, String, Vec<String>)> {
    out.get("g:1")
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    (
                        r["dim"].as_str().unwrap_or("").to_string(),
                        r["op"].as_str().unwrap_or("").to_string(),
                        r["values"]
                            .as_array()
                            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                            .unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_unnegated_equality_membership_test_makes_the_listed_members_a_restriction() {
    let tables = workspace(membership("allowIDs", false),
                           literal("allowIDs", &["Beta", "Alpha"]));
    let (out, stats) = run(tables);
    assert_eq!(rows_of(&out),
               vec![("showConfig.allowIDs".to_string(), "in".to_string(),
                     vec!["Alpha".to_string(), "Beta".to_string()])]);
    assert_eq!(stats.resolved, 1);
}

#[test]
fn the_same_member_tested_under_a_NOT_is_unknown_rather_than_a_restriction() {
    // `!blockIDs.some(...)` does not hide the element - it disables it.
    // Reading it as a restriction would withhold content from users entitled to it.
    let tables = workspace(membership("allowIDs", true),
                           literal("allowIDs", &["Beta"]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "unknown");
}

#[test]
fn the_polarity_comes_from_the_test_and_never_from_the_members_name() {
    // Same enum, opposite meaning, and only the class's own test tells them apart.
    // The one CALLED `disabled...` is tested un-negated here, so it restricts.
    let tables = workspace(membership("blockIDs", false),
                           literal("blockIDs", &["Beta"]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out),
               vec![("showConfig.blockIDs".to_string(),
                     "in".to_string(), vec!["Beta".to_string()])]);
}

#[test]
fn a_member_tested_both_ways_is_unknown() {
    // The map cannot say which test decides the render, and an upper bound that
    // guesses is worse than one that admits it.
    let both = json!({"k": "Binary", "op": "&&",
                      "left": membership("allowIDs", false),
                      "right": membership("allowIDs", true)});
    let tables = workspace(both, literal("allowIDs", &["Beta"]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "unknown");
}

#[test]
fn a_non_equality_predicate_closes_the_whole_test() {
    // `some((m) => m.foo > 3)` says something about the members' shape rather than
    // about which members are permitted.
    let call = json!({
        "k": "Call",
        "receiver": {"k": "Read", "name": "some",
                     "receiver": {"k": "Read", "name": "allowIDs",
                                  "receiver": {"k": "Read", "name": "input"}}},
        "args": [{"k": "Binary", "op": ">",
                  "left": {"k": "Read", "name": "m"}, "right": {"k": "Read", "name": "x"}}]
    });
    let tables = workspace(call, literal("allowIDs", &["Beta"]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "unknown");
}

#[test]
fn asking_a_set_whether_it_holds_the_member_is_an_identity_test_too() {
    // `some((id) => activeIds.has(id))`. Accepting only `m === x` would have made all
    // the resolved rows unknown.
    let call = json!({
        "k": "Call",
        "receiver": {"k": "Read", "name": "some",
                     "receiver": {"k": "Read", "name": "allowIDs",
                                  "receiver": {"k": "Read", "name": "input"}}},
        "args": [{"k": "Call",
                  "receiver": {"k": "Read", "name": "has",
                               "receiver": {"k": "Read", "name": "activeIds"}},
                  "args": [{"k": "Read", "name": "id"}]}]
    });
    let tables = workspace(call, literal("allowIDs", &["Beta"]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "in");
}

#[test]
fn one_unresolvable_item_makes_the_whole_dimension_unknown() {
    // A list missing one member permits LESS than the gate does.
    let mut lit = literal("allowIDs", &["Beta"]);
    lit["values"][0]["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"k": "Read", "name": "somethingElse"}));
    let tables = workspace(membership("allowIDs", false), lit);
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "unknown");
}

#[test]
fn a_member_merely_READ_still_participates_and_is_unknown() {
    // Omitting it would restore for one dimension the silence this pass removes.
    let read = json!({"k": "Read", "name": "allowIDs",
                      "receiver": {"k": "Read", "name": "input"}});
    let tables = workspace(read, literal("allowIDs", &["Beta"]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "unknown");
}

#[test]
fn no_literal_at_all_is_still_a_restriction() {
    // The directive tests the member whatever the bound object turns out to hold.
    let tables = workspace(membership("allowIDs", false),
                           json!({"k": "Read", "name": "somethingDynamic"}));
    let (out, _) = run(tables);
    let rows = rows_of(&out);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "unknown", "tested, but nothing says with which members");
}

#[test]
fn a_literal_that_omits_the_member_reports_nothing_for_it() {
    let tables = workspace(membership("allowIDs", false),
                           literal("someOtherKey", &["Beta"]));
    assert!(run(tables).0.is_empty());
}

#[test]
fn a_gate_whose_selector_no_class_claims_contributes_nothing() {
    let mut tables = workspace(membership("allowIDs", false),
                               literal("allowIDs", &["Beta"]));
    tables["selector_index"] = json!([{"selector": "[other]", "class": "c:1"}]);
    assert!(run(tables).0.is_empty());
}

/// What `listed` a row carries, or nothing.
fn listed_of_row(out: &IndexMap<String, Vec<Value>>, dim: &str) -> Option<Vec<String>> {
    let rows = out.get("g:1")?;
    let row = rows.iter().find(|r| r["dim"] == json!(format!("showConfig.{dim}")))?;
    row.get("listed")?
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
}

/// `cond ? then : else`
fn cond(then: Value, other: Value) -> Value {
    json!({"k": "Cond", "cond": {"k": "Read", "name": "wide"}, "then": then, "else": other})
}

#[test]
fn an_exclusion_tested_under_a_NOT_stays_unknown_and_still_lists_its_members() {
    // The op cannot be read (hide or disable?), the members can.
    let tables = workspace(membership("blockIDs", true),
                           literal("blockIDs", &["Gamma", "Beta"]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "unknown");
    assert!(rows_of(&out)[0].2.is_empty(), "values stay the permitted side: none known");
    assert_eq!(listed_of_row(&out, "blockIDs"),
               Some(vec!["Beta".to_string(), "Gamma".to_string()]));
}

#[test]
fn an_empty_list_is_listed_as_empty_and_restricts_nothing_the_map_can_state() {
    let tables = workspace(membership("allowIDs", false), literal("allowIDs", &[]));
    let (out, _) = run(tables);
    assert_eq!(rows_of(&out)[0].1, "unknown");
    assert_eq!(listed_of_row(&out, "allowIDs"), Some(vec![]));
}

#[test]
fn a_list_with_an_unresolvable_item_lists_nothing() {
    let mut lit = literal("allowIDs", &["Beta"]);
    lit["values"][0]["items"].as_array_mut().unwrap().push(json!({"k": "Read", "name": "other"}));
    let (out, _) = run(workspace(membership("allowIDs", false), lit));
    assert_eq!(listed_of_row(&out, "allowIDs"), None);
}

#[test]
fn a_conditional_config_permits_the_union_of_what_its_branches_list() {
    let gate = cond(literal("allowIDs", &["Beta"]), literal("allowIDs", &["Gamma"]));
    let (out, _) = run(workspace(membership("allowIDs", false), gate));
    assert_eq!(rows_of(&out),
               vec![("showConfig.allowIDs".to_string(), "in".to_string(),
                     vec!["Beta".to_string(), "Gamma".to_string()])]);
}

#[test]
fn a_conditional_config_lists_no_member_neither_branch_writes() {
    // Every typed member of the interface was listed for a conditional.
    let body = json!({"k": "Binary", "op": "&&",
                      "left": membership("allowIDs", false),
                      "right": membership("blockIDs", true)});
    let gate = cond(literal("allowIDs", &["Beta"]), literal("allowIDs", &["Gamma"]));
    let (out, _) = run(workspace(body, gate));
    let dims: Vec<String> = rows_of(&out).into_iter().map(|r| r.0).collect();
    assert_eq!(dims, vec!["showConfig.allowIDs".to_string()]);
}

#[test]
fn a_member_one_branch_omits_is_unknown_and_lists_what_the_other_writes() {
    let gate = cond(literal("allowIDs", &["Beta"]), literal("blockIDs", &["Gamma"]));
    let (out, _) = run(workspace(membership("allowIDs", false), gate));
    assert_eq!(rows_of(&out)[0].1, "unknown");
    assert_eq!(listed_of_row(&out, "allowIDs"), Some(vec!["Beta".to_string()]));
}

#[test]
fn a_conditional_with_an_arm_that_is_not_a_literal_is_still_unread() {
    let gate = cond(literal("allowIDs", &["Beta"]), json!({"k": "Read", "name": "dynamic"}));
    let tables = workspace(json!({"k": "Binary", "op": "&&",
                                  "left": membership("allowIDs", false),
                                  "right": membership("blockIDs", true)}), gate);
    let (out, _) = run(tables);
    let rows = rows_of(&out);
    assert_eq!(rows.len(), 2, "no literal to say which members it writes");
    assert!(rows.iter().all(|r| r.1 == "unknown"));
}
