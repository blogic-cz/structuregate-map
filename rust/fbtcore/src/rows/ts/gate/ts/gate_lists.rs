//! A VALUE TESTED AGAINST A FIXED LIST OF ENUM CONSTANTS — `FEATURED_TIERS.includes(id)`,
//! `[A, B].some((x) => x === id)`, `LIST.indexOf(id) !== -1` — as the `in` it spells, and
//! a function whose one return is such a test as the same `in` at every call site.
//!
//! `gate_predicates` reads the MIRROR shape: constants handed IN, tested against a set the
//! class holds at run time. Here the list is the constant and the value is the dimension, so
//! both directions are exact: the list cannot change, and `!LIST.includes(id)` is `not_in` as
//! surely as `LIST.includes(id)` is `in`.
//!
//! THE LIST IS FOUND BY ITS ROW, never by its name: an array literal in the tree itself, or
//! the member or module `const` the read resolves to, whose value is every member spelled
//! out. One item that is not a constant of one enum and the list is not a list of it.
//!
//! A BARE LOCAL IS NOT A DIMENSION. `LIST.includes(tierID)` in a method tests a variable
//! whose value came from somewhere this does not follow, and naming the dimension after the
//! local would put a restriction on a name no gate shares. Dropped, never guessed.

use super::astreads::{is_read, unwrap};
use super::gate_predicates::{kind, literal_is, local, method_call, minus_one, only_return, param_enum};
use super::gate_tsrows::{bare, TsRows};
use super::gate_values::{
    constant_of, dimension_of, enum_of_type_ref, untyped_dimension, EnumIndex, MemberEnums,
};
use super::store::Store;
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

/// A list a row holds: the enum its items NAME (`TierIDs` of `TierIDs.Small`), the one it
/// is declared as when the declaration says, and the members in order.
#[derive(Debug, Clone)]
struct List {
    named: String,
    enum_id: Option<String>,
    members: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Lists {
    by_row: IndexMap<String, List>,
    /// Function or method id -> the enum and the members its one return tests its argument in.
    preds: IndexMap<String, (String, Vec<String>)>,
}

/// `[{"$enum": "TierIDs.Small"}, …]` as (`TierIDs`, [`Small`, …]), or nothing.
fn spelled(value: Option<&Value>) -> Option<(String, Vec<String>)> {
    let items = value?.as_array().filter(|a| !a.is_empty())?;
    let mut named: Option<&str> = None;
    let mut members = Vec::new();
    for i in items {
        let (prefix, member) = i.get("$enum")?.as_str()?.rsplit_once('.')?;
        if named.is_some_and(|n| n != prefix) {
            return None;
        }
        named = Some(prefix);
        members.push(member.to_string());
    }
    Some((named?.to_string(), members))
}

/// `x === d` with `x` the callback's element: `d`.
fn element_test(body: &Value) -> Option<&Value> {
    if kind(body) != Some("Binary") || !matches!(body.get("op")?.as_str()?, "===" | "==") {
        return None;
    }
    let (l, r) = (body.get("left")?, body.get("right")?);
    let element = |n: &Value| n.get("lambda") == Some(&Value::Bool(true));
    match (element(l), element(r)) {
        (true, false) => Some(r),
        (false, true) => Some(l),
        _ => None,
    }
}

/// The (list, value) a test compares, for the three shapes that mean membership.
fn list_test(n: &Value) -> Option<(&Value, &Value)> {
    if let Some(found) = method_call(n, "includes") {
        return Some(found);
    }
    if let Some((list, cb)) = method_call(n, "some") {
        return element_test(only_return(cb)?).map(|d| (list, d));
    }
    if kind(n) != Some("Binary") {
        return None;
    }
    let right = n.get("right")?;
    let found = match n.get("op")?.as_str()? {
        "!==" | "!=" | ">" => minus_one(right),
        ">=" => literal_is(right, 0),
        _ => false,
    };
    method_call(n.get("left")?, "indexOf").filter(|_| found)
}

/// The enum a list named `named` belongs to: its declaration's, else the dimension's when
/// that enum IS called so, else the one enum of that name — and nothing when there are two.
fn enum_for(list: &List, dim_enum: Option<&String>, idx: &EnumIndex) -> Option<String> {
    if let Some(en) = &list.enum_id {
        return Some(en.clone());
    }
    if let Some(en) = dim_enum.filter(|e| idx.by_id.get(*e).and_then(|i| i.name.as_deref()) == Some(&list.named)) {
        return Some(en.clone());
    }
    match idx.by_decl.get(&list.named)?.as_slice() {
        [(_, en)] => Some(en.clone()),
        _ => None,
    }
}

/// Names the callbacks of `parent` bind, which is what marks an element in its trees.
fn bound_by(store: &Store<'_>) -> IndexMap<String, IndexSet<String>> {
    let mut out: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for f in store.table("functions").iter() {
        let Some(parent) = f.get("parent").and_then(|v| v.as_str()) else { continue };
        let Some(Value::Array(params)) = f.get("params") else { continue };
        let names = params.iter().filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(str::to_string));
        out.entry(parent.to_string()).or_default().extend(names);
    }
    out
}

