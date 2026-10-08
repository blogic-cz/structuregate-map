//! The tests of `value_folds.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `value_folds::tests`, so `super::*` is `value_folds`.

use super::*;
use super::super::gate_values::EnumInfo;

fn idx_of(domain: &[&str]) -> EnumIndex {
    let mut idx = EnumIndex::default();
    idx.by_id.insert(
        "e:1".to_string(),
        EnumInfo {
            name: Some("GroupIDs".to_string()),
            domain: domain.iter().map(|s| s.to_string()).collect(),
        },
    );
    idx
}

const FOUR: &[&str] = &["A", "B", "C", "D"];

fn gate(op: &str, values: &[&str]) -> Vec<Value> {
    vec![json!({"enum": "e:1", "dim": "categoryID", "row": null,
                "op": op, "values": values})]
}

fn map(entries: &[(&str, Vec<Value>)]) -> GateMap {
    entries
        .iter()
        .map(|(g, rows)| (g.to_string(), (Some("ng:1".to_string()), rows.clone())))
        .collect()
}

fn way(gates: &[&str]) -> Way {
    gates.iter().map(|g| g.to_string()).collect()
}

#[test]
fn two_gates_on_one_way_intersect_because_both_must_hold() {
    let gates = map(&[("g:1", gate("in", &["A", "B"])), ("g:2", gate("in", &["B", "C"]))]);
    let per = per_way_allowed(&[way(&["g:1", "g:2"])], &gates, &idx_of(FOUR));
    assert_eq!(per[0]["e:1#categoryID"], way(&["B"]));
}

#[test]
fn two_set_ins_on_one_way_keep_the_narrower_and_never_empty() {
    // `isItemActive([A])` nested inside `isItemActive([B, C])`: both hold. As values
    // they intersect to nothing, and the key would read as rendering for no one; each
    // alone is true, so the narrower is kept.
    let set = |v: &[&str]| vec![json!({"enum": "e:1", "dim": "active", "row": null,
                                       "op": "in", "values": v, "set": true})];
    let gates = map(&[("g:1", set(&["B", "C"])), ("g:2", set(&["A"])), ("g:3", set(&["A"]))]);
    let per = per_way_allowed(&[way(&["g:1", "g:2"])], &gates, &idx_of(FOUR));
    assert_eq!(per[0]["e:1#active"], way(&["A"]), "the narrower, never emptied");
    // One of them alone still narrows.
    let per = per_way_allowed(&[way(&["g:1"])], &gates, &idx_of(FOUR));
    assert_eq!(per[0]["e:1#active"], way(&["B", "C"]));
    // Met in the other order, the narrower still wins.
    let per = per_way_allowed(&[way(&["g:3", "g:1"])], &gates, &idx_of(FOUR));
    assert_eq!(per[0]["e:1#active"], way(&["A"]));
}

#[test]
fn the_fold_across_ways_is_a_union_because_any_way_in_renders_the_key() {
    // Getting this backwards would invent restrictions that do not hold.
    let gates = map(&[("g:1", gate("in", &["A"])), ("g:2", gate("in", &["B"]))]);
    let ways = [way(&["g:1"]), way(&["g:2"])];
    let out = fold_values(&ways, &gates, &idx_of(FOUR), None);
    assert_eq!(out[0]["op"], json!("in"));
    assert_eq!(out[0]["values"], json!(["A", "B"]));
}

#[test]
fn the_union_takes_every_way_until_it_is_as_large_as_the_domain() {
    // The walk stops early only once the union cannot be published; before that every way counts.
    let gates = map(&[("g:1", gate("in", &["A"])), ("g:2", gate("in", &["B"])), ("g:3", gate("in", &["C"]))]);
    let ways = [way(&["g:1"]), way(&["g:2"]), way(&["g:3"])];
    let out = fold_values(&ways, &gates, &idx_of(FOUR), None);
    assert_eq!(out[0]["op"], json!("not_in"));
    assert_eq!(out[0]["values"], json!(["D"]));
}

#[test]
fn a_way_that_never_mentions_the_dimension_permits_all_of_it() {
    // Which collapses the union to the whole domain and correctly reports no
    // restriction at all.
    let gates = map(&[("g:1", gate("in", &["A"])), ("g:other", vec![])]);
    let ways = [way(&["g:1"]), way(&["g:other"])];
    assert!(fold_values(&ways, &gates, &idx_of(FOUR), None).is_empty());
}

