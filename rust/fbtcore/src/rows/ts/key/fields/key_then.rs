//! A FIELD A `.then` CALLBACK WRITES FROM ITS PARAMETER holds what the call resolves to.
//!
//! `this.svc.load().then((x) => { this.f = x; })` over `load()` returning `Promise.resolve(t('k'))`, or an `async`
//! method returning the call: the parameter is the awaited result of the call `.then` is made on, so the write holds
//! the keys that call's callee returns, as `this.f = this.svc.load()` does. The `.then` is the compiler's own
//! (`target.external`, never any method of that name), the callback is the call's first argument (`$fn_id`), and only a
//! write of the bare parameter counts: `this.f = 'k'` or `this.f = x + y` inside it is no such hand-over.

use super::key_fields::{id_of, text};
use super::store::Row;
use serde_json::{json, Value};
use std::collections::HashMap;

/// `functions` row id of a `.then` callback -> (its first parameter's name, the call value `.then` is made on).
pub(super) type Thens = HashMap<String, (String, Value)>;

pub(super) fn thens(calls: &[Row], params: &HashMap<String, Vec<String>>) -> Thens {
    let by_id: HashMap<String, &Row> = calls.iter().filter_map(|c| Some((id_of(c, "id")?, c))).collect();
    let mut out = Thens::new();
    for c in calls.iter() {
        let target = c.get("target");
        let lib = target.and_then(|t| t.get("name")).and_then(Value::as_str) == Some("then")
            && target.and_then(|t| t.get("external")).and_then(Value::as_bool) == Some(true);
        let function = c.get("args").and_then(Value::as_array).and_then(|a| a.first()).and_then(|a| a.get("$fn_id")).and_then(Value::as_str);
        let inner = id_of(c, "receiver_call").and_then(|id| by_id.get(&id).copied());
        let (Some(function), Some(inner), true) = (function, inner, lib) else { continue };
        let (Some(param), Some(callee), Some(target)) = (params.get(function).and_then(|p| p.first()), text(inner, "callee"), inner.get("target").filter(|t| !t.is_null())) else { continue };
        // A `calls` row's target file is relative to the root; `member_named` matches the tail of an absolute path.
        let mut target = target.clone();
        if let Some(file) = target.get("file").and_then(Value::as_str).filter(|f| !f.starts_with('/') && !f.contains(':')).map(|f| format!("/{f}")) {
            target["file"] = Value::String(file);
        }
        let args = inner.get("args").cloned().unwrap_or_else(|| json!([]));
        out.insert(function.to_string(), (param.clone(), json!({"$call": callee, "$args": args, "$target": target})));
    }
    out
}

/// The call an assignment's value stands for, when the assignment is a plain `=` of a `.then` callback's parameter.
pub(super) fn resolved<'t>(a: &Row, thens: &'t Thens) -> Option<&'t Value> {
    let (param, call) = thens.get(&id_of(a, "member")?)?;
    let value = a.get("value")?;
    (text(a, "operator").as_deref() == Some("=") && value.get("$expr").and_then(Value::as_str) == Some(param.as_str())).then_some(call)
}
