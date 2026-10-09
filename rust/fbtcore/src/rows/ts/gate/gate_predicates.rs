//! A GATE THAT CALLS A MEMBERSHIP PREDICATE — `isItemActive([ItemIDs.Gamma])`
//! — as the restriction the predicate's own body proves.
//!
//! The comparison walk in `gate_values` reads `===` and nothing else, so a restriction
//! written as a CALL over enum constants produced no row, and the fields behind it read as
//! shown for every product. Resolving the call BY NAME would be a guess: `isItemActive`
//! and `isItemHidden` spell the same shape at the call site and mean opposite things.
//! So the method is resolved by declaration, and its RETURN TREE has to prove it tests its
//! argument for membership in a collection the class holds:
//!
//!   `this.S.includes(p)`   `this.S.indexOf(p) !== -1`   `p.some((x) => this.S.includes(x))`
//!   `p.map((x) => this.S.includes(x)).some((e) => e)`   `this.S.some((x) => p.includes(x))`
//!
//! Anything else — a second return, a branch, a body that calls something else — proves
//! nothing, and the call stays unread.
//!
//! THE DIMENSION IS A SET, NOT A VALUE. Every shape above means "SOME argument is in S", so
//! the call is `in` its arguments and its negation is `not_in` them, and that holds in both
//! directions. `this.S && <one of the above>` holds only one way — false may just mean S was
//! never loaded — so NEGATED it is `unknown`, never `not_in`. And because S may hold
//! several members at once, two `in`s on it do not intersect: see `gate_collapse`.

use super::astreads::{is_read, unwrap};
use super::gate_lookups::{delegated, lookups};
use super::gate_values::{constant_of, enum_of_type_ref, EnumIndex, MemberEnums};
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

/// What a method's body proves about its one parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Predicate {
    pub enum_id: String,
    /// The collection tested, as its path off `this`.
    pub set: String,
    /// The parameter is a LIST of members, so the call site passes an array.
    pub array: bool,
    /// The body is the test itself, so false means "none is in S" too.
    pub both: bool,
}

fn text<'a>(n: &'a Value, field: &str) -> Option<&'a str> {
    n.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

pub(crate) fn kind(n: &Value) -> Option<&str> {
    text(n, "k")
}

/// `this.a.b` as `a.b`; anything not rooted at `this` is not a collection the class holds.
pub(crate) fn this_path(n: &Value) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let mut current = n;
    while is_read(current) {
        parts.insert(0, text(current, "name")?);
        current = current.get("receiver")?;
    }
    (kind(current) == Some("This") && !parts.is_empty()).then(|| parts.join("."))
}

/// A bare identifier: `p`, `x`.
pub(crate) fn local(n: &Value) -> Option<&str> {
    let receiver = n.get("receiver")?;
    (kind(n) == Some("Read") && kind(receiver) == Some("Implicit")).then(|| text(n, "name"))?
}

/// `<receiver>.<method>(<one argument>)`, as (receiver, argument).
pub(crate) fn method_call<'a>(n: &'a Value, method: &str) -> Option<(&'a Value, &'a Value)> {
    if kind(n) != Some("Call") {
        return None;
    }
    let callee = n.get("receiver")?;
    if !is_read(callee) || text(callee, "name") != Some(method) {
        return None;
    }
    match n.get("args")?.as_array()?.as_slice() {
        [arg] => Some((callee.get("receiver")?, arg)),
        _ => None,
    }
}

/// An arrow with exactly one returned expression.
pub(crate) fn only_return(n: &Value) -> Option<&Value> {
    if kind(n) != Some("Fn") {
        return None;
    }
    match n.get("returns")?.as_array()?.as_slice() {
        [one] => Some(one),
        _ => None,
    }
}

pub(crate) fn literal_is(n: &Value, v: i64) -> bool {
    kind(n) == Some("Literal") && n.get("v").and_then(|x| x.as_f64()) == Some(v as f64)
}

pub(crate) fn minus_one(n: &Value) -> bool {
    kind(n) == Some("Unary") && text(n, "op") == Some("-")
        && n.get("expr").is_some_and(|e| literal_is(e, 1))
}

