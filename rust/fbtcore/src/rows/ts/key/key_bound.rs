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

use super::astreads::unwrap;
use super::closure::RenderPath;
use super::gate_input_flags::{bound_inputs, input_alias, written_members};
use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::{json, Value};

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// What the ways read: the per-edge conditions, the edges that are no way, and where each gate sits.
#[derive(Default)]
pub struct Bound {
    /// `bind:<render>#<gate>` -> the gate's condition with the edge's own binding in place of the flag.
    pub conds: IndexMap<String, (Value, bool)>,
    /// Edges on which the gate's condition is the literal `false`.
    pub dead: indexmap::IndexSet<String>,
    /// Gate id -> its component, for the gates above only.
    pub component_of_gate: IndexMap<String, String>,
}

fn literal(ast: &Value, v: bool) -> bool {
    ast.get("k").and_then(|k| k.as_str()) == Some("Literal") && ast.get("v") == Some(&Value::Bool(v))
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
    let written = written_members(store);
    let defined = bound_inputs(store, &written);
    // class -> (input alias -> member id, initializer literal)
    let mut inputs: IndexMap<String, IndexMap<String, (String, Option<Value>)>> = IndexMap::new();
    for m in store.table("members").iter() {
        let (Some(id), Some(class)) = (cell(m, "id"), cell(m, "class")) else { continue };
        if cell(m, "kind").as_deref() != Some("property") || written.contains(&id) || defined.contains_key(&id) {
            continue;
        }
        let Some(alias) = input_alias(m) else { continue };
        let init = m.get("value").filter(|v| matches!(v, Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Null)).cloned();
        inputs.entry(class).or_default().insert(alias, (id, init));
    }
    if inputs.is_empty() {
        return out;
    }
    let expressions = store.table("expressions");
    let mut tree_of_binding: IndexMap<String, &Row> = IndexMap::new();
    let mut by_id: IndexMap<String, &Row> = IndexMap::new();
    for e in expressions.iter() {
        if let Some(b) = cell(e, "binding") {
            tree_of_binding.insert(b, e);
        }
        if let Some(id) = cell(e, "id") {
            by_id.insert(id, e);
        }
    }
    // (node, input name) -> the one tree bound there; two bindings of one name are read as none.
    let mut bound: IndexMap<(String, String), Option<Value>> = IndexMap::new();
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
    let mut gates_of: IndexMap<String, Vec<(String, &Row)>> = IndexMap::new();
    let gates = store.table("gates");
    for g in gates.iter() {
        if let (Some(id), Some(comp)) = (cell(g, "id"), cell(g, "component")) {
            gates_of.entry(comp).or_default().push((id, g));
        }
    }
    for r in store.table("renders").iter() {
        let (Some(id), Some(to), Some(class), Some(node)) = (cell(r, "id"), cell(r, "to"), cell(r, "to_class"), cell(r, "node")) else { continue };
        let (Some(own), Some(gs)) = (inputs.get(&class), gates_of.get(&to)) else { continue };
        // member row -> this edge's value for it
        let mut with: IndexMap<String, Value> = IndexMap::new();
        for (alias, (member, init)) in own {
            match bound.get(&(node.clone(), alias.clone())) {
                Some(Some(tree)) => with.insert(member.clone(), tree.clone()),
                Some(None) => continue,
                None => match init {
                    Some(v) => with.insert(member.clone(), json!({"k": "Literal", "v": v})),
                    None => continue,
                },
            };
        }
        if with.is_empty() {
            continue;
        }
        for (gid, g) in gs {
            let Some(ast) = cell(g, "expression").and_then(|x| by_id.get(&x).and_then(|e| e.get("ast")).filter(|a| !a.is_null()).cloned()) else { continue };
            let mut hit = false;
            let folded = fold(&substitute(&ast, &with, &mut hit));
            if !hit {
                continue;
            }
            let link = format!("bind:{id}#{gid}");
            out.component_of_gate.insert(gid.clone(), to.clone());
            if literal(unwrap(&folded), false) {
                out.dead.insert(link);
            } else {
                out.conds.insert(link, (folded, false));
            }
        }
    }
    out
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
