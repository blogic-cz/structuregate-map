//! The tests of `key_branches.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `key_branches::tests`, so `super::*` is the pass.
use super::*;
use serde_json::json;

fn store_of(tables: Value) -> Store<'static> {
    Store::from_payload(tables.as_object().cloned().expect("tables"), "typescript")
}

fn nested() -> Value {
    json!({
        "branches": [
            {"id": "br:1", "sense": "then", "condition_expr": "x:1", "member": "m:1"},
            {"id": "br:2", "sense": "else", "parent": "br:1", "condition_expr": "x:2", "member": "m:1"},
        ],
        "expressions": [
            {"id": "x:1", "ast": {"k": "Read", "name": "a", "receiver": {"k": "This"}}},
            {"id": "x:2", "ast": {"k": "Read", "name": "b", "receiver": {"k": "This"}}},
        ],
    })
}

#[test]
fn the_chain_is_every_enclosing_branch_innermost_first() {
    let b = Branches::new(&store_of(nested()));
    assert_eq!(b.chain(Some("br:2".into())), vec!["br:2", "br:1"]);
    assert!(b.chain(None).is_empty());
}

#[test]
fn a_statement_joins_its_cases_to_its_branches() {
    let mut t = nested();
    t["switch_cases"] = json!([{"id": "sc:1"}, {"id": "sc:2", "parent": "sc:1"}]);
    let b = Branches::new(&store_of(t));
    let row = Row::Built(json!({"branch": "br:1", "case": "sc:2"}).as_object().cloned().unwrap());
    assert_eq!(b.chain_of(&row), vec!["br:1", "sc:2", "sc:1"]);
    assert!(b.contains("sc:1"));
}

#[test]
fn a_statement_in_a_conditional_arm_carries_that_arm_and_its_condition_resolves() {
    // `case Basic: add(c ? this.t('a') : this.t('b'))` - each call names its arm.
    let mut t = nested();
    t["switch_cases"] = json!([{"id": "sc:1"}]);
    t["calls"] = json!([{"id": "cl:1", "member": "m:1", "case": "sc:1", "choices": ["!x:2"]}]);
    let b = Branches::new(&store_of(t));
    let row = Row::Built(json!({"case": "sc:1", "choices": ["!x:2"]}).as_object().cloned().unwrap());
    assert_eq!(b.chain_of(&row), vec!["!x:2", "sc:1"]);
    assert!(b.contains("!x:2") && b.contains("x:2"));
    assert_eq!(b.sense("!x:2"), "else");
    let mut t = nested();
    t["calls"] = json!([{"id": "cl:1", "member": "m:1", "choices": ["!x:2"]}]);
    let store = store_of(t);
    let conds = Branches::new(&store).conds(&store, &super::super::gate_values::EnumIndex::default());
    assert!(conds.contains_key("!x:2") && conds.contains_key("x:2"), "both polarities are conditions");
}

