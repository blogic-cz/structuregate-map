//! THE MIRROR FOR A LIST: a structural directive the template hands a LIST of enum members, which
//! the class tests for MEMBERSHIP rather than comparing.
//!
//! `*whenStock="[StockKind.Alpha]"` over a directive whose setter stores the list
//! in `kinds` and renders under `this.items.some((c) =>
//! this.kinds.includes(c.Kind))` renders only when some order is of a listed
//! type: `items.Kind in ["Alpha"]`, a SET dimension as `gate_predicates` states one.
//! `this.types.includes(this.current)` over a member typed by the enum is the VALUE form: `current in
//! [...]`. The test is `F.includes(x)`, `F.indexOf(x) !== -1` or `F.some((e) => e === x)`.
//!
//! Every other step is the comparison mirror's own (`gate_directive_inputs`): the input, the fields
//! its setter copies it into, the render sites and their polarity. Two readings of a render condition
//! were added there for this shape, and serve both:
//!   * a `const` local is what it was initialised to - `const has = …; if (has) render();`;
//!   * an `||` whose other side can NEVER hold is the side that can. The directive also ORs
//!     `whenStockShelf`, and no template in the tree binds it: a field only that
//!     input's setter writes, with no initializer, is `undefined` for ever, and a side that needs it
//!     truthy (`!!this.shelf && …`) is false. A side that only COMPARES it proves nothing -
//!     `undefined === x` may hold - and the restriction stays `unknown`.
//!
//! UNKNOWN, NEVER ABSENT, as in the comparison mirror: a template value that is not a list of the
//! enum's constants, two tests of the field, a copy under an `if`, or a render the test does not
//! decide keeps the dimension and states no value. An EMPTY list is `unknown` too - `[].includes(x)`
//! is never true, and a template writing `[]` more likely means "no restriction" than "never".

use super::*;
use super::super::super::astreads::is_read;
use super::super::super::gate_predicates::{literal_is, method_call, minus_one, only_return, this_path};

/// One membership test of the list field.
pub(super) struct ListTest {
    dim: String,
    row: Option<String>,
    /// The tested value is each item of a collection, so the dimension is that collection's.
    set: bool,
}

fn kind(n: &Value) -> Option<&str> {
    n.get("k").and_then(|k| k.as_str())
}

/// What holds the template's list: a FIELD (`field` is its row), or a PARAMETER named `name` of the
/// method the setter hands the list to (`field` is that method's row) - see `param_carried`.
pub(super) struct Held<'h> {
    pub(super) field: &'h str,
    pub(super) name: &'h str,
    pub(super) param: bool,
}

/// `this.F`, by its row or - a link inside a chain carries no target - by its name off `this`; for a
/// parameter, the bare name inside its method.
fn is_field(cx: &Ctx<'_>, n: &Value, h: &Held<'_>) -> bool {
    let (field, name) = (h.field, h.name);
    if h.param {
        return n.as_object().and_then(bare) == Some(name);
    }
    if !is_read(n) || n.get("receiver").and_then(kind) != Some("This") {
        return false;
    }
    match n.get("target") {
        Some(t) if t.is_object() => member_at(&cx.midx, Some(t)).as_deref() == Some(field),
        _ => n.get("name").and_then(|v| v.as_str()) == Some(name),
    }
}

/// The value a membership test of the field tests.
fn tested<'v>(cx: &Ctx<'_>, n: &'v Value, h: &Held<'_>) -> Option<&'v Value> {
    if let Some((list, x)) = method_call(n, "includes") {
        return is_field(cx, list, h).then_some(x);
    }
    if let Some((list, cb)) = method_call(n, "some") {
        let body = only_return(cb).filter(|_| is_field(cx, list, h))?;
        if kind(body) != Some("Binary") || !EQ.contains(&body.get("op")?.as_str()?) {
            return None;
        }
        // The element is the BARE side - the callback's own parameter.
        let (l, r) = (body.get("left")?, body.get("right")?);
        let element = |s: &Value| s.as_object().and_then(bare).is_some();
        return match (element(l), element(r)) {
            (true, false) => Some(r),
            (false, true) => Some(l),
            _ => None,
        };
    }
    let found = match n.get("op").and_then(|o| o.as_str()) {
        Some("!==" | "!=" | ">") => n.get("right").is_some_and(minus_one),
        Some(">=") => n.get("right").is_some_and(|r| literal_is(r, 0)),
        _ => false,
    };
    let (list, x) = method_call(n.get("left")?, "indexOf").filter(|_| found)?;
    is_field(cx, list, h).then_some(x)
}

