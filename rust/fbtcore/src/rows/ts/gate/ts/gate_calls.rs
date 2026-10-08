//! A CALL TO A FUNCTION OF ONE RETURN, READ AS THAT RETURN.
//!
//! `isWide(sizeID)` over `isWide(p: number) { return p === P.One || p === P.Two; }`
//! restricts what its body compares, and so does `this.featureService.isBetaEnabled()` over a
//! service method comparing its own fields. Neither is a shape `gate_predicates` or `gate_lists`
//! reads - a parameter typed `number`, an `||` of equalities, a body with no parameter at all - so
//! both gates restricted nothing while a gate spelling the body out restricted its product.
//!
//! THE CALL IS REPLACED BY ITS CALLEE'S RETURN TREE, every bare read of a parameter by the
//! argument handed to it, and the polarity walk reads the result like any other condition. That is
//! exact in both polarities, as a getter is (`gate_props`): a function of one unconditional return
//! IS that expression's value. The callee is found BY ITS ROW - the declaration the call resolved
//! to - never by name, and it is a method, a function-valued property, a function or an arrow
//! `const`, or a property holding one (`isPlus = isPlusItem`).
//!
//! NOT INLINED, so the call stays what it was: a second return or one under a branch (a second
//! answer), a body whose declared type is a `Promise` or an `Observable` (its value is truthy
//! whatever it resolves to), a parameter a callback rebinds (a bare read of it says nothing about
//! which), an argument count that differs, and a callee `gate_predicates` or `gate_lists` reads
//! already - they read the set the body tests, and a substituted body would lose it.

use super::astreads::unwrap;
use super::gate_tsrows::{bare, TsRows};
use super::gate_values::EnumIndex;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;
use std::rc::Rc;

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// A declared return type whose value is truthy whatever it holds.
fn deferred(declared: Option<String>) -> bool {
    let Some(text) = declared else { return false };
    let returned = text.rsplit_once("=>").map(|(_, r)| r).unwrap_or(&text).trim_start();
    returned.starts_with("Promise") || returned.starts_with("Observable")
}

/// A callee: its parameter names, the names its callbacks bind, and where its return's tree is.
struct Callee {
    params: Vec<String>,
    bound: IndexSet<String>,
    at: usize,
}

/// Callee row -> where its one return is. The tree is read and resolved only when a gate CALLS it,
/// never up front: a pass holding every tree of hundreds of thousands is what `store::Cell` exists to avoid.
pub struct Calls<'a> {
    fns: IndexMap<String, Callee>,
    /// A property holding a function (`isPlus = isPlusItem`) -> that function's row.
    alias: IndexMap<String, String>,
    exprs: Rc<Vec<Row>>,
    rows: TsRows,
    idx: &'a EnumIndex,
}

impl<'a> Calls<'a> {
    /// The resolver the callees are read through, for a caller resolving trees of its own.
    pub fn rows(&self) -> &TsRows {
        &self.rows
    }

