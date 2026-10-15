use super::*;
use serde_json::json;

fn ternary(a: &str, b: &str) -> Value {
    json!({"$cond_expr": "c", "$then": a, "$else": b})
}

#[test]
fn every_combination_of_ternary_arguments_is_spelled_however_many() {
    // Five ternaries of two strings: 32 argument lists, past the 16 the product was once cut at.
    let args: Vec<Value> = (0..5).map(|i| ternary(&format!("a{i}"), &format!("b{i}"))).collect();
    let lists = spellings(&args);
    assert_eq!(lists.len(), 32);
    assert!(lists.contains(&vec![json!("b0"), json!("b1"), json!("b2"), json!("b3"), json!("b4")]));
}

#[test]
fn an_argument_list_is_measured_in_tree_nodes() {
    assert_eq!(size(&[]), 0);
    assert_eq!(size(&[json!("a")]), 1);
    assert_eq!(size(&[json!({"k": "Binary", "left": "x", "right": {"v": "a"}})]), 5);
}