/// `this.S.includes(v)` or `this.S.indexOf(v) !== -1`, for a `v` the caller accepts.
fn member_test(n: &Value, var: &dyn Fn(&str) -> bool) -> Option<String> {
    if let Some((set, arg)) = method_call(n, "includes") {
        return this_path(set).filter(|_| local(arg).is_some_and(var));
    }
    if kind(n) != Some("Binary") {
        return None;
    }
    let (left, right) = (n.get("left")?, n.get("right")?);
    let found = match text(n, "op")? {
        "!==" | "!=" | ">" => minus_one(right),
        ">=" => literal_is(right, 0),
        _ => false,
    };
    let (set, arg) = method_call(left, "indexOf").filter(|_| found)?;
    this_path(set).filter(|_| local(arg).is_some_and(var))
}

/// The collection a return tree tests `param` against, and whether false proves anything.
fn test_of(n: &Value, param: &str, array: bool, lambdas: &IndexSet<String>) -> Option<(String, bool)> {
    // A GUARD IN FRONT is necessary for true and proves nothing for false.
    if kind(n) == Some("Binary") && text(n, "op") == Some("&&") {
        let (left, right) = (n.get("left")?, n.get("right")?);
        let tested = if this_path(left).is_some() { right } else if this_path(right).is_some() { left } else { return None };
        return test_of(tested, param, array, lambdas).map(|(set, _)| (set, false));
    }
    if !array {
        return member_test(n, &|v| v == param).map(|set| (set, true));
    }
    // A LAMBDA'S PARAMETER, never the method's own: the only other names a one-expression
    // body can bind.
    let bound = |v: &str| v != param && lambdas.contains(v);
    let (receiver, callback) = method_call(n, "some")?;
    let body = only_return(callback)?;
    if local(receiver) == Some(param) {
        return member_test(body, &bound).map(|set| (set, true));
    }
    if let Some(set) = this_path(receiver) {
        let (list, arg) = method_call(body, "includes")?;
        return (local(list) == Some(param) && local(arg).is_some_and(bound)).then_some((set, true));
    }
    // `.map(test).some((e) => e)`: the `.some` is the identity over what `.map` tested.
    let (list, mapper) = method_call(receiver, "map")?;
    if local(list) != Some(param) || !local(body).is_some_and(bound) {
        return None;
    }
    member_test(only_return(mapper)?, &bound).map(|set| (set, true))
}

/// The enum a parameter is typed by, and whether it is a list of it. A list's reference is
/// the `Array` it is, and names what it holds as its `element`.
pub(crate) fn param_enum(p: &Value, idx: &EnumIndex) -> Option<(String, bool)> {
    let element = p.get("type_ref").and_then(|r| r.get("element")).filter(|e| e.is_object());
    if let Some(element) = element {
        let name = text(element, "name")?;
        return enum_of_type_ref(idx, name, Some(element)).map(|e| (e, true));
    }
    enum_of_type_ref(idx, text(p, "type")?, p.get("type_ref")).map(|e| (e, false))
}

