//! A PREDICATE THAT DELEGATES TO A KEYED LOOKUP — `isPartShown(PartIDs.Red)`,
//! whose method hands its argument and a collection of `this` to a free function:
//!
//!   const v = items.find((item) => item.PartID === key);
//!   if (!v) { return false; }
//!   return v.Active || v.Pinned || v.Locked;
//!
//! TRUE PROVES THE KEY IS IN THE COLLECTION, AND FALSE PROVES NOTHING. Every return either
//! supplies nothing or reads through `v`, and a read through an undefined `v` throws rather
//! than yields true — so a true result means `find` matched. A false one may be an item that
//! is present and switched off, so a negated call is `unknown`, never `not_in`. The
//! dimension is the collection's key field: `model.Config.Parts.PartID`.
//!
//! THE FUNCTION IS FOUND BY DECLARATION, through the target the checker gave the call, and
//! each step is read off a tree: the local's initializer (`locals.expression`), every
//! return (`returns.expression`), the lambda's own parameter (`functions`).

use super::featurewalk::is_blank;
use super::gate_predicates::{kind, local, method_call, only_return, param_enum, this_path, Predicate};
use super::gate_values::EnumIndex;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

fn text<'a>(n: &'a Value, field: &str) -> Option<&'a str> {
    n.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn slash(s: &str) -> String {
    s.replace('\\', "/")
}

/// What a lookup function proves, by parameter POSITION, since the caller passes arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct Lookup {
    pub name: String,
    /// The declaring file, FE-relative.
    pub path: String,
    pub enum_id: String,
    pub key: usize,
    pub list: usize,
    /// The field of each item compared against the key.
    pub field: String,
}

/// `list.find((x) => x.F === key)`, as `F`.
fn keyed_find(n: &Value, list: &str, key: &str, lambdas: &IndexSet<String>) -> Option<String> {
    let (receiver, callback) = method_call(n, "find")?;
    if local(receiver) != Some(list) {
        return None;
    }
    let test = only_return(callback)?;
    if kind(test) != Some("Binary") || !matches!(text(test, "op"), Some("===") | Some("==")) {
        return None;
    }
    let (left, right) = (test.get("left")?, test.get("right")?);
    let (field, other) = if local(right) == Some(key) { (left, right) } else { (right, left) };
    let item = field.get("receiver").and_then(local)?;
    let bound = item != key && lambdas.contains(item) && kind(field) == Some("Read");
    (bound && local(other) == Some(key)).then(|| text(field, "name").map(str::to_string))?
}

/// True only when `v` was found: `v`, `v.a`, `v?.a.b`, or `||`/`&&` over those. A read
/// through an undefined `v` throws or yields undefined, and neither is true.
fn needs(n: &Value, v: &str) -> bool {
    match kind(n) {
        Some("Binary") => {
            let (Some(left), Some(right)) = (n.get("left"), n.get("right")) else { return false };
            match text(n, "op") {
                Some("||") => needs(left, v) && needs(right, v),
                Some("&&") => needs(left, v) || needs(right, v),
                _ => false,
            }
        }
        Some("Read") | Some("SafeRead") => {
            let mut current = n;
            while let Some(next) = current.get("receiver").filter(|r| kind(r) != Some("Implicit")) {
                current = next;
            }
            local(current) == Some(v)
        }
        _ => false,
    }
}

fn by_member<'a>(rows: &'a [Row], wanted: &IndexMap<String, Lookup>) -> IndexMap<String, Vec<&'a Row>> {
    let mut out: IndexMap<String, Vec<&Row>> = IndexMap::new();
    for r in rows {
        if let Some(m) = r.get("member").and_then(|v| v.as_str())
            && wanted.contains_key(m)
        {
            out.entry(m.to_string()).or_default().push(r);
        }
    }
    out
}

