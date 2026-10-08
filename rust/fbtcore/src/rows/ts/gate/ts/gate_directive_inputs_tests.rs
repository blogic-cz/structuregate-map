//! The tests of `gate_directive_inputs.rs`, through `directive_restrictions` - what the closure calls.
//! The rows are shaped as the TypeScript half writes them for the two directives below, minimal.

use super::super::super::gate_values::{enum_index, member_enums};
use super::super::*;
use serde_json::Map;

const DIR: &str = "C:/work/fe/app/when-mode.directive.ts";

fn read(name: &str, line: i64) -> Value {
    json!({"k": "Read", "name": name, "receiver": {"k": "This"},
           "target": {"name": name, "file": DIR, "line": line}})
}

fn cmp(op: &str) -> Value {
    json!({"k": "Binary", "op": op, "left": read("mode", 5), "right": read("activeMode", 6)})
}

/// `*whenMode="Modes.A"` over a directive whose setter stores the value in
/// `mode` and whose `updateView` renders under `if (this.mode === this.activeMode)`.
fn tables(over: Value) -> Map<String, Value> {
    let typed = json!({"name": "Modes", "file": "C:/work/fe/app/modes.ts"});
    let mut t: Map<String, Value> = serde_json::from_value(json!({
        "files": [{"id": "f:1", "path": "app/when-mode.directive.ts"}, {"id": "f:2", "path": "app/modes.ts"}],
        "enums": [{"id": "e:1", "name": "Modes", "file": "f:2",
                   "members": [{"name": "A"}, {"name": "B"}, {"name": "C"}]}],
        "classes": [{"id": "c:1", "name": "WhenModeDirective", "file": "f:1", "line": 3, "end_line": 20},
                    {"id": "c:9", "name": "ModeComponent", "file": "f:2", "line": 1, "end_line": 9}],
        "members": [
            {"id": "m:field", "class": "c:1", "name": "mode", "kind": "property", "line": 5,
             "file": "f:1", "type": "Modes", "type_ref": typed},
            {"id": "m:env", "class": "c:1", "name": "activeMode", "kind": "property", "line": 6,
             "file": "f:1", "type": "Modes", "type_ref": typed},
            {"id": "m:set", "class": "c:1", "name": "whenMode", "kind": "setter", "line": 8,
             "file": "f:1", "decorators": [{"name": "Input", "args": []}],
             "params": [{"name": "mode", "type": "Modes", "type_ref": typed}]},
            {"id": "m:view", "class": "c:1", "name": "updateView", "kind": "method", "line": 12, "file": "f:1"},
            {"id": "m:alias", "class": "c:9", "name": "Modes", "kind": "property", "line": 3,
             "file": "f:2", "type": "typeof Modes", "type_ref": typed}
        ],
        "assignments": [{"class": "c:1", "member": "m:set", "target_id": "m:field", "scope": "this",
                         "value": {"$expr": "mode", "$kind": "Identifier"}}],
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 13, "ast": cmp("===")},
            {"id": "x:gate", "ast": {"k": "Source", "ast": {"k": "Read", "name": "A",
             "receiver": {"k": "Read", "name": "Modes", "receiver": {"k": "Implicit"},
                          "target": {"name": "Modes", "file": "app/mode.component.ts", "line": 3, "row": "m:alias"}}}}}
        ],
        "branches": [{"id": "br:1", "member": "m:view", "sense": "then", "condition_expr": "x:cmp"},
                     {"id": "br:2", "member": "m:view", "sense": "else", "condition_expr": "x:cmp"}],
        "calls": [
            {"member": "m:set", "method": "updateView", "callee": "this.updateView", "line": 10},
            {"member": "m:view", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView",
             "branch": "br:1", "line": 14},
            {"member": "m:view", "method": "clear", "callee": "this.viewContainer.clear", "branch": "br:2", "line": 16}
        ],
        "returns": [],
        "selector_index": [{"selector": "[whenMode]", "class": "c:1"}],
        "gates": [{"id": "g:1", "name": "whenMode", "gate_kind": "structural", "expression": "x:gate"}]
    })).unwrap();
    if let Value::Object(map) = over {
        for (k, v) in map {
            t.insert(k, v);
        }
    }
    t
}

