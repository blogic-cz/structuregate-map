//! THE LIST A CALLED FUNCTION OR METHOD RETURNS, for `key_loops`, and the reader that follows the calls in it.

use super::key_fields::{callables, id_of, local_writes, one_returns, returns_by_member};
use super::key_follow::Returned;
use super::key_branches::KeySet;
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::HashMap;

/// Whether the callback `function` has one return handing back its first parameter, bare or as the spread of an object.
fn keeps_parameter(function: &str, by_member: &HashMap<String, Vec<&Row>>, params: &HashMap<String, Vec<String>>) -> bool {
    let (Some(param), Some([ret])) = (params.get(function).and_then(|p| p.first()), by_member.get(function).map(Vec::as_slice)) else { return false };
    let bare = |v: &Value| v.get("$kind").and_then(Value::as_str) == Some("Identifier") && v.get("$expr").and_then(Value::as_str) == Some(param.as_str());
    ret.get("value").is_some_and(|v| bare(v) || v.get("$spread").is_some_and(bare))
}

/// Where a list's elements are read from: the callees' returns, the callbacks' parameters, and every `.push(..)` with the
/// member each callback sits in.
pub(super) struct ListSource<'r, 'a> {
    pub(super) rows: &'r super::gate_tsrows::TsRows,
    pub(super) by_member: &'r HashMap<String, Vec<&'a Row>>,
    pub(super) params: &'r HashMap<String, Vec<String>>,
    pub(super) pushes: &'r super::key_pushes::Pushes<'a>,
    pub(super) parent: &'r HashMap<String, String>,
    /// Function id -> the names its first parameter's ARRAY pattern binds, by index (`([value, label]) => ..`).
    pub(super) unpacked: &'r HashMap<String, Vec<Option<String>>>,
    /// The config's `i18nDictionaryCalls`: a call of one handed to a callee's parameter is every key under its prefix there.
    pub(super) dict: &'r [String],
    /// `shape_members` id -> the value the property is written with, for the ones a template input reads (`shape_values`).
    pub(super) shaped: &'r HashMap<String, Value>,
    /// The pipes a template applies to a bound list, by name (`key_pipes`).
    pub(super) pipes: &'r super::key_pipes::Pipes,
}

/// Function id -> the names an array pattern in its first parameter binds, by index (`functions.params[0].bindings`).
pub(super) fn unpacked(store: &Store<'_>) -> HashMap<String, Vec<Option<String>>> {
    let mut out = HashMap::new();
    for f in store.table("functions").iter() {
        let (Some(id), Some(Value::Array(ps))) = (id_of(f, "id"), f.get("params")) else { continue };
        let Some(Value::Array(binds)) = ps.first().and_then(|p| p.get("bindings")) else { continue };
        let mut names: Vec<Option<String>> = Vec::new();
        for b in binds {
            let (Some(i), Some(name)) = (b.get("index").and_then(Value::as_u64), b.get("name").and_then(Value::as_str)) else { continue };
            let i = i as usize;
            if names.len() <= i {
                names.resize(i + 1, None);
            }
            names[i] = Some(name.to_string());
        }
        out.insert(id, names);
    }
    out
}

impl ListSource<'_, '_> {
    /// The elements one `.push(..)` adds: each argument, a `...` spread of a call replaced by what its callee returns -
    /// or of a chain on a list (`...toList(e).map(..)`, `chained`).
    pub(super) fn pushed(&self, push: &Row) -> Vec<(Value, Option<String>)> {
        self.pushed_in(push, &mut Vec::new())
    }

    fn pushed_in(&self, push: &Row, seen: &mut Vec<String>) -> Vec<(Value, Option<String>)> {
        let mut out = Vec::new();
        for arg in push.get("args").and_then(Value::as_array).into_iter().flatten() {
            let spread = arg.get("$spread");
            if let Some((items, owner)) = self.callee_lists_in(spread, seen) {
                out.extend(items.into_iter().map(|i| (i, Some(owner.clone()))));
            } else if let Some(items) = spread.filter(|s| !s.is_array()).and_then(|s| self.chained(s, seen)) {
                out.extend(items.into_iter().map(|i| (i, None)));
            } else {
                out.push((arg.clone(), None));
            }
        }
        out
    }