/// Member id -> what its body proves, for every method of ONE enum-typed parameter whose
/// single return is a membership test, or a call handing it to a proven lookup.
pub fn predicates(store: &Store<'_>, idx: &EnumIndex) -> IndexMap<String, Predicate> {
    let mut candidates: IndexMap<String, (String, String, bool)> = IndexMap::new();
    for m in store.table("members").iter() {
        let Some(Value::Array(params)) = m.get("params") else { continue };
        let ([p], Some(id)) = (params.as_slice(), m.get("id").and_then(|v| v.as_str())) else { continue };
        let (Some(name), Some((en, array))) = (text(p, "name"), param_enum(p, idx)) else { continue };
        candidates.insert(id.to_string(), (name.to_string(), en, array));
    }

    // ONE RETURN, OUTSIDE ANY BRANCH: a second one is a second answer this does not read.
    let mut returns: IndexMap<String, Vec<&Row>> = IndexMap::new();
    let all = store.table("returns");
    for r in all.iter() {
        if let Some(member) = r.get("member").and_then(|v| v.as_str())
            && candidates.contains_key(member)
        {
            returns.entry(member.to_string()).or_default().push(r);
        }
    }
    let mut lambdas: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for f in store.table("functions").iter() {
        let Some(parent) = f.get("parent").and_then(|v| v.as_str()) else { continue };
        if !candidates.contains_key(parent) {
            continue;
        }
        let Some(Value::Array(params)) = f.get("params") else { continue };
        let names = params.iter().filter_map(|p| text(p, "name").map(str::to_string));
        lambdas.entry(parent.to_string()).or_default().extend(names);
    }
    // Expression id -> the member it is the return of, and the declaration the call resolved to.
    let mut wanted: IndexMap<String, (String, Option<Value>)> = IndexMap::new();
    for (member, rows) in &returns {
        let [r] = rows.as_slice() else { continue };
        let unconditional = ["branch", "case"].iter().all(|f| r.get(f).is_none_or(Value::is_null));
        if let (true, Some(x)) = (unconditional, r.get("expression").and_then(|v| v.as_str())) {
            let target = r.get("value").and_then(|v| v.get("$target")).cloned();
            wanted.insert(x.to_string(), (member.clone(), target));
        }
    }
    let delegates = lookups(store, idx);

    let mut out = IndexMap::new();
    let none = IndexSet::new();
    for e in store.table("expressions").iter() {
        let Some((member, target)) = e.get("id").and_then(|v| v.as_str()).and_then(|x| wanted.get(x)) else { continue };
        let (Some(ast), Some((param, en, array))) = (e.get("ast"), candidates.get(member)) else { continue };
        let bound = lambdas.get(member).unwrap_or(&none);
        let tree = unwrap(ast);
        if let Some((set, both)) = test_of(tree, param, *array, bound) {
            out.insert(member.clone(), Predicate { enum_id: en.clone(), set, array: *array, both });
        } else if let Some(p) = delegated(tree, target.as_ref(), param, en, &delegates).filter(|_| !*array) {
            out.insert(member.clone(), p);
        }
    }
    out
}

