//! WHAT A LIST IS PUSHED: `out.push(t('k'))` on a local a method returns, `this.items.push({ label: t('k') })` on a
//! field its own class fills. The element is the call's argument, read where the list's other elements are.

use super::key_fields::{id_of, text};
use super::store::Row;
use serde_json::Value;
use std::collections::HashMap;

/// Every `.push(..)` call of the compiler's own `Array`, by the list it is made on.
#[derive(Default)]
pub(super) struct Pushes<'a> {
    /// (member id, local name) -> the pushes on that local, in source order.
    pub(super) local: HashMap<(String, String), Vec<&'a Row>>,
    /// (class id, field name) -> every `this.<field>.push(..)` in a member of that class.
    pub(super) field: HashMap<(String, String), Vec<&'a Row>>,
}

fn name_only(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// THE PUSH IS THE LIB'S (`target.external`), never any method of that name: a service's own `push` adds nothing to a
/// list. The list is named by the call's receiver text - a bare name is a local of the member, `this.<name>` a field of
/// its class - and no other receiver (a property of a local, a call's result) is read: nothing says which list it is.
///
/// A PUSH INSIDE A CALLBACK (`.then(() => { this.items.push(..) })`) is made by the anonymous function, a `functions` row whose
/// `parent` is the member it sits in: the field is its class's, found through that chain.
pub(super) fn pushes<'a>(calls: &'a [Row], members: &[Row], functions: &[Row]) -> Pushes<'a> {
    let mut class_of: HashMap<String, String> = members.iter().filter_map(|m| Some((id_of(m, "id")?, text(m, "class")?))).collect();
    for _ in 0..4 {
        let more: Vec<(String, String)> = functions.iter().filter_map(|f| Some((id_of(f, "id")?, class_of.get(&text(f, "parent")?)?.clone()))).filter(|(id, _)| !class_of.contains_key(id)).collect();
        if more.is_empty() {
            break;
        }
        class_of.extend(more);
    }
    let mut out = Pushes::default();
    for c in calls.iter() {
        let target = c.get("target");
        let lib = target.and_then(|t| t.get("name")).and_then(Value::as_str) == Some("push")
            && target.and_then(|t| t.get("external")).and_then(Value::as_bool) == Some(true);
        let (Some(receiver), Some(member)) = (text(c, "callee").and_then(|t| t.strip_suffix(".push").map(str::to_string)), id_of(c, "member")) else { continue };
        if !lib {
            continue;
        }
        if name_only(&receiver) {
            out.local.entry((member, receiver)).or_default().push(c);
        } else if let (Some(field), Some(class)) = (receiver.strip_prefix("this.").filter(|f| name_only(f)), class_of.get(&member)) {
            out.field.entry((class.clone(), field.to_string())).or_default().push(c);
        }
    }
    out
}