    /// THE ELEMENTS A CHAIN ON A LIST HOLDS (rows 113, #117): an array literal; a call of a function whose every return is a list
    /// (`callee_lists`); `<list>.filter(..)`/`.slice(..)` - a subset of the same elements, so all of them are read, the union of what
    /// any call can show; `<list>.concat(..)` - the receiver's and every argument's; `<list>.map(cb)` - every element of the list through
    /// `cb`'s one return: the element itself when `cb` hands its parameter back (bare or spread), else the object it builds with each
    /// read of the parameter's property (`x.name`) replaced by that element's (`through`). `<list>` is an array, another such chain,
    /// or a call of a function whose every return is a list (`callee_lists`): `toItems(Hue, dict).filter(..)`. None for anything else -
    /// a list read in part would hand the control fewer elements than it holds.
    pub(super) fn chained(&self, value: &Value, seen: &mut Vec<String>) -> Option<Vec<Value>> {
        let o = match value {
            Value::Array(xs) => return Some(xs.clone()),
            Value::Object(o) => o,
            _ => return None,
        };
        // A CALL OF A FUNCTION RETURNING A LIST (`return toItems(Hue, dict);`) is its list.
        let Some(receiver) = o.get("$receiver") else { return self.callee_lists_in(Some(value), seen).map(|(items, _)| items) };
        let method = o.get("$call").and_then(Value::as_str)?.rsplit_once('.')?.1;
        match method {
            "filter" | "slice" => self.chained(receiver, seen),
            // `[..].concat(a, b)`: the receiver's elements, then every argument's - each a list, or the whole is none.
            "concat" => {
                let mut items = self.chained(receiver, seen)?;
                for arg in o.get("$args")?.as_array()? {
                    items.extend(self.chained(arg, seen)?);
                }
                Some(items)
            }
            "map" => {
                let callback = o.get("$args")?.as_array()?.first()?.get("$fn_id")?.as_str()?;
                if keeps_parameter(callback, self.by_member, self.params) {
                    return self.chained(receiver, seen);
                }
                let built = self.mapped_object(value)?;
                let param = self.params.get(callback).and_then(|p| p.first())?.clone();
                Some(self.chained(receiver, seen)?.iter().map(|element| through(built, &param, element)).collect())
            }
            _ => None,
        }
    }

    /// THE ELEMENTS OF EVERY LIST A CALL'S CALLEE RETURNS, when every return of it is such a list, and the callee's
    /// id: `items(flag) { if (flag) { return [{ label: 'a' }]; } return [{ label: 'b' }]; }` is both elements. The
    /// callee is a class member, or - when no member of that name sits in the file - a free function (a `function`
    /// or a `const` arrow), found by the call's `$target`. A return that is anything else reads nothing. The
    /// branches are not carried - they test the callee's parameters, which no gate shares.
    ///
    /// A RETURNED LOCAL IS ITS PUSHES TOO: `const out = []; out.push(new A()); return out;` - in a callback of the callee
    /// as well (`.then(() => out.push(..))`), which the run may finish later, but whose elements the list still holds.
    pub(super) fn callee_lists(&self, value: Option<&Value>) -> Option<(Vec<Value>, String)> {
        self.callee_lists_in(value, &mut Vec::new())
    }

    /// `callee_lists` along a path of callees: one already on it (a helper reaching itself) reads nothing.
    fn callee_lists_in(&self, value: Option<&Value>, seen: &mut Vec<String>) -> Option<(Vec<Value>, String)> {
        let target = value?.get("$target")?;
        let callee = self.rows.member_named(target).or_else(|| self.rows.function_named(target))?;
        if seen.contains(&callee) {
            return None;
        }
        seen.push(callee.clone());
        let found = self.callee_items(value, &callee, seen);
        seen.pop();
        Some((found?, callee))
    }