pub fn lists(store: &Store<'_>, idx: &EnumIndex) -> Lists {
    let mut out = Lists::default();
    for table in ["members", "consts", "locals"] {
        for r in store.table(table).iter() {
            let (Some(Value::String(id)), Some((named, members))) = (r.get("id"), spelled(r.get("value"))) else { continue };
            let element = r.get("type_ref").and_then(|t| t.get("element"));
            let enum_id = element.and_then(|e| enum_of_type_ref(idx, &named, Some(e)));
            out.by_row.insert(id.clone(), List { named, enum_id, members });
        }
    }

    let rows = TsRows::new(store);
    let bound = bound_by(store);
    let none = IndexSet::new();
    let mut candidates: IndexMap<String, (String, String)> = IndexMap::new();
    for table in ["members", "functions"] {
        for r in store.table(table).iter() {
            let (Some(Value::String(id)), Some(Value::Array(params))) = (r.get("id"), r.get("params")) else { continue };
            let [p] = params.as_slice() else { continue };
            let (Some(name), Some((en, false))) = (p.get("name").and_then(|v| v.as_str()), param_enum(p, idx)) else { continue };
            candidates.insert(id.clone(), (name.to_string(), en));
        }
    }
    // ONE RETURN, OUTSIDE ANY BRANCH — the rule `gate_predicates` holds its own reader to.
    let mut returns: IndexMap<String, Vec<Option<String>>> = IndexMap::new();
    for r in store.table("returns").iter() {
        let Some(member) = r.get("member").and_then(|v| v.as_str()).filter(|m| candidates.contains_key(*m)) else { continue };
        let unconditional = ["branch", "case"].iter().all(|f| r.get(f).is_none_or(Value::is_null));
        let tree = r.get("expression").and_then(|v| v.as_str()).filter(|_| unconditional).map(str::to_string);
        returns.entry(member.to_string()).or_default().push(tree);
    }
    let mut wanted: IndexMap<String, String> = IndexMap::new();
    for (member, trees) in returns {
        if let [Some(x)] = trees.as_slice() {
            wanted.insert(x.clone(), member);
        }
    }
    for e in store.table("expressions").iter() {
        let Some(member) = e.get("id").and_then(|v| v.as_str()).and_then(|x| wanted.get(x)) else { continue };
        let (Some(ast), Some((param, en))) = (e.get("ast"), candidates.get(member)) else { continue };
        let tree = rows.resolve_in(unwrap(ast), bound.get(member).unwrap_or(&none), idx, Some(member.as_str()));
        let Some((list, d)) = list_test(&tree) else { continue };
        if local(d) != Some(param.as_str()) {
            continue;
        }
        if let Some((found, members)) = out.values_of(list, Some(en), &MemberEnums::default(), idx)
            && found == *en
        {
            out.preds.insert(member.clone(), (en.clone(), members));
        }
    }
    out
}

impl Lists {
    /// Whether a call to `row` is read here, so no other reader inlines it.
    pub fn claims(&self, row: &str) -> bool {
        self.preds.contains_key(row)
    }

    /// The enum and members a list node holds: an array literal of constants, or a read of
    /// a row whose value spells them.
    fn values_of(&self, list: &Value, dim_enum: Option<&String>, mem: &MemberEnums, idx: &EnumIndex) -> Option<(String, Vec<String>)> {
        if kind(list) == Some("Array") {
            let items = list.get("items")?.as_array().filter(|a| !a.is_empty())?;
            let found: Option<Vec<(String, String)>> = items.iter().map(|i| constant_of(i, mem, idx)).collect();
            let found = found?;
            let en = found[0].0.clone();
            return found.iter().all(|(e, _)| *e == en).then(|| (en, found.into_iter().map(|(_, v)| v).collect()));
        }
        if !is_read(list) {
            return None;
        }
        let row = list.get("target")?.get("row")?.as_str()?;
        let held = self.by_row.get(row)?;
        let en = enum_for(held, dim_enum, idx)?;
        let domain = &idx.by_id.get(&en)?.domain;
        held.members.iter().all(|m| domain.contains(m)).then(|| (en, held.members.clone()))
    }

