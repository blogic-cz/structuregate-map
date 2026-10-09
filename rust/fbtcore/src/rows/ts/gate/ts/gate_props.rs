//! A BARE BOOLEAN PROPERTY, READ AS WHAT IT IS ASSIGNED — `*ngIf="isRegionB"` and
//! `if (this.isCompact)` restrict what their property's one write compares, and a
//! reader that stops at the property's name reports nothing.
//!
//! THREE DEFINITIONS ARE READ, AND THEY ARE NOT EQUALLY STRONG:
//!   * a GETTER whose one return sits in no branch IS its expression, in both polarities —
//!     `get isX() { return this.a === A; }` is false exactly when the comparison is;
//!     one returning `E` under a top-level `if (C)` and the literal `false` everywhere else is `C && E`;
//!   * a PROPERTY with exactly one plain `=` write, no truthy initializer and no two-way
//!     binding is true only because that write ran with its expression true. So `isX`
//!     proves the expression, and `!isX` proves nothing: the write may never have run. It
//!     is handed on as an `Implied` node, which the polarity walk descends only un-negated -
//!     UNLESS the write ran before the view was first read (`before_render`: the constructor or
//!     `ngOnInit`, unconditionally, not an `@Input`), and then it is exact both ways;
//!   * a PROPERTY INITIALISED with a decision and written by nothing is that expression for its
//!     life, both ways. An evaluated call keeps no tree, so it is spelled back (`called`);
//!   * an `@Input` PROPERTY every usage binds to one tree is that tree - the parent's - both ways
//!     (`gate_input_flags`).
//!
//! A second write, a compound operator, a truthy initializer or a write the template makes
//! and the property is left alone.
//!
//! A CALL IN TRUTH POSITION is replaced by its callee's one return (`gate_calls`), so
//! `isWide(sizeID)` and `this.featureService.isEnabled()` read as what their bodies compare.
//!
//! ONLY A READ IN TRUTH POSITION IS REPLACED: the condition itself, under `!`, under `&&`/`||`,
//! or a ternary's condition. `variant === Variant.First` compares the property's VALUE, and
//! replacing `variant` with the one expression it was assigned broke the comparison: gates
//! lost the restriction they had and keys with them.

use super::astreads::unwrap;
use super::gate_calls::Calls;
use super::gate_features::template_written;
use super::gate_tsrows::TsRows;
use super::gate_values::EnumIndex;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// An initializer that leaves the property false until it is written.
fn falsy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_f64() == Some(0.0),
        Some(Value::String(s)) => s.is_empty() || s == "0" || s == "false",
        _ => false,
    }
}

/// Property id -> its definition, and whether it holds in both polarities.
pub struct Props<'a> {
    defs: IndexMap<String, (Value, bool)>,
    calls: Calls<'a>,
}

/// An initializer that can be a decision - a comparison, a negation, a condition, a logic or a call -
/// rather than a value the template only reads (`new FormControl()`, a list, an object).
fn decides(v: &Value) -> bool {
    let kind = v.get("$kind").and_then(|k| k.as_str());
    v.get("$logic").is_some() || v.get("$call").is_some()
        || matches!(kind, Some("BinaryExpression" | "PrefixUnaryExpression" | "ConditionalExpression"))
}

/// Whether a member is an `@Input`: a parent may write it whatever the class does.
fn input(m: &Row) -> bool {
    m.get("decorators").and_then(|d| d.as_array()).into_iter().flatten()
        .any(|d| d.get("name").and_then(|n| n.as_str()) == Some("Input"))
}