/// `c.A.B` as (`c`, `A.B`), for a chain rooted at a bare name.
fn rooted(n: &Value) -> Option<(&str, String)> {
    let mut parts: Vec<&str> = Vec::new();
    let mut current = n;
    while is_read(current) {
        if let Some(root) = current.as_object().and_then(bare) {
            parts.reverse();
            return Some((root, parts.join(".")));
        }
        parts.push(current.get("name")?.as_str()?);
        current = current.get("receiver")?;
    }
    None
}

/// The test `n` IS, if it tests the field: a collection's `some` whose callback tests the field
/// with an item, or the field's own test of a member of the class.
fn test_of(cx: &Ctx<'_>, mx: &Mirror<'_>, n: &Value, h: &Held<'_>, en: &str) -> Option<ListTest> {
    let typed = |x: &Value| -> Option<String> {
        let resolved = mx.rows.resolve(x, &IndexSet::new(), cx.idx);
        resolved.get("target")?.get("row")?.as_str().map(str::to_string)
    };
    let fits = |row: &Option<String>| row.as_ref().and_then(|r| cx.mem.typed.get(r)).is_none_or(|e| e == en);
    if let Some((items, cb)) = method_call(n, "some") {
        let x = tested(cx, only_return(cb)?, h)?;
        let (_, rest) = rooted(x)?;
        let base = this_path(items).or_else(|| rooted(items).map(|(root, r)| if r.is_empty() { root.into() } else { format!("{root}.{r}") }))?;
        let row = if rest.is_empty() { None } else { typed(x) };
        let dim = if rest.is_empty() { base } else { format!("{base}.{rest}") };
        return fits(&row).then_some(ListTest { dim, row, set: true });
    }
    let x = tested(cx, n, h)?;
    if x.get("receiver").and_then(kind) != Some("This") {
        return None;
    }
    let row = member_at(&cx.midx, x.get("target"));
    let dim = row.as_ref().and_then(|r| cx.midx.name_of.get(r)).cloned().or_else(|| this_path(x))?;
    (fits(&row) && row.as_deref() != Some(h.field)).then_some(ListTest { dim, row, set: false })
}

/// Every distinct test of the field inside `span` - the class that declares it, or the method a
/// parameter belongs to - as (file, first line, last line).
fn tests_in(cx: &Ctx<'_>, mx: &Mirror<'_>, span: (Option<&str>, Option<i64>, Option<i64>), h: &Held<'_>, en: &str) -> Vec<ListTest> {
    let mut out: Vec<ListTest> = Vec::new();
    let (Some(file), Some(from), Some(to)) = span else { return out };
    for r in cx.expr_by_file.get(file).map(Vec::as_slice).unwrap_or(&[]) {
        if int_of(r.get("line")).is_none_or(|l| l < from || l > to) {
            continue;
        }
        let Some(ast) = r.get("ast").filter(|a| truthy(Some(a))) else { continue };
        let mut stack = vec![unwrap(ast)];
        while let Some(n) = stack.pop() {
            match n {
                Value::Array(items) => stack.extend(items.iter()),
                Value::Object(o) => {
                    if let Some(t) = test_of(cx, mx, n, h, en)
                        && !out.iter().any(|seen| seen.dim == t.dim && seen.set == t.set)
                    {
                        out.push(t);
                    }
                    stack.extend(o.values());
                }
                _ => {}
            }
        }
    }
    out
}

/// What the class proves about one list field the template value lands in.
pub(super) fn carried(cx: &Ctx<'_>, mx: &Mirror<'_>, chain: &[Class], field: &str, typed: Option<String>, input: &str, copy: Option<&str>) -> Option<Carried> {
    let en = typed?;
    let declaring = cx.class_of_member.get(field).and_then(|owner| cx.cidx.by_id.get(owner))?;
    let name = cx.midx.name_of.get(field)?;
    let held = Held { field, name, param: false };
    let span = (declaring.file.as_deref(), declaring.line, declaring.end_line);
    let unsure = unsure_of(mx, field, input, copy);
    decided(cx, mx, chain, span, &held, en, unsure)
}