fn rows(over: Value) -> Vec<String> {
    let store = Store::from_payload(tables(over), "typescript");
    let idx = enum_index(&store);
    let mem = member_enums(&store, &idx);
    let (out, _) = directive_restrictions(&store, &mem, &idx);
    out.get("g:1").into_iter().flatten().map(|r| {
        format!("{} {} {}", r["dim"].as_str().unwrap_or(""), r["op"].as_str().unwrap_or(""),
                r.get("value").and_then(|v| v.as_str()).unwrap_or("-"))
    }).collect()
}

#[test]
fn a_constant_the_template_hands_to_a_setter_restricts_what_the_class_compares_it_with() {
    assert_eq!(rows(json!({})), vec!["activeMode in A"]);
}

#[test]
fn a_render_under_the_ELSE_of_an_equality_is_a_not_in() {
    // `if (a === b) { clear } else { render }`: the operator alone would say `in`.
    assert_eq!(rows(json!({"calls": [
        {"member": "m:set", "method": "updateView", "callee": "this.updateView", "line": 10},
        {"member": "m:view", "method": "clear", "callee": "this.viewContainer.clear", "branch": "br:1", "line": 14},
        {"member": "m:view", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView",
         "branch": "br:2", "line": 16}
    ]})), vec!["activeMode not_in A"]);
}

#[test]
fn an_early_return_on_inequality_renders_on_equality() {
    // `if (a !== b) { clear(); return; } render();` spells `!==` and renders on `===`.
    assert_eq!(rows(json!({
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 13, "ast": cmp("!==")},
            {"id": "x:gate", "ast": {"k": "Read", "name": "A",
             "receiver": {"k": "Read", "name": "Modes", "target": {"row": "m:alias"}}}}
        ],
        "branches": [{"id": "br:1", "member": "m:view", "sense": "then", "condition_expr": "x:cmp"}],
        "returns": [{"member": "m:view", "branch": "br:1", "line": 14}],
        "calls": [
            {"member": "m:set", "method": "updateView", "callee": "this.updateView", "line": 10},
            {"member": "m:view", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView", "line": 16}
        ]
    })), vec!["activeMode in A"]);
}

#[test]
fn a_decorated_PROPERTY_compared_with_NOT_EQUAL_in_ngOnInit_is_a_not_in() {
    // `@Input() whenMode: Modes` read straight; Angular calls `ngOnInit`, nothing in the class does.
    let field = |line| json!({"k": "Read", "name": "whenMode", "receiver": {"k": "This"},
                              "target": {"name": "whenMode", "file": DIR, "line": line}});
    assert_eq!(rows(json!({
        "members": [
            {"id": "m:set", "class": "c:1", "name": "whenMode", "kind": "property", "line": 5,
             "file": "f:1", "decorators": [{"name": "Input", "args": []}], "type": "Modes",
             "type_ref": {"name": "Modes", "file": "C:/work/fe/app/modes.ts"}},
            {"id": "m:env", "class": "c:1", "name": "activeMode", "kind": "property", "line": 6,
             "file": "f:1", "type": "Modes", "type_ref": {"name": "Modes", "file": "C:/work/fe/app/modes.ts"}},
            {"id": "m:view", "class": "c:1", "name": "ngOnInit", "kind": "method", "line": 12, "file": "f:1"},
            {"id": "m:alias", "class": "c:9", "name": "Modes", "kind": "property", "line": 3, "file": "f:2",
             "type": "typeof Modes", "type_ref": {"name": "Modes", "file": "C:/work/fe/app/modes.ts"}}
        ],
        "assignments": [],
        "expressions": [
            {"id": "x:cmp", "file": "f:1", "line": 13, "ast":
             {"k": "Binary", "op": "!==", "left": field(5), "right": read("activeMode", 6)}},
            {"id": "x:gate", "ast": {"k": "Read", "name": "B",
             "receiver": {"k": "Read", "name": "Modes", "target": {"row": "m:alias"}}}}
        ],
        "branches": [{"id": "br:1", "member": "m:view", "sense": "then", "condition_expr": "x:cmp"}],
        "calls": [{"member": "m:view", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView",
                   "branch": "br:1", "line": 14}]
    })), vec!["activeMode not_in B"]);
}

#[test]
fn a_value_that_is_not_a_constant_keeps_the_dimension_and_states_no_value() {
    assert_eq!(rows(json!({"expressions": [
        {"id": "x:cmp", "file": "f:1", "line": 13, "ast": cmp("===")},
        {"id": "x:gate", "ast": {"k": "Read", "name": "current", "receiver": {"k": "Implicit"}}}
    ]})), vec!["activeMode unknown -"]);
}