/// Every function of one enum-typed key and one other parameter whose body is a keyed lookup.
pub fn lookups(store: &Store<'_>, idx: &EnumIndex) -> Vec<Lookup> {
    let mut file_path: IndexMap<String, String> = IndexMap::new();
    for f in store.table("files").iter() {
        if let (Some(id), Some(path)) = (f.get("id").and_then(|v| v.as_str()), f.get("path").and_then(|v| v.as_str())) {
            file_path.insert(id.to_string(), slash(path));
        }
    }
    // Function id -> the lookup it would be, before its body is read, and its two names.
    let mut candidates: IndexMap<String, Lookup> = IndexMap::new();
    let mut names: IndexMap<String, (String, String)> = IndexMap::new();
    let mut lambdas: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for f in store.table("functions").iter() {
        let Some(Value::Array(params)) = f.get("params") else { continue };
        if let Some(parent) = f.get("parent").and_then(|v| v.as_str()) {
            let names = params.iter().filter_map(|p| text(p, "name").map(str::to_string));
            lambdas.entry(parent.to_string()).or_default().extend(names);
        }
        let str_of = |field: &str| f.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty());
        let (Some(id), Some(name), [a, b]) = (str_of("id"), str_of("name"), params.as_slice()) else { continue };
        let Some(path) = f.get("file").and_then(|v| v.as_str()).and_then(|x| file_path.get(x)) else { continue };
        let (key, list, enum_id) = match (param_enum(a, idx), param_enum(b, idx)) {
            (Some((e, false)), None) => (0, 1, e),
            (None, Some((e, false))) => (1, 0, e),
            _ => continue,
        };
        let (Some(k), Some(l)) = (text(&params[key], "name"), text(&params[list], "name")) else { continue };
        names.insert(id.to_string(), (k.to_string(), l.to_string()));
        let lookup = Lookup { name: name.into(), path: path.clone(), enum_id, key, list, field: String::new() };
        candidates.insert(id.to_string(), lookup);
    }

    let locals_all = store.table("locals");
    let returns_all = store.table("returns");
    let locals = by_member(&locals_all, &candidates);
    let returns = by_member(&returns_all, &candidates);
    let mut trees: IndexMap<String, Value> = IndexMap::new();
    let wanted: IndexSet<&str> = locals.values().chain(returns.values()).flatten()
        .filter_map(|r| r.get("expression").and_then(|v| v.as_str())).collect();
    for e in store.table("expressions").iter() {
        if let (Some(id), Some(ast)) = (e.get("id").and_then(|v| v.as_str()), e.get("ast"))
            && wanted.contains(id)
        {
            trees.insert(id.to_string(), ast.clone());
        }
    }
    let tree = |r: &Row| r.get("expression").and_then(|v| v.as_str()).and_then(|x| trees.get(x));

    let none = IndexSet::new();
    let mut out = Vec::new();
    for (id, mut lookup) in candidates {
        let Some((key, list)) = names.get(&id) else { continue };
        let bound = lambdas.get(&id).unwrap_or(&none);
        // ONE LOCAL, A CONST, OUTSIDE ANY BRANCH: that is what makes `v` mean the lookup on
        // every return below it.
        let [l] = locals.get(&id).map(Vec::as_slice).unwrap_or(&[]) else { continue };
        let unconditional = ["branch", "case"].iter().all(|f| l.get(f).is_none_or(Value::is_null));
        let (Some(v), true) = (l.get("name").and_then(|n| n.as_str()), unconditional && l.get("declared").and_then(|d| d.as_str()) == Some("const")) else { continue };
        let Some(field) = tree(l).and_then(|t| keyed_find(t, list, key, bound)) else { continue };
        let rows = returns.get(&id).map(Vec::as_slice).unwrap_or(&[]);
        let proven = !rows.is_empty() && rows.iter().all(|r| {
            r.get("value").is_some_and(is_blank) || tree(r).is_some_and(|t| needs(t, v))
        });
        if proven {
            lookup.field = field;
            out.push(lookup);
        }
    }
    out
}

