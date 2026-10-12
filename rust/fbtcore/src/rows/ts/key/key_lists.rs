//! THE LIST A CALLED FUNCTION OR METHOD RETURNS, for `key_loops`, and the reader that follows the calls in it.

use super::key_fields::{callables, id_of, local_writes, one_returns, returns_by_member};
use super::key_follow::Returned;
use super::key_branches::KeySet;
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::HashMap;

/// The elements one `returns` value is a list of: an array literal, or `<list>.filter(..)` over one - the call
/// the extractor reads the receiver of with its local already spelled. A FILTER KEEPS A SUBSET OF THE SAME
/// ELEMENTS, so reading all of them is the union of what any call can show; `map`, `slice` and the rest build
/// other elements, and are no list here.
fn list_of(value: &Value) -> Option<&Vec<Value>> {
    match value {
        Value::Array(xs) => Some(xs),
        Value::Object(o) if o.get("$call").and_then(Value::as_str).is_some_and(|c| c.rsplit_once('.').is_some_and(|(_, m)| m == "filter")) => {
            match o.get("$receiver") {
                Some(Value::Array(xs)) => Some(xs),
                _ => None,
            }
        }
        _ => None,
    }
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
) -> Option<(Vec<Value>, String)> {
    let target = value?.get("$target")?;
    let callee = rows.member_named(target).or_else(|| rows.function_named(target))?;
    let mut items = Vec::new();
    for r in by_member.get(&callee)? {
        items.extend(list_of(r.get("value")?)?.iter().cloned());
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
    let parent: HashMap<String, String> = store
        .table("functions")
        .iter()
        .filter_map(|fun| Some((id_of(fun, "id")?, id_of(fun, "parent").filter(|p| !p.is_empty())?)))
        .collect();
    f(&Returned {
        rows: &rows, one_return: &one_return, every: &every, keys: &keys, i18n: &i18n, dict: &dict, params: &params,
        fn_of_local: &fn_of_local, writes: &writes, parent: &parent,
    })
}
