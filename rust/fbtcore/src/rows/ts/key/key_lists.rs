//! THE LIST A CALLED FUNCTION OR METHOD RETURNS, for `key_loops`, and the reader that follows the calls in it.

use super::key_fields::{callables, id_of, local_writes, one_returns, returns_by_member};
use super::key_follow::Returned;
use super::key_branches::KeySet;
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::HashMap;

/// The elements one `returns` value is a list of: an array literal, or `<list>.filter(..)` / `<list>.slice(..)` over
/// one - the call the extractor reads the receiver of with its local already spelled - or `<list>.map(cb)` over one
/// when `cb` hands its parameter back (`(b) => b`) or an object spreading it (`(b) => ({ ...b, x })`). A FILTER OR A
/// SLICE KEEPS A SUBSET OF THE SAME ELEMENTS, so reading all of them is the union of what any call can show; a `map`
/// building other elements (`(b) => ({ label: 'x' })`) and the rest are no list here, and a spread copy `[...list]` is
/// the extractor's own array.
fn list_of<'v>(value: &'v Value, by_member: &HashMap<String, Vec<&Row>>, params: &HashMap<String, Vec<String>>) -> Option<&'v Vec<Value>> {
    match value {
        Value::Array(xs) => Some(xs),
        Value::Object(o) => {
            let method = o.get("$call").and_then(Value::as_str)?.rsplit_once('.')?.1;
            let keeps = match method {
                "filter" | "slice" => true,
                "map" => keeps_parameter(o.get("$args")?.as_array()?.first()?.get("$fn_id")?.as_str()?, by_member, params),
                _ => false,
            };
            match o.get("$receiver") {
                Some(Value::Array(xs)) if keeps => Some(xs),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether the callback `function` has one return handing back its first parameter, bare or as the spread of an object.
fn keeps_parameter(function: &str, by_member: &HashMap<String, Vec<&Row>>, params: &HashMap<String, Vec<String>>) -> bool {
    let (Some(param), Some([ret])) = (params.get(function).and_then(|p| p.first()), by_member.get(function).map(Vec::as_slice)) else { return false };
    let bare = |v: &Value| v.get("$kind").and_then(Value::as_str) == Some("Identifier") && v.get("$expr").and_then(Value::as_str) == Some(param.as_str());
    ret.get("value").is_some_and(|v| bare(v) || v.get("$spread").is_some_and(bare))
}

/// THE ELEMENTS OF EVERY LIST A CALL'S CALLEE RETURNS, when every return of it is such a list, and the callee's
/// id: `items(flag) { if (flag) { return [{ label: 'a' }]; } return [{ label: 'b' }]; }` is both elements. The
/// callee is a class member, or - when no member of that name sits in the file - a free function (a `function`
/// or a `const` arrow), found by the call's `$target`. A return that is anything else reads nothing. The
/// branches are not carried - they test the callee's parameters, which no gate shares.
pub(super) fn callee_lists(
    value: Option<&Value>,
    rows: &super::gate_tsrows::TsRows,
    by_member: &HashMap<String, Vec<&Row>>,
    params: &HashMap<String, Vec<String>>,
) -> Option<(Vec<Value>, String)> {
    let target = value?.get("$target")?;
    let callee = rows.member_named(target).or_else(|| rows.function_named(target))?;
    let mut items = Vec::new();
    for r in by_member.get(&callee)? {
        items.extend(list_of(r.get("value")?, by_member, params)?.iter().cloned());
    }
    Some((items, callee))
}

/// Runs `f` with the reader `key_fields` follows a call with - for a loop's element, whose property may hold one.
pub(super) fn with_reader<R>(store: &Store<'_>, f: impl FnOnce(&Returned<'_, '_>) -> R) -> R {
    let keys: KeySet = store.table("translations").iter().filter_map(|t| super::key_fields::text(t, "key")).collect();
    let i18n = store.option_list("i18n_calls");
    let dict = store.option_list("i18n_dictionary_calls");
    let rows = super::gate_tsrows::TsRows::new(store);
    let returns = store.table("returns");
    let one_return = one_returns(&returns);
    let every = returns_by_member(&returns);
    let (params, fn_of_local) = callables(store);
    let assignments = store.table("assignments");
    let writes = local_writes(&assignments);
    let (calls, members) = (store.table("calls"), store.table("members"));
    let pushes = super::key_pushes::pushes(&calls, &members, &store.table("functions"));
    let parent: HashMap<String, String> = store
        .table("functions")
        .iter()
        .filter_map(|fun| Some((id_of(fun, "id")?, id_of(fun, "parent").filter(|p| !p.is_empty())?)))
        .collect();
    let classes = super::key_classes::Classes::new(store);
    f(&Returned {
        rows: &rows, one_return: &one_return, every: &every, keys: &keys, i18n: &i18n, dict: &dict, params: &params,
        fn_of_local: &fn_of_local, writes: &writes, parent: &parent, pushes: &pushes, classes: &classes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map_call(function: &str) -> Value {
        json!({"$call": "boxes.map", "$args": [{"$fn_id": function}], "$receiver": [{"label": "k"}]})
    }

    fn callback(ret: Value) -> (HashMap<String, Vec<Row>>, HashMap<String, Vec<String>>) {
        let row = Row::Built(json!({"member": "fn:1", "value": ret}).as_object().cloned().expect("row"));
        (HashMap::from([("fn:1".to_string(), vec![row])]), HashMap::from([("fn:1".to_string(), vec!["b".to_string()])]))
    }

    fn reads(ret: Value, call: &Value) -> bool {
        let (rows, params) = callback(ret);
        let by_member: HashMap<String, Vec<&Row>> = rows.iter().map(|(k, v)| (k.clone(), v.iter().collect())).collect();
        list_of(call, &by_member, &params).is_some()
    }

    #[test]
    fn a_map_handing_its_parameter_back_keeps_the_elements_and_one_building_new_ones_does_not() {
        let bare = json!({"$expr": "b", "$kind": "Identifier"});
        assert!(reads(bare.clone(), &map_call("fn:1")));
        assert!(reads(json!({"$spread": bare, "x": 1}), &map_call("fn:1")));
        assert!(!reads(json!({"label": "x"}), &map_call("fn:1")), "an object of its own is another element");
        assert!(!reads(json!({"$expr": "other", "$kind": "Identifier"}), &map_call("fn:1")), "a name that is not the parameter");
        assert!(reads(bare, &json!({"$call": "boxes.slice", "$args": [], "$receiver": [{"label": "k"}]})));
    }
}
