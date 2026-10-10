//! The tests of `gate_guards.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `gate_guards::tests`, so `super::*` is the pass.
use super::*;
use super::super::gate_values::enum_index;
use serde_json::{json, Map};

fn this_(name: &str) -> Value {
    json!({"k": "Read", "name": name, "receiver": {"k": "This"}})
}

fn bare(name: &str) -> Value {
    json!({"k": "Read", "name": name, "receiver": {"k": "Implicit"}})
}

fn bin(op: &str, l: Value, r: Value) -> Value {
    json!({"k": "Binary", "op": op, "left": l, "right": r})
}

fn constant(name: &str) -> Value {
    json!({"k": "Read", "name": name, "target": {"enum": "e:1"},
           "receiver": {"k": "Read", "name": "Code", "receiver": {"k": "Implicit"}}})
}

/// `!sel || (codes && codes.length > 0 && codes.find((x) => x === sel) === undefined)`
fn hide_unless_listed() -> Value {
    let find = json!({"k": "Call", "receiver": {"k": "Read", "name": "find", "receiver": this_("_codes")},
                      "args": [{"k": "Fn", "returns": [bin("===", bare("x"), this_("_sel"))]}]});
    let length = json!({"k": "Read", "name": "length", "receiver": this_("_codes")});
    bin("||", json!({"k": "Not", "expr": this_("_sel")}),
        bin("&&", bin("&&", this_("_codes"), bin(">", length, json!({"k": "Literal", "v": 0}))),
            bin("===", find, bare("undefined"))))
}

/// The directive in its fuller form: the input setter in the subclass, the copied
/// fields and the early returns in the base, and a second renderer nothing here reaches.
fn workspace(literal: Value) -> Value {
    let setter = json!([{"name": "Input", "args": ["is-x"]}]);
    json!({
        "files": [{"id": "f:1", "path": "d/base.ts"}, {"id": "f:2", "path": "d/dir.ts"}],
        "enums": [{"id": "e:1", "name": "Code", "file": "f:1", "members": [{"name": "A"}, {"name": "B"}, {"name": "C"}]}],
        "classes": [
            {"id": "c:base", "name": "Base", "file": "f:1", "line": 1, "end_line": 100},
            {"id": "c:dir", "name": "Dir", "file": "f:2", "line": 1, "end_line": 30,
             "extends": {"name": "Base", "file": "d/base.ts"}},
        ],
        "members": [
            {"id": "m:set", "class": "c:dir", "name": "config", "kind": "setter",
             "params": [{"name": "config"}], "decorators": setter},
            {"id": "m:copy", "class": "c:base", "name": "copy", "kind": "method", "params": [{"name": "config"}]},
            {"id": "m:check", "class": "c:base", "name": "check", "kind": "method"},
            {"id": "m:render", "class": "c:base", "name": "render", "kind": "method"},
            {"id": "m:other", "class": "c:base", "name": "checkOther", "kind": "method"},
        ],
        "selector_index": [{"selector": "[is-x]", "class": "c:dir"}],
        "assignments": [
            {"class": "c:base", "member": "m:copy", "scope": "this", "target": "_codes", "value": {"$expr": "config.codes"}},
            {"class": "c:base", "member": "m:copy", "scope": "this", "target": "_sel", "value": {"$expr": "config.code"}},
            {"class": "c:base", "member": "m:copy", "scope": "this", "target": "_forced", "value": {"$expr": "config.forced"}},
        ],
        "branches": [
            {"id": "br:1", "member": "m:set", "sense": "then", "condition_expr": "x:1"},
            {"id": "br:2", "member": "m:check", "sense": "then", "condition_expr": "x:2"},
            {"id": "br:3", "member": "m:check", "sense": "then", "parent": "br:2", "condition_expr": "x:3"},
            {"id": "br:4", "member": "m:check", "sense": "then", "condition_expr": "x:4"},
        ],
        "returns": [
            {"member": "m:set", "line": 12, "branch": "br:1"},
            {"member": "m:check", "line": 22, "branch": "br:2"},
            {"member": "m:check", "line": 31, "branch": "br:4"},
        ],
        "calls": [
            {"member": "m:set", "callee": "this.copy", "method": "copy", "line": 14, "owner_file": "f:2"},
            {"member": "m:set", "callee": "this.check", "method": "check", "line": 15, "owner_file": "f:2"},
            {"member": "m:check", "callee": "this.render", "method": "render", "line": 20, "branch": "br:3", "owner_file": "f:1"},
            {"member": "m:check", "callee": "this.render", "method": "render", "line": 35, "owner_file": "f:1"},
            {"member": "m:render", "callee": "this.vc.createEmbeddedView", "method": "createEmbeddedView", "line": 50, "owner_file": "f:1"},
            {"member": "m:other", "callee": "this.render", "method": "render", "line": 60, "owner_file": "f:1"},
        ],
        "expressions": [
            {"id": "x:1", "ast": {"k": "Not", "expr": bare("config")}},
            {"id": "x:2", "ast": {"k": "Not", "expr": {"k": "Not", "expr": this_("_forced")}}},
            {"id": "x:3", "ast": this_("_forced")},
            {"id": "x:4", "ast": hide_unless_listed()},
            {"id": "x:g", "ast": {"k": "Source", "ast": literal}},
        ],
        "gates": [{"id": "g:1", "name": "is-x", "expression": "x:g"}],
    })
}

