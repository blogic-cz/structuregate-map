//! THE THREE SHAPES A FEATURE CHECK IS WRITTEN IN, each proving which properties it fills.
//!
//! The feature code sits in a different row for each: the DIRECT form leaves it in the
//! assignment's own value, the CALLBACK form in the call whose argument the assignment's
//! function is, and the TERNARY form in a SECOND property that has to be resolved before
//! this one can be. Reading only the first reports every property written another way as
//! ungated, which is the same wrong answer as having no row at all.

use super::featurewalk::{
    codes_in, is_blank, necessary, necessary_idents, necessary_nodes, supplies_nothing, walk_value,
    OrderedSet,
};
use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::Value;

pub type Codes = IndexMap<String, bool>;
/// `(class, property)` -> the codes that property necessarily requires.
pub type Features = IndexMap<String, Codes>;
/// Decides which class a write is filed under, or nothing when it is unattributable.
pub type Owner<'a> = dyn Fn(&Row) -> Option<String> + 'a;
/// The polarity walk's leaf resolver.
pub type Resolve<'a> = dyn FnMut(&Value, bool) -> Vec<Value> + 'a;

pub fn slash(p: Option<&Value>) -> String {
    match p {
        Some(Value::String(s)) => s.replace('\\', "/"),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string().replace('\\', "/"),
    }
}

fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn id_of(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

fn number(row: &Row, field: &str) -> Option<i64> {
    match row.get(field) {
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

/// `got` narrowed to what `codes` also holds, KEEPING `got`'s order.
///
/// THE CODE SETS ARE INTERSECTED ACROSS THE CONTRIBUTING WRITES, not unioned: two writes
/// each proving a DIFFERENT feature mean the property is truthy under either one, so
/// neither is necessary.
pub fn intersect(got: Option<Codes>, codes: &Codes) -> Codes {
    match got {
        None => codes.clone(),
        Some(held) => held.into_iter().filter(|(c, _)| codes.contains_key(c)).collect(),
    }
}

/// The codes named anywhere in a set of nodes.
fn codes_in_nodes(nodes: &[&serde_json::Map<String, Value>], root: &str) -> Codes {
    let mut out = IndexMap::new();
    for n in nodes {
        for (code, _) in codes_in(&Value::Object((*n).clone()), root) {
            out.insert(code, true);
        }
    }
    out
}

/// Every write, grouped by the property it fills.
///
/// The rows are borrowed from the caller, which holds the store's `Rc` for as long as it
/// needs them. Reaching into the `Rc` here instead would need an unsafe lifetime
/// extension for no gain.
fn per_property<'a>(
    rows: &'a [Row],
    owner: &Owner<'_>,
    require_value: bool,
) -> IndexMap<String, Vec<&'a Row>> {
    let mut out: IndexMap<String, Vec<&'a Row>> = IndexMap::new();
    for a in rows.iter() {
        if require_value && matches!(a.get("value"), None | Some(Value::Null)) {
            continue;
        }
        let Some(cls) = owner(a) else { continue };
        let key = format!("{cls} {}", id_of(a, "target").unwrap_or_else(|| "None".to_string()));
        out.entry(key).or_default().push(a);
    }
    out
}

/// `(class, property)` -> codes, for fields assigned from a RESOLVED feature check.
///
/// EVERY WRITE TO THE PROPERTY MUST CLEAR THE SAME BAR. Crediting a property from a SINGLE
/// assignment whose value contains a resolved check said nothing about the other writes:
/// one property is set from the check by many panels and to a bare `true` elsewhere,
/// and another to `true` outright by several of them — gates that would have demanded a
/// feature the content plainly renders without.
///
/// THE CHECK MUST SIT AT A NECESSARY POSITION IN THE VALUE. Walking the whole value for a
/// check call counted `!(a || b) || (await isOn(X))` as proving X.
pub fn property_features(
    store: &Store<'_>,
    checks: &[Check],
    root: &str,
    written: &std::collections::HashSet<String>,
    owner: &Owner<'_>,
) -> Features {
    let paths: Vec<String> = checks.iter().map(|c| format!("/{}", c.path)).collect();
    let mut out = Features::new();
    let assignments = store.table("assignments");

    for (key, writes) in per_property(&assignments, owner, true) {
        // The template fills it whatever the check answered.
        if written.contains(&key) {
            continue;
        }
        let mut got: Option<Codes> = None;
        let mut proven = true;
        for a in writes {
            if text(a, "operator").as_deref() != Some("=") {
                proven = false;
                break;
            }
            let Some(v) = a.get("value") else { continue };
            if is_blank(v) {
                continue;
            }
            let need = necessary_nodes(v);

            let mut hit = false;
            for n in &need {
                walk_value(&Value::Object((*n).clone()), &mut |x| {
                    if hit {
                        return;
                    }
                    let is_call = x.get("$call").map(|c| !c.is_null()).unwrap_or(false);
                    if !is_call {
                        return;
                    }
                    let Some(t) = x.get("$target").and_then(|t| t.as_object()) else { return };
                    let normalised = slash(t.get("file"));
                    if paths.iter().any(|p| normalised.ends_with(p)) {
                        hit = true;
                    }
                });
                if hit {
                    break;
                }
            }
            if !hit {
                proven = false;
                break;
            }
            // The codes are read from the SAME node set, so a conjunct is kept and its
            // disjuncts dropped.
            let codes = codes_in_nodes(&need, root);
            if codes.is_empty() {
                proven = false;
                break;
            }
            got = Some(intersect(got, &codes));
        }
        if let Some(got) = got
            && proven
            && !got.is_empty()
        {
            out.insert(key, got);
        }
    }
    out
}

/// A resolved feature-check declaration.
#[derive(Clone, Debug)]
pub struct Check {
    pub member: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct Callback {
    pub code: String,
    pub param: String,
    pub multi: bool,
}

/// Fn id -> the code and parameter of every inline callback a RESOLVED check hands its
/// answer to.
///
/// THE LINK IS BY SPAN AND ID, NEVER BY THE WRAPPER'S NAME. The check call and the call
/// that wraps it start on the same line in the same member and the wrapper ends later.
/// Matching a wrapper called `then` would miss `subscribe` and would match any method
/// sharing the word.
///
/// ONE FEATURE PER CALLBACK. A callback fed by two checks would have to say which of them
/// each assignment depends on, and nothing in the rows does, so those are dropped.
pub fn callback_checks(
    store: &Store<'_>,
    check_members: &std::collections::HashSet<String>,
    root: &str,
) -> IndexMap<String, Callback> {
    let calls_table = store.table("calls");
    let mut by_member: IndexMap<String, Vec<&Row>> = IndexMap::new();
    for c in calls_table.iter() {
        if id_of(c, "member").is_none() || matches!(c.get("args"), None | Some(Value::Null)) {
            continue;
        }
        by_member
            .entry(id_of(c, "member").expect("checked"))
            .or_default()
            .push(c);
    }

    let expressions = store.table("expressions");
    let mut expr_at: IndexMap<String, String> = IndexMap::new();
    for r in expressions.iter() {
        if text(r, "role").as_deref() == Some("expr")
            && let Some(id) = id_of(r, "id")
        {
            expr_at.insert(
                id,
                format!(
                    "{} {}",
                    id_of(r, "file").unwrap_or_else(|| "None".into()),
                    id_of(r, "line").unwrap_or_else(|| "None".into())
                ),
            );
        }
    }

    let functions = store.table("functions");
    let mut fn_at: IndexMap<String, Option<&Row>> = IndexMap::new();
    for f in functions.iter() {
        let inline = matches!(f.get("inline"), Some(Value::Bool(true)))
            || matches!(f.get("inline"), Some(Value::Number(n)) if n.as_i64() == Some(1));
        if !inline {
            continue;
        }
        let k = format!(
            "{} {}",
            id_of(f, "file").unwrap_or_else(|| "None".into()),
            id_of(f, "line").unwrap_or_else(|| "None".into())
        );
        // An ambiguous anchor resolves to nothing, not to a guess.
        if fn_at.contains_key(&k) {
            fn_at.insert(k, None);
        } else {
            fn_at.insert(k, Some(f));
        }
    }

    let mut out: IndexMap<String, Callback> = IndexMap::new();
    for calls in by_member.values() {
        for check in calls {
            let Some(target) = id_of(check, "target_id") else { continue };
            if !check_members.contains(&target) {
                continue;
            }
            let Some(args) = check.get("args") else { continue };
            let codes = codes_in(args, root);
            if codes.is_empty() {
                continue;
            }
            for wrap in calls {
                let (Some(w_end), Some(c_end)) = (number(wrap, "end_line"), number(check, "end_line"))
                else {
                    continue;
                };
                if id_of(wrap, "id") == id_of(check, "id")
                    || number(wrap, "line") != number(check, "line")
                    || w_end <= c_end
                {
                    continue;
                }
                let Some(wrap_args) = wrap.get("args") else { continue };
                walk_value(wrap_args, &mut |n| {
                    let has_fn = n.get("$fn").map(|f| !f.is_null()).unwrap_or(false);
                    let Some(Value::String(expr_id)) = n.get("$expr_id") else { return };
                    if !has_fn {
                        return;
                    }
                    let Some(anchor) = expr_at.get(expr_id) else { return };
                    let Some(Some(f)) = fn_at.get(anchor) else { return };
                    let Some(Value::Array(params)) = f.get("params") else { return };
                    let Some(first) = params.first().and_then(|p| p.as_object()) else { return };
                    let Some(Value::String(param)) = first.get("name") else { return };
                    if param.is_empty() {
                        return;
                    }
                    let Some(fn_id) = id_of(f, "id") else { return };
                    let code = codes.keys().next().cloned().unwrap_or_default();
                    match out.get_mut(&fn_id) {
                        Some(seen) => {
                            seen.multi = seen.multi || seen.code != code || codes.len() > 1;
                        }
                        None => {
                            out.insert(
                                fn_id,
                                Callback { code, param: param.clone(), multi: codes.len() > 1 },
                            );
                        }
                    }
                });
            }
        }
    }
    out.retain(|_, v| !v.multi);
    out
}

/// `(class, property)` -> codes for properties a feature callback can only leave TRUTHY
/// when the feature is on.
///
/// THE UNIVERSE IS EVERY WRITE TO THE PROPERTY, NOT EVERY WRITE INSIDE THE CALLBACK.
/// Filtering to the writes whose enclosing function IS a feature callback, proving those,
/// then publishing the attribution globally let a truthy write in a DIFFERENT method
/// disqualify nothing — and two gates would have shipped that no grant can satisfy.
///
/// BEING WRITTEN IN THE CALLBACK IS NOT EVIDENCE. A callback also does its ordinary work
/// there, and those assignments run whichever answer came back.
pub fn callback_features(
    store: &Store<'_>,
    callbacks: &IndexMap<String, Callback>,
    written: &std::collections::HashSet<String>,
    owner: &Owner<'_>,
) -> Features {
    let branch_rows = store.table("branches");
    let mut branches: IndexMap<String, &Row> = IndexMap::new();
    for b in branch_rows.iter() {
        if let Some(id) = id_of(b, "id") {
            branches.insert(id, b);
        }
    }

    let guarded = |start: Option<String>, param: &str| -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut current = start;
        while let Some(id) = current {
            if !seen.insert(id.clone()) {
                return false;
            }
            let Some(b) = branches.get(&id) else { return false };
            if text(b, "sense").as_deref() == Some("then")
                && let Some(condition) = b.get("condition").filter(|c| !c.is_null())
                && necessary_idents(condition).contains_key(param)
            {
                return true;
            }
            current = id_of(b, "parent");
        }
        false
    };

    let mut out = Features::new();
    let assignments = store.table("assignments");
    for (key, writes) in per_property(&assignments, owner, false) {
        if written.contains(&key) {
            continue;
        }
        let mut got: Option<Codes> = None;
        let mut proven = true;
        for a in writes {
            if text(a, "operator").as_deref() != Some("=") {
                proven = false;
                break;
            }
            let Some(v) = a.get("value") else { continue };
            if is_blank(v) {
                continue;
            }
            // A truthy write outside any callback runs regardless.
            let Some(cb) = id_of(a, "member").and_then(|m| callbacks.get(&m)) else {
                proven = false;
                break;
            };
            if !necessary_idents(v).contains_key(&cb.param)
                && !guarded(id_of(a, "branch"), &cb.param)
            {
                proven = false;
                break;
            }
            let mut one = Codes::new();
            one.insert(cb.code.clone(), true);
            got = Some(intersect(got, &one));
        }
        if let Some(got) = got
            && proven
            && !got.is_empty()
        {
            out.insert(key, got);
        }
    }
    out
}

/// `(class, property)` -> codes for properties whose only content comes from a ternary a
/// feature guards, plus the assignment rows that proved it.
///
/// `$cond` IS A SOURCE STRING, NOT AN AST. The AST is its sibling `$cond_expr`, an
/// `expressions` row with `role='ternary'`, so the polarity walk is REUSED and `a || b`
/// falls out as proving nothing for free. Reading `$cond` instead would have run clean and
/// attributed nothing at all — the failure that looks exactly like a shape the frontend
/// does not contain.
///
/// THE ELSE BRANCH IS WHAT MAKES THE GATE NECESSARY. A ternary that supplies a DIFFERENT
/// non-empty value when the feature is off proves nothing about what renders.
pub fn ternary_features(
    store: &Store<'_>,
    resolve: &mut Resolve<'_>,
    iterated: bool,
    written: &std::collections::HashSet<String>,
    owner: &Owner<'_>,
) -> (Features, IndexMap<String, Row>) {
    let expressions = store.table("expressions");
    let mut asts: IndexMap<String, &Value> = IndexMap::new();
    for r in expressions.iter() {
        if text(r, "role").as_deref() == Some("ternary")
            && let Some(ast) = r.get("ast").filter(|a| !a.is_null())
            && let Some(id) = id_of(r, "id")
        {
            asts.insert(id, ast);
        }
    }

    let mut feats = Features::new();
    let mut writes_out: IndexMap<String, Row> = IndexMap::new();
    let assignments = store.table("assignments");

    for (key, writes) in per_property(&assignments, owner, true) {
        // The template fills it whatever the condition answered.
        if written.contains(&key) {
            continue;
        }
        let mut got: Option<Codes> = None;
        let mut proven: Vec<&Row> = Vec::new();
        let mut ok = true;

        for a in writes {
            if text(a, "operator").as_deref() != Some("=") {
                ok = false;
                break;
            }
            let Some(v) = a.get("value") else { continue };
            if supplies_nothing(v, iterated) {
                continue;
            }
            let Some(object) = v.as_object() else {
                ok = false;
                break;
            };
            let Some(Value::String(cond_expr)) = object.get("$cond_expr") else {
                ok = false;
                break;
            };
            if !supplies_nothing(object.get("$else").unwrap_or(&Value::Null), iterated) {
                ok = false;
                break;
            }
            if supplies_nothing(object.get("$then").unwrap_or(&Value::Null), iterated) {
                continue;
            }
            let Some(ast) = asts.get(cond_expr) else {
                ok = false;
                break;
            };
            let mut codes = OrderedSet::new();
            necessary(ast, false, resolve, &mut codes);
            if codes.is_empty() {
                ok = false;
                break;
            }
            let mut as_map = Codes::new();
            for c in codes.strings() {
                as_map.insert(c, true);
            }
            got = Some(intersect(got, &as_map));
            proven.push(a);
        }

        if !ok {
            continue;
        }
        let Some(got) = got.filter(|g| !g.is_empty()) else { continue };
        feats.insert(key, got);
        for a in proven {
            if let Some(id) = id_of(a, "id") {
                writes_out.insert(id, a.clone());
            }
        }
    }
    (feats, writes_out)
}
