//! A LIST A PIPE REBUILDS BEFORE A CONTROL GETS IT (#117 follow-up): `[options]="vm.hues | labelled : vm.huePath"` over a pipe
//! whose `transform(items, path)` returns `items.map((item) => ({ ...item, label: dict[item.label] ?? '' }))` with
//! `const dict = this.t.getTranslationDictionary(path)` - every element of the bound list, its `label` every key under the
//! prefix the template hands as `path` (a string literal, or a read of a property an object literal writes with one,
//! `shape_values`).
//!
//! The pipe is the `pipes` row of that name (two of one name are neither), its `transform` the member read. Each return of it
//! is read: an array literal is its elements; the list parameter itself is the bound list; `<list>.map(cb)` is each element
//! through `cb`'s one returned object - the parameter's reads replaced by the element's (`key_lists::through`), a spread of it
//! the element's own properties under the ones written beside it, and a property reading a `const` local of `transform` by
//! an index (`dict[item.label]`, alone or a side of `??`/`||`) every key the dictionary call it holds spells once the
//! parameters it is handed are the template's strings. Any other return refuses the pipe: a list read in part would hand
//! the control fewer elements than it holds.

use super::astreads::{is_read, read_rows};
use super::key_fields::{id_of, text};
use super::key_lists::{spread, through, ListSource};
use super::key_loops::Element;
use super::store::Store;
use serde_json::Value;
use std::collections::HashMap;

pub(super) struct Pipes {
    /// pipe name -> its `transform` member; None when two pipes share the name or the class has none.
    transform: HashMap<String, Option<String>>,
    /// (member, name) -> the value of the one `const` local the member declares of that name; None for two.
    locals: HashMap<(String, String), Option<Value>>,
}

impl Pipes {
    pub(super) fn new(store: &Store<'_>) -> Pipes {
        let mut by_class: HashMap<String, String> = HashMap::new();
        for m in store.table("members").iter() {
            if let (Some(id), Some(class), Some("transform")) = (id_of(m, "id"), text(m, "class"), text(m, "name").as_deref()) {
                by_class.insert(class, id);
            }
        }
        let mut transform: HashMap<String, Option<String>> = HashMap::new();
        for p in store.table("pipes").iter() {
            let (Some(name), Some(class)) = (text(p, "pipe_name"), text(p, "class")) else { continue };
            let member = by_class.get(&class).cloned();
            transform.entry(name).and_modify(|t| *t = None).or_insert(member);
        }
        let mut locals: HashMap<(String, String), Option<Value>> = HashMap::new();
        if !transform.is_empty() {
            for l in store.table("locals").iter().filter(|l| text(l, "declared").as_deref() == Some("const")) {
                let (Some(member), Some(name)) = (text(l, "member"), text(l, "name")) else { continue };
                let value = l.get("value").filter(|v| !v.is_null()).cloned();
                locals.entry((member, name)).and_modify(|v| *v = None).or_insert(value);
            }
        }
        Pipes { transform, locals }
    }

