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
    At { file: "f:1", member: "m:1", class: None, line: 5, bound: None }
}

fn call(name: &str, first: Value) -> Value {
    json!({"$call": name, "$args": [first], "$target": target(name), "$expr": format!("{name}(..)")})
}

#[test]
fn a_helper_returning_its_first_argument_is_read_through_and_a_url_builder_is_what_it_returns() {
    let w = world();
    let url = json!({"$expr": "url"});
    // `createQueryUrl(url)`: the base is the parameter, and a caller's own url folds to the argument.
    assert_eq!(w.base(&call("createQueryUrl", url.clone()), &at()).as_deref(), Some("url"));
    assert_eq!(w.url(&call("createQueryUrl", json!("/calculations/count")), &at()), "/calculations/count");
    // `getBase(id)` returns something else: the parameter handed to it is no base, and (rows 88) the call is what the builder returns, the suffix kept.
    let id = json!({"$expr": "url"});
    let suffixed = json!({"$template": ["", "/get-vinculants"], "$hole_values": [call("getBase", id.clone())]});
    assert_eq!(w.base(&suffixed, &at()), None);
    assert_eq!(finished(&w.url(&suffixed, &at())), "*/vehicle/get-vinculants");
    // (rows 88) A BUILDER'S PARAMETER IS WHAT ITS CALLER HANDED IN: `ordersUrl('list')` over `${this.root}orders/${relative}`, and a caller's own parameter stays `{}`.
    let mut built = world();
    built.callees.insert(("/abs/api.ts".into(), "ordersUrl".into()), vec!["m:5".into()]);
    built.params.insert("m:5".into(), vec!["relative".into()]);
    built.returns.insert("m:5".into(), vec![json!({"$template": ["", "orders/", ""], "$hole_values": [{"$expr": "this.root"}, {"$expr": "relative"}]})]);
    assert_eq!(finished(&built.url(&call("ordersUrl", json!("list")), &at())), "*/orders/list");
    assert_eq!(finished(&built.url(&call("ordersUrl", json!({"$expr": "url"})), &at())), "*/orders/{}");
    // THE CONTROL: a builder whose returns fold apart is an unknown piece, the suffix after it kept.
    built.returns.get_mut("m:5").unwrap().push(json!("/other"));
    let apart = json!({"$template": ["", "/x"], "$hole_values": [call("ordersUrl", json!("list"))]});
    assert_eq!(finished(&built.url(&apart, &at())), "*/x");
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
        json!({"id": "call:1", "file": "f:1", "line": 4, "member": "m:1", "callee_id": "", "target": primitive, "args": [{"$expr": "url"}], "arg_params": []}),
        json!({"id": "call:2", "file": "f:1", "line": 9, "member": "m:2", "callee_id": "m:1", "args": [{"$expr": "url"}], "arg_params": []}),
        json!({"id": "call:3", "file": "f:1", "line": 12, "member": "m:4", "callee_id": "m:1",
            "args": [{"$template": ["", "/get-vinculants"], "$hole_values": [call("getBase", json!({"$expr": "id"}))]}], "arg_params": []}),
    ]);
    let sites = load(&conn, &files_of(&conn, "typescript").unwrap()).unwrap();
    let urls: Vec<(&str, &str)> = sites.iter().map(|s| (s.call.as_str(), s.url.as_str())).collect();
    // `count` is no wrapper (its url is a local the helper made, a base nobody hands in): its call reads the argument of the helper, query cut.
    assert!(urls.contains(&("call:2", "*/calculations/count")), "{urls:?}");
    // `vinculants` hands an id to a url builder: not a wrapper either, and its own call is what the builder returns, the suffix kept (rows 88).
    assert!(urls.contains(&("call:3", "*/vehicle/get-vinculants")), "{urls:?}");
}

/// `createQueryUrl(url, parameter)`: `if (..) { const parsedUrl = new URL(url); return parsedUrl.toString(); } return url + stringify(parameter);`
fn url_builder(first: Value, returns: Vec<Value>) -> World {
    let mut w = world();
    w.returns.insert("fn:1".into(), returns);
    w.locals.insert("fn:1".into(), vec![("parsedUrl".into(), 5, json!({"$new": "URL", "$args": [first, {"$expr": "base"}]}))]);
    w
}

fn to_string_of_local() -> Value {
    json!({"$call": "parsedUrl.toString", "$args": [], "$owner": "URL", "$target": {"name": "toString", "file": "/lib/lib.dom.d.ts"}})
}

fn concat(operands: Vec<Value>) -> Value {
    json!({"$concat": operands})
}

#[test]
fn a_concatenation_opening_with_the_argument_and_a_url_object_made_of_it_hand_it_back() {
    let url = json!({"$expr": "url"});
    let stringify = json!({"$call": "stringify", "$args": [{"$expr": "parameter"}], "$target": target("stringify")});
    let both = vec![to_string_of_local(), concat(vec![url.clone(), stringify.clone()])];
    let w = url_builder(url.clone(), both.clone());
    // A caller's url folds to its first argument (the query is cut), and a wrapper's base reads through it.
    assert_eq!(finished(&w.url(&call("createQueryUrl", json!("/x/y")), &at())), "/x/y");
    assert_eq!(w.base(&call("createQueryUrl", url.clone()), &at()).as_deref(), Some("url"));
    // A longer chain nests to the left: its first operand is still the argument; a query's own text is a tail.
    let chain = concat(vec![concat(vec![url.clone(), json!("?x=")]), stringify.clone()]);
    assert_eq!(url_builder(url.clone(), vec![chain]).base(&call("createQueryUrl", url.clone()), &at()).as_deref(), Some("url"));
    // `.href` of the same object is the same url.
    let href = json!({"$expr": "parsedUrl.href"});
    assert_eq!(url_builder(url.clone(), vec![href]).base(&call("createQueryUrl", url.clone()), &at()).as_deref(), Some("url"));
    // THE CONTROLS: every return must forward. One that does not (`/fixed`) is no helper.
    let mut one_bad = both.clone();
    one_bad.push(json!("/fixed"));
    assert_eq!(url_builder(url.clone(), one_bad).base(&call("createQueryUrl", url.clone()), &at()), None);
    // A concatenation that OPENS with something else, or that appends path text of its own, changes the url.
    let opens_elsewhere = concat(vec![stringify.clone(), url.clone()]);
    assert_eq!(url_builder(url.clone(), vec![opens_elsewhere]).base(&call("createQueryUrl", url.clone()), &at()), None);
    let path = concat(vec![url.clone(), json!("/orders"), stringify]);
    assert_eq!(url_builder(url.clone(), vec![path]).base(&call("createQueryUrl", url.clone()), &at()), None);
    // A URL object made of something other than the first parameter, a receiver that is no `URL`, and a local the callee never held.
    let other = url_builder(json!({"$expr": "other"}), vec![to_string_of_local()]);
    assert_eq!(other.base(&call("createQueryUrl", url.clone()), &at()), None);
    let mut foreign = url_builder(url.clone(), vec![to_string_of_local()]);
    foreign.returns.insert("fn:1".into(), vec![json!({"$call": "parsedUrl.toString", "$args": [], "$owner": "Buffer"})]);
    assert_eq!(foreign.base(&call("createQueryUrl", url.clone()), &at()), None);
    let mut none = url_builder(url.clone(), vec![to_string_of_local()]);
    none.locals.clear();
    assert_eq!(none.base(&call("createQueryUrl", url), &at()), None);
}