    /// A leaf of the polarity walk, as the restriction its list proves.
    pub fn restriction(&self, n: &Value, negated: bool, mem: &MemberEnums, idx: &EnumIndex) -> Option<Value> {
        let (values, d) = if let Some((list, d)) = list_test(n) {
            let dim_enum = dimension_of(d, mem).map(|(e, _, _)| e);
            (self.values_of(list, dim_enum.as_ref(), mem, idx)?, d)
        } else if kind(n) == Some("Call") {
            let row = n.get("receiver")?.get("target")?.get("row")?.as_str()?;
            let [d] = n.get("args")?.as_array()?.as_slice() else { return None };
            (self.preds.get(row)?.clone(), d)
        } else {
            return None;
        };
        if d.as_object().is_none_or(|o| bare(o).is_some()) || d.get("lambda").is_some() {
            return None;
        }
        let (en, members) = values;
        let (dim, row) = match dimension_of(d, mem) {
            Some((typed, dim, row)) if typed != en => {
                return Some(json!({"enum": typed, "dim": dim, "row": row, "op": "unknown"}));
            }
            Some((_, dim, row)) => (dim, row),
            None => (untyped_dimension(d)?, None),
        };
        Some(json!({
            "enum": en, "dim": dim, "row": row,
            "op": if negated { "not_in" } else { "in" }, "values": members,
        }))
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use super::super::gate_values::EnumInfo;

    fn idx() -> EnumIndex {
        let mut idx = EnumIndex::default();
        idx.by_id.insert("e:1".into(), EnumInfo {
            name: Some("TierIDs".into()), 
            domain: ["Small", "Big", "Mid", "Low"].iter().map(|s| s.to_string()).collect(),
        });
        idx.by_decl.insert("TierIDs".into(), vec![("app/t.ts".into(), "e:1".into())]);
        idx
    }

    fn constant(name: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": {"k": "Read", "name": "TierIDs", "receiver": {"k": "Implicit"}},
               "target": {"enum": "e:1"}})
    }

    fn dim() -> Value {
        json!({"k": "Read", "name": "TierID", "receiver": {"k": "Read", "name": "model", "receiver": {"k": "This"}}})
    }

    fn call(receiver: Value, method: &str, args: Vec<Value>) -> Value {
        json!({"k": "Call", "receiver": {"k": "Read", "name": method, "receiver": receiver}, "args": args})
    }

    fn held(members: &[&str]) -> Lists {
        let mut l = Lists::default();
        l.by_row.insert("k:1".into(), List {
            named: "TierIDs".into(), enum_id: None, members: members.iter().map(|s| s.to_string()).collect(),
        });
        l
    }

    fn list_read() -> Value {
        json!({"k": "Read", "name": "ELEMENT", "receiver": {"k": "Implicit"}, "target": {"row": "k:1"}})
    }

    #[test]
    fn a_const_list_including_the_value_is_in_its_members_and_its_negation_not_in_them() {
        let n = call(list_read(), "includes", vec![dim()]);
        let r = held(&["Small", "Big"]).restriction(&n, false, &MemberEnums::default(), &idx()).expect("a restriction");
        assert_eq!((r["op"].clone(), r["values"].clone(), r["dim"].clone()),
                   (json!("in"), json!(["Small", "Big"]), json!("model.TierID")));
        let r = held(&["Small", "Big"]).restriction(&n, true, &MemberEnums::default(), &idx()).expect("a restriction");
        assert_eq!(r["op"], json!("not_in"), "a list that cannot change is exact both ways");
    }

    #[test]
    fn an_array_literal_of_constants_is_a_list_in_place() {
        let list = json!({"k": "Array", "items": [constant("Mid"), constant("Low")]});
        let el = json!({"k": "Read", "name": "x", "receiver": {"k": "Implicit"}, "lambda": true});
        let cb = json!({"k": "Fn", "returns": [{"k": "Binary", "op": "===", "left": el, "right": dim()}]});
        let n = call(list, "some", vec![cb]);
        let r = Lists::default().restriction(&n, false, &MemberEnums::default(), &idx()).expect("a restriction");
        assert_eq!(r["values"], json!(["Mid", "Low"]));
    }

    #[test]
    fn a_some_whose_element_is_not_marked_proves_nothing() {
        // Without the mark, `x === y` does not say which side is the list's element.
        let list = json!({"k": "Array", "items": [constant("Mid")]});
        let el = json!({"k": "Read", "name": "x", "receiver": {"k": "Implicit"}});
        let cb = json!({"k": "Fn", "returns": [{"k": "Binary", "op": "===", "left": el, "right": dim()}]});
        let n = call(list, "some", vec![cb]);
        assert!(Lists::default().restriction(&n, false, &MemberEnums::default(), &idx()).is_none());
    }

    #[test]
    fn a_bare_local_is_not_a_dimension() {
        let local = json!({"k": "Read", "name": "tierID", "receiver": {"k": "Implicit"}});
        let n = call(list_read(), "includes", vec![local]);
        assert!(held(&["Small"]).restriction(&n, false, &MemberEnums::default(), &idx()).is_none());
    }

    #[test]
    fn one_item_the_enum_does_not_have_and_the_list_is_not_a_list_of_it() {
        let n = call(list_read(), "includes", vec![dim()]);
        assert!(held(&["Small", "Gone"]).restriction(&n, false, &MemberEnums::default(), &idx()).is_none());
    }

    #[test]
    fn index_of_against_minus_one_is_includes() {
        let n = json!({"k": "Binary", "op": "!==", "left": call(list_read(), "indexOf", vec![dim()]),
                       "right": {"k": "Unary", "op": "-", "expr": {"k": "Literal", "v": 1}}});
        let r = held(&["Big"]).restriction(&n, false, &MemberEnums::default(), &idx()).expect("a restriction");
        assert_eq!(r["values"], json!(["Big"]));
    }

    #[test]
    fn a_call_to_a_list_predicate_is_its_list_at_the_call_site() {
        let mut l = Lists::default();
        l.preds.insert("fn:1".into(), ("e:1".into(), vec!["Small".into()]));
        let n = json!({"k": "Call", "receiver": {"k": "Read", "name": "isSmall", "receiver": {"k": "Implicit"},
                                                 "target": {"row": "fn:1"}}, "args": [dim()]});
        let r = l.restriction(&n, true, &MemberEnums::default(), &idx()).expect("a restriction");
        assert_eq!((r["op"].clone(), r["values"].clone()), (json!("not_in"), json!(["Small"])));
    }

    #[test]
    fn the_list_predicate_is_found_by_its_one_return() {
        let tables = json!({
            "functions": [{"id": "fn:1", "name": "isSmall",
                           "params": [{"name": "t", "type": "TierIDs",
                                       "type_ref": {"name": "TierIDs", "file": "C:/fe/app/t.ts"}}]},
                          {"id": "fn:2", "parent": "fn:1", "params": [{"name": "x"}]}],
            "returns": [{"member": "fn:1", "expression": "x:1"}],
            "expressions": [{"id": "x:1", "ast": call(
                json!({"k": "Array", "items": [constant("Small"), constant("Big")]}), "some",
                vec![json!({"k": "Fn", "returns": [{"k": "Binary", "op": "===",
                            "left": {"k": "Read", "name": "x", "receiver": {"k": "Implicit"}},
                            "right": {"k": "Read", "name": "t", "receiver": {"k": "Implicit"}}}]})])}],
        });
        let store = Store::from_payload(tables.as_object().cloned().unwrap(), "typescript");
        let l = lists(&store, &idx());
        assert_eq!(l.preds.get("fn:1"), Some(&("e:1".to_string(), vec!["Small".to_string(), "Big".to_string()])));
    }

    #[test]
    fn a_predicate_with_a_second_return_is_not_one() {
        let tables = json!({
            "functions": [{"id": "fn:1", "params": [{"name": "t", "type": "TierIDs",
                           "type_ref": {"name": "TierIDs", "file": "C:/fe/app/t.ts"}}]}],
            "returns": [{"member": "fn:1", "expression": "x:1"}, {"member": "fn:1", "expression": "x:2"}],
            "expressions": [{"id": "x:1", "ast": call(list_read(), "includes",
                             vec![json!({"k": "Read", "name": "t", "receiver": {"k": "Implicit"}})])}],
        });
        let store = Store::from_payload(tables.as_object().cloned().unwrap(), "typescript");
        assert!(lists(&store, &idx()).preds.is_empty());
    }
}