/// The tests of what `held` holds inside `span`, and the polarity every render site requires of the one.
fn decided(cx: &Ctx<'_>, mx: &Mirror<'_>, chain: &[Class], span: (Option<&str>, Option<i64>, Option<i64>), held: &Held<'_>, en: String, unsure: bool) -> Option<Carried> {
    let tests = tests_in(cx, mx, span, held, &en);
    let renders_on = match tests.as_slice() {
        [] => return None,
        [want] => {
            let test = |n: &Value| test_of(cx, mx, n, held, &en).filter(|t| t.dim == want.dim && t.set == want.set).map(|_| true);
            renders_on(cx, mx, chain, &test)
        }
        _ => None,
    };
    Some(Carried { enum_id: Some(en), comparisons: Vec::new(), tests, unsure, renders_on })
}

/// The class member named `name` in the chain, nearest class first.
fn member_named<'r>(cx: &Ctx<'r>, chain: &[Class], name: &str) -> Option<&'r Row> {
    chain.iter().flat_map(|c| cx.members_by_class.get(&c.id).map(Vec::as_slice).unwrap_or(&[]))
        .find(|m| str_of(m.get("name")).as_deref() == Some(name)).copied()
}

/// Every `this.M(...)` call in the class chain, by the callee's name.
fn calls_of<'r>(cx: &Ctx<'_>, mx: &Mirror<'r>, chain: &[Class], method: &str) -> Vec<&'r Row> {
    let callee = format!("this.{method}");
    let mut out = Vec::new();
    for cls in chain {
        for m in cx.members_by_class.get(&cls.id).map(Vec::as_slice).unwrap_or(&[]) {
            if let Some(mid) = id_of(m, "id") {
                out.extend(mx.calls_in(&mid).into_iter().filter(|c| str_of(c.get("callee")).as_deref() == Some(callee.as_str())));
            }
        }
    }
    out
}

/// THE LIST HANDED ON: a setter that passes its parameter to a method -
/// `set whenStock(types) { this.refresh(types, …); }` - lands the template's list
/// in THAT method's parameter, and the method's tests of it (in a store callback as often as not) are
/// the directive's. Every `this.refresh` call must hand it the same thing: the setter's
/// parameter, unconditionally, or be a call from a setter no template binds; anything else is unsure.
pub(super) fn param_carried(cx: &Ctx<'_>, mx: &Mirror<'_>, chain: &[Class], input: &str, param: &str, typed: Option<String>) -> Vec<Carried> {
    let mut out = Vec::new();
    let Some(en) = typed else { return out };
    for call in mx.calls_in(input) {
        let Some(Value::Array(args)) = call.get("args") else { continue };
        let Some(at) = args.iter().position(|a| is_identifier(a, param)) else { continue };
        let Some(callee) = str_of(call.get("callee")) else { continue };
        let Some(method) = callee.strip_prefix("this.") else { continue };
        let Some(m) = member_named(cx, chain, method) else { continue };
        let Some(q) = m.get("params").and_then(|p| p.get(at)).and_then(|p| str_of(p.get("name"))) else { continue };
        let Some(mid) = id_of(m, "id") else { continue };
        let file = id_of(m, "file").or_else(|| cx.class_of_member.get(&mid).and_then(|c| cx.cidx.by_id.get(c)).and_then(|c| c.file.clone()));
        let span = (file.as_deref(), int_of(m.get("line")), int_of(m.get("end_line")));
        let unsure = calls_of(cx, mx, chain, method).iter().any(|c| {
            let from_input = id_of(c, "member").as_deref() == Some(input);
            let hands = c.get("args").and_then(|a| a.get(at)).is_some_and(|a| is_identifier(a, param));
            let dead = id_of(c, "member").is_some_and(|s| mx.dead.contains(&s));
            !dead && !(from_input && hands && !truthy(c.get("branch")) && !truthy(c.get("case")))
        });
        out.extend(decided(cx, mx, chain, span, &Held { field: &mid, name: &q, param: true }, en.clone(), unsure));
    }
    out
}

/// An evaluated argument that is the bare identifier `name`.
fn is_identifier(a: &Value, name: &str) -> bool {
    a.get("$kind").and_then(|k| k.as_str()) == Some("Identifier") && a.get("$expr").and_then(|e| e.as_str()) == Some(name)
}