    fn callee_items(&self, value: Option<&Value>, callee: &str, seen: &mut Vec<String>) -> Option<Vec<Value>> {
        let args = value?.get("$args").and_then(Value::as_array);
        if let Some(items) = self.member_lists_in(callee, seen) {
            return Some(match args {
                Some(args) => self.handed(callee, items, args),
                None => items,
            });
        }
        let args = args?;
        self.table_lists(callee, args).or_else(|| self.mapped_lists(callee, args))
    }

    /// Each handed dictionary by the parameter it lands in - None when the call hands none.
    fn dictionaries(&self, callee: &str, args: &[Value]) -> Option<HashMap<&str, Value>> {
        let names = self.params.get(callee)?;
        let dicts: HashMap<&str, Value> = names
            .iter()
            .zip(args)
            .filter_map(|(n, a)| Some((n.as_str(), super::key_follow::dictionary(a, self.dict)?)))
            .collect();
        (!dicts.is_empty()).then_some(dicts)
    }

    /// A DICTIONARY THE CALLER HANDS IN, INDEXED BY THE ELEMENTS (rows 110, #117): `convertToItems(Hue,
    /// t.getTranslationDictionary('enums.hue'))` over a callee that pushes `...convertEnumToArray(e).map((x) => ({ label:
    /// translations?.[x.name] ?? x.name }))` - a property reading the parameter by any index (`$param_index`, alone or
    /// as a side of `??`/`||`) is a key under the prefix the caller's dictionary call spells (`key_follow::dictionary`).
    /// A spread of a `.map(cb)` whose one returned object holds such a property is that object: the elements the
    /// receiver yields are not read, only what each of them is labelled with. Anything else stays as it was.
    fn handed(&self, callee: &str, items: Vec<Value>, args: &[Value]) -> Vec<Value> {
        let Some(dicts) = self.dictionaries(callee, args) else { return items };
        items
            .into_iter()
            .map(|item| {
                let built = item.get("$spread").and_then(|s| self.mapped_object(s)).unwrap_or(&item);
                relabelled(built, &dicts).unwrap_or(item)
            })
            .collect()
    }

    /// A LIST MAPPED STRAIGHT OUT OF ITS RECEIVER, LABELLED FROM A HANDED DICTIONARY (#118): every return of `callee` is a
    /// `.map(cb)` - `Object.entries(e).map(([name, v]) => ({ label: dict[name] }))`, `Object.keys(e).filter(..).map(..)` -
    /// whose one returned object reads a dictionary the call hands in (`handed`). Each return is one element, that object
    /// with the dictionary's keys in place of the read; what the receiver yields is not read, as for a pushed spread. None
    /// when any return is anything else, or reads no handed dictionary.
    fn mapped_lists(&self, callee: &str, args: &[Value]) -> Option<Vec<Value>> {
        let dicts = self.dictionaries(callee, args)?;
        self.by_member.get(callee)?.iter().map(|r| relabelled(self.mapped_object(r.get("value")?)?, &dicts)).collect()
    }

    /// The one object a `.map(cb)` call's callback returns, `cb` the function its first argument names.
    fn mapped_object(&self, call: &Value) -> Option<&Value> {
        let method = call.get("$call").and_then(Value::as_str)?.rsplit_once('.')?.1;
        if method != "map" {
            return None;
        }
        let callback = call.get("$args")?.as_array()?.first()?.get("$fn_id")?.as_str()?;
        let [ret] = self.by_member.get(callback)?.as_slice() else { return None };
        ret.get("value").filter(|v| v.as_object().is_some_and(|o| o.keys().all(|k| !k.starts_with('$'))))
    }