#[test]
fn a_carrier_written_by_anything_else_too_is_unknown() {
    assert_eq!(rows(json!({"assignments": [
        {"class": "c:1", "member": "m:set", "target_id": "m:field", "scope": "this",
         "value": {"$expr": "mode", "$kind": "Identifier"}},
        {"class": "c:1", "member": "m:view", "target_id": "m:field", "scope": "this",
         "value": {"$enum": "Modes.C"}}
    ]})), vec!["activeMode unknown -"]);
}

#[test]
fn a_render_site_the_comparison_does_not_guard_is_unknown() {
    // A second render, unconditional: the occurrence renders whatever the comparison says.
    assert_eq!(rows(json!({"calls": [
        {"member": "m:set", "method": "updateView", "callee": "this.updateView", "line": 10},
        {"member": "m:view", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView",
         "branch": "br:1", "line": 14},
        {"member": "m:set", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView", "line": 9}
    ]})), vec!["activeMode unknown -"]);
}

#[test]
fn a_setter_that_stores_nothing_the_class_compares_is_no_row_at_all() {
    assert!(rows(json!({"assignments": []})).is_empty());
}

// ---- a LIST handed in, `gate_directive_lists.rs` --------------------------------------

const TYPES: &str = "C:/work/fe/app/stock.ts";

fn this_read(name: &str, line: Option<i64>) -> Value {
    match line {
        Some(line) => json!({"k": "Read", "name": name, "receiver": {"k": "This"},
                             "target": {"name": name, "file": DIR, "line": line}}),
        None => json!({"k": "Read", "name": name, "receiver": {"k": "This"}}),
    }
}

fn call(receiver: Value, method: &str, args: Vec<Value>) -> Value {
    json!({"k": "Call", "receiver": {"k": "Read", "name": method, "receiver": receiver}, "args": args})
}

/// `this.items.some((c) => this.kinds.includes(c.Kind))`
fn any_item() -> Value {
    let item = json!({"k": "Read", "name": "Kind", "receiver": {"k": "Read", "name": "c", "receiver": {"k": "Implicit"}},
                      "target": {"name": "Kind", "file": TYPES, "line": 2}});
    let test = call(this_read("kinds", None), "includes", vec![item]);
    call(this_read("items", None), "some", vec![json!({"k": "Fn", "returns": [test]})])
}

/// `*whenStock="[StockKind.Alpha, StockKind.Gamma]"` over a directive of this
/// shape: the setter stores the list, `updateView` renders under `const hasItem = <any item> ||
/// (!!this.shelf && …)`, and no template binds `whenStockShelf`.
fn list_tables(over: Value) -> Map<String, Value> {
    let alias = json!({"name": "StockKind", "file": TYPES});
    let constant = |m: &str| json!({"k": "Read", "name": m, "receiver": {"k": "Read", "name": "StockKind",
                                     "receiver": {"k": "Implicit"}, "target": {"row": "m:alias"}}});
    let unbound = json!({"k": "Binary", "op": "&&",
                         "left": {"k": "Not", "expr": {"k": "Not", "expr": this_read("shelf", Some(6))}},
                         "right": call(this_read("items", None), "some", vec![json!({"k": "Fn"})])});
    let mut t: Map<String, Value> = serde_json::from_value(json!({
        "files": [{"id": "f:1", "path": "app/when-mode.directive.ts"}, {"id": "f:2", "path": "app/stock.ts"}],
        "enums": [{"id": "e:1", "name": "StockKind", "file": "f:2", "line": 1,
                   "members": [{"name": "Alpha"}, {"name": "Beta"}, {"name": "Gamma"}, {"name": "Other"}]}],
        "type_members": [{"id": "tm:1", "name": "StockKind", "file": "f:2", "line": 2,
                          "type": "StockKind", "type_ref": alias}],
        "classes": [{"id": "c:1", "name": "WhenStockDirective", "file": "f:1", "line": 3, "end_line": 30},
                    {"id": "c:9", "name": "OrdersComponent", "file": "f:2", "line": 1, "end_line": 9}],
        "members": [
            {"id": "m:types", "class": "c:1", "name": "kinds", "kind": "property", "line": 5, "file": "f:1",
             "type": "StockKind[]"},
            {"id": "m:shelf", "class": "c:1", "name": "shelf", "kind": "property", "line": 6, "file": "f:1"},
            {"id": "m:current", "class": "c:1", "name": "current", "kind": "property", "line": 7, "file": "f:1",
             "type": "StockKind", "type_ref": alias},
            {"id": "m:set", "class": "c:1", "name": "whenStock", "kind": "setter", "line": 9, "file": "f:1",
             "decorators": [{"name": "Input", "args": []}],
             "params": [{"name": "types", "type": "StockKind[]",
                         "type_ref": {"name": "Array", "element": alias}}]},
            {"id": "m:setc", "class": "c:1", "name": "whenStockShelf", "kind": "setter", "line": 13,
             "file": "f:1", "decorators": [{"name": "Input", "args": []}], "params": [{"name": "shelf", "type": "number"}]},
            {"id": "m:view", "class": "c:1", "name": "updateView", "kind": "method", "line": 17, "file": "f:1"},
            {"id": "m:alias", "class": "c:9", "name": "StockKind", "kind": "property", "line": 3, "file": "f:2",
             "type": "typeof StockKind", "type_ref": alias}
        ],
        "assignments": [
            {"class": "c:1", "member": "m:set", "target_id": "m:types", "scope": "this", "value": {"$expr": "types", "$kind": "Identifier"}},
            {"class": "c:1", "member": "m:setc", "target_id": "m:shelf", "scope": "this", "value": {"$expr": "shelf", "$kind": "Identifier"}}
        ],
        "locals": [{"id": "lo:1", "member": "m:view", "name": "hasOrder", "declared": "const", "expression": "x:has"}],
        "expressions": [
            {"id": "x:has", "file": "f:1", "line": 19, "ast": {"k": "Binary", "op": "||", "left": any_item(), "right": unbound}},
            {"id": "x:br", "file": "f:1", "line": 21, "ast": {"k": "Read", "name": "hasOrder", "receiver": {"k": "Implicit"}}},
            {"id": "x:gate", "ast": {"k": "Source", "ast": {"k": "Array", "items": [constant("Alpha"), constant("Gamma")]}}}
        ],
        "branches": [{"id": "br:1", "member": "m:view", "sense": "then", "condition_expr": "x:br"}],
        "calls": [
            {"member": "m:set", "method": "updateView", "callee": "this.updateView", "line": 11},
            {"member": "m:view", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView",
             "branch": "br:1", "line": 22}
        ],
        "returns": [],
        "bindings": [{"name": "whenStock"}],
        "selector_index": [{"selector": "[whenStock]", "class": "c:1"}],
        "gates": [{"id": "g:1", "name": "whenStock", "gate_kind": "structural", "expression": "x:gate"}]
    })).unwrap();
    if let Value::Object(map) = over {
        for (k, v) in map {
            t.insert(k, v);
        }
    }
    t
}

fn list_rows(over: Value) -> Vec<String> {
    let store = Store::from_payload(list_tables(over), "typescript");
    let idx = enum_index(&store);
    let mem = member_enums(&store, &idx);
    let (out, _) = directive_restrictions(&store, &mem, &idx);
    out.get("g:1").into_iter().flatten().map(|r| {
        format!("{} {} {} set={}", r["dim"].as_str().unwrap_or(""), r["op"].as_str().unwrap_or(""), r["values"], r["set"])
    }).collect()
}

/// The fixture's expressions with the one at `at` replaced.
fn with_expression(at: usize, ast: Value) -> Value {
    let mut exprs = list_tables(json!({}))["expressions"].clone();
    exprs[at]["ast"] = ast;
    json!({"expressions": exprs})
}

#[test]
fn a_list_the_template_hands_in_restricts_the_items_the_class_tests_against_it() {
    // Through the `const` local, and past the `||` whose side needs an input no template binds.
    assert_eq!(list_rows(json!({})), vec![r#"items.Kind in ["Alpha","Gamma"] set=true"#]);
}

#[test]
fn an_or_whose_other_input_a_template_binds_is_unknown() {
    // Bound somewhere, `shelf` may be set, and that side may be the one that held.
    let bound = json!({"bindings": [{"name": "whenStock"}, {"name": "whenStockShelf"}]});
    assert_eq!(list_rows(bound), vec!["items.Kind unknown null set=true"]);
}

#[test]
fn a_list_that_is_not_constants_of_the_enum_is_unknown() {
    let runtime = json!({"k": "Read", "name": "chosen", "receiver": {"k": "Implicit"}});
    assert_eq!(list_rows(with_expression(2, runtime)), vec!["items.Kind unknown null set=true"]);
    let empty = json!({"k": "Array", "items": []});
    assert_eq!(list_rows(with_expression(2, empty)), vec!["items.Kind unknown null set=true"]);
}

#[test]
fn a_list_tested_for_a_member_of_the_class_restricts_that_member() {
    // `const hasItem = this.kinds.includes(this.current); if (hasOrder) render();`, and its negation.
    let test = call(this_read("kinds", None), "includes", vec![this_read("current", Some(7))]);
    assert_eq!(list_rows(with_expression(0, test.clone())), vec![r#"current in ["Alpha","Gamma"] set=false"#]);
    let not = json!({"k": "Not", "expr": test});
    assert_eq!(list_rows(with_expression(0, not)), vec![r#"current not_in ["Alpha","Gamma"] set=false"#]);
}

/// The directive in its fuller form: the setter HANDS its list to `refresh(types,
/// this.shelf)`, which tests the parameter in a nested `some` inside a store callback, ORed with a side
/// that needs its `shelf` parameter truthy - filled only by a field no bound input writes.
fn handed_on(shelf_arg: Value) -> Value {
    let x_type = json!({"k": "Read", "name": "Kind", "receiver": {"k": "Read", "name": "x", "receiver": {"k": "Implicit"}},
                        "target": {"name": "Kind", "file": TYPES, "line": 2}});
    let i = json!({"k": "Read", "name": "i", "receiver": {"k": "Implicit"}});
    let inner = call(json!({"k": "Read", "name": "types", "receiver": {"k": "Implicit"}}), "some",
                     vec![json!({"k": "Fn", "returns": [{"k": "Binary", "op": "===", "left": i, "right": x_type}]})]);
    let any = call(json!({"k": "Read", "name": "items", "receiver": {"k": "Implicit"}}), "some",
                   vec![json!({"k": "Fn", "returns": [inner]})]);
    let shelf = json!({"k": "Read", "name": "shelf", "receiver": {"k": "Implicit"}});
    let side = json!({"k": "Binary", "op": "&&", "left": shelf, "right": {"k": "Literal", "v": true}});
    let mut members = list_tables(json!({}))["members"].clone();
    members.as_array_mut().unwrap().push(json!({"id": "m:check", "class": "c:1", "name": "refresh", "kind": "method",
        "line": 17, "end_line": 28, "file": "f:1", "params": [{"name": "types"}, {"name": "shelf"}]}));
    json!({
        "members": members,
        "functions": [{"id": "fn:1", "parent": "m:check", "params": [{"name": "items"}]}],
        "assignments": [{"class": "c:1", "member": "m:setc", "target_id": "m:shelf", "scope": "this",
                         "value": {"$expr": "shelf", "$kind": "Identifier"}}],
        "locals": [{"id": "lo:1", "member": "fn:1", "name": "isVisible", "declared": "const", "expression": "x:has"}],
        "expressions": [
            {"id": "x:has", "file": "f:1", "line": 19, "ast": {"k": "Binary", "op": "||", "left": any, "right": side}},
            {"id": "x:br", "file": "f:1", "line": 21, "ast": {"k": "Read", "name": "isVisible", "receiver": {"k": "Implicit"}}},
            list_tables(json!({}))["expressions"][2].clone()
        ],
        "branches": [{"id": "br:1", "member": "fn:1", "sense": "then", "condition_expr": "x:br"}],
        "calls": [
            {"member": "m:set", "method": "refresh", "callee": "this.refresh", "line": 11,
             "args": [{"$expr": "types", "$kind": "Identifier"}, shelf_arg]},
            {"member": "fn:1", "method": "createEmbeddedView", "callee": "this.viewContainer.createEmbeddedView",
             "branch": "br:1", "line": 22}
        ]
    })
}

#[test]
fn a_list_handed_to_a_method_restricts_what_its_callback_tests_the_parameter_against() {
    let never = json!({"$expr": "this.shelf", "$kind": "PropertyAccessExpression"});
    assert_eq!(list_rows(handed_on(never)), vec![r#"items.Kind in ["Alpha","Gamma"] set=true"#]);
    // A caller that fills `shelf` with something live keeps that side possible: unread.
    let live = json!({"$expr": "id", "$kind": "Identifier"});
    assert_eq!(list_rows(handed_on(live)), vec!["items.Kind unknown null set=true"]);
}