    pub fn new(store: &Store<'_>, idx: &'a EnumIndex, rows: TsRows, claimed: &dyn Fn(&str) -> bool) -> Calls<'a> {
        let mut bound: IndexMap<String, IndexSet<String>> = IndexMap::new();
        for f in store.table("functions").iter() {
            let Some(parent) = cell(f, "parent") else { continue };
            let names = f.get("params").and_then(|p| p.as_array()).into_iter().flatten()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(str::to_string));
            bound.entry(parent).or_default().extend(names);
        }
        // (id, params, declared type) of every callable row; a property holding a function is
        // an ALIAS of the function its tree is anchored on.
        let mut callable: IndexMap<String, (Vec<String>, Option<String>)> = IndexMap::new();
        let mut alias: IndexMap<String, String> = IndexMap::new();
        let mut anchors: IndexMap<String, String> = IndexMap::new();
        for table in ["members", "functions"] {
            for r in store.table(table).iter() {
                let Some(id) = cell(r, "id") else { continue };
                if let Some(Value::Array(params)) = r.get("params") {
                    let names: Option<Vec<String>> = params.iter()
                        .map(|p| p.get("name").and_then(|n| n.as_str()).map(str::to_string)).collect();
                    let kind = cell(r, "kind");
                    if let (Some(names), false) = (names, matches!(kind.as_deref(), Some("constructor" | "setter"))) {
                        callable.insert(id, (names, cell(r, "type")));
                    }
                    continue;
                }
                let tree = r.get("value").filter(|v| v.get("$fn").is_some()).and_then(|v| v.get("$expr_id"));
                if let (Some(Value::String(x)), "members") = (tree, table) {
                    anchors.insert(x.clone(), id);
                }
            }
        }
        for e in store.table("expressions").iter() {
            let Some(member) = cell(e, "id").and_then(|x| anchors.get(&x)) else { continue };
            let (Some(file), Some(line)) = (cell(e, "file"), cell(e, "line")) else { continue };
            if let Some(f) = rows.declared_at(&file, &line) {
                alias.insert(member.clone(), f);
            }
        }

        let mut returns: IndexMap<String, Vec<&Row>> = IndexMap::new();
        let all = store.table("returns");
        for r in all.iter() {
            if let Some(m) = cell(r, "member").filter(|m| callable.contains_key(m)) {
                returns.entry(m).or_default().push(r);
            }
        }
        let mut wanted: IndexMap<String, String> = IndexMap::new();
        for (member, rs) in &returns {
            let [r] = rs.as_slice() else { continue };
            let plain = ["branch", "case"].iter().all(|f| cell(r, f).is_none());
            if let (true, Some(x)) = (plain, cell(r, "expression")) {
                wanted.insert(x, member.clone());
            }
        }
        let exprs = store.table("expressions");
        let mut fns = IndexMap::new();
        for (at, e) in exprs.iter().enumerate() {
            let Some(member) = cell(e, "id").and_then(|x| wanted.get(&x)) else { continue };
            let Some((params, declared)) = callable.get(member) else { continue };
            let names = bound.shift_remove(member).unwrap_or_default();
            if claimed(member) || deferred(declared.clone()) || params.iter().any(|p| names.contains(p)) {
                continue;
            }
            fns.insert(member.clone(), Callee { params: params.clone(), bound: names, at });
        }
        Calls { fns, alias, exprs, rows, idx }
    }

    pub fn is_empty(&self) -> bool {
        self.fns.is_empty() && self.alias.is_empty()
    }

    /// The callee's row and its return with every parameter replaced by its argument, for a call
    /// to a function read here.
    pub fn inline(&self, call: &Value) -> Option<(String, Value)> {
        if !matches!(call.get("k").and_then(|k| k.as_str()), Some("Call" | "SafeCall")) {
            return None;
        }
        let named = call.get("receiver")?.get("target")?.get("row")?.as_str()?;
        let row = self.alias.get(named).map(String::as_str).unwrap_or(named);
        let Some(callee) = self.fns.get(row) else {
            // A property holding a function another reader reads: the call, aimed at the function.
            let mut aimed = call.clone();
            aimed["receiver"]["target"]["row"] = Value::String(row.to_string());
            return (row != named).then(|| (row.to_string(), aimed));
        };
        let args = call.get("args")?.as_array()?;
        if args.len() != callee.params.len() {
            return None;
        }
        let ast = self.exprs.get(callee.at)?.get("ast").filter(|a| !a.is_null())?;
        let mut out = self.rows.resolve_in(unwrap(ast), &callee.bound, self.idx, Some(row));
        substitute(&mut out, &callee.params, args);
        Some((row.to_string(), out))
    }
}