/// ONE OCCURRENCE, DECIDED: the template's list of constants as the members the test permits.
pub(super) fn restriction(cx: &Ctx<'_>, f: &Carried, en: &str, gate_ast: Option<&Value>) -> Vec<Value> {
    let unknown: Vec<Value> = f.tests.iter()
        .map(|t| json!({"enum": en, "dim": t.dim, "row": t.row, "op": "unknown", "set": t.set}))
        .collect();
    let items = gate_ast.map(unwrap).filter(|a| kind(a) == Some("Array")).and_then(|a| a.get("items")?.as_array());
    let values: Option<Vec<String>> = items.filter(|i| !i.is_empty()).and_then(|i| {
        i.iter().map(|x| constant_of(x, cx.mem, cx.idx).filter(|(e, _)| e == en).map(|(_, v)| v)).collect()
    });
    let (Some(values), Some(on), false, [t]) = (values, f.renders_on, f.unsure, f.tests.as_slice()) else {
        return unknown;
    };
    vec![json!({"enum": en, "dim": t.dim, "row": t.row, "op": if on { "in" } else { "not_in" },
                "values": values, "set": t.set})]
}

/// Fields that are `undefined` for the life of the class: no initializer, and written only by the
/// setters of inputs no template binds (or by nothing at all) - and those SETTERS, which never run.
pub(super) fn never_set(store: &Store<'_>, writes: &IndexMap<String, Vec<&Row>>, bound: &IndexSet<String>) -> (IndexSet<String>, IndexSet<String>) {
    let members = store.table("members");
    let mut unbound: IndexMap<String, Option<String>> = IndexMap::new();
    for m in members.iter() {
        let Some(id) = id_of(m, "id") else { continue };
        let decorators = m.get("decorators").and_then(|d| d.as_array());
        let Some(d) = decorators.into_iter().flatten().find(|d| d.get("name").and_then(|n| n.as_str()) == Some("Input")) else { continue };
        let alias = d.get("args").and_then(|a| a.get(0)).and_then(|a| a.as_str()).map(str::to_string).or_else(|| str_of(m.get("name")));
        if alias.is_some_and(|a| !bound.contains(&a)) {
            let param = m.get("params").and_then(|p| p.get(0)).and_then(|p| str_of(p.get("name")));
            unbound.insert(id, param);
        }
    }
    let mut out = IndexSet::new();
    for m in members.iter() {
        let Some(id) = id_of(m, "id") else { continue };
        let blank = m.get("value").is_none_or(|v| v.is_null() || *v == Value::Bool(false));
        if str_of(m.get("kind")).as_deref() != Some("property") || !blank {
            continue;
        }
        let ws = writes.get(&id).map(Vec::as_slice).unwrap_or(&[]);
        let only_unbound = ws.iter().all(|a| {
            id_of(a, "member").and_then(|s| unbound.get(&s).cloned()).flatten().is_some_and(|p| copies(a, &id_of(a, "member").unwrap_or_default(), &p))
        });
        // A field ANOTHER decorator fills (`@ViewChild`, `@ContentChild`) is Angular's to set.
        let decorated = m.get("decorators").and_then(|d| d.as_array()).is_some_and(|d| !d.is_empty());
        if only_unbound && (unbound.contains_key(&id) || !decorated) {
            out.insert(id);
        }
    }
    let dead = members.iter()
        .filter(|m| str_of(m.get("kind")).as_deref() == Some("setter"))
        .filter_map(|m| id_of(m, "id")).filter(|id| unbound.contains_key(id)).collect();
    (out, dead)
}

/// The scopes a body sits in, innermost first: a callback, the callback around it, ... its method.
pub(super) fn scopes(store: &Store<'_>) -> (IndexMap<String, String>, IndexMap<String, Vec<String>>) {
    let mut parent = IndexMap::new();
    let mut params: IndexMap<String, Vec<String>> = IndexMap::new();
    for table in ["members", "functions"] {
        for r in store.table(table).iter() {
            let Some(id) = id_of(r, "id") else { continue };
            if table == "functions" && let Some(p) = id_of(r, "parent") {
                parent.insert(id.clone(), p);
            }
            let names = r.get("params").and_then(|p| p.as_array()).into_iter().flatten().map(|p| str_of(p.get("name")).unwrap_or_default());
            params.insert(id, names.collect());
        }
    }
    (parent, params)
}