/// A call in a gate, under the polarity that reached it, as a restriction over the SET the
/// called predicate tests.
///
/// An argument the map cannot read as constants of the predicate's own enum — a variable, a
/// member of another enum — still names the dimension, so it is `unknown`, never omitted.
pub fn predicate_restriction(
    node: &Value,
    negated: bool,
    preds: &IndexMap<String, Predicate>,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> Option<Value> {
    if kind(node) != Some("Call") {
        return None;
    }
    let callee = node.get("receiver")?;
    let row = callee.get("target")?.get("row")?.as_str()?;
    let pred = preds.get(row).filter(|_| is_read(callee))?;
    let [arg] = node.get("args")?.as_array()?.as_slice() else { return None };

    let items: Vec<&Value> = match (pred.array, kind(arg)) {
        (true, Some("Array")) => arg.get("items")?.as_array()?.iter().collect(),
        (true, _) => Vec::new(),
        (false, _) => vec![arg],
    };
    let values: Option<Vec<String>> = items
        .iter()
        .map(|i| constant_of(i, mem, idx).filter(|(e, _)| *e == pred.enum_id).map(|(_, v)| v))
        .collect();
    let op = match values.as_ref().filter(|v| !v.is_empty()) {
        None => "unknown",
        Some(_) if negated && !pred.both => "unknown",
        Some(_) if negated => "not_in",
        Some(_) => "in",
    };
    Some(json!({
        "enum": pred.enum_id, "dim": pred.set, "row": null, "op": op,
        "values": if op == "unknown" { vec![] } else { values.unwrap_or_default() },
        "set": true,
    }))
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use super::super::gate_values::EnumInfo;
    use serde_json::Map;

    fn idx() -> EnumIndex {
        let mut idx = EnumIndex::default();
        idx.by_id.insert("e:p".into(), EnumInfo {
            name: Some("PlanIDs".into()),
            domain: ["Gamma", "Delta", "Omega"].iter().map(|s| s.to_string()).collect(),
        });
        idx.by_decl.insert("PlanIDs".into(), vec![("app/products.ts".into(), "e:p".into())]);
        idx
    }

    fn read(name: &str, receiver: Value) -> Value {
        json!({"k": "Read", "name": name, "receiver": receiver})
    }

    fn ident(name: &str) -> Value {
        read(name, json!({"k": "Implicit"}))
    }

    fn this_(name: &str) -> Value {
        read(name, json!({"k": "This"}))
    }

    fn call(receiver: Value, method: &str, args: Vec<Value>) -> Value {
        json!({"k": "Call", "receiver": read(method, receiver), "args": args})
    }

    fn arrow(returned: Value) -> Value {
        json!({"k": "Fn", "src": "(x) => ...", "returns": [returned]})
    }

    /// `ids.map((i) => this.active.includes(i)).some((e) => e)` — a shape a predicate
    /// commonly spells.
    fn map_some() -> Value {
        let tested = call(this_("active"), "includes", vec![ident("i")]);
        call(call(ident("ids"), "map", vec![arrow(tested)]), "some", vec![arrow(ident("e"))])
    }

    fn lambdas(names: &[&str]) -> IndexSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_map_then_some_over_the_parameter_is_a_membership_test() {
        let got = test_of(&map_some(), "ids", true, &lambdas(&["i", "e"]));
        assert_eq!(got, Some(("active".to_string(), true)));
    }

    #[test]
    fn some_over_the_parameter_and_some_over_the_set_are_the_same_test() {
        let over_param = call(ident("ids"), "some",
                              vec![arrow(call(this_("active"), "includes", vec![ident("x")]))]);
        let over_set = call(this_("active"), "some",
                            vec![arrow(call(ident("ids"), "includes", vec![ident("x")]))]);
        for n in [over_param, over_set] {
            assert_eq!(test_of(&n, "ids", true, &lambdas(&["x"])), Some(("active".into(), true)));
        }
    }

    #[test]
    fn a_name_no_lambda_binds_is_not_the_lambdas_argument() {
        // `x` bound by nothing in the body is a module constant, and `includes(CONST)` tests
        // one fixed member, not the argument.
        let n = call(ident("ids"), "some",
                     vec![arrow(call(this_("active"), "includes", vec![ident("CONST")]))]);
        assert_eq!(test_of(&n, "ids", true, &lambdas(&["x"])), None);
    }

    #[test]
    fn index_of_against_minus_one_is_includes_for_one_member() {
        let found = call(this_("types"), "indexOf", vec![ident("t")]);
        let n = json!({"k": "Binary", "op": "!==", "left": found,
                       "right": {"k": "Unary", "op": "-", "expr": {"k": "Literal", "v": 1}}});
        assert_eq!(test_of(&n, "t", false, &lambdas(&[])), Some(("types".into(), true)));
        // `=== -1` is the OPPOSITE test and must not read as this one.
        let mut opposite = n.clone();
        opposite["op"] = json!("===");
        assert_eq!(test_of(&opposite, "t", false, &lambdas(&[])), None);
    }

    #[test]
    fn a_guarded_test_proves_membership_only_when_true() {
        let n = json!({"k": "Binary", "op": "&&", "left": this_("types"),
                       "right": call(this_("types"), "includes", vec![ident("t")])});
        assert_eq!(test_of(&n, "t", false, &lambdas(&[])), Some(("types".into(), false)));
    }

    #[test]
    fn a_body_that_tests_something_else_is_not_a_predicate() {
        // Same call shape, but the collection is a parameter's property, not the class's.
        let n = call(read("active", ident("other")), "includes", vec![ident("t")]);
        assert_eq!(test_of(&n, "t", false, &lambdas(&[])), None);
        // The class's collection, but tested for a fixed member rather than the argument.
        let n = call(this_("active"), "includes", vec![ident("DEFAULT")]);
        assert_eq!(test_of(&n, "t", false, &lambdas(&[])), None);
        // And a comparison is not a membership test at all.
        let n = json!({"k": "Binary", "op": "===", "left": ident("t"), "right": this_("one")});
        assert_eq!(test_of(&n, "t", false, &lambdas(&[])), None);
    }

    // ---- the call site --------------------------------------------------------------

    fn preds(both: bool) -> IndexMap<String, Predicate> {
        let mut p = IndexMap::new();
        p.insert("m:is".into(), Predicate { enum_id: "e:p".into(), set: "active".into(),
                                            array: true, both });
        p
    }

    fn mem() -> MemberEnums {
        let mut mem = MemberEnums::default();
        mem.alias.insert("m:alias".into(), "e:p".into());
        mem
    }

    /// `PlanIDs.<name>`, read off the component's alias member.
    fn constant(name: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": {"k": "Read", "name": "PlanIDs",
               "receiver": {"k": "Implicit"}, "target": {"row": "m:alias"}}})
    }

    fn site(args: Vec<Value>) -> Value {
        json!({"k": "Call", "receiver": {"k": "Read", "name": "isItemActive",
               "receiver": {"k": "Implicit"}, "target": {"row": "m:is"}}, "args": args})
    }

    fn items(names: &[&str]) -> Value {
        json!({"k": "Array", "items": names.iter().map(|n| constant(n)).collect::<Vec<_>>()})
    }

    #[test]
    fn a_call_over_constants_is_in_them_and_its_negation_is_not_in_them() {
        let n = site(vec![items(&["Gamma", "Delta"])]);
        let r = predicate_restriction(&n, false, &preds(true), &mem(), &idx()).expect("read");
        assert_eq!((r["op"].clone(), r["values"].clone()), (json!("in"), json!(["Gamma", "Delta"])));
        assert_eq!(r["dim"], json!("active"));
        assert_eq!(r["set"], json!(true));
        let r = predicate_restriction(&n, true, &preds(true), &mem(), &idx()).expect("read");
        assert_eq!(r["op"], json!("not_in"));
    }

    #[test]
    fn a_negated_guarded_predicate_is_unknown_and_never_not_in() {
        // False may only mean the collection was never loaded.
        let n = site(vec![items(&["Gamma"])]);
        let r = predicate_restriction(&n, true, &preds(false), &mem(), &idx()).expect("read");
        assert_eq!(r["op"], json!("unknown"));
        let r = predicate_restriction(&n, false, &preds(false), &mem(), &idx()).expect("read");
        assert_eq!(r["op"], json!("in"), "true still proves it");
    }

    #[test]
    fn an_argument_that_is_not_constants_names_the_dimension_and_no_value() {
        for arg in [ident("chosen"), json!({"k": "Array", "items": [constant("Gamma"), ident("x")]})] {
            let r = predicate_restriction(&site(vec![arg]), false, &preds(true), &mem(), &idx())
                .expect("the dimension is known");
            assert_eq!(r["op"], json!("unknown"));
        }
    }

    #[test]
    fn a_call_to_a_method_that_is_not_a_proven_predicate_is_not_read() {
        let mut n = site(vec![items(&["Gamma"])]);
        n["receiver"]["target"]["row"] = json!("m:other");
        assert!(predicate_restriction(&n, false, &preds(true), &mem(), &idx()).is_none());
    }

    #[test]
    fn the_method_is_found_by_its_rows_and_only_with_one_unconditional_return() {
        let element = json!({"name": "PlanIDs", "file": "/repo/app/products.ts"});
        let tables = |returns: Value| -> Map<String, Value> {
            json!({
                "members": [{"id": "m:is", "name": "isItemActive", "kind": "method",
                             "params": [{"name": "ids", "type": "PlanIDs[]",
                                         "type_ref": {"name": "Array", "element": element}}]}],
                "functions": [{"id": "fn:1", "parent": "m:is", "params": [{"name": "i"}]},
                              {"id": "fn:2", "parent": "m:is", "params": [{"name": "e"}]}],
                "returns": returns,
                "expressions": [{"id": "x:1", "ast": map_some()}],
            }).as_object().cloned().unwrap()
        };
        let one = json!([{"id": "rv:1", "member": "m:is", "expression": "x:1"}]);
        let store = Store::from_payload(tables(one), "typescript");
        let got = predicates(&store, &idx());
        assert_eq!(got.get("m:is").map(|p| (p.set.as_str(), p.array)), Some(("active", true)));

        let two = json!([{"id": "rv:1", "member": "m:is", "expression": "x:1"},
                         {"id": "rv:2", "member": "m:is", "expression": null}]);
        let store = Store::from_payload(tables(two), "typescript");
        assert!(predicates(&store, &idx()).is_empty(), "a second return is a second answer");

        let branched = json!([{"id": "rv:1", "member": "m:is", "expression": "x:1", "branch": "br:1"}]);
        let store = Store::from_payload(tables(branched), "typescript");
        assert!(predicates(&store, &idx()).is_empty(), "a branch makes it conditional");
    }
}
