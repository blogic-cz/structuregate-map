//! WHAT A DIRECTIVE'S CONFIG OBJECT RESTRICTS — the `gate_values` row for the gate whose
//! members are listed in an object literal instead of compared.
//!
//! THE FORM: a structural directive whose input is a CONFIG OBJECT, each member of which
//! narrows one dimension —
//! `<x *showConfig="{shadeID: Category.Software, allowedIds: [A, …, B]}">`. No
//! `Binary` reaches the comparison reader for any of it, so without this rule every gate
//! of the mechanism produces NO value row, and silence there reads as
//! "unrestricted".
//!
//! THE DECLARED TYPE CANNOT GIVE THE POLARITY, AND THE NAME MUST NOT BE ASKED. One config
//! declares `allowIDs: HueIDs[]` AND `blockIDs:
//! HueIDs[]` — same enum, opposite meaning, and one of them does not hide the
//! element at all, it disables it. Reading the first as a restriction because of what it
//! is CALLED is the pattern-matching this tool exists to replace.
//!
//! SO THE POLARITY IS READ OFF THE CLASS'S OWN MEMBERSHIP TEST:
//!   * `allowIDs.some((m) => m === x)` — un-negated, equality → a RESTRICTION
//!   * `!blockIDs.some((id) => …)` — under a `Not` → `unknown`
//!
//! THE MEMBERS A LIST NAMES ARE PUBLISHED WHATEVER ITS POLARITY (`listed`). Under
//! a `Not` the map cannot say whether the list hides the element or only disables it, so
//! its `op` stays `unknown` — but which members the source wrote is a fact, and a consumer
//! re-reading them off `expressions.ast` was doing this file's work again.

use super::astreads::{is_read, unwrap};
use super::gate_values::{constant_of, EnumIndex, MemberEnums};
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

#[path = "gate_config_index.rs"]
mod index;
use index::*;

/// The member name a membership call tests: `input.allowIDs.some(…)` ->
/// `allowIDs`.
fn tested_member(call: &Value) -> Option<String> {
    let object = call.as_object()?;
    if object.get("k").and_then(|k| k.as_str()) != Some("Call") {
        return None;
    }
    let receiver = object.get("receiver")?;
    if !is_read(receiver) || !MEMBERSHIP.contains(&name_of(receiver)?) {
        return None;
    }
    let owner = receiver.get("receiver")?;
    if is_read(owner) { name_of(owner).map(|s| s.to_string()) } else { None }
}

/// Whether a predicate tests the ITERATED MEMBER for identity — by equality, or by asking
/// a set whether it holds it.
///
/// Nothing else counts: `some((m) => m.foo > 3)` says something about the members' shape
/// rather than about which members are permitted, and ONE non-equality comparison closes
/// the whole predicate.
///
/// BOTH FORMS ARE COMMON. Accepting only `m === x` would call a directive whose
/// PRIMARY test is a set lookup non-equality and make every one of its rows
/// `unknown`.
fn tests_identity(ast: &Value) -> bool {
    let mut found = false;
    let mut stack: Vec<&Value> = vec![ast];
    while let Some(n) = stack.pop() {
        match n {
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(object) => {
                if object.get("k").and_then(|k| k.as_str()) == Some("Binary") {
                    match object.get("op").and_then(|o| o.as_str()) {
                        Some(op) if EQ.contains(&op) => found = true,
                        _ => return false,
                    }
                }
                if object.get("k").and_then(|k| k.as_str()) == Some("Call")
                    && let Some(receiver) = object.get("receiver")
                    && is_read(receiver)
                    && name_of(receiver).map(|n| HAS.contains(&n)).unwrap_or(false)
                {
                    found = true;
                }
                stack.extend(object.values());
            }
            _ => continue,
        }
    }
    found
}

