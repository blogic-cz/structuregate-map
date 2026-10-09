//! THE MIRROR SHAPE: a structural directive whose restricting CONSTANT arrives from the
//! TEMPLATE, while the class compares the field that holds it against a value of its own.
//!
//! `*whenMode="Modes.A"` renders iff `this.mode ===
//! this.activeMode`, and `mode` is what the `@Input` setter stored. The gate's
//! expression is a bare constant - a `Read`, where `restriction_of` needs a `Binary` - and the
//! class binds no constant for the constructor rule beside this one to find, so a few gates on
//! a large Angular workspace stated a provable restriction and published nothing.
//!
//! A child of `gate_directives` by `#[path]`, and the same machinery sourced from the other
//! side. EVERY STEP IS A RESOLVED ROW:
//!   the INPUT       the member whose `@Input` alias is the gate's name, nearest class first
//!   the CARRIER     that member itself (a decorated property), or each field the setter copies
//!                   its one parameter into (`assignments`: `this.F = p`, written by the setter)
//!   the COMPARISON  `comparisons_in`, the reader the constructor rule uses, from the carrier
//!   the POLARITY    read off the RENDER SITES, never off the operator alone
//!
//! WHY THE RENDER SITES. `if (a !== b) { clear(); return; } render();` spells `!==` and renders
//! on equality, so an operator taken as the render decision states the opposite of the truth.
//! The polarity is what EVERY `createEmbeddedView` in the class chain requires: a branch it
//! sits under, the negation of a one-level `return` before it in the same method, or - when its
//! method proves nothing itself - what every `this.` call reaching that method requires.
//!
//! UNKNOWN, NEVER ABSENT. A carrier written by anything but the copy, a copy under an `if`, two
//! comparisons, a value that is not a constant of the enum, or a render site that does not
//! require the comparison: each keeps the dimension and states no value. A carrier no
//! comparison reads restricts nothing, and is not a row at all.

use super::*;
use super::super::gate_predicates::param_enum;
use super::super::gate_tsrows::{bare, TsRows};
use super::super::gate_values::{constant_of, enum_of_type_ref};

/// The rows the mirror reads beyond what `Ctx` holds: who writes a field, and the render path.
pub(super) struct Mirror<'a> {
    pub(super) writes: IndexMap<String, Vec<&'a Row>>,
    pub(super) exprs: IndexMap<String, &'a Row>,
    /// Branch id -> (parent, under an `else`, the condition's expression id).
    pub(super) branches: IndexMap<String, (Option<String>, bool, Option<String>)>,
    pub(super) calls: IndexMap<String, Vec<&'a Row>>,
    pub(super) returns: IndexMap<String, Vec<&'a Row>>,
    /// `member + " " + name` -> the tree of the ONE `const` local of that name: `const has = …;
    /// if (has) render();` requires what `has` was initialised to. Empty for a name declared twice.
    locals: IndexMap<String, String>,
    /// Fields `undefined` for the life of the class, and the setters that never run - see
    /// `lists::never_set` - and the method parameters no live call fills (`lists::dead_params`).
    never: IndexSet<String>,
    dead: IndexSet<String>,
    dead_params: IndexSet<(String, String)>,
    /// A callback -> the body it is written in, and every body's parameter names.
    parent: IndexMap<String, String>,
    children: IndexMap<String, Vec<String>>,
    params: IndexMap<String, Vec<String>>,
    /// The TypeScript reads, tied to their rows: a list test's element is a TYPE member.
    rows: TsRows,
}

