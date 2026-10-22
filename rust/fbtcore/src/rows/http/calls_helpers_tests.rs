//! A call hands its first argument back only when the callee's own returns say so (`calls_helpers.rs`).
use super::*;
use serde_json::json;

fn target(name: &str) -> Value {
    json!({"name": name, "file": "/abs/api.ts"})
}

/// `createQueryUrl(url, params)`: returns `${url}${query}`; `getBase(id)`: returns `${this.root}vehicle`.
fn world() -> World {
    let mut w = World::default();
    w.params.insert("m:1".into(), vec!["url".into()]);
    w.callees.insert(("/abs/api.ts".into(), "createQueryUrl".into()), vec!["fn:1".into()]);
    w.params.insert("fn:1".into(), vec!["url".into(), "params".into()]);
    w.returns.insert("fn:1".into(), vec![json!({"$template": ["", "", ""], "$hole_values": [{"$expr": "url"}, {"$expr": "query"}]})]);
    w.callees.insert(("/abs/api.ts".into(), "getBase".into()), vec!["m:3".into()]);
    w.params.insert("m:3".into(), vec!["id".into()]);
    w.returns.insert("m:3".into(), vec![json!({"$template": ["", "vehicle"], "$hole_values": [{"$expr": "this.root"}]})]);
    w
}

fn at() -> At<'static> {
    At { file: "f:1", member: "m:1", class: None, line: 5 }
}

fn call(name: &str, first: Value) -> Value {
    json!({"$call": name, "$args": [first], "$target": target(name), "$expr": format!("{name}(..)")})
}

#[test]
fn a_helper_returning_its_first_argument_is_read_through_and_a_url_builder_is_an_unknown_piece() {
    let w = world();
    let url = json!({"$expr": "url"});
    // `createQueryUrl(url)`: the base is the parameter, and a caller's own url folds to the argument.
    assert_eq!(w.base(&call("createQueryUrl", url.clone()), &at()).as_deref(), Some("url"));
    assert_eq!(w.url(&call("createQueryUrl", json!("/calculations/count")), &at()), "/calculations/count");
    // `getBase(id)` returns something else: the parameter handed to it is no base, and the call is `*` before the literal suffix.
    let id = json!({"$expr": "url"});
    let suffixed = json!({"$template": ["", "/get-vinculants"], "$hole_values": [call("getBase", id.clone())]});
    assert_eq!(w.base(&suffixed, &at()), None);
    assert_eq!(finished(&w.url(&suffixed, &at())), "*/get-vinculants");
    // A callee the tree does not hold (a library function), and one that returns nothing it can read: no helper.
    assert_eq!(w.base(&call("encodeURI", id.clone()), &at()), None);
    assert!(!w.forwards_first(call("encodeURI", id.clone()).as_object().unwrap(), false));
    // A CALL NO TARGET RESOLVED keeps its old reading as a base, and a caller's fold takes none of it.
    let bare = json!({"$call": "q", "$args": [url.clone()], "$expr": "q(url)"});
    assert_eq!(w.base(&bare, &at()).as_deref(), Some("url"));
    assert_eq!(w.url(&bare, &at()), "*");
    // THE CONTROL: a return with text of its own (`${url}/x`) or a second return that is not the argument is no helper.
    let mut other = world();
    other.returns.insert("fn:1".into(), vec![json!({"$template": ["", "/x"], "$hole_values": [{"$expr": "url"}]})]);
    assert_eq!(other.base(&call("createQueryUrl", url.clone()), &at()), None);
    let mut two = world();
    two.returns.get_mut("fn:1").unwrap().push(json!("/fixed"));
    assert_eq!(two.base(&call("createQueryUrl", url), &at()), None);
}

#[test]
fn a_call_site_reads_the_helper_from_the_rows() {
    let conn = Connection::open_in_memory().unwrap();
    let rows = |table: &str, rows: Vec<Value>| crate::rows::store::insert_rows(&conn, table, &rows).unwrap();
    rows("files", vec![json!({"id": "f:1", "path": "api.ts", "abs": "/abs/api.ts", "lang": "typescript"})]);
    rows("functions", vec![json!({"id": "fn:1", "file": "f:1", "name": "createQueryUrl", "params": [{"name": "url"}, {"name": "params"}]})]);
    rows("returns", vec![json!({"id": "rv:1", "member": "fn:1", "value": {"$template": ["", "", ""], "$hole_values": [{"$expr": "url"}, {"$expr": "query"}]}})]);
    rows("members", vec![
        json!({"id": "m:1", "file": "f:1", "class": "c:1", "name": "get", "params": [{"name": "url"}], "value": ""}),
        json!({"id": "m:2", "file": "f:1", "class": "c:1", "name": "count", "params": [{"name": "options"}], "value": ""}),
        json!({"id": "m:3", "file": "f:1", "class": "c:1", "name": "getBase", "params": [{"name": "id"}], "value": ""}),
        json!({"id": "m:4", "file": "f:1", "class": "c:1", "name": "vinculants", "params": [{"name": "id"}], "value": ""}),
    ]);
    rows("returns", vec![json!({"id": "rv:2", "member": "m:3", "value": {"$template": ["", "vehicle"], "$hole_values": [{"$expr": "this.root"}]}})]);
    rows("locals", vec![json!({"id": "l:1", "file": "f:1", "member": "m:2", "name": "url", "line": 8,
        "value": call("createQueryUrl", json!({"$template": ["", "/calculations/count"], "$hole_values": [{"$expr": "this.base"}]}))})]);
    let primitive = json!({"file": "node_modules/@angular/common/http/index.d.ts", "owner": "HttpClient", "name": "get"});
    rows("calls", vec![
        json!({"id": "call:1", "file": "f:1", "line": 4, "member": "m:1", "target_id": "", "target": primitive, "args": [{"$expr": "url"}], "arg_params": []}),
        json!({"id": "call:2", "file": "f:1", "line": 9, "member": "m:2", "target_id": "m:1", "args": [{"$expr": "url"}], "arg_params": []}),
        json!({"id": "call:3", "file": "f:1", "line": 12, "member": "m:4", "target_id": "m:1",
            "args": [{"$template": ["", "/get-vinculants"], "$hole_values": [call("getBase", json!({"$expr": "id"}))]}], "arg_params": []}),
    ]);
    let sites = load(&conn, &files_of(&conn, "typescript").unwrap()).unwrap();
    let urls: Vec<(&str, &str)> = sites.iter().map(|s| (s.call.as_str(), s.url.as_str())).collect();
    // `count` is no wrapper (its url is a local the helper made, a base nobody hands in): its call reads the argument of the helper, query cut.
    assert!(urls.contains(&("call:2", "*/calculations/count")), "{urls:?}");
    // `vinculants` hands an id to a url builder: not a wrapper either, and its own call keeps the suffix.
    assert!(urls.contains(&("call:3", "*/get-vinculants")), "{urls:?}");
}