fn map(entries: &[(&str, Value)]) -> Value {
    let keys: Vec<Value> = entries.iter().map(|(k, _)| json!({"key": k})).collect();
    let values: Vec<Value> = entries.iter().map(|(_, v)| v.clone()).collect();
    json!({"k": "Map", "keys": keys, "values": values})
}

fn listed() -> Value {
    map(&[("code", bare("code")), ("codes", json!({"k": "Array", "items": [constant("A"), constant("B")]}))])
}

fn run(tables: Value) -> IndexMap<String, Vec<Value>> {
    let tables: Map<String, Value> = tables.as_object().cloned().expect("tables");
    let store = Store::from_payload(tables, "typescript");
    let idx = enum_index(&store);
    guard_restrictions(&store, &MemberEnums::default(), &idx)
}

#[test]
fn a_value_the_directive_renders_only_when_listed_is_in_the_list() {
    let out = run(workspace(listed()));
    let r = &out["g:1"][0];
    assert_eq!((r["dim"].clone(), r["op"].clone(), r["values"].clone()), (json!("code"), json!("in"), json!(["A", "B"])));
}

#[test]
fn a_forced_render_the_literal_may_take_leaves_no_restriction() {
    // `forced` is a runtime value, so the first render site can happen with no membership.
    let mut entries = listed();
    entries["keys"].as_array_mut().unwrap().push(json!({"key": "forced"}));
    entries["values"].as_array_mut().unwrap().push(bare("isForced"));
    assert!(run(workspace(entries)).is_empty());
}

#[test]
fn an_empty_list_hides_nothing_so_it_restricts_nothing() {
    let entries = map(&[("code", bare("code")), ("codes", json!({"k": "Array", "items": []}))]);
    assert!(run(workspace(entries)).is_empty());
}

#[test]
fn a_render_inside_a_callback_refuses_the_directive() {
    let mut t = workspace(listed());
    t["calls"].as_array_mut().unwrap().push(json!(
        {"member": "fn:9", "callee": "this.render", "method": "render", "line": 70, "owner_file": "f:1"}));
    assert!(run(t).is_empty(), "a way in the walk cannot see");
}

#[test]
fn a_render_reachable_from_a_lifecycle_hook_refuses_the_directive() {
    let mut t = workspace(listed());
    t["members"].as_array_mut().unwrap().push(json!({"id": "m:init", "class": "c:dir", "name": "ngOnInit", "kind": "method"}));
    t["calls"].as_array_mut().unwrap().push(json!(
        {"member": "m:init", "callee": "this.checkOther", "method": "checkOther", "line": 8, "owner_file": "f:2"}));
    assert!(run(t).is_empty());
}

#[test]
fn a_field_written_from_something_else_as_well_is_not_the_config_member() {
    let mut t = workspace(listed());
    t["assignments"].as_array_mut().unwrap().push(json!(
        {"class": "c:base", "member": "m:check", "scope": "this", "target": "_codes", "value": [1]}));
    assert!(run(t).is_empty());
}

#[test]
fn a_render_after_an_unconditional_return_is_unreachable() {
    let mut t = workspace(listed());
    t["returns"].as_array_mut().unwrap().push(json!({"member": "m:check", "line": 33}));
    assert!(run(t).is_empty(), "the only reachable site is the forced one, and it cannot happen");
}