impl<'a> Mirror<'a> {
    pub(super) fn new(
        store: &Store<'_>,
        assignments: &'a [Row],
        expressions: &'a [Row],
        branches: &'a [Row],
        calls: &'a [Row],
        returns: &'a [Row],
    ) -> Self {
        let mut m = Mirror {
            writes: IndexMap::new(),
            exprs: IndexMap::new(),
            branches: IndexMap::new(),
            calls: IndexMap::new(),
            returns: IndexMap::new(),
            locals: IndexMap::new(),
            never: IndexSet::new(),
            dead: IndexSet::new(),
            dead_params: IndexSet::new(),
            parent: IndexMap::new(),
            children: IndexMap::new(),
            params: IndexMap::new(),
            rows: TsRows::new(store),
        };
        (m.parent, m.params) = lists::scopes(store);
        for (child, up) in &m.parent {
            m.children.entry(up.clone()).or_default().push(child.clone());
        }
        for l in store.table("locals").iter() {
            let (Some(member), Some(name)) = (id_of(l, "member"), str_of(l.get("name"))) else { continue };
            let key = format!("{member} {name}");
            let constant = str_of(l.get("declared")).as_deref() == Some("const");
            let tree = id_of(l, "expression").filter(|_| constant && !m.locals.contains_key(&key));
            m.locals.insert(key, tree.unwrap_or_default());
        }
        for a in assignments {
            if let Some(t) = id_of(a, "target_id") {
                m.writes.entry(t).or_default().push(a);
            }
        }
        for e in expressions {
            if let Some(id) = id_of(e, "id") {
                m.exprs.insert(id, e);
            }
        }
        for b in branches {
            if let Some(id) = id_of(b, "id") {
                let negated = str_of(b.get("sense")).as_deref() == Some("else");
                m.branches.insert(id, (id_of(b, "parent"), negated, id_of(b, "condition_expr")));
            }
        }
        for c in calls {
            if let Some(member) = id_of(c, "member") {
                m.calls.entry(member).or_default().push(c);
            }
        }
        for r in returns {
            if let Some(member) = id_of(r, "member") {
                m.returns.entry(member).or_default().push(r);
            }
        }
        let bound: IndexSet<String> = store.table("bindings").iter().filter_map(|b| str_of(b.get("name"))).collect();
        (m.never, m.dead) = lists::never_set(store, &m.writes, &bound);
        m.dead_params = lists::dead_params(store, &m);
        m
    }

    /// A member and every callback written inside it, at any depth: a render in a store callback
    /// is a render site of the method that subscribes.
    /// Every call a member makes, in its own body and in every callback written inside it: what a
    /// method does includes what its `subscribe` does. The ONE way this rule reads a body's calls.
    pub(super) fn calls_in(&self, member: &str) -> Vec<&'a Row> {
        self.inner_of(member).iter().flat_map(|b| self.calls.get(b).into_iter().flatten().copied()).collect()
    }

    pub(super) fn inner_of(&self, member: &str) -> Vec<String> {
        let mut out = vec![member.to_string()];
        let mut i = 0;
        while i < out.len() && out.len() < 256 {
            let more = self.children.get(&out[i]).cloned().unwrap_or_default();
            out.extend(more.into_iter().filter(|c| !out.contains(c)).collect::<Vec<_>>());
            i += 1;
        }
        out
    }

    fn condition(&self, branch: &str) -> Option<&'a Value> {
        let expr = self.branches.get(branch)?.2.as_ref()?;
        self.exprs.get(expr)?.get("ast").filter(|a| !a.is_null())
    }

    /// The branches around a statement, innermost first, as (id, under an `else`).
    fn chain(&self, start: Option<String>) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        let mut current = start;
        while let Some(id) = current {
            if out.iter().any(|(seen, _)| *seen == id) {
                break;
            }
            let Some((parent, negated, _)) = self.branches.get(&id) else { break };
            out.push((id.clone(), *negated));
            current = parent.clone();
        }
        out
    }

    /// The tree a bare `const` local of `member` was initialised to.
    fn local(&self, member: &str, n: &Value) -> Option<&'a Value> {
        let name = n.as_object().and_then(bare)?;
        let expr = self.locals.get(&format!("{member} {name}")).filter(|x| !x.is_empty())?;
        self.exprs.get(expr)?.get("ast").filter(|a| !a.is_null())
    }
}

/// What one occurrence's directive class proves about the field the template value lands in.
pub(super) struct Carried {
    enum_id: Option<String>,
    comparisons: Vec<Comparison>,
    /// A LIST input's field: every membership test of it (`lists`), in place of a comparison.
    tests: Vec<lists::ListTest>,
    /// Written by anything but the copy of the input, or the copy sits under an `if`/`case`.
    unsure: bool,
    /// The polarity every render site requires: `Some(true)` renders on equality.
    renders_on: Option<bool>,
}