/// (method, parameter) pairs NO LIVE CALL FILLS: every `this.M(...)` anywhere - a same-named method
/// of another class only adds calls, so it can only keep a parameter alive - sits in a setter no
/// template binds, omits the argument, or hands it a field that is never set.
pub(super) fn dead_params(store: &Store<'_>, mx: &Mirror<'_>) -> IndexSet<(String, String)> {
    let members = store.table("members");
    let mut class_of: IndexMap<String, String> = IndexMap::new();
    let mut field_named: IndexMap<(String, String), String> = IndexMap::new();
    let mut methods: Vec<(String, String, String)> = Vec::new();
    for m in members.iter() {
        let (Some(id), Some(cls), Some(name)) = (id_of(m, "id"), id_of(m, "class"), str_of(m.get("name"))) else { continue };
        class_of.insert(id.clone(), cls.clone());
        field_named.insert((cls.clone(), name.clone()), id.clone());
        if str_of(m.get("kind")).as_deref() == Some("method") {
            methods.push((id, cls, name));
        }
    }
    let owner = |mut id: String| -> Option<String> {
        for _ in 0..16 {
            if let Some(c) = class_of.get(&id) {
                return Some(c.clone());
            }
            id = mx.parent.get(&id)?.clone();
        }
        None
    };
    let mut by_callee: IndexMap<String, Vec<&Row>> = IndexMap::new();
    for c in mx.calls.values().flatten() {
        if let Some(callee) = str_of(c.get("callee")) {
            by_callee.entry(callee).or_default().push(c);
        }
    }
    let mut out = IndexSet::new();
    for (id, _, name) in &methods {
        let calls = by_callee.get(&format!("this.{name}")).map(Vec::as_slice).unwrap_or(&[]);
        for (at, param) in mx.params.get(id).into_iter().flatten().enumerate() {
            let unfilled = |c: &&Row| {
                let member = id_of(c, "member").unwrap_or_default();
                let Some(arg) = c.get("args").and_then(|a| a.get(at)) else { return true };
                let field = owner(member.clone()).and_then(|cls| {
                    let held = arg.get("$expr").and_then(|e| e.as_str())?.strip_prefix("this.")?;
                    (arg.get("$kind").and_then(|k| k.as_str()) == Some("PropertyAccessExpression"))
                        .then(|| field_named.get(&(cls, held.to_string())).cloned()).flatten()
                });
                mx.dead.contains(&member) || field.is_some_and(|f| mx.never.contains(&f))
            };
            if !calls.is_empty() && calls.iter().all(unfilled) {
                out.insert((id.clone(), param.clone()));
            }
        }
    }
    out
}

/// A side of an `||` that can never hold: one of its conjuncts needs a never-set field truthy, or
/// a parameter of the method `member` sits in that no live call fills (`dead_params`).
pub(super) fn never(mx: &Mirror<'_>, member: &str, side: &Value) -> bool {
    let n = unwrap(side);
    match kind(n) {
        Some("Binary") if n.get("op").and_then(|o| o.as_str()) == Some("&&") => {
            n.get("left").is_some_and(|l| never(mx, member, l)) || n.get("right").is_some_and(|r| never(mx, member, r))
        }
        Some("Not") => n.get("expr").filter(|e| kind(e) == Some("Not")).and_then(|e| e.get("expr")).is_some_and(|e| never(mx, member, e)),
        Some("Read") if n.get("receiver").and_then(kind) == Some("This") => {
            let resolved = mx.rows.resolve(n, &IndexSet::new(), &Default::default());
            resolved.get("target").and_then(|t| t.get("row")).and_then(|r| r.as_str()).is_some_and(|r| mx.never.contains(r))
        }
        // THE NEAREST SCOPE DECLARING THE NAME owns it: a callback's own parameter shadows the method's.
        Some("Read") => {
            let Some(name) = n.as_object().and_then(bare) else { return false };
            let mut scope = member.to_string();
            for _ in 0..16 {
                if mx.params.get(&scope).is_some_and(|p| p.iter().any(|q| q == name)) {
                    return mx.dead_params.contains(&(scope, name.to_string()));
                }
                let Some(up) = mx.parent.get(&scope) else { return false };
                scope = up.clone();
            }
            false
        }
        _ => false,
    }
}