#[test]
fn a_way_that_permits_NOTHING_is_not_a_way_that_permits_everything() {
    // An empty Set is truthy in the language this is ported from, and an `or` fallback
    // kept it - widening the union to everything and dropping the restriction. It cost
    // dozens of key_reach rows their always_values.
    let gates = map(&[
        ("g:1", gate("in", &["A"])),
        ("g:x", gate("in", &["A", "B"])),
        ("g:y", gate("in", &["C"])),
    ]);
    // The second way intersects to nothing: A,B then C share no member.
    let ways = [way(&["g:1"]), way(&["g:x", "g:y"])];
    let per = per_way_allowed(&ways, &gates, &idx_of(FOUR));
    assert!(per[1]["e:1#categoryID"].is_empty(), "the way permits nothing");
    let out = fold_values(&ways, &gates, &idx_of(FOUR), None);
    assert_eq!(out[0]["values"], json!(["A"]), "the empty way adds nothing");
}

#[test]
fn nothing_permitted_on_every_way_is_in_nothing_and_never_unreadable() {
    // Two lists nested on the only way share no member, so the key renders for NOBODY. Read as
    // unreadable, the dimension would vanish and the key would look unrestricted.
    let gates = map(&[("g:x", gate("in", &["A", "B"])), ("g:y", gate("in", &["C"]))]);
    let ways = [way(&["g:x", "g:y"])];
    let out = fold_values(&ways, &gates, &idx_of(FOUR), None);
    assert_eq!(out.len(), 1);
    assert_eq!((&out[0]["op"], &out[0]["values"]), (&json!("in"), &json!([])));
    assert!(unreadable_values(&ways, &gates, &idx_of(FOUR), None).is_empty());
}

#[test]
fn a_dimension_first_restricted_on_a_later_way_still_sees_the_earlier_ones() {
    // Folded per way as they were met, it missed the ways above it and came out
    // looking restricted when those allow everything.
    let gates = map(&[("g:plain", vec![]), ("g:2", gate("in", &["A"]))]);
    let ways = [way(&["g:plain"]), way(&["g:2"])];
    assert!(fold_values(&ways, &gates, &idx_of(FOUR), None).is_empty());
}

#[test]
fn an_unknown_is_the_whole_domain_because_that_is_the_honest_upper_bound() {
    let gates = map(&[("g:u", gate("unknown", &[]))]);
    let per = per_way_allowed(&[way(&["g:u"])], &gates, &idx_of(FOUR));
    assert_eq!(per[0]["e:1#categoryID"].len(), 4);
    assert!(fold_values(&[way(&["g:u"])], &gates, &idx_of(FOUR), None).is_empty());
}

#[test]
fn a_dimension_narrowed_on_every_way_whose_union_is_everything_is_unreadable() {
    // Published by neither fold unless this one takes it - the hole that left dozens of
    // (key, dimension) pairs silent.
    let gates = map(&[
        ("g:u", gate("unknown", &[])),
        ("g:in", gate("in", &["A", "B", "C", "D"])),
    ]);
    let ways = [way(&["g:u"]), way(&["g:in"])];
    assert!(fold_values(&ways, &gates, &idx_of(FOUR), None).is_empty(), "union is the domain");
    let out = unreadable_values(&ways, &gates, &idx_of(FOUR), None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["dimension"], json!("categoryID"));
}

#[test]
fn breadth_alone_is_not_unreadability() {
    // If every way knows its own set precisely and the sets merely cover the domain
    // between them, the key genuinely renders for everything.
    let gates = map(&[
        ("g:1", gate("in", &["A", "B"])),
        ("g:2", gate("in", &["C", "D"])),
    ]);
    let ways = [way(&["g:1"]), way(&["g:2"])];
    assert!(unreadable_values(&ways, &gates, &idx_of(FOUR), None).is_empty());
}

#[test]
fn a_dimension_absent_from_one_way_is_not_narrowed_on_every_way() {
    let gates = map(&[("g:u", gate("unknown", &[])), ("g:plain", vec![])]);
    let ways = [way(&["g:u"]), way(&["g:plain"])];
    assert!(unreadable_values(&ways, &gates, &idx_of(FOUR), None).is_empty());
}