/// Whether a node IS the test a render is asked about, and which way: `Some(true)` holds on
/// equality (or membership), `Some(false)` on its opposite.
type Test<'t> = &'t dyn Fn(&Value) -> Option<bool>;

/// The `field`/`other` comparison as a `Test`.
fn pair_test<'p>(cx: &'p Ctx<'_>, pair: (&'p str, &'p str)) -> impl Fn(&Value) -> Option<bool> + 'p {
    move |n| {
        let op = str_of(n.get("op")).filter(|_| n.get("k").and_then(|k| k.as_str()) == Some("Binary"))?;
        if !EQ.contains(&op.as_str()) && !NE.contains(&op.as_str()) {
            return None;
        }
        let side = |s: Option<&Value>| member_at(&cx.midx, s?.get("target"));
        let (a, b) = (side(n.get("left")), side(n.get("right")));
        let (a, b) = (a.as_deref(), b.as_deref());
        let hit = (a, b) == (Some(pair.0), Some(pair.1)) || (a, b) == (Some(pair.1), Some(pair.0));
        hit.then(|| EQ.contains(&op.as_str()))
    }
}

/// Every polarity of the test `ast` cannot be true without, in `member`. A `const` local is read
/// as what it was initialised to, and an `||` whose other side can never hold (`lists::never`) as
/// the side that can.
fn required(mx: &Mirror<'_>, member: &str, ast: &Value, negated: bool, test: Test<'_>, depth: usize, out: &mut Vec<bool>) {
    let n = unwrap(ast);
    if depth > 8 {
        return;
    }
    match n.get("k").and_then(|k| k.as_str()) {
        Some("Not") | Some("NonNull") => {
            let flip = n.get("k").and_then(|k| k.as_str()) == Some("Not");
            if let Some(e) = n.get("expr") {
                required(mx, member, e, negated != flip, test, depth + 1, out);
            }
            return;
        }
        Some("Binary") if matches!(str_of(n.get("op")).as_deref(), Some("&&" | "||")) => {
            let op = str_of(n.get("op")).unwrap_or_default();
            let (Some(l), Some(r)) = (n.get("left"), n.get("right")) else { return };
            if (op == "&&" && !negated) || (op == "||" && negated) {
                required(mx, member, l, negated, test, depth + 1, out);
                required(mx, member, r, negated, test, depth + 1, out);
            } else if op == "||" && lists::never(mx, member, l) {
                required(mx, member, r, negated, test, depth + 1, out);
            } else if op == "||" && lists::never(mx, member, r) {
                required(mx, member, l, negated, test, depth + 1, out);
            }
            return;
        }
        Some("Read") => {
            if let Some(tree) = mx.local(member, n) {
                return required(mx, member, tree, negated, test, depth + 1, out);
            }
        }
        _ => {}
    }
    if let Some(p) = test(n) {
        out.push(p != negated);
    }
}

/// What `call` cannot run without, through its own method and then every `this.` call into it.
/// `None` when nothing proves a polarity or two contradict; an unreachable call is `Some(None)`.
fn site(
    cx: &Ctx<'_>,
    mx: &Mirror<'_>,
    chain: &[Class],
    call: &Row,
    test: Test<'_>,
    depth: usize,
) -> Option<Option<bool>> {
    if depth > 8 {
        return None;
    }
    let mut found = Vec::new();
    let own = mx.chain(id_of(call, "branch"));
    let member = id_of(call, "member")?;
    for (b, negated) in &own {
        if let Some(ast) = mx.condition(b) {
            required(mx, &member, ast, *negated, test, 0, &mut found);
        }
    }
    let line = int_of(call.get("line")).unwrap_or(0);
    for r in mx.returns.get(&member).map(Vec::as_slice).unwrap_or(&[]) {
        if int_of(r.get("line")).unwrap_or(0) >= line {
            continue;
        }
        let ret = mx.chain(id_of(r, "branch"));
        match ret.as_slice() {
            [] => return Some(None),
            [(b, _)] if own.iter().any(|(o, _)| o == b) => return Some(None),
            // NOT of one branch is that branch's condition, negated; NOT of a conjunction of
            // two proves nothing either way.
            [(b, negated)] => {
                if let Some(ast) = mx.condition(b) {
                    required(mx, &member, ast, !*negated, test, 0, &mut found);
                }
            }
            _ => {}
        }
    }
    if found.is_empty() {
        let name = cx.midx.name_of.get(&member)?;
        let callee = format!("this.{name}");
        let mut via: Option<bool> = None;
        for cls in chain {
            for m in cx.members_by_class.get(&cls.id).map(Vec::as_slice).unwrap_or(&[]) {
                let Some(mid) = id_of(m, "id") else { continue };
                // A CALL IN A CALLBACK reaches the method too: a real directive renders through
                // `this.show()` inside a `subscribe`.
                for c in mx.calls_in(&mid) {
                    if str_of(c.get("callee")).as_deref() != Some(callee.as_str()) {
                        continue;
                    }
                    match site(cx, mx, chain, c, test, depth + 1)? {
                        None => {}
                        Some(p) if via.is_none_or(|v| v == p) => via = Some(p),
                        Some(_) => return None,
                    }
                }
            }
        }
        return via.map(Some);
    }
    let first = found[0];
    found.iter().all(|p| *p == first).then_some(Some(first))
}

