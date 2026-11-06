//! AN `@Input` FLAG THE PARENTS BIND DIFFERENTLY IS READ PER RENDER EDGE.
//!
//! `gate_input_flags::bound_inputs` defines a child's flag only when EVERY element it renders at binds
//! one identical tree. Two parents that disagree - `<child [isCompany]="true" [showExtra]="extra">` and
//! `<child [isCompany]="true">` - left the flag with no definition, so the child's
//! `*ngIf="isCompany && showExtra"` stated nothing on ANY way in, and the restriction the first parent
//! proves (`extra = isAlphaLike(this.item.kind)`) was lost with the second.
//!
//! EACH RENDER EDGE INTO THE CHILD RESOLVES THE FLAG BY ITS OWN BINDING: the tree it binds, or - when it
//! binds nothing - the input's initializer, when that is a literal. A gate over such a flag is then one
//! condition PER EDGE (`bind:<render>#<gate>`), and the way through that edge carries it. Where the
//! condition folds to the literal `false` (the unbound edge leaves `showExtra` at `false`) the way is no
//! way, as for any `*ngIf="false"` (`key_dead`).
//!
//! THE PER-GATE `gate_values` ROW IS NOT WRITTEN: one row cannot say what two ways disagree on, and a
//! gate that states the first parent's restriction for everyone is the wrong answer the pinned rule
//! refused. The restriction rides the (ref, path) ways only, so `key_reach` moves and `gate_values` does not.
//!
//! An input the child writes itself, and one `bound_inputs` already defines, are left alone.
//!
//! A COMPONENT CREATED IN CODE IS BOUND BY WHAT ITS CREATOR SETS ON THE INSTANCE (#147): `ref.instance.showExtra = true`
//! in the function whose `createComponent` (or factory call) drew the edge is that edge's binding, and an edge whose creator
//! sets nothing leaves the input at its initializer - but only while every write of the input sits in some creator of that
//! class, no `setInput` names it, and the creator creates that class once: a write anywhere else may be on this instance too.
//! A write under a condition the creating call is not under, in a loop or of a value that is no literal binds nothing either way.
//! An input with no initializer is `undefined` on an edge that binds nothing, which a condition reads as `false`. A child that
//! writes the input only as `this.f = this.f && ..` keeps a falsy value falsy: such an edge binds only a falsy value.

use super::astreads::unwrap;
use super::closure::RenderPath;
use super::gate_input_flags::{bound_inputs, input_alias, written_members};
use super::store::{Row, Store};
use crate::rows::IndexMap;
use serde_json::{json, Value};
use crate::rows::ts::rowread::text as cell;

/// What the ways read: the per-edge conditions, the edges that are no way, and where each gate sits.
#[derive(Default)]
pub struct Bound {
    /// `bind:<render>#<gate>` -> the gate's condition with the edge's own binding in place of the flag.
    pub conds: IndexMap<String, (Value, bool)>,
    /// Edges on which the gate's condition is the literal `false`.
    pub dead: crate::rows::IndexSet<String>,
    /// Gate id -> its component, for the gates above only.
    pub component_of_gate: IndexMap<String, String>,
}

/// Whether `ast` is a literal that JavaScript reads as `v` in a condition: `null` (which stands for `undefined` too), `0` and
/// `""` read `false`, as `*ngIf` reads them.
fn literal(ast: &Value, v: bool) -> bool {
    if ast.get("k").and_then(|k| k.as_str()) != Some("Literal") {
        return false;
    }
    let truthy = match ast.get("v") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Null) => false,
        _ => return false,
    };
    truthy == v
}

/// AN INPUT'S VALUE BEFORE ANYONE BINDS IT (#147): its literal initializer, or `undefined` - spelt `null`, which reads the
/// same in a condition - when it has none (`@Input() showExtra: boolean;`), is `= null` or `= undefined`. A non-literal
/// initializer is an object here (`TsDecls/TsExpr/TsValue.mjs::unresolved`) and states nothing.
fn initial(m: &Row) -> Option<Value> {
    match m.get("value") {
        None | Some(Value::Null) => Some(Value::Null),
        Some(v @ (Value::Bool(_) | Value::Number(_) | Value::String(_))) => Some(v.clone()),
        Some(Value::Object(o)) if o.get("$literal").and_then(Value::as_str) == Some("undefined") => Some(Value::Null),
        Some(_) => None,
    }
}

