//! A FIELD THE CLASS READS THROUGH `this`, IN AN ELEMENT IT WRITES (#117 follow-up): `{ Label: this.labels.fees.entry }` over
//! `labels = { fees: { entry: `${this.resource}.entry` } }`, `resource = `${this.base}.fees`` and a base class's `base = 'demo'` is
//! the string `demo.fees.entry` - so a list a component writes in place names the key its labels table spells.
//!
//! A read is `this.<field>` and a dotted path of plain names into an object literal the field is initialised with; a template
//! literal is its pieces with every hole folded, when each hole is such a read that folds to a string. The field is looked up
//! on the class and up its `extends` chain (`classes.extends.id`) - `this` is the instance, so a subclass's own field wins.
//! READ ONLY WHEN NOTHING ELSE CAN SET IT: a field any assignment writes, or one carrying a decorator (an `@Input`), is not read;
//! a field reached again while it is being read (`a = `${this.a}x``) is not either. Anything else stays the read it was.

use super::key_fields::{id_of, text};
use super::store::Store;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) struct ThisFields {
    /// (class id, field name) -> (member id, its initializer).
    fields: HashMap<(String, String), (String, Value)>,
    /// class id -> the class it extends.
    bases: HashMap<String, String>,
    /// member id -> its class: an element a method hands back is folded only in that method's class or a subclass of it.
    member_class: HashMap<String, String>,
}

impl ThisFields {
    pub(super) fn new(store: &Store<'_>) -> ThisFields {
        let written: HashSet<String> = store.table("assignments").iter().filter_map(|a| id_of(a, "target_id")).collect();
        let mut fields = HashMap::new();
        let mut member_class = HashMap::new();
        for m in store.table("members").iter() {
            let (Some(id), Some(class), Some(name)) = (id_of(m, "id"), text(m, "class"), text(m, "name")) else { continue };
            member_class.insert(id.clone(), class.clone());
            if written.contains(&id) || m.get("decorator_line").is_some_and(|d| !d.is_null()) {
                continue;
            }
            let Some(value) = m.get("value").filter(|v| v.is_string() || v.is_object()) else { continue };
            fields.insert((class, name), (id, value.clone()));
        }
        let mut bases = HashMap::new();
        for c in store.table("classes").iter() {
            if let (Some(id), Some(base)) = (id_of(c, "id"), c.get("extends").and_then(|e| e.get("id")).and_then(Value::as_str)) {
                bases.insert(id, base.to_string());
            }
        }
        ThisFields { fields, bases, member_class }
    }

    /// Every element of a class's own member's list, folded in that class: one written in place, and one a method of that class (or
    /// of a base) hands back - `this` there is the same instance. One another class's method hands back is not: its `this` is another.
    pub(super) fn fold_lists(&self, lists: &mut HashMap<String, Vec<super::key_loops::Element>>, owner: &HashMap<String, Option<String>>) {
        for (id, list) in lists.iter_mut() {
            let Some(class) = owner.get(id).and_then(|c| c.as_deref()).filter(|c| !c.is_empty()) else { continue };
            for element in list.iter_mut() {
                let ours = !element.from_call || element.owner().and_then(|m| self.member_class.get(m)).is_some_and(|c| self.extends(class, c));
                if ours {
                    element.value = self.fold(&element.value, class);
                }
            }
        }
    }

    /// A write's or an initializer's value as its list is read (#117): `this.` reads folded in the writing class - a dictionary call's
    /// `${this.base}.hues` argument included - and a write adding to the field it reads (`key_lists::unconcat`) as what it adds.
    pub(super) fn prepared(&self, value: &Value, class: Option<&str>, field: Option<&str>) -> Value {
        let folded = class.filter(|c| !c.is_empty()).map_or_else(|| value.clone(), |c| self.fold(value, c));
        super::key_lists::unconcat(folded, field)
    }

    /// Whether `class` is `base` or extends it, through any number of classes.
    fn extends(&self, class: &str, base: &str) -> bool {
        let (mut at, mut seen) = (class.to_string(), HashSet::new());
        loop {
            if at == base {
                return true;
            }
            if !seen.insert(at.clone()) {
                return false;
            }
            let Some(next) = self.bases.get(&at) else { return false };
            at = next.clone();
        }
    }

    /// `value` with every `this.` read and every template literal of such reads under it folded, in an instance of `class`.
    pub(super) fn fold(&self, value: &Value, class: &str) -> Value {
        self.fold_in(value, class, &mut Vec::new())
    }