    /// A TABLE THE CALLER HANDS IN, REBUILT INTO ITEMS (rows 109, #117): every return of `callee` is
    /// `Object.entries(p).map(([k, v]) => ({ value: k, label: v }))` - or `Object.values(p).map((v) => ({ label: v }))` - over a
    /// parameter `p`, and the call hands `p` a const object (`toItems(hueLabels)` over `{ Red: 'hue.red' }`). Each entry is one
    /// element: the callback's one returned object, a property naming the entry's key name its key, one naming its value its
    /// value, any other property kept as written. None for any other shape - a table read in part would hand the child fewer
    /// elements than it holds.
    fn table_lists(&self, callee: &str, args: &[Value]) -> Option<Vec<Value>> {
        let names = self.params.get(callee)?;
        let mut items = Vec::new();
        for r in self.by_member.get(callee)? {
            let o = r.get("value")?.as_object()?;
            if o.get("$method").and_then(Value::as_str) != Some("map") {
                return None;
            }
            let receiver = o.get("$receiver")?.as_object()?;
            let entries = match receiver.get("$call").and_then(Value::as_str)? {
                "Object.entries" => true,
                "Object.values" => false,
                _ => return None,
            };
            let [param] = receiver.get("$args")?.as_array()?.as_slice() else { return None };
            let bare = param.get("$kind").and_then(Value::as_str) == Some("Identifier");
            let at = names.iter().position(|n| bare && param.get("$expr").and_then(Value::as_str) == Some(n.as_str()))?;
            let table = args.get(at)?.as_object().filter(|t| t.keys().all(|k| !k.starts_with('$')))?;
            let callback = o.get("$args")?.as_array()?.first()?.get("$fn_id")?.as_str()?;
            let [ret] = self.by_member.get(callback)?.as_slice() else { return None };
            let built = ret.get("value")?.as_object().filter(|b| b.keys().all(|k| !k.starts_with('$')))?;
            let (key_name, value_name) = if entries {
                let bound = self.unpacked.get(callback)?;
                (bound.first().cloned().flatten(), bound.get(1).cloned().flatten())
            } else {
                (None, self.params.get(callback)?.first().cloned())
            };
            for (key, value) in table {
                let mut element = serde_json::Map::new();
                for (prop, v) in built {
                    let named = v.get("$kind").and_then(Value::as_str) == Some("Identifier");
                    let name = v.get("$expr").and_then(Value::as_str).filter(|_| named);
                    let filled = match name {
                        Some(n) if key_name.as_deref() == Some(n) => Value::String(key.clone()),
                        Some(n) if value_name.as_deref() == Some(n) => value.clone(),
                        _ => v.clone(),
                    };
                    element.insert(prop.clone(), filled);
                }
                items.push(Value::Object(element));
            }
        }
        Some(items)
    }

    /// The elements of every list `member` returns - a method, a getter, a function - when every return is one.
    pub(super) fn member_lists(&self, member: &str) -> Option<Vec<Value>> {
        self.member_lists_in(member, &mut vec![member.to_string()])
    }

    fn member_lists_in(&self, member: &str, seen: &mut Vec<String>) -> Option<Vec<Value>> {
        let mut items = Vec::new();
        for r in self.by_member.get(member)? {
            items.extend(self.chained(r.get("value")?, seen)?);
            for p in super::key_instances::local_pushes(r, self.pushes, self.parent) {
                items.extend(self.pushed_in(p, seen).into_iter().map(|(i, _)| i));
            }
        }
        Some(items)
    }
}

/// `built` - the object a `.map(cb)` callback returns - for one element of the list: a read of the parameter's own property
/// (`x.name`, `x?.name`) is the element's property and a bare read of the parameter the element, anywhere in the tree; a property
/// the element lacks stays the read.
pub(super) fn through(built: &Value, param: &str, element: &Value) -> Value {
    match built {
        Value::Object(o) => {
            if let Some(text) = o.get("$expr").and_then(Value::as_str).filter(|_| o.contains_key("$kind")) {
                if text == param {
                    return element.clone();
                }
                let property = text.strip_prefix(param).and_then(|t| t.strip_prefix("?.").or_else(|| t.strip_prefix('.')));
                return property.filter(|p| !p.contains(['.', '[', '('])).and_then(|p| element.get(p)).unwrap_or(built).clone();
            }
            Value::Object(o.iter().map(|(k, v)| (k.clone(), through(v, param, element))).collect())
        }
        Value::Array(xs) => Value::Array(xs.iter().map(|x| through(x, param, element)).collect()),
        other => other.clone(),
    }
}