/// A membership call's predicate: its arguments, or — for `includes(x)` — the call itself.
fn predicate_of(call: &Value) -> Value {
    let includes = call
        .get("receiver")
        .and_then(name_of)
        .map(|n| n == "includes")
        .unwrap_or(false);
    if includes {
        call.clone()
    } else {
        call.get("args").cloned().unwrap_or_else(|| json!([]))
    }
}

struct Ctx<'a> {
    expr_by_id: IndexMap<String, &'a Row>,
    expr_by_file: IndexMap<String, Vec<&'a Row>>,
    class_by_id: IndexMap<String, &'a Row>,
    by_class: IndexMap<String, Vec<&'a Row>>,
}

/// THE EVIDENCE THE AST TABLE DOES NOT HOLD, and the reason this exists at all.
///
/// `expressions` carries the directive's `some((id) => activeIds.has(id))` predicate — but
/// not the `.some()` CALL that owns it. The call is the initializer of a local, so it is
/// an `assignments`/`locals`/`returns` row in the `$` VALUE VOCABULARY, with only the
/// inner arrow reaching `expressions`. Reading only the AST table therefore MISSED the
/// directive's primary test, and left the whole resolved population resting on a single
/// test inside a conditional method.
///
/// There is no negation marker in this vocabulary, so evidence found here is un-negated by
/// construction.
fn vocab_tests(
    ctx: &Ctx<'_>,
    cls: &str,
    members: &IndexMap<String, Option<ConfigMember>>,
    good: &mut IndexSet<String>,
    bad: &mut IndexSet<String>,
) {
    let Some(rows) = ctx.by_class.get(cls) else { return };
    for r in rows {
        let Some(value) = r.get("value").filter(|v| !v.is_null()) else { continue };
        let mut stack: Vec<&Value> = vec![value];
        while let Some(n) = stack.pop() {
            match n {
                Value::Array(items) => stack.extend(items.iter()),
                Value::Object(object) => {
                    // `$call` is the callee's dotted path as the CHECKER resolved it, so
                    // the walk crosses back into `expressions` by id, never by re-parsing.
                    if let Some(Value::String(call)) = object.get("$call") {
                        let path: Vec<&str> = call.split('.').collect();
                        let name = if path.len() > 1 && MEMBERSHIP.contains(&path[path.len() - 1]) {
                            Some(path[path.len() - 2])
                        } else {
                            None
                        };
                        if let Some(name) = name
                            && members.get(name).map(|m| m.is_some()).unwrap_or(false)
                        {
                            let mut ok = false;
                            if let Some(Value::Array(args)) = object.get("$args") {
                                for a in args {
                                    let Some(Value::String(expr_id)) =
                                        a.as_object().and_then(|a| a.get("$expr_id"))
                                    else {
                                        continue;
                                    };
                                    let Some(row) = ctx.expr_by_id.get(expr_id) else { continue };
                                    let Some(ast) = row.get("ast").filter(|a| !a.is_null()) else {
                                        continue;
                                    };
                                    ok = tests_identity(unwrap(ast)) || ok;
                                }
                            }
                            if ok { good.insert(name.to_string()); } else { bad.insert(name.to_string()); }
                        }
                    }
                    stack.extend(object.values());
                }
                _ => continue,
            }
        }
    }
}

