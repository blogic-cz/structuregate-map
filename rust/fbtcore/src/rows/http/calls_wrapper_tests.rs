//! A wrapper whose url reaches the primitive through a local, a ternary or a call of its parameter (`calls.rs`).
use super::*;
use serde_json::json;

fn world() -> World {
    let mut w = World::default();
    w.params.insert("m:1".into(), vec!["http".into(), "url".into(), "params".into()]);
    w
}

fn at(line: i64) -> At<'static> {
    At { file: "f:1", member: "m:1", class: None, line }
}

/// The evaluator's value for `params ? createQueryUrl(url, params) : url`: both arms and the ternary's own text.
fn ternary() -> Value {
    json!({
        "$cond": "params",
        "$then": {"$call": "createQueryUrl", "$args": [{"$expr": "url"}, {"$expr": "params"}]},
        "$else": {"$expr": "url"},
        "$expr": "params ? createQueryUrl(url, params) : url", "$kind": "ConditionalExpression",
    })
}

#[test]
fn a_url_through_a_ternary_or_a_call_of_its_parameter_still_begins_with_it() {
    let mut w = world();
    w.locals.insert("m:1".into(), vec![("q".into(), 3, ternary())]);
    assert_eq!(w.base(&json!({"$expr": "q"}), &at(5)).as_deref(), Some("url"));
    // EITHER ARM: the parameter in the else arm only, and in the then arm only.
    let one = |then: Value, other: Value| json!({"$cond": "c", "$then": then, "$else": other, "$expr": "c ? a : b"});
    assert_eq!(w.base(&one(json!("/x"), json!({"$expr": "url"})), &at(5)).as_deref(), Some("url"));
    assert_eq!(w.base(&one(json!({"$expr": "url"}), json!("/x")), &at(5)).as_deref(), Some("url"));
    // A CALL OF THE PARAMETER AS ITS FIRST ARGUMENT only: the parameter second is an argument, not the base.
    assert_eq!(w.base(&json!({"$call": "q", "$args": [{"$expr": "url"}]}), &at(5)).as_deref(), Some("url"));
    assert_eq!(w.base(&json!({"$call": "q", "$args": ["/a", {"$expr": "url"}]}), &at(5)), None);
    assert_eq!(w.base(&one(json!("/x"), json!("/y")), &at(5)), None);
}

#[test]
fn a_wrapper_over_a_derived_local_yields_a_row_per_caller_and_none_for_its_own_body() {
    let conn = Connection::open_in_memory().unwrap();
    let rows = |table: &str, rows: Vec<Value>| crate::rows::store::insert_rows(&conn, table, &rows).unwrap();
    rows("files", vec![json!({"id": "f:1", "path": "api.service.ts", "lang": "typescript"})]);
    rows("members", vec![
        json!({"id": "m:1", "file": "f:1", "class": "", "name": "apiGet", "params": [{"name": "http"}, {"name": "url"}, {"name": "params"}], "value": ""}),
        json!({"id": "m:2", "file": "f:1", "class": "", "name": "list", "params": [], "value": ""}),
    ]);
    rows("locals", vec![json!({"id": "l:1", "file": "f:1", "member": "m:1", "name": "q", "line": 3, "value": ternary()})]);
    let primitive = json!({"file": "node_modules/@angular/common/http/index.d.ts", "owner": "HttpClient", "name": "get"});
    rows("calls", vec![
        json!({"id": "call:1", "file": "f:1", "line": 4, "member": "m:1", "callee_id": "", "target": primitive,
            "args": [{"$expr": "q"}], "arg_params": []}),
        json!({"id": "call:2", "file": "f:1", "line": 9, "member": "m:2", "callee_id": "m:1",
            "args": [{"$expr": "this.http"}, "/orders/list", {"$expr": "p"}], "arg_params": []}),
    ]);
    let sites = load(&conn, &files_of(&conn, "typescript").unwrap()).unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!((sites[0].verb.as_str(), sites[0].url.as_str(), sites[0].via.as_str()), ("GET", "/orders/list", "m:1"));
}
