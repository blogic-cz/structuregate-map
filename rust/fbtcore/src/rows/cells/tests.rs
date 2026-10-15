use super::*;
use serde_json::json;

fn round_trip(value: &Value) -> Value {
    let db = Connection::open_in_memory().unwrap();
    let mut rows = Vec::new();
    flatten("x:1", value, &Map::new(), &mut rows);
    let names: Vec<String> = {
        let mut seen: Vec<String> = Vec::new();
        for row in &rows {
            for key in row.as_object().unwrap().keys() {
                if !seen.contains(key) {
                    seen.push(key.clone());
                }
            }
        }
        seen
    };
    schema::create(&db, "t__c", &names).unwrap();
    for row in &rows {
        let object = row.as_object().unwrap();
        let marks = vec!["?"; names.len()].join(",");
        let quoted: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
        let values: Vec<_> = names.iter().map(|n| schema::cell(object.get(n))).collect();
        db.execute(&format!("INSERT INTO t__c ({}) VALUES ({marks})", quoted.join(",")), rusqlite::params_from_iter(values)).unwrap();
    }
    register(&db, "t", "c").unwrap();
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
    let inherit: Map<String, Value> = [("owner_file".to_string(), json!("f:3"))].into_iter().collect();
    flatten("x:9", &json!({"k": "Call", "args": [{"k": "Read", "name": "a"}]}), &inherit, &mut rows);
    let shape: Vec<(String, String, String, String)> = rows
        .iter()
        .map(|r| {
            let s = |k: &str| r[k].as_str().map(String::from).unwrap_or_else(|| r[k].to_string());
            (s("id"), s("parent"), s("role"), format!("{}={}", s("node"), s("value")))
        })
        .collect();
    let want = [
        ("x:9/0", "null", "null", "object=null"),
        ("x:9/1", "x:9/0", "k", "string=Call"),
        ("x:9/2", "x:9/0", "args", "list=null"),
        ("x:9/3", "x:9/2", "args", "object=null"),
        ("x:9/4", "x:9/3", "k", "string=Read"),
        ("x:9/5", "x:9/3", "name", "string=a"),
    ];
    let want: Vec<(String, String, String, String)> =
        want.iter().map(|(a, b, c, d)| (a.to_string(), b.to_string(), c.to_string(), d.to_string())).collect();
    assert_eq!(shape, want);
    assert_eq!(rows[3]["ord"], 0);
    assert!(rows.iter().all(|r| r["owner"] == "x:9" && r["owner_file"] == "f:3"));
}

#[test]
fn a_member_name_is_a_role_never_a_column_so_names_that_are_data_cannot_collide() {
    // Ids keying a map, an object literal's own keys, and two names SQLite would call one column.
    let senses = json!({"br:12": "then", "!arm:x#1": "else", "Home": 1, "home": true, "id": "a member called id"});
    let mut rows = Vec::new();
    flatten("k:1", &senses, &Map::new(), &mut rows);
    let columns: std::collections::BTreeSet<&String> = rows.iter().flat_map(|r| r.as_object().unwrap().keys()).collect();
    let fixed: std::collections::BTreeSet<String> = ["id", "owner", "parent", "role", "ord", "node", "value"].iter().map(|c| c.to_string()).collect();
    assert_eq!(columns.into_iter().cloned().collect::<std::collections::BTreeSet<_>>(), fixed);
    assert!(rows.iter().any(|r| r["role"] == "br:12" && r["value"] == "then"));
    assert!(rows.iter().any(|r| r["role"] == "home" && r["node"] == "bool" && r["value"] == 1));
    assert_eq!(round_trip(&senses), senses);
}