fn keys(k: &[&str]) -> KeySet {
    k.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_ternary_gives_each_arm_its_own_polarity_and_nests() {
    let v = json!({"$cond_expr": "x:1", "$then": "k.a",
                   "$else": {"$cond_expr": "x:2", "$then": "k.b", "$else": "not.a.key"}});
    let mut out = Vec::new();
    value_keys(&v, &["br:1".to_string()], &keys(&["k.a", "k.b"]), &mut out);
    let chain = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    assert_eq!(out, vec![("k.a".to_string(), chain(&["br:1", "x:1"])),
                         ("k.b".to_string(), chain(&["br:1", "!x:1", "x:2"]))]);
}

#[test]
fn a_logical_operator_picks_its_side_and_nullish_states_nothing() {
    let mut out = Vec::new();
    let v = json!({"$logic": "||", "$operands": ["k.a", "k.b"], "$cond_expr": "x:1"});
    value_keys(&v, &[], &keys(&["k.a", "k.b"]), &mut out);
    assert_eq!(out[1], ("k.b".to_string(), vec!["!x:1".to_string()]));
    let mut out = Vec::new();
    let v = json!({"$logic": "??", "$operands": [{"$expr": "a"}, "k.b"], "$cond_expr": "x:1"});
    value_keys(&v, &[], &keys(&["k.b"]), &mut out);
    assert_eq!(out, vec![("k.b".to_string(), vec![])]);
}

#[test]
fn a_template_is_every_key_its_pieces_spell_and_names_its_holes() {
    let v = json!({"$template": ["menu.", ".label"], "$holes": ["name"], "$expr_id": "x:5"});
    let mut out = Vec::new();
    value_keys(&v, &["br:1".to_string()], &keys(&["menu.home.label", "menu.home.icon",
                                                  "menu..label", "other.home.label"]), &mut out);
    assert_eq!(out, vec![("menu.home.label".to_string(),
                          vec!["br:1".to_string(), "tpl:x:5=home".to_string()])],
               "a hole is at least one character, and every piece must be there");
}

#[test]
fn two_holes_split_the_key_between_its_pieces() {
    assert_eq!(segments("a.X.b.Y.c", &["a.", ".b.", ".c"]), Some(vec!["X".to_string(), "Y".to_string()]));
    assert_eq!(segments("a.X.c", &["a.", ".b.", ".c"]), None);
    assert_eq!(segments("a.X", &["a.", ""]), Some(vec!["X".to_string()]), "an empty last piece closes the key");
}

#[test]
fn a_template_with_no_leading_piece_matches_nothing() {
    let v = json!({"$template": ["", ".title"], "$holes": ["x"]});
    let mut out = Vec::new();
    value_keys(&v, &[], &keys(&["a.title"]), &mut out);
    assert!(out.is_empty());
}

#[test]
fn a_key_passed_to_a_call_is_not_the_value() {
    let mut out = Vec::new();
    let v = json!({"$call": "this.t.get", "$args": ["k.a"]});
    value_keys(&v, &[], &keys(&["k.a"]), &mut out);
    assert!(out.is_empty());
}

#[test]
fn a_resolved_constant_is_the_string_it_stands_for() {
    let mut out = Vec::new();
    let v = json!({"$member": "KEYS.hint", "value": {"$enum": "K.A", "value": "k.a"}});
    value_keys(&v, &["br:1".to_string()], &keys(&["k.a"]), &mut out);
    assert_eq!(out, vec![("k.a".to_string(), vec!["br:1".to_string()])]);
    let v = json!({"$cond_expr": "x:1", "$then": {"$index": "L[0]", "value": "k.a"}, "$else": "k.b"});
    let mut out = Vec::new();
    value_keys(&v, &[], &keys(&["k.a", "k.b"]), &mut out);
    assert_eq!(out[0], ("k.a".to_string(), vec!["x:1".to_string()]), "a constant keeps its arm's link");
}

#[test]
fn a_list_or_a_plain_object_holding_a_key_is_not_a_constant_of_it() {
    let mut out = Vec::new();
    for v in [json!([{"Message": "k.a"}]), json!({"value": "k.a"}), json!({"$member": "L", "value": ["k.a"]}),
              json!({"$index": "L[0]", "value": {"Message": "k.a"}})] {
        value_keys(&v, &[], &keys(&["k.a"]), &mut out);
    }
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn a_concat_is_every_key_its_pieces_spell() {
    let hole = json!({"$expr": "this.name"});
    let v = json!({"$concat": [{"$concat": ["regions.", hole]}, ".title"]});
    let mut out = Vec::new();
    value_keys(&v, &["br:1".to_string()], &keys(&["regions.Uk.title", "regions.Uk", "other.Uk.title"]), &mut out);
    assert_eq!(out, vec![("regions.Uk.title".to_string(), vec!["br:1".to_string()])]);
    let v = json!({"$concat": [{"$member": "K.p", "value": "regions."}, {"$concat": [hole.clone(), hole.clone()]}]});
    let mut out = Vec::new();
    value_keys(&v, &[], &keys(&["regions.U"]), &mut out);
    assert_eq!(out.len(), 1, "a constant is a piece, and two holes side by side are one - of one character");
    let v = json!({"$concat": [{"$member": "K.p", "value": "regions."}, "Uk"]});
    let mut out = Vec::new();
    value_keys(&v, &[], &keys(&["regions.Uk"]), &mut out);
    assert_eq!(out.len(), 1, "a chain with every operand known is that one string");
    let v = json!({"$concat": [hole, ".title"]});
    let mut out = Vec::new();
    value_keys(&v, &[], &keys(&["regions.Uk.title"]), &mut out);
    assert!(out.is_empty(), "no leading piece matches nothing, as a template's");
}

#[test]
fn a_choice_is_a_condition_under_both_polarities() {
    let mut t = nested();
    t["assignments"] = json!([{"member": "m:1", "value": {"$cond_expr": "x:1", "$then": "k", "$else": "j"}}]);
    let store = store_of(t);
    let b = Branches::new(&store);
    assert!(b.contains("x:1") && b.contains("!x:1"));
    let conds = b.conds(&store, &EnumIndex::default());
    assert!(!conds["x:1"].1 && conds["!x:1"].1);
}

#[test]
fn a_parent_cycle_ends_the_walk() {
    let mut t = nested();
    t["branches"][0]["parent"] = json!("br:2");
    let b = Branches::new(&store_of(t));
    assert_eq!(b.chain(Some("br:2".into())), vec!["br:2", "br:1"]);
}

#[test]
fn an_else_is_handed_over_negated_and_a_then_is_not() {
    let store = store_of(nested());
    let conds = Branches::new(&store).conds(&store, &EnumIndex::default());
    assert!(!conds["br:1"].1);
    assert!(conds["br:2"].1, "an else holds its condition NEGATED");
}

#[test]
fn a_branch_with_no_tree_is_no_condition_at_all() {
    let mut t = nested();
    t["branches"][1]["condition_expr"] = Value::Null;
    let store = store_of(t);
    let conds = Branches::new(&store).conds(&store, &EnumIndex::default());
    assert!(!conds.contains_key("br:2"));
}

fn literals() -> Value {
    json!({
        "translations": [{"key": "k.a"}],
        "branches": [{"id": "br:1", "sense": "then"}],
        "assignments": [{"value": "k.a", "owner_file": "f:1", "line": 10, "end_line": 10, "branch": "br:1"}],
        "string_literals": [{"value": "k.a", "file": "f:1", "line": 10}],
    })
}

#[test]
fn a_literal_inside_a_guarded_write_takes_that_writes_chain() {
    let store = store_of(literals());
    let ways = literal_ways(&store, &Branches::new(&store));
    assert_eq!(ways["k.a f:1"], vec![vec!["br:1".to_string()]]);
}

#[test]
fn a_second_literal_no_write_encloses_is_a_way_with_no_condition() {
    let mut t = literals();
    t["string_literals"] = json!([{"value": "k.a", "file": "f:1", "line": 10},
                                  {"value": "k.a", "file": "f:1", "line": 40}]);
    let store = store_of(t);
    let ways = literal_ways(&store, &Branches::new(&store));
    assert_eq!(ways["k.a f:1"], vec![vec!["br:1".to_string()], vec![]]);
}

#[test]
fn a_write_in_another_file_encloses_nothing() {
    let mut t = literals();
    t["assignments"][0]["owner_file"] = json!("f:9");
    let store = store_of(t);
    let ways = literal_ways(&store, &Branches::new(&store));
    assert_eq!(ways["k.a f:1"], vec![Vec::<String>::new()]);
}

#[test]
fn a_template_head_reaches_exactly_the_keys_that_start_with_it() {
    // The sorted run a head selects: no key that merely sorts beside it, every key that starts with it.
    let set = keys(&["a", "a.b", "a.b.c", "a.bz", "a.c", "ab", "b.a"]);
    assert_eq!(set.starting_with("a.b"), ["a.b", "a.b.c", "a.bz"]);
    assert_eq!(set.starting_with("a."), ["a.b", "a.b.c", "a.bz", "a.c"]);
    assert!(set.starting_with("c").is_empty());
    // A template over that head still finds each key it can spell, and only those.
    let v = json!({"$template": ["a.b", ""]});
    let mut out = Vec::new();
    value_keys(&v, &[], &set, &mut out);
    let mut found: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
    found.sort();
    assert_eq!(found, ["a.b.c", "a.bz"]);
}
