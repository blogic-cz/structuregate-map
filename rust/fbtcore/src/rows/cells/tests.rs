use super::*;
use serde_json::json;

fn round_trip(value: &Value) -> Value {
    // Through the real writer and the real reader: the table the store makes, nothing hand-built.
    let db = Connection::open_in_memory().unwrap();
    Writer::new(&db, "t", "c").unwrap().tree("x:1", value).unwrap();
    load_all(&db, "t", "c").unwrap().remove("x:1").unwrap()
}

#[test]
fn every_structure_comes_back_as_it_went_in() {
    let tree = json!({
        "k": "Source", "src": "a.b ? 1 : 'x'",
        "ast": {"k": "Cond",
                "cond": {"k": "Read", "name": "b", "receiver": {"k": "Read", "name": "a", "receiver": {"k": "Implicit"}},
                         "target": {"row": "m:1"}, "param": true},
                "then": {"k": "Literal", "v": 1},
                "else": {"k": "Literal", "v": "x"}},
        "args": [], "empty": {}, "gone": null,
        "keys": [{"key": "a", "quoted": false}, {"key": "b", "quoted": true}],
        "mixed": [1, "1", true, null, 2.5, [], [[["deep"]]]],
        "id": "a member called id", "parent": 7, "value": "a member called value"
    });
    assert_eq!(round_trip(&tree), tree);
    assert_eq!(round_trip(&json!([])), json!([]));
    assert_eq!(round_trip(&json!(["a", "b"])), json!(["a", "b"]));
    assert_eq!(round_trip(&json!({})), json!({}));
}

#[test]
fn a_boolean_is_never_a_number_and_a_number_never_a_boolean() {
    let back = round_trip(&json!([true, 1, false, 0, {"v": true}, {"v": 1}]));
    assert_eq!(back, json!([true, 1, false, 0, {"v": true}, {"v": 1}]));
    assert!(back[0].is_boolean() && back[1].is_number() && back[4]["v"].is_boolean() && back[5]["v"].is_number());
}

#[test]
fn a_tree_deeper_than_any_cap_is_kept_whole() {
    // Deeper than serde's own default of 128 and the 60 a template expression was once cut at.
    let mut tree = json!({"k": "Read", "name": "leaf"});
    for i in 0..500 {
        tree = json!({"k": "Binary", "op": "+", "left": tree, "right": {"k": "Literal", "v": i}});
    }
    let text = serde_json::to_string(&tree).unwrap();
    assert_eq!(deep(&text).unwrap(), tree);
    assert_eq!(round_trip(&tree), tree);
}

#[test]
fn every_node_is_a_row_joined_by_owner_and_parent() {
    let mut rows = Vec::new();
    flatten("x:9", &json!({"k": "Call", "args": [{"k": "Read", "name": "a"}]}), &mut rows);
    let shape: Vec<String> = rows
        .iter()
        .map(|r| format!("{}/{} p={} {} {}={}", r["owner"].as_str().unwrap(), r["n"], r["parent"], r["role"], r["node"].as_str().unwrap(), r["value"]))
        .collect();
    assert_eq!(shape, [
        "x:9/0 p=null null object=null",
        "x:9/1 p=0 \"k\" string=\"Call\"",
        "x:9/2 p=0 \"args\" list=null",
        "x:9/3 p=2 \"args\" object=null",
        "x:9/4 p=3 \"k\" string=\"Read\"",
        "x:9/5 p=3 \"name\" string=\"a\"",
    ]);
    assert_eq!(rows[3]["ord"], 0);
}

#[test]
fn a_member_name_is_a_role_never_a_column_so_names_that_are_data_cannot_collide() {
    // Ids keying a map, an object literal's own keys, and two names SQLite would call one column.
    let senses = json!({"br:12": "then", "!arm:x#1": "else", "Home": 1, "home": true, "id": "a member called id"});
    let mut rows = Vec::new();
    flatten("k:1", &senses, &mut rows);
    let columns: std::collections::BTreeSet<&String> = rows.iter().flat_map(|r| r.as_object().unwrap().keys()).collect();
    let fixed: std::collections::BTreeSet<String> = COLUMNS.iter().map(|c| c.to_string()).collect();
    assert_eq!(columns.into_iter().cloned().collect::<std::collections::BTreeSet<_>>(), fixed);
    assert!(rows.iter().any(|r| r["role"] == "br:12" && r["value"] == "then"));
    assert!(rows.iter().any(|r| r["role"] == "home" && r["node"] == "bool" && r["value"] == 1));
    assert_eq!(round_trip(&senses), senses);
}

#[test]
fn the_store_writes_every_structure_as_nodes_and_reads_it_back_whole() {
    let db = Connection::open_in_memory().unwrap();
    let ast = json!({"k": "Cond", "cond": {"k": "Read", "name": "a", "param": true}, "then": {"k": "Literal", "v": 1}, "else": null});
    let rows = vec![
        json!({"id": "x:1", "file": "f:1", "ast": ast.clone(), "reads": ["a", "b"], "line": 3}),
        json!({"id": "x:2", "file": "f:1", "ast": null, "reads": [], "line": 4}),
        json!({"id": "x:3", "file": "f:2", "reads": "not a list here", "line": 5}),
    ];
    super::super::store::insert_rows(&db, "expressions", &rows).unwrap();
    let parent = schema::existing_columns(&db, "expressions").unwrap();
    assert!(!parent.contains(&"ast".to_string()), "a column only ever structured stays out of its table: {parent:?}");
    assert!(parent.contains(&"reads".to_string()), "a column scalar in some row keeps it: {parent:?}");
    let asts = load_all(&db, "expressions", "ast").unwrap();
    assert_eq!(asts.get("x:1"), Some(&ast));
    assert_eq!(asts.get("x:2"), None, "a null cell is no node at all");
    let reads = load_all(&db, "expressions", "reads").unwrap();
    assert_eq!(reads.get("x:1"), Some(&json!(["a", "b"])));
    assert_eq!(reads.get("x:2"), Some(&json!([])));
    let scalar: String = db.query_row("SELECT reads FROM expressions WHERE id = 'x:3'", [], |r| r.get(0)).unwrap();
    assert_eq!(scalar, "not a list here");
    // A DELETE OF THE ROWS TAKES THEIR NODES, through the rows: the nodes carry no scope of their own.
    delete_children(&db, "expressions", "t.file = ?1", &[&"f:1"]).unwrap();
    let left: i64 = db.query_row("SELECT count(*) FROM expressions__ast", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 0, "every node of a row of f:1 is gone");
    let kept: i64 = db.query_row("SELECT count(*) FROM expressions__reads WHERE owner = 'x:3'", [], |r| r.get(0)).unwrap();
    assert_eq!(kept, 0, "x:3 held a scalar - it never had nodes");
}

#[test]
fn dropping_a_half_takes_its_nodes_and_leaves_the_other_halves() {
    let db = Connection::open_in_memory().unwrap();
    let rows = vec![
        json!({"id": "x:1", "half": "typescript", "ast": {"k": "Read", "name": "a"}}),
        json!({"id": "x:2", "ast": {"k": "Read", "name": "kept"}}),
    ];
    super::super::store::insert_rows(&db, "expressions", &rows).unwrap();
    super::super::half::drop_half(&db, "typescript").unwrap();
    let all = load_all(&db, "expressions", "ast").unwrap();
    assert!(!all.contains_key("x:1"), "the half's nodes went with its rows");
    assert_eq!(all.get("x:2"), Some(&json!({"k": "Read", "name": "kept"})));
}