/// Every bare read of a parameter, replaced by its argument. A name a callback binds is marked
/// `lambda` and is never a parameter (see `gate_tsrows`).
fn substitute(n: &mut Value, params: &[String], args: &[Value]) {
    let hit = n.as_object().filter(|o| !o.contains_key("lambda")).and_then(bare)
        .and_then(|name| params.iter().position(|p| p == name));
    if let Some(i) = hit {
        *n = args[i].clone();
        return;
    }
    match n {
        Value::Array(items) => items.iter_mut().for_each(|i| substitute(i, params, args)),
        Value::Object(o) => o.values_mut().for_each(|v| substitute(v, params, args)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn store_of(tables: Value) -> Store<'static> {
        let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
        Store::from_payload(map, "typescript")
    }

    /// No enum is needed by these trees; the index lives as long as the test.
    fn none() -> &'static EnumIndex {
        Box::leak(Box::new(EnumIndex::default()))
    }

    fn calls(tables: Value) -> Calls<'static> {
        let store = store_of(tables);
        Calls::new(&store, none(), TsRows::new(&store), &|_| false)
    }

    fn param(name: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": {"k": "Implicit"}})
    }

    /// `p === 1 || p === 2`
    fn body() -> Value {
        let eq = |v: i64| json!({"k": "Binary", "op": "===", "left": param("p"), "right": {"k": "Literal", "v": v}});
        json!({"k": "Binary", "op": "||", "left": eq(1), "right": eq(2)})
    }

    fn method(extra: Value) -> Value {
        let mut t = json!({
            "members": [{"id": "m:is", "kind": "method", "name": "isPlus", "type": "boolean",
                         "params": [{"name": "p", "type": "number"}]}],
            "returns": [{"member": "m:is", "expression": "x:1"}],
            "expressions": [{"id": "x:1", "ast": body()}],
        });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            t[k] = v;
        }
        t
    }

    fn site(args: Vec<Value>) -> Value {
        json!({"k": "Call", "receiver": {"k": "Read", "name": "isPlus", "receiver": {"k": "Implicit"},
               "target": {"row": "m:is"}}, "args": args})
    }

    #[test]
    fn a_call_is_its_one_return_with_the_argument_in_place_of_the_parameter() {
        let arg = json!({"k": "Read", "name": "productID", "receiver": {"k": "Implicit"}, "target": {"row": "m:pid"}});
        let (row, tree) = calls(method(json!({}))).inline(&site(vec![arg.clone()])).expect("inlined");
        assert_eq!(row, "m:is");
        assert_eq!(tree["left"]["left"], arg);
        assert_eq!(tree["right"]["left"], arg);
    }

    #[test]
    fn a_second_return_a_branch_a_promise_or_a_wrong_arity_is_not_inlined() {
        let two = method(json!({"returns": [{"member": "m:is", "expression": "x:1"}, {"member": "m:is", "expression": "x:1"}]}));
        assert!(calls(two).is_empty());
        let branched = method(json!({"returns": [{"member": "m:is", "expression": "x:1", "branch": "br:1"}]}));
        assert!(calls(branched).is_empty());
        let mut deferred = method(json!({}));
        deferred["members"][0]["type"] = json!("Promise<boolean>");
        assert!(calls(deferred).is_empty());
        assert!(calls(method(json!({}))).inline(&site(vec![])).is_none(), "no argument for `p`");
    }

    #[test]
    fn a_parameter_a_callback_rebinds_or_a_claimed_callee_is_not_inlined() {
        let rebound = method(json!({"functions": [{"id": "fn:9", "parent": "m:is", "params": [{"name": "p"}]}]}));
        assert!(calls(rebound).is_empty());
        let store = store_of(method(json!({})));
        let claimed = Calls::new(&store, none(), TsRows::new(&store), &|row| row == "m:is");
        assert!(claimed.is_empty());
    }

    #[test]
    fn a_property_holding_a_function_is_that_function() {
        // `isPlus = isPlusItem`: the property's value is the arrow's tree, anchored where the
        // function row is declared.
        let t = json!({
            "files": [{"id": "f:1", "path": "app/p.ts"}],
            "functions": [{"id": "fn:1", "name": "isPlusItem", "file": "f:1", "line": 3,
                           "params": [{"name": "p"}], "type": "(p: number) => boolean"}],
            "members": [{"id": "m:is", "kind": "property", "name": "isPlus",
                         "value": {"$fn": "(p) => ...", "$expr_id": "x:2"}}],
            "returns": [{"member": "fn:1", "expression": "x:1"}],
            "expressions": [{"id": "x:1", "ast": body()}, {"id": "x:2", "file": "f:1", "line": 3, "ast": {"k": "Fn"}}],
        });
        let got = calls(t.clone()).inline(&site(vec![param("chosen")])).expect("the alias resolves");
        assert_eq!(got.1["left"]["left"], param("chosen"));
        // A function another reader reads is not inlined: the call is aimed at it instead.
        let store = store_of(t);
        let claimed = Calls::new(&store, none(), TsRows::new(&store), &|row| row == "fn:1");
        let (row, aimed) = claimed.inline(&site(vec![param("chosen")])).expect("aimed");
        assert_eq!((row.as_str(), aimed["receiver"]["target"]["row"].clone()), ("fn:1", json!("fn:1")));
        assert_eq!(aimed["k"], json!("Call"));
    }
}