#[test]
fn the_rows_are_ordered_by_collation_and_not_by_code_point() {
    let mut idx = idx_of(FOUR);
    idx.by_id.insert("e:2".to_string(), EnumInfo {
        name: Some("Shape".to_string()),
        domain: ["A", "B"].iter().map(|s| s.to_string()).collect(),
    });
    idx.by_id.insert("e:3".to_string(), EnumInfo {
        name: Some("ShapeType".to_string()),
        domain: ["A", "B"].iter().map(|s| s.to_string()).collect(),
    });
    let gates = map(&[
        ("g:a", vec![json!({"enum": "e:3", "dim": "detail", "row": null,
                            "op": "in", "values": ["A"]})]),
        ("g:b", vec![json!({"enum": "e:2", "dim": "detail", "row": null,
                            "op": "in", "values": ["A"]})]),
    ]);
    let ways = [way(&["g:a", "g:b"])];
    let out = fold_values(&ways, &gates, &idx, None);
    let names: Vec<&str> = out.iter().map(|r| r["enum_name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Shape", "ShapeType"],
               "collation compares base letters before case");
}

fn emit_values(gates: &GateMap, idx: &EnumIndex) -> Vec<Row> {
    let mut store = Store::from_payload(serde_json::Map::new(), "typescript");
    write_gate_values(&mut store, gates, idx);
    store.emitted.get("gate_values").cloned().unwrap_or_default()
}

#[test]
fn the_values_column_is_COMPACT_unlike_every_other_json_cell() {
    // This one names its separators `(",", ":")`. A cell the row store writes puts a
    // space after the comma, and a diff against the other side reads every row as
    // changed if this one does too.
    let gates = map(&[("g:1", gate("in", &["A", "B"]))]);
    let rows = emit_values(&gates, &idx_of(FOUR));
    assert_eq!(rows[0]["values_json"], json!("[\"A\",\"B\"]"));
}

#[test]
fn listed_is_a_compact_cell_of_its_own_and_null_when_nothing_lists() {
    let mut rows = gate("unknown", &[]);
    rows[0]["listed"] = json!(["A", "B"]);
    let out = emit_values(&map(&[("g:1", rows)]), &idx_of(FOUR));
    assert_eq!((out[0]["values_json"].clone(), out[0]["listed_json"].clone()),
               (json!("[]"), json!("[\"A\",\"B\"]")));
    let out = emit_values(&map(&[("g:1", gate("in", &["A"]))]), &idx_of(FOUR));
    assert_eq!(out[0]["listed_json"], Value::Null);
}

#[test]
fn a_falsy_dimension_row_is_written_as_null_and_not_as_itself() {
    let mut rows = gate("in", &["A"]);
    rows[0]["row"] = json!("");
    let out = emit_values(&map(&[("g:1", rows)]), &idx_of(FOUR));
    assert_eq!(out[0]["dimension_row"], Value::Null);

    let mut rows = gate("in", &["A"]);
    rows[0]["row"] = json!("r:7");
    let out = emit_values(&map(&[("g:1", rows)]), &idx_of(FOUR));
    assert_eq!(out[0]["dimension_row"], json!("r:7"));
}

#[test]
fn a_nameless_declaration_sorts_under_None_and_not_under_the_empty_string() {
    // Stringified the way the runtime does it, which puts it among the N names.
    let mut idx = idx_of(FOUR);
    idx.by_id.get_mut("e:1").unwrap().name = None;
    let out = fold_values(&[way(&["g:1"])], &map(&[("g:1", gate("in", &["A"]))]), &idx, None);
    assert_eq!(out[0]["enum_name"], Value::Null);
    assert_eq!(sort_key("NonecategoryID"), sort_key(&format!("None{}", "categoryID")));
}

#[test]
fn every_feature_a_gate_requires_gets_its_own_row_to_intersect() {
    let mut store = Store::from_payload(serde_json::Map::new(), "typescript");
    let mut gates: IndexMap<String, (Option<String>, Vec<String>)> = IndexMap::new();
    gates.insert("g:1".into(), (Some("ng:1".into()), vec!["A".into(), "B".into()]));
    gates.insert("g:2".into(), (None, vec![]));
    assert_eq!(write_gate_features(&mut store, &gates), 2);
    let rows = store.emitted.get("gate_features").unwrap();
    assert_eq!(rows[1]["feature"], json!("B"));
    assert_eq!(rows[1]["component"], json!("ng:1"));
}

#[test]
fn no_ways_at_all_is_no_restriction_and_nothing_unreadable() {
    let gates = map(&[("g:1", gate("in", &["A"]))]);
    assert!(fold_values(&[], &gates, &idx_of(FOUR), None).is_empty());
    assert!(unreadable_values(&[], &gates, &idx_of(FOUR), None).is_empty());
}