/// Class id -> what the class DOES with each member.
///
/// TWO TIERS, because a member whose meaning the map cannot resolve still restricts:
///   * a membership test, un-negated, stating equality -> `restriction`
///   * any other use the class makes of the member -> `unknown`
///
/// A member tested BOTH ways is `unknown`: the map cannot say which test decides the
/// render, and an upper bound that guesses is worse than one that admits it. A member
/// merely READ is `unknown` rather than absent — it demonstrably participates.
fn member_use(
    ctx: &Ctx<'_>,
    by_class: &IndexMap<String, IndexMap<String, Option<ConfigMember>>>,
) -> IndexMap<String, IndexMap<String, &'static str>> {
    let mut out = IndexMap::new();
    for (cls, members) in by_class {
        let Some(c) = ctx.class_by_id.get(cls) else { continue };
        let Some(file) = text(c, "file") else { continue };

        let mut good: IndexSet<String> = IndexSet::new();
        let mut bad: IndexSet<String> = IndexSet::new();
        let mut read: IndexSet<String> = IndexSet::new();
        vocab_tests(ctx, cls, members, &mut good, &mut bad);

        if let Some(rows) = ctx.expr_by_file.get(&file) {
            for r in rows {
                let (Some(line), Some(start), Some(end)) =
                    (number(r, "line"), number(c, "line"), number(c, "end_line"))
                else {
                    continue;
                };
                if line < start || line > end {
                    continue;
                }
                let Some(ast) = r.get("ast").filter(|a| !a.is_null()) else { continue };

                let mut stack: Vec<(&Value, bool)> = vec![(unwrap(ast), false)];
                while let Some((n, neg)) = stack.pop() {
                    match n {
                        Value::Array(items) => stack.extend(items.iter().map(|v| (v, neg))),
                        Value::Object(object) => {
                            if let Some(tested) = tested_member(n)
                                && members.get(&tested).map(|m| m.is_some()).unwrap_or(false)
                            {
                                if !neg && tests_identity(&predicate_of(n)) {
                                    good.insert(tested);
                                } else {
                                    bad.insert(tested);
                                }
                            }
                            // A READ OFF A RECEIVER, never a bare identifier:
                            // `input.shadeID` is the config member, while a local of the
                            // same name is not the thing the occurrence supplies.
                            if is_read(n)
                                && object.get("receiver").map(is_read).unwrap_or(false)
                                && let Some(name) = name_of(n)
                                && members.get(name).map(|m| m.is_some()).unwrap_or(false)
                            {
                                read.insert(name.to_string());
                            }
                            let flip = if object.get("k").and_then(|k| k.as_str()) == Some("Not") {
                                !neg
                            } else {
                                neg
                            };
                            stack.extend(object.values().map(|v| (v, flip)));
                        }
                        _ => continue,
                    }
                }
            }
        }

        let mut verdict: IndexMap<String, &'static str> = IndexMap::new();
        let ordered = good
            .iter()
            .cloned()
            .chain(bad.iter().filter(|k| !good.contains(*k)).cloned())
            .chain(
                read.iter()
                    .filter(|k| !good.contains(*k) && !bad.contains(*k))
                    .cloned(),
            );
        for name in ordered {
            let settled = good.contains(&name) && !bad.contains(&name);
            verdict.insert(name, if settled { "restriction" } else { "unknown" });
        }
        out.insert(cls.clone(), verdict);
    }
    out
}

/// The entries an occurrence's object literal supplies, or nothing if it is not one.
pub(crate) fn literal_entries(ast: Option<&Value>) -> Option<IndexMap<String, Value>> {
    let n = unwrap(ast?);
    let object = n.as_object()?;
    if object.get("k").and_then(|k| k.as_str()) != Some("Map") {
        return None;
    }
    let Some(Value::Array(keys)) = object.get("keys") else { return None };
    let empty = Vec::new();
    let values = match object.get("values") {
        Some(Value::Array(v)) => v,
        _ => &empty,
    };
    let mut out = IndexMap::new();
    for (i, k) in keys.iter().enumerate() {
        if let Some(Value::String(key)) = k.as_object().and_then(|k| k.get("key")) {
            out.insert(key.clone(), values.get(i).cloned().unwrap_or(Value::Null));
        }
    }
    Some(out)
}

#[derive(Debug, Default)]
pub struct ConfigStats {
    pub gates: usize,
    pub resolved: usize,
    pub unknown: usize,
    pub classes: usize,
}