/// The polarity EVERY render site in the chain requires, or `None`.
fn renders_on(cx: &Ctx<'_>, mx: &Mirror<'_>, chain: &[Class], test: Test<'_>) -> Option<bool> {
    let mut on: Option<bool> = None;
    for cls in chain {
        for m in cx.members_by_class.get(&cls.id).map(Vec::as_slice).unwrap_or(&[]) {
            let Some(mid) = id_of(m, "id") else { continue };
            for c in mx.calls_in(&mid) {
                if str_of(c.get("method")).as_deref() != Some("createEmbeddedView") {
                    continue;
                }
                match site(cx, mx, chain, c, test, 0)? {
                    None => {}
                    Some(p) if on.is_none_or(|o| o == p) => on = Some(p),
                    Some(_) => return None,
                }
            }
        }
    }
    on
}

/// The member whose `@Input` alias is `gate`, nearest class first.
fn input_of<'r>(cx: &Ctx<'r>, chain: &[Class], gate: &str) -> Option<&'r Row> {
    for cls in chain {
        for m in cx.members_by_class.get(&cls.id).map(Vec::as_slice).unwrap_or(&[]) {
            let Some(Value::Array(decorators)) = m.get("decorators") else { continue };
            let Some(d) = decorators.iter().find(|d| d.get("name").and_then(|n| n.as_str()) == Some("Input"))
            else {
                continue;
            };
            let alias = d.get("args").and_then(|a| a.get(0)).and_then(|a| a.as_str()).map(str::to_string);
            if alias.or_else(|| str_of(m.get("name"))).as_deref() == Some(gate) {
                return Some(*m);
            }
        }
    }
    None
}