/// An object with every property that reads a handed dictionary (`$param_index`, alone or as a side of `??`/`||`)
/// replaced by that dictionary's keys - None when no property does.
fn relabelled(object: &Value, dicts: &HashMap<&str, Value>) -> Option<Value> {
    fn reads<'d>(v: &Value, dicts: &'d HashMap<&str, Value>) -> Option<&'d Value> {
        if let Some(found) = v.get("$param_index").and_then(Value::as_str).and_then(|p| dicts.get(p)) {
            return Some(found);
        }
        let either = matches!(v.get("$logic").and_then(Value::as_str), Some("??" | "||"));
        v.get("$operands").and_then(Value::as_array).filter(|_| either)?.iter().find_map(|o| reads(o, dicts))
    }
    let mut out = object.as_object()?.clone();
    let mut changed = false;
    for v in out.values_mut() {
        if let Some(keys) = reads(v, dicts) {
            *v = keys.clone();
            changed = true;
        }
    }
    changed.then_some(Value::Object(out))
}

/// Runs `f` with the reader `key_fields` follows a call with - for a loop's element, whose property may hold one.
pub(super) fn with_reader<R>(store: &Store<'_>, f: impl FnOnce(&Returned<'_, '_>) -> R) -> R {
    let keys: KeySet = store.table("translations").iter().filter_map(|t| super::key_fields::text(t, "key")).collect();
    let i18n = store.option_list("i18n_calls");
    let dict = store.option_list("i18n_dictionary_calls");
    let rows = super::gate_tsrows::TsRows::of(store);
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
    let branches = store.shared("branches", super::key_branches::Branches::new);
    f(&Returned {
        rows: &rows, one_return: &one_return, every: &every, keys: &keys, i18n: &i18n, dict: &dict, params: &params,
        fn_of_local: &fn_of_local, writes: &writes, parent: &parent, pushes: &pushes, classes: &classes, branches: &branches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn keeps(ret: Value) -> bool {
        let row = Row::Built(json!({"member": "fn:1", "value": ret}).as_object().cloned().expect("row"));
        let by_member = HashMap::from([("fn:1".to_string(), vec![&row])]);
        keeps_parameter("fn:1", &by_member, &HashMap::from([("fn:1".to_string(), vec!["b".to_string()])]))
    }

    #[test]
    fn a_map_handing_its_parameter_back_keeps_the_elements_and_one_building_new_ones_does_not() {
        let bare = json!({"$expr": "b", "$kind": "Identifier"});
        assert!(keeps(bare.clone()));
        assert!(keeps(json!({"$spread": bare, "x": 1})));
        assert!(!keeps(json!({"label": "x"})), "an object of its own is another element");
        assert!(!keeps(json!({"$expr": "other", "$kind": "Identifier"})), "a name that is not the parameter");
    }

    #[test]
    fn a_mapped_object_reads_the_element_through_its_parameter() {
        let read = |t: &str| json!({"$expr": t, "$kind": "PropertyAccessExpression"});
        let built = json!({"label": read("x.name"), "value": read("x?.value"), "deep": read("x.name.length"), "other": read("y.name"),
            "either": {"$logic": "??", "$operands": [read("x.missing"), read("x.name")]}});
        let element = json!({"name": "demo.alpha", "value": 1});
        assert_eq!(through(&built, "x", &element), json!({"label": "demo.alpha", "value": 1, "deep": read("x.name.length"), "other": read("y.name"),
            "either": {"$logic": "??", "$operands": [read("x.missing"), "demo.alpha"]}}));
        assert_eq!(through(&json!({"$expr": "x", "$kind": "Identifier"}), "x", &element), element);
    }
}