/// A WRITE THAT HAS RUN BEFORE THE TEMPLATE IS FIRST READ: in the constructor or `ngOnInit` of the
/// property's own class, under no branch or case, and with no `return` before it in that member.
/// Both run before Angular first checks the view, so the property is never seen unwritten and `!isX`
/// proves `!expr` as surely as `isX` proves `expr`.
fn before_render(a: &Row, class: Option<&String>, lifecycle: &IndexMap<String, String>, returns: &IndexMap<String, Vec<&Row>>) -> bool {
    let line = |r: &Row| r.get("line").and_then(|l| l.as_i64()).unwrap_or(i64::MAX);
    let Some(member) = cell(a, "member") else { return false };
    lifecycle.get(&member).is_some_and(|c| Some(c) == class)
        && ["branch", "case"].iter().all(|f| cell(a, f).is_none())
        && returns.get(&member).into_iter().flatten().all(|r| line(r) > line(a))
}

/// `show = this.featureService.isEnabled()`: an evaluated call keeps no tree, only the declaration
/// it resolved to, so the call is spelled back as the one node the call reader needs. Only a call
/// of NO argument - an evaluated argument is a value, not a tree a parameter can be replaced by.
fn called(value: Option<&Value>, rows: &TsRows) -> Option<Value> {
    let v = value?;
    if !v.get("$args")?.as_array()?.is_empty() || v.get("$call").is_none() {
        return None;
    }
    let row = rows.member_named(v.get("$target")?)?;
    let name = v.get("$target")?.get("name")?.clone();
    Some(json!({"k": "Call", "receiver": {"k": "Read", "name": name, "receiver": {"k": "Implicit"},
                "target": {"row": row}}, "args": []}))
}