    fn fold_in(&self, value: &Value, class: &str, seen: &mut Vec<String>) -> Value {
        match value {
            Value::Object(o) => {
                if let Some(path) = this_path(o) {
                    return self.read(class, &path, seen).map(Value::String).unwrap_or_else(|| value.clone());
                }
                if let Some(text) = self.template(o, class, seen) {
                    return Value::String(text);
                }
                // EVERY KEY, a call's `$args` and `$receiver` included: a dictionary call's argument reads `this.` too.
                let fold = |(k, v): (&String, &Value)| (k.clone(), self.fold_in(v, class, &mut seen.clone()));
                Value::Object(o.iter().map(fold).collect())
            }
            Value::Array(xs) => Value::Array(xs.iter().map(|x| self.fold_in(x, class, &mut seen.clone())).collect()),
            other => other.clone(),
        }
    }

    /// The string a `this.<field>.<path>` read folds to: the field on the class or the nearest base declaring it.
    fn read(&self, class: &str, path: &[&str], seen: &mut Vec<String>) -> Option<String> {
        let (field, rest) = path.split_first()?;
        let mut at = class.to_string();
        let mut climbed = HashSet::new();
        let (id, value) = loop {
            if let Some(found) = self.fields.get(&(at.clone(), field.to_string())) {
                break found;
            }
            if !climbed.insert(at.clone()) {
                return None;
            }
            at = self.bases.get(&at)?.clone();
        };
        if seen.contains(id) {
            return None;
        }
        let mut value = value;
        for step in rest {
            value = value.as_object().filter(|o| o.keys().all(|k| !k.starts_with('$')))?.get(*step)?;
        }
        seen.push(id.clone());
        let folded = self.fold_in(value, class, seen);
        seen.pop();
        folded.as_str().map(str::to_string)
    }

    /// A template literal whose every hole folds to a string, as the string.
    fn template(&self, o: &serde_json::Map<String, Value>, class: &str, seen: &mut Vec<String>) -> Option<String> {
        let pieces = o.get("$template")?.as_array()?;
        let holes = o.get("$hole_values")?.as_array()?;
        if pieces.len() != holes.len() + 1 {
            return None;
        }
        let mut out = pieces[0].as_str()?.to_string();
        for (hole, piece) in holes.iter().zip(&pieces[1..]) {
            out.push_str(self.fold_in(hole, class, seen).as_str()?);
            out.push_str(piece.as_str()?);
        }
        Some(out)
    }
}

/// The names a `this.a.b` (or `this.a?.b`) read walks after `this`; None for anything else.
fn this_path(o: &serde_json::Map<String, Value>) -> Option<Vec<&str>> {
    if o.get("$kind").and_then(Value::as_str) != Some("PropertyAccessExpression") {
        return None;
    }
    let rest = o.get("$expr")?.as_str()?.strip_prefix("this.")?;
    let path: Vec<&str> = rest.split('.').map(|s| s.strip_suffix('?').unwrap_or(s)).collect();
    path.iter().all(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_read_through_this_folds_up_the_class_chain() {
        let read = |t: &str| json!({"$expr": t, "$kind": "PropertyAccessExpression"});
        let tpl = |hole: &str, tail: &str| json!({"$template": ["", tail], "$hole_values": [read(hole)]});
        let fields = HashMap::from([
            (("c:2".to_string(), "base".to_string()), ("m:1".to_string(), json!("demo"))),
            (("c:1".to_string(), "resource".to_string()), ("m:2".to_string(), tpl("this.base", ".fees"))),
            (("c:1".to_string(), "labels".to_string()), ("m:3".to_string(), json!({"fees": {"entry": tpl("this.resource", ".entry")}}))),
            (("c:1".to_string(), "loop".to_string()), ("m:4".to_string(), tpl("this.loop", "x"))),
        ]);
        let this = ThisFields { fields, bases: HashMap::from([("c:1".to_string(), "c:2".to_string())]), member_class: HashMap::new() };
        let element = json!({"Label": read("this.labels.fees.entry"), "Other": read("this.missing"), "Self": read("this.loop")});
        assert_eq!(this.fold(&element, "c:1"), json!({"Label": "demo.fees.entry", "Other": read("this.missing"), "Self": read("this.loop")}));
        assert_eq!(this.fold(&read("this.resource"), "c:2"), read("this.resource"), "a base does not see its subclass's field");
    }
}