/// Gate id -> the restrictions its CONFIG OBJECT states, in `gate_values`' own row shape.
pub fn config_restrictions(
    store: &Store<'_>,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> (IndexMap<String, Vec<Value>>, ConfigStats) {
    let expressions = store.table("expressions");
    let classes = store.table("classes");
    let locals = store.table("locals");
    let assignments = store.table("assignments");
    let returns = store.table("returns");

    let mut ctx = Ctx {
        expr_by_id: IndexMap::new(),
        expr_by_file: IndexMap::new(),
        class_by_id: IndexMap::new(),
        by_class: IndexMap::new(),
    };
    for e in expressions.iter() {
        if let Some(id) = id_of(e, "id") {
            ctx.expr_by_id.insert(id, e);
        }
        if let Some(file) = text(e, "file") {
            ctx.expr_by_file.entry(file).or_default().push(e);
        }
    }
    for c in classes.iter() {
        if let Some(id) = id_of(c, "id") {
            ctx.class_by_id.insert(id, c);
        }
    }
    for table in [&locals, &assignments, &returns] {
        for r in table.iter() {
            if let Some(cls) = id_of(r, "class") {
                ctx.by_class.entry(cls).or_default().push(r);
            }
        }
    }

    let by_class = config_members(store, idx);
    let uses = member_use(&ctx, &by_class);

    let mut classes_of_selector: IndexMap<String, Vec<String>> = IndexMap::new();
    for si in store.table("selector_index").iter() {
        let Some(selector) = text(si, "selector") else { continue };
        if let Some(cls) = id_of(si, "class") {
            classes_of_selector.entry(selector).or_default().push(cls);
        }
    }

    let mut out: IndexMap<String, Vec<Value>> = IndexMap::new();
    let mut stats = ConfigStats::default();
    for cls in by_class.keys() {
        if uses.get(cls).map(|v| !v.is_empty()).unwrap_or(false) {
            stats.classes += 1;
        }
    }

    for g in store.table("gates").iter() {
        let Some(gate_name) = text(g, "name") else { continue };
        let Some(candidates) = classes_of_selector.get(&format!("[{gate_name}]")) else { continue };
        for cls in candidates {
            let (Some(members), Some(verdict)) = (by_class.get(cls), uses.get(cls)) else {
                continue;
            };
            let branches = id_of(g, "expression")
                .and_then(|x| ctx.expr_by_id.get(&x).copied())
                .and_then(|e| e.get("ast"))
                .and_then(literal_branches);

            let mut rows = Vec::new();
            for (name, kind) in verdict {
                let Some(Some(decl)) = members.get(name) else { continue };
                // NO LITERAL AT ALL is still a restriction: the directive tests this member
                // whatever the bound object turns out to hold, and reporting nothing would
                // be the silence this file removes. A literal - or every branch of a
                // conditional - that never writes the member reports nothing for it.
                let said = match &branches {
                    Some(branches) => match supplied(branches, name, &decl.enum_id, mem, idx) {
                        Some(said) => Some(said),
                        None => continue,
                    },
                    None => None,
                };
                let values = match (kind, &said) {
                    (&"restriction", Some(said)) => said.permitted(),
                    _ => None,
                };
                let dim = format!("{gate_name}.{name}");
                let mut row = match values {
                    Some(values) => json!({"enum": decl.enum_id, "dim": dim, "row": decl.row,
                                           "op": "in", "values": values}),
                    None => json!({"enum": decl.enum_id, "dim": dim, "row": decl.row,
                                   "op": "unknown"}),
                };
                if let Some(listed) = said.and_then(|s| s.listed()) {
                    row["listed"] = json!(listed);
                }
                rows.push(row);
            }
            if rows.is_empty() {
                continue;
            }
            stats.gates += 1;
            stats.resolved += rows.iter().filter(|r| r["op"] != json!("unknown")).count();
            stats.unknown += rows.iter().filter(|r| r["op"] == json!("unknown")).count();
            if let Some(id) = id_of(g, "id") {
                out.insert(id, rows);
            }
        }
    }
    (out, stats)
}

#[cfg(test)]
#[allow(non_snake_case)]
#[path = "gate_config_tests.rs"]
mod tests;