impl<'a> Props<'a> {
    pub fn new(store: &Store<'_>, idx: &'a EnumIndex, claimed: &dyn Fn(&str) -> bool) -> Props<'a> {
        let calls = Calls::new(store, idx, TsRows::new(store), claimed);
        let defs = Self::defs(store, idx, calls.rows());
        Props { defs, calls }
    }

    fn defs(store: &Store<'_>, idx: &EnumIndex, rows: &TsRows) -> IndexMap<String, (Value, bool)> {
        let mut asts: IndexMap<String, &Row> = IndexMap::new();
        let expressions = store.table("expressions");
        // BY ROW, NOT BY TREE: an AST is decoded when it is read (`store::Cell`), so only the trees a lookup
        // asks for are ever decoded.
        for e in expressions.iter() {
            if let Some(id) = cell(e, "id") {
                asts.insert(id, e);
            }
        }
        let mut bound: IndexMap<String, IndexSet<String>> = IndexMap::new();
        for f in store.table("functions").iter() {
            let Some(parent) = cell(f, "parent") else { continue };
            let names = f.get("params").and_then(|p| p.as_array()).into_iter().flatten()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(str::to_string));
            bound.entry(parent).or_default().extend(names);
        }
        let none = IndexSet::new();
        let tree = |x: Option<String>, member: Option<String>| -> Option<Value> {
            let ast = asts.get(&x?)?.get("ast").filter(|a| !a.is_null())?;
            let names = member.as_ref().and_then(|m| bound.get(m)).unwrap_or(&none);
            Some(rows.resolve_in(unwrap(ast), names, idx, member.as_deref()))
        };

        let mut returns: IndexMap<String, Vec<&Row>> = IndexMap::new();
        let all_returns = store.table("returns");
        for r in all_returns.iter() {
            if let Some(m) = cell(r, "member") {
                returns.entry(m).or_default().push(r);
            }
        }
        let mut writes: IndexMap<String, Vec<&Row>> = IndexMap::new();
        let assignments = store.table("assignments");
        for a in assignments.iter() {
            if let Some(t) = cell(a, "target_id") {
                writes.entry(t).or_default().push(a);
            }
        }
        // A TOP-LEVEL `then` a getter's return may sit under - see the getter arm below.
        let all_branches = store.table("branches");
        let guards: IndexMap<String, &Row> = all_branches.iter()
            .filter(|b| cell(b, "sense").as_deref() == Some("then") && cell(b, "parent").is_none())
            .filter_map(|b| cell(b, "id").map(|id| (id, b)))
            .collect();
        let written_by_template = template_written(store);
        let mut lifecycle: IndexMap<String, String> = IndexMap::new();
        for m in store.table("members").iter() {
            let first = cell(m, "kind").as_deref() == Some("constructor") || cell(m, "name").as_deref() == Some("ngOnInit");
            if let (true, Some(id), Some(class)) = (first, cell(m, "id"), cell(m, "class")) {
                lifecycle.insert(id, class);
            }
        }

        // AN `@Input` FLAG every usage binds alike is the parent's expression, both ways.
        let written: IndexSet<String> = writes.keys().cloned().collect();
        let mut out: IndexMap<String, (Value, bool)> = super::gate_input_flags::bound_inputs(store, &written)
            .into_iter().map(|(id, tree)| (id, (tree, true))).collect();
        for m in store.table("members").iter() {
            let Some(id) = cell(m, "id") else { continue };
            if out.contains_key(&id) {
                continue;
            }
            let two_way = cell(m, "class")
                .zip(cell(m, "name"))
                .is_some_and(|(c, n)| written_by_template.contains(&format!("{c} {n}")));
            match cell(m, "kind").as_deref() {
                Some("getter") => {
                    let rs = returns.get(&id).map(|v| v.as_slice()).unwrap_or_default();
                    let plain = |r: &Row| ["branch", "case"].iter().all(|f| cell(r, f).is_none());
                    if let [r] = rs {
                        if let Some(ast) = tree(cell(r, "expression"), Some(id.clone())).filter(|_| plain(r)) {
                            out.insert(id, (ast, true));
                        }
                        continue;
                    }
                    // `if (C) { return E; } return false;` IS `C && E`, both ways: one return under one
                    // top-level `then`, and every other return the literal `false`.
                    let (falses, kept): (Vec<&&Row>, Vec<&&Row>) =
                        rs.iter().partition(|r| plain(r) && cell(r, "source").as_deref() == Some("false"));
                    let (Some(_), [r]) = (falses.first(), kept.as_slice()) else { continue };
                    let Some(b) = cell(r, "branch").and_then(|b| guards.get(&b)).filter(|_| cell(r, "case").is_none()) else { continue };
                    let cond = tree(cell(b, "condition_expr"), Some(id.clone()));
                    if let (Some(c), Some(e)) = (cond, tree(cell(r, "expression"), Some(id.clone()))) {
                        out.insert(id, (json!({"k": "Binary", "op": "&&", "left": c, "right": e}), true));
                    }
                }
                Some("property") if !two_way && falsy(m.get("value")) => {
                    let Some([a]) = writes.get(&id).map(|v| v.as_slice()) else { continue };
                    if cell(a, "operator").as_deref() != Some("=") {
                        continue;
                    }
                    let exact = !input(m) && before_render(a, cell(m, "class").as_ref(), &lifecycle, &returns);
                    if let Some(ast) = tree(cell(a, "expression"), cell(a, "member")) {
                        out.insert(id, (ast, exact));
                    }
                }
                // AN INITIALIZER NOTHING OVERWRITES is the property's value for its whole life.
                Some("property") if !two_way && !input(m) && !writes.contains_key(&id) => {
                    let value = m.get("value").filter(|v| v.get("$fn").is_none() && decides(v));
                    let expr = value.and_then(|v| v.get("$expr_id")).and_then(|x| x.as_str()).map(str::to_string);
                    if let Some(ast) = called(value, rows).or_else(|| tree(expr, None)) {
                        out.insert(id, (ast, true));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// `ast` with every read of a defined property replaced by its definition, to a small
    /// depth: a definition may read another defined property.
    pub fn expand(&self, ast: &Value) -> Value {
        if self.defs.is_empty() && self.calls.is_empty() {
            return ast.clone();
        }
        let mut out = ast.clone();
        self.walk(&mut out, true, &mut Vec::new());
        out
    }

    /// A call in truth position, replaced by its callee's return (`gate_calls`).
    fn inline(&self, n: &mut Value, seen: &mut Vec<String>) {
        let Some((row, mut inner)) = self.calls.inline(n).filter(|(row, _)| !seen.contains(row)) else { return };
        seen.push(row);
        self.walk(&mut inner, true, seen);
        seen.pop();
        *n = inner;
    }

    fn walk(&self, n: &mut Value, truth: bool, seen: &mut Vec<String>) {
        let kind = n.get("k").and_then(|k| k.as_str()).map(str::to_string);
        let logic = n.get("op").and_then(|o| o.as_str()).is_some_and(|o| o == "&&" || o == "||");
        match n {
            Value::Array(items) => {
                for i in items.iter_mut() {
                    self.walk(i, false, seen);
                }
                return;
            }
            Value::Object(o) => {
                for (key, v) in o.iter_mut() {
                    let child_truth = match (kind.as_deref(), key.as_str()) {
                        (Some("Source"), "ast") | (Some("Not"), "expr") | (Some("Cond"), "cond") => truth || key == "cond",
                        (Some("Binary"), "left" | "right") => truth && logic,
                        _ => false,
                    };
                    self.walk(v, child_truth, seen);
                }
            }
            _ => return,
        }
        if !truth || seen.len() > 4 {
            return;
        }
        if matches!(kind.as_deref(), Some("Call" | "SafeCall")) {
            return self.inline(n, seen);
        }
        let read =matches!(n.get("k").and_then(|k| k.as_str()), Some("Read") | Some("SafeRead"));
        let own = matches!(n.get("receiver").and_then(|r| r.get("k")).and_then(|k| k.as_str()), Some("This") | Some("Implicit"));
        let Some(row) = n.get("target").and_then(|t| t.get("row")).and_then(|r| r.as_str()).map(str::to_string) else { return };
        if !read || !own || seen.contains(&row) {
            return;
        }
        let Some((def, exact)) = self.defs.get(&row) else { return };
        seen.push(row);
        let mut inner = def.clone();
        self.walk(&mut inner, true, seen);
        seen.pop();
        *n = if *exact { inner } else { json!({"k": "Implied", "expr": inner}) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn read(name: &str, row: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": {"k": "This"}, "target": {"row": row}})
    }

    fn cmp() -> Value {
        json!({"k": "Binary", "op": "===", "left": {"k": "Read", "name": "a", "receiver": {"k": "This"}},
               "right": {"k": "Literal", "v": 1}})
    }

    fn props(tables: Value) -> Props<'static> {
        let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
        let store = Store::from_payload(map, "typescript");
        Props::new(&store, Box::leak(Box::new(EnumIndex::default())), &|_| false)
    }

    fn written(extra_writes: Value, init: Value) -> Value {
        let mut writes = vec![json!({"target_id": "m:p", "operator": "=", "expression": "x:1", "member": "m:w"})];
        writes.extend(extra_writes.as_array().cloned().unwrap_or_default());
        json!({
            "members": [{"id": "m:p", "class": "c:1", "name": "isX", "kind": "property", "value": init}],
            "assignments": writes,
            "expressions": [{"id": "x:1", "ast": cmp()}],
        })
    }

    #[test]
    fn a_property_written_once_is_its_expression_when_true_only() {
        let p = props(written(json!([]), json!(false)));
        assert_eq!(p.expand(&read("isX", "m:p")), json!({"k": "Implied", "expr": cmp()}));
    }

    #[test]
    fn a_second_write_or_a_truthy_initializer_leaves_the_property_alone() {
        let twice = props(written(json!([{"target_id": "m:p", "operator": "=", "expression": "x:1"}]), json!(false)));
        assert_eq!(twice.expand(&read("isX", "m:p")), read("isX", "m:p"));
        let started = props(written(json!([]), json!(true)));
        assert_eq!(started.expand(&read("isX", "m:p")), read("isX", "m:p"));
    }

    #[test]
    fn a_getter_with_one_unconditional_return_is_its_expression_both_ways() {
        let p = props(json!({
            "members": [{"id": "m:g", "class": "c:1", "name": "isX", "kind": "getter"}],
            "returns": [{"member": "m:g", "expression": "x:1"}],
            "expressions": [{"id": "x:1", "ast": cmp()}],
        }));
        assert_eq!(p.expand(&read("isX", "m:g")), cmp());
    }

    /// `get isX() { if (C) { return E; } return false; }`, the branch's sense and parent as given.
    fn guarded(branch: Value, last: &str) -> Value {
        json!({
            "members": [{"id": "m:g", "class": "c:1", "name": "isX", "kind": "getter"}],
            "branches": [branch],
            "returns": [{"member": "m:g", "expression": "x:e", "branch": "br:1", "line": 3},
                        {"member": "m:g", "source": last, "line": 5}],
            "expressions": [{"id": "x:c", "ast": cmp()}, {"id": "x:e", "ast": read("e", "m:e")}],
        })
    }

    #[test]
    fn a_getter_returning_under_one_if_and_false_after_it_is_the_if_and_the_return() {
        let then = json!({"id": "br:1", "sense": "then", "condition_expr": "x:c"});
        let both = json!({"k": "Binary", "op": "&&", "left": cmp(), "right": read("e", "m:e")});
        assert_eq!(props(guarded(then.clone(), "false")).expand(&read("isX", "m:g")), both);
        // Anything else after the `if` may be true, and an `else` or a nested `if` is another condition.
        assert_eq!(props(guarded(then, "true")).expand(&read("isX", "m:g")), read("isX", "m:g"));
        let other = json!({"id": "br:1", "sense": "else", "condition_expr": "x:c"});
        assert_eq!(props(guarded(other, "false")).expand(&read("isX", "m:g")), read("isX", "m:g"));
        let nested = json!({"id": "br:1", "sense": "then", "condition_expr": "x:c", "parent": "br:0"});
        assert_eq!(props(guarded(nested, "false")).expand(&read("isX", "m:g")), read("isX", "m:g"));
    }

    #[test]
    fn a_property_compared_by_value_is_not_replaced() {
        // `variant === Variant.First` reads the property's VALUE; its one write says nothing
        // about which value that is.
        let p = props(written(json!([]), json!(false)));
        let n = json!({"k": "Binary", "op": "===", "left": read("isX", "m:p"), "right": {"k": "Literal", "v": 2}});
        assert_eq!(p.expand(&n), n);
        let under_and = json!({"k": "Binary", "op": "&&", "left": read("isX", "m:p"), "right": {"k": "Literal", "v": true}});
        assert_eq!(p.expand(&under_and)["left"]["k"], json!("Implied"), "a truth operand still is");
    }

    #[test]
    fn a_read_that_is_a_receiver_is_not_the_propertys_value() {
        let p = props(written(json!([]), json!(false)));
        let n = json!({"k": "Read", "name": "length", "receiver": read("isX", "m:p")});
        assert_eq!(p.expand(&n), n);
    }

    #[test]
    fn a_property_the_template_writes_is_left_alone() {
        let mut t = written(json!([]), json!(false));
        t["components"] = json!([{"id": "ng:1", "class": "c:1"}]);
        t["bindings"] = json!([{"kind": "output", "component": "ng:1", "source": "isX =$event"}]);
        assert_eq!(props(t).expand(&read("isX", "m:p")), read("isX", "m:p"));
    }

    /// The one write sits in `m:w`, here named and classed as the caller says.
    fn written_in(kind: &str, name: &str, write: Value, member: Value) -> Value {
        let mut t = written(json!([]), json!(false));
        t["members"].as_array_mut().unwrap().push(json!({"id": "m:w", "class": "c:1", "kind": kind, "name": name}));
        for (k, v) in write.as_object().cloned().unwrap_or_default() {
            t["assignments"][0][k] = v;
        }
        for (k, v) in member.as_object().cloned().unwrap_or_default() {
            t["members"][0][k] = v;
        }
        t
    }

    #[test]
    fn a_write_in_ngoninit_or_the_constructor_is_its_expression_both_ways() {
        // Both run before Angular first reads the view, so `!isX` is `!expr`.
        for (kind, name) in [("method", "ngOnInit"), ("constructor", "constructor")] {
            let p = props(written_in(kind, name, json!({"line": 5}), json!({})));
            assert_eq!(p.expand(&read("isX", "m:p")), cmp(), "{name}");
        }
    }

    #[test]
    fn a_write_elsewhere_after_a_return_under_a_branch_or_into_an_input_is_true_only() {
        let implied = json!({"k": "Implied", "expr": cmp()});
        let p = props(written_in("method", "onClick", json!({}), json!({})));
        assert_eq!(p.expand(&read("isX", "m:p")), implied, "a handler may never run");
        let mut t = written_in("method", "ngOnInit", json!({"line": 5}), json!({}));
        t["returns"] = json!([{"member": "m:w", "line": 3}]);
        assert_eq!(props(t).expand(&read("isX", "m:p")), implied, "an early return may skip it");
        let p = props(written_in("method", "ngOnInit", json!({"branch": "br:1"}), json!({})));
        assert_eq!(p.expand(&read("isX", "m:p")), implied);
        let p = props(written_in("method", "ngOnInit", json!({}), json!({"decorators": [{"name": "Input"}]})));
        assert_eq!(p.expand(&read("isX", "m:p")), implied, "a parent may write an input");
    }

    #[test]
    fn an_initializer_nothing_overwrites_is_its_expression_both_ways() {
        let t = json!({
            "members": [{"id": "m:p", "class": "c:1", "name": "isX", "kind": "property",
                         "value": {"$expr": "this.a === 1", "$kind": "BinaryExpression", "$expr_id": "x:1"}}],
            "expressions": [{"id": "x:1", "ast": cmp()}],
        });
        assert_eq!(props(t.clone()).expand(&read("isX", "m:p")), cmp());
        // ...but not once anything writes it, and never a value that decides nothing.
        let mut w = t.clone();
        w["assignments"] = json!([{"target_id": "m:p", "operator": "=", "expression": "x:1"}]);
        assert_eq!(props(w).expand(&read("isX", "m:p")), read("isX", "m:p"));
        let mut made = t;
        made["members"][0]["value"]["$kind"] = json!("NewExpression");
        assert_eq!(props(made).expand(&read("isX", "m:p")), read("isX", "m:p"));
    }

    #[test]
    fn a_call_in_truth_position_is_its_callees_return() {
        // `!isWide(sizeID)` over `isWide(p) { return this.a === 1; }` - the call reads the method's body.
        let t = json!({
            "members": [{"id": "m:is", "class": "c:1", "name": "isWide", "kind": "method", "params": [{"name": "p"}]}],
            "returns": [{"member": "m:is", "expression": "x:1"}],
            "expressions": [{"id": "x:1", "ast": cmp()}],
        });
        let call = json!({"k": "Call", "receiver": {"k": "Read", "name": "isWide", "receiver": {"k": "Implicit"},
                          "target": {"row": "m:is"}}, "args": [{"k": "Literal", "v": 7}]});
        let not = json!({"k": "Not", "expr": call.clone()});
        assert_eq!(props(t.clone()).expand(&not), json!({"k": "Not", "expr": cmp()}));
        // A call the template only compares (`isWide(p) === x`) is a value, not a decision.
        let compared = json!({"k": "Binary", "op": "===", "left": call, "right": {"k": "Literal", "v": true}});
        assert_eq!(props(t).expand(&compared), compared);
    }
}
