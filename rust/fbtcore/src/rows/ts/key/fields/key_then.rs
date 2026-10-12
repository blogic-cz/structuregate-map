//! A FIELD A `.then` OR `.subscribe` CALLBACK WRITES FROM ITS PARAMETER holds what the call resolves to or emits.
//!
//! `this.svc.load().then((x) => { this.f = x; })` over `load()` returning `Promise.resolve(t('k'))`, or an `async`
//! method returning the call: the parameter is the awaited result of the call `.then` is made on, so the write holds
//! the keys that call's callee returns, as `this.f = this.svc.load()` does. The `.then` is the compiler's own
//! (`target.external`, never any method of that name), the callback is the call's first argument (`$fn_id`), and only a
//! write of the bare parameter counts: `this.f = 'k'` or `this.f = x + y` inside it is no such hand-over.
//!
//! RXJS'S `subscribe` IS THE SAME HAND-OVER (`this.svc.label$().subscribe((v) => { this.f = v; })`, or `{ next: (v) => .. }`):
//! the parameter is what the observable the call returns emits - `of(t('k'))` is read through as `Promise.resolve` is
//! (`key_follow`). The `subscribe` is rxjs's own, by the declaration's file, never any method of that name. A `.then` on
//! `Promise.resolve(x)` itself holds `x`.

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
        let external = target.and_then(|t| t.get("external")).and_then(Value::as_bool) == Some(true);
        let lib = external && match target.and_then(|t| t.get("name")).and_then(Value::as_str) {
            Some("then") => true,
            Some("subscribe") => target.and_then(|t| t.get("file")).and_then(Value::as_str).is_some_and(|f| f.split(['/', '\\']).any(|p| p == "rxjs")),
            _ => false,
        };
        let first = c.get("args").and_then(Value::as_array).and_then(|a| a.first());
        let function = first.and_then(|a| a.get("$fn_id").or_else(|| a.get("next")?.get("$fn_id"))).and_then(Value::as_str);
        let mut inner = id_of(c, "receiver_call").and_then(|id| by_id.get(&id).copied());
        // `.pipe(take(1), filter(..))` HANDS ON WHAT IT IS MADE ON: an operator that only passes values through (or drops
        // some) leaves the parameter what the source emits. Any other operator - `map`, `switchMap` - is a value of its own.
        for _ in 0..8 {
            match inner.filter(|i| keeps_values(i)) {
                Some(pipe) => inner = id_of(pipe, "receiver_call").and_then(|id| by_id.get(&id).copied()),
                None => break,
            }
        }
        let (Some(function), Some(inner), true) = (function, inner, lib) else { continue };
        // `Promise.resolve(x).then(..)`: the lib's own `resolve` hands `x` on, so the parameter is `x`.
        let inner_target = inner.get("target");
        if inner_target.and_then(|t| t.get("name")).and_then(Value::as_str) == Some("resolve") && inner_target.and_then(|t| t.get("external")).and_then(Value::as_bool) == Some(true) {
            let handed = inner.get("args").and_then(Value::as_array).and_then(|a| a.first());
            if let (Some(param), Some(handed)) = (params.get(function).and_then(|p| p.first()), handed) {
                out.insert(function.to_string(), (param.clone(), handed.clone()));
            }
            continue;
        }
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

/// The rxjs operators that hand on the values they receive, unchanged.
const KEEPS: [&str; 14] = ["take", "first", "last", "filter", "tap", "takeUntil", "takeWhile", "skip", "distinctUntilChanged", "debounceTime", "throttleTime", "auditTime", "delay", "shareReplay"];

/// An rxjs `pipe(..)` every argument of which is a call of a `KEEPS` operator - each by its declaration under `rxjs`.
fn keeps_values(call: &Row) -> bool {
    let rxjs = |t: Option<&Value>| t.and_then(|t| t.get("file")).and_then(Value::as_str).is_some_and(|f| f.split(['/', '\\']).any(|p| p == "rxjs"));
    let target = call.get("target");
    if target.and_then(|t| t.get("name")).and_then(Value::as_str) != Some("pipe") || !rxjs(target) {
        return false;
    }
    let args = call.get("args").and_then(Value::as_array);
    args.is_some_and(|a| !a.is_empty() && a.iter().all(|x| {
        let t = x.get("$target");
        t.and_then(|t| t.get("name")).and_then(Value::as_str).is_some_and(|n| KEEPS.contains(&n)) && rxjs(t)
    }))
}