/// THE FOLD IS ONLY AS LARGE AS THE SUBSTITUTION NEEDS: `true && x` is `x`, a `false` operand is `false`.
/// A wider fold would read conditions the gate readers already read.
fn fold(ast: &Value) -> Value {
    let kind = ast.get("k").and_then(|k| k.as_str());
    match kind {
        Some("Source") => {
            let mut out = ast.clone();
            out["ast"] = fold(&ast["ast"]);
            out
        }
        Some("Not") => {
            let inner = fold(&ast["expr"]);
            if literal(&inner, true) {
                json!({"k": "Literal", "v": false})
            } else if literal(&inner, false) {
                json!({"k": "Literal", "v": true})
            } else {
                json!({"k": "Not", "expr": inner})
            }
        }
        Some("Binary") if ast.get("op").and_then(|o| o.as_str()) == Some("&&") => {
            let (l, r) = (fold(&ast["left"]), fold(&ast["right"]));
            if literal(&l, false) || literal(&r, false) {
                json!({"k": "Literal", "v": false})
            } else if literal(&l, true) {
                r
            } else if literal(&r, true) {
                l
            } else {
                json!({"k": "Binary", "op": "&&", "left": l, "right": r})
            }
        }
        _ => ast.clone(),
    }
}

/// The ast with every read of one of `inputs` (member row -> its replacement) replaced; whether any was.
fn substitute(ast: &Value, inputs: &IndexMap<String, Value>, hit: &mut bool) -> Value {
    match ast {
        Value::Object(o) => {
            let implicit = o.get("receiver").and_then(|r| r.get("k")).and_then(|k| k.as_str()) == Some("Implicit");
            let row = o.get("target").and_then(|t| t.get("row")).and_then(|r| r.as_str());
            if let (true, Some(Value::String(k)), Some(with)) = (implicit, o.get("k"), row.and_then(|r| inputs.get(r))) {
                if k == "Read" {
                    *hit = true;
                    return with.clone();
                }
            }
            Value::Object(o.iter().map(|(k, v)| (k.clone(), substitute(v, inputs, hit))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| substitute(v, inputs, hit)).collect()),
        other => other.clone(),
    }
}

pub fn bound_conds(store: &Store<'_>) -> Bound {
    let mut out = Bound::default();
    let polarity = super::gate_values::class_polarity(store);
    let written = written_members(store);
    let defined = bound_inputs(store, &written);
    let set = instance_writes(store);
    // class -> (input alias -> member id, initializer literal)
    let mut inputs: IndexMap<String, IndexMap<String, (String, Option<Value>)>> = IndexMap::default();
    for m in store.table("members").iter() {
        let (Some(id), Some(class)) = (cell(m, "id"), cell(m, "class")) else { continue };
        if cell(m, "kind").as_deref() != Some("property") || set.own.contains(&id) || defined.contains_key(&id) {
            continue;
        }
        let Some(alias) = input_alias(m) else { continue };
        let init = initial(m);
        inputs.entry(class).or_default().insert(alias, (id, init));
    }
    if inputs.is_empty() {
        return out;
    }
    let expressions = store.table("expressions");
    let mut tree_of_binding: IndexMap<String, &Row> = IndexMap::default();
    let mut by_id: IndexMap<String, &Row> = IndexMap::default();
    for e in expressions.iter() {
        if let Some(b) = cell(e, "binding") {
            tree_of_binding.insert(b, e);
        }
        if let Some(id) = cell(e, "id") {
            by_id.insert(id, e);
        }
    }
    // (node, input name) -> the one tree bound there; two bindings of one name are read as none.
    let mut bound: IndexMap<(String, String), Option<Value>> = IndexMap::default();
    for b in store.table("bindings").iter() {
        let (Some(id), Some(node), Some(name)) = (cell(b, "id"), cell(b, "node"), cell(b, "name")) else { continue };
        if cell(b, "kind").as_deref() != Some("input") {
            continue;
        }
        let tree = tree_of_binding.get(&id).and_then(|e| e.get("ast")).filter(|a| !a.is_null()).map(|a| unwrap(a).clone());
        let key = (node, name);
        let slot = if bound.contains_key(&key) { None } else { tree };
        bound.insert(key, slot);
    }
    let mut gates_of: IndexMap<String, Vec<(String, &Row)>> = IndexMap::default();
    let gates = store.table("gates");
    for g in gates.iter() {
        if let (Some(id), Some(comp)) = (cell(g, "id"), cell(g, "component")) {
            gates_of.entry(comp).or_default().push((id, g));
        }
    }
    // class -> creating function -> how many edges it draws into the class
    let mut creators: IndexMap<String, IndexMap<String, usize>> = IndexMap::default();
    let creator_of = |r: &Row| if cell(r, "node").is_none() { cell(r, "call").and_then(|c| set.fn_of_call.get(&c).cloned()) } else { None };
    for r in store.table("renders").iter() {
        if let (Some(class), Some(f)) = (cell(r, "to_class"), creator_of(r)) {
            *creators.entry(class).or_default().entry(f).or_default() += 1;
        }
    }
    for r in store.table("renders").iter() {
        let (Some(id), Some(to), Some(class)) = (cell(r, "id"), cell(r, "to"), cell(r, "to_class")) else { continue };
        let (Some(own), Some(gs)) = (inputs.get(&class), gates_of.get(&to)) else { continue };
        let (node, creator) = (cell(r, "node"), creator_of(r));
        let under = cell(r, "call").and_then(|c| set.branch_of_call.get(&c).cloned());
        // member row -> this edge's value for it
        let mut with: IndexMap<String, Value> = IndexMap::default();
        for (alias, (member, init)) in own {
            let value = match (&node, &creator) {
                (Some(node), _) if !written.contains(member) => match bound.get(&(node.clone(), alias.clone())) {
                    Some(tree) => tree.clone(),
                    None => init.as_ref().map(|v| json!({"k": "Literal", "v": v})),
                },
                (None, Some(f)) => set.on_edge(member, alias, init.as_ref(), f, under.as_deref(), &creators[&class]).filter(|v| !set.narrowed.contains(member) || literal(v, false)),
                _ => None,
            };
            if let Some(v) = value {
                with.insert(member.clone(), v);
            }
        }
        if with.is_empty() {
            continue;
        }
        for (gid, g) in gs {
            let Some(ast) = cell(g, "expression").and_then(|x| by_id.get(&x).and_then(|e| e.get("ast")).filter(|a| !a.is_null()).cloned()) else { continue };
            // ONLY A GATE THAT HIDES ITS ELEMENT MAKES A WAY NO WAY: `*ngIf`, an `@if` branch, `[hidden]`, a class a
            // stylesheet proves hiding. A `gateInputs` property (`[disabled]`), a style, `*ngFor`, `*ngSwitchCase` at
            // `false` hide nothing - the edge then carries no condition for that gate (#37).
            let kind = cell(g, "gate_kind");
            let hides = matches!(polarity.get(gid), Some(super::gate_values::Polarity::Hides | super::gate_values::Polarity::Shown(_)))
                || kind.as_deref() == Some("hidden")
                || super::key_dead::renders_at_false(kind.as_deref(), cell(g, "name").as_deref());
            // A CLASS GATE: the element shows while it is FALSE when its class hides; no condition when it only styles.
            let ast = match polarity.get(gid) {
                Some(super::gate_values::Polarity::Hides) => json!({"k": "Not", "expr": ast}),
                Some(super::gate_values::Polarity::Styles) => continue,
                Some(super::gate_values::Polarity::Shown(c)) => c.clone(),
                None => ast,
            };
            let mut hit = false;
            let folded = fold(&substitute(&ast, &with, &mut hit));
            if !hit {
                continue;
            }
            let dead = literal(unwrap(&folded), false);
            if dead && !hides {
                continue;
            }
            let link = format!("bind:{id}#{gid}");
            out.component_of_gate.insert(gid.clone(), to.clone());
            if dead {
                out.dead.insert(link);
            } else {
                out.conds.insert(link, (folded, false));
            }
        }
    }
    out
}

/// A `setInput` whose name is no literal: `*` is no input's name.
const ANY_NAME: &str = "*";

/// What code writes into the inputs of the components it creates (#147).
#[derive(Default)]
struct InstanceWrites {
    /// Input member id -> every write from outside its class: the function it sits in, and its value when that is a literal
    /// written unconditionally.
    /// written unconditionally but for the branch it sits under, and that branch.
    writes: IndexMap<String, Vec<(String, Option<Value>, Option<String>)>>,
    /// Members their own class writes, which no binding defines.
    own: crate::rows::IndexSet<String>,
    /// Inputs their own class writes ONLY as `this.f = this.f && ..`: a falsy value handed in stays falsy, a truthy one is no
    /// longer known.
    narrowed: crate::rows::IndexSet<String>,
    /// Input names some `setInput` call passes.
    by_name: crate::rows::IndexSet<String>,
    /// Call id -> the function it sits in.
    fn_of_call: IndexMap<String, String>,
    /// Call id -> the innermost branch it sits under.
    branch_of_call: IndexMap<String, String>,
}

impl InstanceWrites {
    /// The value a code edge from the creating function `f` gives the input `member`, or nothing when it is not proved.
    /// A WRITE UNDER THE BRANCH THE CREATING CALL SITS UNDER (`under`) is as unconditional as the edge itself: `if (byMail) { const m =
    /// create(..); m.flag = false; }` (#147).
    fn on_edge(&self, member: &str, alias: &str, init: Option<&Value>, f: &str, under: Option<&str>, creators: &IndexMap<String, usize>) -> Option<Value> {
        if creators.get(f) != Some(&1) || self.by_name.contains(alias) || self.by_name.contains(ANY_NAME) {
            return None;
        }
        let writes = self.writes.get(member).map(Vec::as_slice).unwrap_or_default();
        let mut here = writes.iter().filter(|(w, _, _)| w == f);
        match (here.next(), here.next()) {
            (Some((_, Some(v), b)), None) if b.as_deref() == under => Some(json!({"k": "Literal", "v": v})),
            (Some(_), _) => None,
            (None, _) if writes.iter().all(|(w, _, _)| creators.contains_key(w)) => init.map(|v| json!({"k": "Literal", "v": v})),
            (None, _) => None,
        }
    }
}

fn instance_writes(store: &Store<'_>) -> InstanceWrites {
    let mut out = InstanceWrites::default();
    let class_of: IndexMap<String, String> =
        store.table("members").iter().filter_map(|m| Some((cell(m, "id")?, cell(m, "class")?))).collect();
    for a in store.table("assignments").iter() {
        let Some(target) = cell(a, "target_id") else { continue };
        if cell(a, "class").is_some() && cell(a, "class") == class_of.get(&target).cloned() {
            if narrows(a) {
                out.narrowed.insert(target);
            } else {
                out.own.insert(target);
            }
            continue;
        }
        let plain = cell(a, "operator").as_deref() == Some("=") && ["loop", "case", "choices"].iter().all(|k| a.get(*k).is_none_or(Value::is_null));
        let value = a.get("value").filter(|v| plain && matches!(v, Value::Bool(_) | Value::Number(_) | Value::String(_))).cloned();
        out.writes.entry(target).or_default().push((cell(a, "member").unwrap_or_default(), value, cell(a, "branch")));
    }
    let own = std::mem::take(&mut out.own);
    out.narrowed.retain(|m| !own.contains(m));
    out.own = own;
    for c in store.table("calls").iter() {
        if let (Some(id), Some(f)) = (cell(c, "id"), cell(c, "member")) {
            if let Some(b) = cell(c, "branch") {
                out.branch_of_call.insert(id.clone(), b);
            }
            out.fn_of_call.insert(id, f);
        }
        if cell(c, "method").as_deref() == Some("setInput") {
            // A NAME NO LITERAL SPELLS may be any input's.
            let name = c.get("args").and_then(|a| a.get(0)).and_then(Value::as_str).unwrap_or(ANY_NAME);
            out.by_name.insert(name.to_string());
        }
    }
    out
}

/// `this.f = this.f && ..` - the component's own write that can only keep a falsy `f` falsy (`await this.features.isOn(..)` on the right).
fn narrows(a: &Row) -> bool {
    let Some(target) = cell(a, "target") else { return false };
    let Some(value) = a.get("value") else { return false };
    let first = value.get("$operands").and_then(Value::as_array).and_then(|o| o.first());
    cell(a, "operator").as_deref() == Some("=")
        && value.get("$logic").and_then(Value::as_str) == Some("&&")
        && first.and_then(|f| f.get("$kind")).and_then(Value::as_str) == Some("PropertyAccessExpression")
        && first.and_then(|f| f.get("$expr")).and_then(Value::as_str) == Some(format!("this.{target}").as_str())
}

/// The links a way through `path` holds for `gates`, or nothing when one of its edges is no way.
pub fn bind_links<'a>(bound: &Bound, path: &RenderPath, gates: impl Iterator<Item = &'a str>) -> Option<Vec<String>> {
    let mut links = Vec::new();
    if bound.component_of_gate.is_empty() {
        return Some(links);
    }
    for g in gates {
        // THE EDGE INTO THE GATE'S COMPONENT: `edges[i - 1]` renders `hops[i]`. A gate in the root has none.
        let Some(comp) = bound.component_of_gate.get(g) else { continue };
        let Some(i) = path.hops.iter().position(|h| &**h == comp.as_str()).filter(|i| *i > 0) else { continue };
        let link = format!("bind:{}#{g}", path.edges[i - 1]);
        if bound.dead.contains(&link) {
            return None;
        }
        if bound.conds.contains_key(&link) {
            links.push(link);
        }
    }
    Some(links)
}