/// A method whose one return is `lookup(p, this.S)` — the arguments in the lookup's own
/// positions — as the predicate that lookup proves over `S.<field>`.
pub fn delegated(tree: &Value, target: Option<&Value>, param: &str, enum_id: &str, all: &[Lookup]) -> Option<Predicate> {
    let target = target?;
    let (name, file) = (text(target, "name")?, slash(text(target, "file")?));
    let lookup = all.iter().find(|l| l.name == name && l.enum_id == enum_id && file.ends_with(&format!("/{}", l.path)))?;
    if kind(tree) != Some("Call") || local(tree.get("receiver")?) != Some(name) {
        return None;
    }
    let args = tree.get("args")?.as_array()?;
    let set = this_path(args.get(lookup.list)?)?;
    (args.len() == 2 && local(args.get(lookup.key)?) == Some(param)).then(|| Predicate {
        enum_id: enum_id.to_string(),
        set: format!("{set}.{}", lookup.field),
        array: false,
        both: false,
    })
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use super::super::gate_values::EnumInfo;
    use serde_json::{json, Map};

    fn idx() -> EnumIndex {
        let mut idx = EnumIndex::default();
        idx.by_id.insert("e:i".into(), EnumInfo {
            name: Some("ItemIDs".into()),
            domain: ["Red", "Green", "Blue"].iter().map(|s| s.to_string()).collect(),
        });
        idx.by_decl.insert("ItemIDs".into(), vec![("app/items.ts".into(), "e:i".into())]);
        idx
    }

    fn ident(name: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": {"k": "Implicit"}})
    }

    fn off(receiver: Value, name: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": receiver})
    }

    fn find(field: &str, op: &str) -> Value {
        let test = json!({"k": "Binary", "op": op, "left": off(ident("item"), field), "right": ident("key")});
        json!({"k": "Call", "receiver": off(ident("items"), "find"),
               "args": [{"k": "Fn", "src": "(item) => ...", "returns": [test]}]})
    }

    fn or(a: Value, b: Value) -> Value {
        json!({"k": "Binary", "op": "||", "left": a, "right": b})
    }

    /// The lookup function in its usual form, as rows, with its returns swappable.
    fn tables(returns: Value, local_tree: Value) -> Map<String, Value> {
        let key_ref = json!({"name": "ItemIDs", "file": "/repo/app/items.ts"});
        json!({
            "files": [{"id": "f:1", "path": "app/items.functions.ts"}],
            "functions": [
                {"id": "fn:1", "name": "isVisible", "file": "f:1",
                 "params": [{"name": "key", "type": "ItemIDs", "type_ref": key_ref},
                            {"name": "items", "type": "ItemDto[]"}]},
                {"id": "fn:2", "parent": "fn:1", "params": [{"name": "item"}]}
            ],
            "locals": [{"id": "lo:1", "member": "fn:1", "name": "v", "declared": "const",
                        "expression": "x:1"}],
            "returns": returns,
            "expressions": [
                {"id": "x:1", "ast": local_tree},
                {"id": "x:2", "ast": or(off(ident("v"), "Active"), off(ident("v"), "Pinned"))},
                {"id": "x:3", "ast": ident("other")},
            ],
        }).as_object().cloned().unwrap()
    }

    fn run(returns: Value, local_tree: Value) -> Vec<Lookup> {
        lookups(&Store::from_payload(tables(returns, local_tree), "typescript"), &idx())
    }

    fn real_returns() -> Value {
        json!([{"id": "rv:1", "member": "fn:1", "branch": "br:1", "value": false},
               {"id": "rv:2", "member": "fn:1", "expression": "x:2"}])
    }

    #[test]
    fn the_usual_lookup_is_proven_with_its_key_field() {
        let got = run(real_returns(), find("PartID", "==="));
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].key, got[0].list, got[0].field.as_str()), (0, 1, "PartID"));
    }

    #[test]
    fn a_return_that_does_not_read_through_the_found_item_proves_nothing() {
        // `return other` may be true whether or not the item was found.
        let returns = json!([{"id": "rv:1", "member": "fn:1", "branch": "br:1", "value": false},
                             {"id": "rv:2", "member": "fn:1", "expression": "x:3"}]);
        assert!(run(returns, find("PartID", "===")).is_empty());
        // And `return true` in the missing branch undoes the whole proof.
        let returns = json!([{"id": "rv:1", "member": "fn:1", "branch": "br:1", "value": true},
                             {"id": "rv:2", "member": "fn:1", "expression": "x:2"}]);
        assert!(run(returns, find("PartID", "===")).is_empty());
    }

    #[test]
    fn a_find_that_is_not_an_equality_on_the_key_is_not_a_lookup() {
        assert!(run(real_returns(), find("PartID", "!==")).is_empty());
        let other_list = json!({"k": "Call", "receiver": off(ident("elsewhere"), "find"),
                                "args": find("PartID", "===")["args"].clone()});
        assert!(run(real_returns(), other_list).is_empty());
    }

    #[test]
    fn an_and_needs_one_side_and_an_or_needs_both() {
        let through = off(ident("v"), "Active");
        let loose = ident("flag");
        let and = json!({"k": "Binary", "op": "&&", "left": loose.clone(), "right": through.clone()});
        assert!(needs(&and, "v"));
        assert!(!needs(&or(loose, through), "v"), "`flag || v.x` is true without v");
    }

    #[test]
    fn the_method_delegating_with_the_argument_and_a_collection_of_this_is_the_predicate() {
        let lookup = Lookup { name: "isVisible".into(), path: "app/items.functions.ts".into(),
                              enum_id: "e:i".into(), key: 0, list: 1, field: "PartID".into() };
        let this_items = off(off(json!({"k": "This"}), "model"), "Items");
        let call = |args: Vec<Value>| json!({"k": "Call", "receiver": ident("isVisible"), "args": args});
        let target = json!({"name": "isVisible", "file": "C:/repo/app/items.functions.ts"});

        let p = delegated(&call(vec![ident("id"), this_items.clone()]), Some(&target), "id", "e:i",
                          std::slice::from_ref(&lookup)).expect("a predicate");
        assert_eq!(p.set, "model.Items.PartID");
        assert!(!p.both, "false proves nothing");
        // A fixed key is not the argument: `isVisible(OTHER, this.model.Items)` ignores `id`.
        assert!(delegated(&call(vec![ident("OTHER"), this_items.clone()]), Some(&target), "id", "e:i",
                          std::slice::from_ref(&lookup)).is_none());
        // Swapped arguments, another enum, another declaration: none of them is this lookup.
        assert!(delegated(&call(vec![this_items.clone(), ident("id")]), Some(&target), "id", "e:i",
                          std::slice::from_ref(&lookup)).is_none());
        assert!(delegated(&call(vec![ident("id"), this_items.clone()]), Some(&target), "id", "e:x",
                          std::slice::from_ref(&lookup)).is_none());
        let elsewhere = json!({"name": "isVisible", "file": "C:/repo/other/items.functions.ts"});
        assert!(delegated(&call(vec![ident("id"), this_items]), Some(&elsewhere), "id", "e:i",
                          std::slice::from_ref(&lookup)).is_none());
    }
}