/// Every field the template value lands in, as what the class proves about it.
pub(super) fn carried(cx: &Ctx<'_>, mx: &Mirror<'_>, chain: &[Class], gate: &str) -> Vec<Carried> {
    let Some(input) = input_of(cx, chain, gate) else { return Vec::new() };
    let Some(input_id) = id_of(input, "id") else { return Vec::new() };
    // (field, the enum the input itself is typed by); `list` when it takes a LIST of that enum.
    let mut fields: Vec<(String, Option<String>)> = Vec::new();
    let mut copy: Option<String> = None;
    let mut setter_typed: Option<String> = None;
    let list: bool;
    match str_of(input.get("kind")).as_deref() {
        Some("setter") => {
            let Some(Value::Array(params)) = input.get("params") else { return Vec::new() };
            let [p] = params.as_slice() else { return Vec::new() };
            let Some(param) = str_of(p.get("name")) else { return Vec::new() };
            let (typed, many) = param_enum(p, cx.idx).map_or((None, false), |(e, many)| (Some(e), many));
            list = many;
            setter_typed = typed.clone();
            for (target, writes) in &mx.writes {
                if writes.iter().any(|a| copies(a, &input_id, &param))
                    && !fields.iter().any(|(f, _)| f == target)
                {
                    fields.push((target.clone(), typed.clone()));
                }
            }
            copy = Some(param);
        }
        Some("property") => {
            let element = input.get("type_ref").and_then(|t| t.get("element")).filter(|e| e.is_object());
            let typed = element.and_then(|e| enum_of_type_ref(cx.idx, e.get("name")?.as_str()?, Some(e)));
            list = typed.is_some();
            fields.push((input_id.clone(), typed));
        }
        _ => return Vec::new(),
    }
    let mut out = Vec::new();
    // A LIST THE SETTER HANDS TO A METHOD lands in that method's parameter - `lists::param_carried`.
    if list && let Some(param) = copy.as_deref() {
        out.extend(lists::param_carried(cx, mx, chain, &input_id, param, setter_typed.clone()));
    }
    for (field, typed) in fields {
        if list {
            out.extend(lists::carried(cx, mx, chain, &field, typed, &input_id, copy.as_deref()));
            continue;
        }
        let unsure = unsure_of(mx, &field, &input_id, copy.as_deref());
        let enum_id = cx.mem.typed.get(&field).cloned().or(typed);
        let Some(owner) = cx.class_of_member.get(&field) else { continue };
        let Some(declaring) = cx.cidx.by_id.get(owner) else { continue };
        let Some(en) = enum_id.as_deref() else { continue };
        let comparisons = comparisons_in(&cx.expr_by_file, cx.mem, &cx.midx, declaring, &field, en, true);
        if comparisons.is_empty() {
            continue;
        }
        let renders_on = match comparisons.as_slice() {
            [c] => renders_on(cx, mx, chain, &pair_test(cx, (&field, &c.row))),
            _ => None,
        };
        out.push(Carried { enum_id, comparisons, tests: Vec::new(), unsure, renders_on });
    }
    out
}

/// Written by anything but the copy of the input, or the copy sits under an `if`/`case`.
fn unsure_of(mx: &Mirror<'_>, field: &str, input: &str, copy: Option<&str>) -> bool {
    mx.writes.get(field).map(Vec::as_slice).unwrap_or(&[]).iter().any(|a| {
        copy.is_none_or(|p| !copies(a, input, p)) || truthy(a.get("branch")) || truthy(a.get("case"))
    })
}

/// `this.F = p` inside the input's own setter.
fn copies(a: &Row, setter: &str, param: &str) -> bool {
    let Some(v) = a.get("value").and_then(|v| v.as_object()) else { return false };
    id_of(a, "member").as_deref() == Some(setter)
        && str_of(a.get("scope")).as_deref() == Some("this")
        && v.get("$kind").and_then(|k| k.as_str()) == Some("Identifier")
        && v.get("$expr").and_then(|e| e.as_str()) == Some(param)
}

/// ONE OCCURRENCE, DECIDED: the class's facts with the template's value filled in.
pub(super) fn mirror_restriction(cx: &Ctx<'_>, f: &Carried, gate_ast: Option<&Value>) -> Vec<Value> {
    let Some(en) = f.enum_id.as_deref() else { return Vec::new() };
    if !f.tests.is_empty() {
        return lists::restriction(cx, f, en, gate_ast);
    }
    let mut by_dim: IndexMap<&str, &Comparison> = IndexMap::new();
    for c in &f.comparisons {
        by_dim.insert(c.dim.as_str(), c);
    }
    let unknown: Vec<Value> = by_dim
        .values()
        .map(|c| json!({"enum": en, "dim": c.dim, "row": c.row, "op": "unknown"}))
        .collect();
    let constant = gate_ast.and_then(|a| constant_of(unwrap(a), cx.mem, cx.idx));
    let (Some((cen, value)), Some(on), false, [c]) =
        (constant, f.renders_on, f.unsure, f.comparisons.as_slice())
    else {
        return unknown;
    };
    if cen != en {
        return unknown;
    }
    vec![json!({
        "enum": en, "dim": c.dim, "row": c.row,
        "op": if on { "in" } else { "not_in" },
        "value": value,
    })]
}

#[path = "gate_directive_lists.rs"]
mod lists;

#[cfg(test)]
#[allow(non_snake_case)]
#[path = "gate_directive_inputs_tests.rs"]
mod tests;