    /// The list a `Pipe` node of a template hands on, `input` reading the list it is applied to; its holder is the input's.
    pub(super) fn piped(&self, tree: &Value, src: &ListSource<'_, '_>, input: impl Fn(&Value) -> Option<(String, Vec<Element>)>) -> Option<(String, Vec<Element>)> {
        let name = tree.get("name")?.as_str()?;
        // `x$ | async`: what the observable emits - the list its field holds.
        if name == "async" {
            return input(tree.get("exp")?);
        }
        let member = self.transform.get(name)?.as_deref()?;
        let params = src.params.get(member)?;
        let list = params.first()?;
        // A LIST THE MAP CANNOT READ (an API's) still takes the labels the pipe writes: one element standing for each, so a dictionary
        // the pipe relabels from is every key under its prefix - and anything else the pipe hands on reads no key.
        let exp = tree.get("exp")?;
        let (holder, items) = input(exp).or_else(|| Some((read_rows(exp).into_iter().next()?, vec![Element::written(Value::Object(Default::default()), Vec::new())])))?;
        let args: Vec<Option<String>> = tree.get("args").and_then(Value::as_array).into_iter().flatten().map(|a| spelled(a, src)).collect();
        let handed: HashMap<&str, &str> = params.iter().skip(1).zip(&args).filter_map(|(p, a)| Some((p.as_str(), a.as_deref()?))).collect();
        let mut out = Vec::new();
        for r in src.by_member.get(member)? {
            let value = r.get("value")?;
            if let Value::Array(xs) = value {
                out.extend(xs.iter().map(|x| Element::written(x.clone(), Vec::new())));
                continue;
            }
            if value.get("$kind").and_then(Value::as_str) == Some("Identifier") && value.get("$expr").and_then(Value::as_str) == Some(list.as_str()) {
                out.extend(items.iter().cloned());
                continue;
            }
            let call = value.get("$call").and_then(Value::as_str)?;
            let on_list = |m: &str| call.strip_suffix(m).map(|r| r.strip_suffix('?').unwrap_or(r)) == Some(list.as_str());
            // A FILTER OR A SLICE OF THE LIST keeps a subset of it: every element any call can show.
            if on_list(".filter") || on_list(".slice") {
                out.extend(items.iter().cloned());
                continue;
            }
            if !on_list(".map") {
                return None;
            }
            let callback = value.get("$args")?.as_array()?.first()?.get("$fn_id")?.as_str()?;
            let [ret] = src.by_member.get(callback)?.as_slice() else { return None };
            let (built, param) = (ret.get("value")?, src.params.get(callback)?.first()?);
            for item in &items {
                let mut element = item.clone();
                element.value = self.relabelled(spread(through(built, param, &item.value)), member, &handed, src.dict);
                out.push(element);
            }
        }
        Some((holder, out))
    }

    /// Every property reading a dictionary `member` holds in a `const` local, by an index, as the keys that dictionary spells.
    fn relabelled(&self, object: Value, member: &str, handed: &HashMap<&str, &str>, dict: &[String]) -> Value {
        let Value::Object(mut o) = object else { return object };
        for v in o.values_mut() {
            if let Some(keys) = self.dictionary_read(v, member, handed, dict) {
                *v = keys;
            }
        }
        Value::Object(o)
    }

    fn dictionary_read(&self, v: &Value, member: &str, handed: &HashMap<&str, &str>, dict: &[String]) -> Option<Value> {
        if v.get("$kind").and_then(Value::as_str) == Some("ElementAccessExpression") {
            let name = v.get("$expr")?.as_str()?.split_once('[')?.0.trim_end_matches('?');
            let mut call = self.locals.get(&(member.to_string(), name.to_string()))?.clone()?;
            for arg in call.get_mut("$args")?.as_array_mut()? {
                let bare = arg.get("$kind").and_then(Value::as_str) == Some("Identifier");
                if let Some(text) = arg.get("$expr").and_then(Value::as_str).filter(|_| bare).and_then(|p| handed.get(p)) {
                    *arg = Value::String(text.to_string());
                }
            }
            return super::key_follow::dictionary(&call, dict);
        }
        let either = matches!(v.get("$logic").and_then(Value::as_str), Some("??" | "||"));
        v.get("$operands").and_then(Value::as_array).filter(|_| either)?.iter().find_map(|o| self.dictionary_read(o, member, handed, dict))
    }
}

/// The string a pipe argument is: a literal, or a read of an object literal's property written with one (`shape_values`).
fn spelled(arg: &Value, src: &ListSource<'_, '_>) -> Option<String> {
    if arg.get("k").and_then(Value::as_str) == Some("Literal") {
        return arg.get("v")?.as_str().map(str::to_string);
    }
    let row = read_rows(arg).into_iter().next().filter(|_| is_read(arg))?;
    src.shaped.get(&row)?.as_str().map(str::to_string)
}
