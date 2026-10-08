//! WHAT A STRUCTURAL DIRECTIVE RESTRICTS — the same `gate_values` row, for the gate whose
//! comparison is not in the template.
//!
//! A restriction is read out of a `Binary` in the gate's own expression, and a directive
//! carries none: `<x *if-kind="{ kind: item.kind }">`
//! renders iff the selected kind is that one, but the constant lives in the
//! directive's CONSTRUCTOR and the comparison in its BASE CLASS, so the template holds only
//! an attribute and an object literal. Without this rule every such occurrence resolves to
//! NOTHING — which a reader folding `always_values` takes as "unrestricted", so a key
//! carrying a per-kind restriction would look renderable for every kind.
//!
//! ONE OF TWO. Here the constant is in the CLASS and the comparison in its base. THE MIRROR -
//! a template that hands the constant IN while the directive compares the field holding it
//! against a value of its own - leaves a bare `Read` where a `Binary` is needed and binds no
//! constant in a constructor, so neither this rule nor the comparison walk fired, and a few
//! gates on a large Angular workspace stated a provable restriction and published nothing. It is
//! the same machinery sourced from the other side, in `ts/gate_directive_inputs.rs`.
//!
//! NOTHING HERE IS PARSED OUT OF SOURCE — every step is a row the map already RESOLVED:
//!   `assignments.target_id`  the constructor's write resolved ACROSS the inheritance
//!                            boundary to the base class's declaration
//!   `members.type_ref`       both fields typed by the enum BY FILE, so the enum is
//!                            identified by declaration and not by a name many of them share
//!   `expressions.ast`        the comparison with BOTH sides' targets resolved — the
//!                            operator is READ
//!
//! THE TRAP — THE ASSIGNMENT ALONE IS NOT THE GATE. One directive assigns a constant of the
//! enum too, and its `@Input` takes a config type which declares NO member of that enum: the
//! value can never arrive, so the binding is dead code for its render decision. Such a
//! directive can carry most of the occurrences, and reading it as a restriction attributes keys to a kind
//! they do not belong to — keys that then came out gated on two kinds at once, which is
//! unsatisfiable. The separating test is DECLARED and needs no method body: the input's type
//! must declare a member of the bound enum.
//!
//! WHY THE OCCURRENCE'S OBJECT LITERAL IS CHECKED, NOT JUST THE CLASS. The base renders
//! unconditionally when an override flag is set, BEFORE the comparison is reached,
//! so the constant restricts only an occurrence that cannot take that escape. Modelling
//! which branch dominates which would mean walking the method body; the map instead reads
//! what the CALL SITE supplies, which is in the gate's own expression: a literal setting
//! exactly the one input member typed by the enum can reach no other decision, and anything
//! else is `unknown`.
//!
//! EVERY UNRESOLVABLE CASE IS `unknown`, NEVER ABSENT — the honesty rule of the table it
//! writes into. The two exceptions return nothing at all because they are not claims of
//! uncertainty but PROOFS of irrelevance: a bound constant whose enum no input can carry,
//! and one no comparison reads, restrict nothing.

use super::astreads::unwrap;
use super::gate_values::{EnumIndex, MemberEnums};
use super::jsstr::sort_key;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

#[path = "gate_directives_index.rs"]
mod index;
use index::*;

#[path = "ts/gate_directive_inputs.rs"]
mod inputs;

/// The `@Input` members typed by the enum, on the class or any ancestor.
///
/// The INTERFACE is matched by name — a setter parameter is recorded as a type STRING with no
/// declaring file — but the member's own enum is resolved by declaration through `mem.typed`,
/// so a same-named config interface elsewhere contributes a member only if it is typed by
/// this very enum.
fn input_keys_of(
    members_by_class: &IndexMap<String, Vec<&Row>>,
    type_members: &IndexMap<String, Vec<&Row>>,
    mem: &MemberEnums,
    chain: &[Class],
    enum_id: &str,
) -> Vec<String> {
    let mut keys: IndexSet<String> = IndexSet::new();
    for cls in chain {
        for m in members_by_class.get(&cls.id).map(Vec::as_slice).unwrap_or(&[]) {
            if str_of(m.get("kind")).as_deref() != Some("setter") {
                continue;
            }
            let Some(Value::Array(params)) = m.get("params") else { continue };
            for p in params {
                let Some(ty) = p.as_object().and_then(|p| str_of(p.get("type"))) else { continue };
                for tm in type_members.get(&ty).map(Vec::as_slice).unwrap_or(&[]) {
                    let Some(id) = id_of(tm, "id") else { continue };
                    if mem.typed.get(&id).map(String::as_str) == Some(enum_id)
                        && let Some(name) = str_of(tm.get("name"))
                    {
                        keys.insert(name);
                    }
                }
            }
        }
    }
    let mut out: Vec<String> = keys.into_iter().collect();
    out.sort();
    out
}

/// The keys the OCCURRENCE supplies; `None` for an input that is not an object literal at all.
pub fn literal_keys_of(ast: Option<&Value>) -> Option<Vec<String>> {
    let n = unwrap(ast?);
    let map = n.as_object()?;
    if map.get("k").and_then(|k| k.as_str()) != Some("Map") {
        return None;
    }
    let Some(Value::Array(keys)) = map.get("keys") else { return None };
    let mut out: Vec<String> = keys
        .iter()
        .filter_map(|k| k.as_object().and_then(|k| str_of(k.get("key"))))
        .collect();
    out.sort();
    Some(out)
}

/// What one directive CLASS proves, independent of any occurrence of it.
pub struct Facts {
    enum_id: String,
    bound: Vec<String>,
    conditional: bool,
    comparisons: Vec<Comparison>,
    input_keys: Vec<String>,
}

/// ONE OCCURRENCE, DECIDED. `facts` is what the class proves; `literal_keys` is what the
/// call site supplies.
fn directive_restriction(facts: &Facts, literal_keys: Option<&Vec<String>>) -> Vec<Value> {
    // NOT uncertainty — proof that the binding cannot reach a render decision. See the
    // header's two exceptions.
    if facts.input_keys.is_empty() || facts.comparisons.is_empty() {
        return Vec::new();
    }
    // Deduplicated by dimension: a later comparison on a dimension already seen REPLACES the
    // value while keeping the first one's position, which is what building a map from
    // entries does.
    let mut by_dim: IndexMap<&str, &Comparison> = IndexMap::new();
    for c in &facts.comparisons {
        by_dim.insert(c.dim.as_str(), c);
    }
    let unknown: Vec<Value> = by_dim
        .values()
        .map(|c| json!({"enum": facts.enum_id, "dim": c.dim, "row": c.row, "op": "unknown"}))
        .collect();

    if facts.conditional
        || facts.bound.len() != 1
        || facts.comparisons.len() != 1
        || facts.input_keys.len() != 1
    {
        return unknown;
    }
    match literal_keys {
        Some(keys) if keys.len() == 1 && keys[0] == facts.input_keys[0] => {}
        _ => return unknown,
    }
    let c = &facts.comparisons[0];
    vec![json!({
        "enum": facts.enum_id, "dim": c.dim, "row": c.row,
        "op": if EQ.contains(&c.op.as_str()) { "in" } else { "not_in" },
        "value": facts.bound[0],
    })]
}

struct Ctx<'a> {
    mem: &'a MemberEnums,
    idx: &'a EnumIndex,
    midx: MemberIdx,
    cidx: ClassIdx,
    expr_by_file: IndexMap<String, Vec<&'a Row>>,
    assignments_by_class: IndexMap<String, Vec<&'a Row>>,
    members_by_class: IndexMap<String, Vec<&'a Row>>,
    class_of_member: IndexMap<String, String>,
    type_members: IndexMap<String, Vec<&'a Row>>,
}

fn facts_for(ctx: &Ctx<'_>, chain: &[Class]) -> Vec<Facts> {
    let mut by_field: IndexMap<String, (String, IndexSet<String>, bool)> = IndexMap::new();
    // NEAREST BINDER WINS, which is what the instance ends up holding: the base constructor
    // runs first and the subclass overwrites it, so an ancestor's constant is not a second
    // value on the same field. The chain is walked at all because the binding need not be on
    // the concrete class. A field the nearer class assigns something UNREADABLE also closes
    // it — the ancestor's constant no longer survives.
    let mut nearest: IndexMap<String, String> = IndexMap::new();
    for cls in chain {
        for a in ctx.assignments_by_class.get(&cls.id).map(Vec::as_slice).unwrap_or(&[]) {
            let Some(target) = id_of(a, "target_id") else { continue };
            let Some(value) = a.get("value").filter(|v| !v.is_null()) else { continue };
            nearest.entry(target.clone()).or_insert_with(|| cls.id.clone());
            if nearest.get(&target).map(String::as_str) != Some(cls.id.as_str()) {
                continue;
            }
            let Some(member) = value.as_object().and_then(|v| str_of(v.get("$enum"))) else {
                continue;
            };
            let Some((name, rest)) = member.split_once('.') else { continue };
            let Some(enum_id) = ctx.mem.typed.get(&target) else { continue };
            let Some(en) = ctx.idx.by_id.get(enum_id) else { continue };
            if en.name.as_deref() != Some(name) || !en.domain.contains(rest) {
                continue;
            }
            let slot = by_field
                .entry(target)
                .or_insert_with(|| (enum_id.clone(), IndexSet::new(), false));
            slot.1.insert(rest.to_string());
            // An assignment under an `if` or a `case` binds the constant CONDITIONALLY, so
            // the class no longer proves one value reaches the comparison.
            if truthy(a.get("branch")) || truthy(a.get("case")) {
                slot.2 = true;
            }
        }
    }

    let mut out = Vec::new();
    for (field, (enum_id, bound_set, conditional)) in by_field {
        let Some(owner) = ctx.class_of_member.get(&field) else { continue };
        let Some(declaring) = ctx.cidx.by_id.get(owner) else { continue };
        let mut bound: Vec<String> = bound_set.into_iter().collect();
        bound.sort();
        out.push(Facts {
            comparisons: comparisons_in(
                &ctx.expr_by_file,
                ctx.mem,
                &ctx.midx,
                declaring,
                &field,
                &enum_id,
            ),
            input_keys: input_keys_of(
                &ctx.members_by_class,
                &ctx.type_members,
                ctx.mem,
                chain,
                &enum_id,
            ),
            enum_id,
            bound,
            conditional,
        });
    }
    out
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DirectiveStats {
    pub classes: usize,
    pub inert: usize,
    pub gates: usize,
    pub resolved: usize,
    pub unknown: usize,
}

/// Gate id -> the restrictions its DIRECTIVE proves, for every structural gate whose
/// selector resolves to a class that binds an enum constant.
///
/// `stats` is reported by the closure build: a rule that silently stops matching looks
/// exactly like a frontend that stopped using directives.
pub fn directive_restrictions(
    store: &Store<'_>,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> (IndexMap<String, Vec<Value>>, DirectiveStats) {
    let expressions = store.table("expressions");
    let assignments = store.table("assignments");
    let members = store.table("members");
    let interfaces = store.table("interfaces");
    let type_members_rows = store.table("type_members");
    let selector_index = store.table("selector_index");
    let gates = store.table("gates");

    let mut expr_by_file: IndexMap<String, Vec<&Row>> = IndexMap::new();
    let mut expr_by_id: IndexMap<String, &Row> = IndexMap::new();
    for e in expressions.iter() {
        if let Some(file) = id_of(e, "file") {
            expr_by_file.entry(file).or_default().push(e);
        }
        if let Some(id) = id_of(e, "id") {
            expr_by_id.insert(id, e);
        }
    }
    let mut assignments_by_class: IndexMap<String, Vec<&Row>> = IndexMap::new();
    for a in assignments.iter() {
        if let Some(class) = id_of(a, "class") {
            assignments_by_class.entry(class).or_default().push(a);
        }
    }
    let mut members_by_class: IndexMap<String, Vec<&Row>> = IndexMap::new();
    let mut class_of_member: IndexMap<String, String> = IndexMap::new();
    for m in members.iter() {
        let Some(id) = id_of(m, "id") else { continue };
        if let Some(class) = id_of(m, "class") {
            class_of_member.insert(id, class.clone());
            members_by_class.entry(class).or_default().push(m);
        }
    }
    let mut interface_name: IndexMap<String, String> = IndexMap::new();
    for i in interfaces.iter() {
        if let (Some(id), Some(name)) = (id_of(i, "id"), str_of(i.get("name"))) {
            interface_name.insert(id, name);
        }
    }
    let mut type_members: IndexMap<String, Vec<&Row>> = IndexMap::new();
    for tm in type_members_rows.iter() {
        let Some(owner) = id_of(tm, "owner") else { continue };
        let Some(name) = interface_name.get(&owner) else { continue };
        type_members.entry(name.clone()).or_default().push(tm);
    }
    // A SELECTOR CAN NAME SEVERAL CLASSES, and the join this replaces produced one row per
    // class. Keeping only one of them would decide a gate by whichever class happened to be
    // indexed last.
    let mut classes_of_selector: IndexMap<String, Vec<String>> = IndexMap::new();
    for si in selector_index.iter() {
        let Some(selector) = str_of(si.get("selector")) else { continue };
        if let Some(class) = id_of(si, "class") {
            classes_of_selector.entry(selector).or_default().push(class);
        }
    }

    let ctx = Ctx {
        mem,
        idx,
        midx: member_index(store),
        cidx: class_index(store),
        expr_by_file,
        assignments_by_class,
        members_by_class,
        class_of_member,
        type_members,
    };

    let (branches, calls, returns) =
        (store.table("branches"), store.table("calls"), store.table("returns"));
    let mirror = inputs::Mirror::new(store, &assignments, &expressions, &branches, &calls, &returns);
    let mut per_input: IndexMap<(String, String), Vec<inputs::Carried>> = IndexMap::new();

    let mut per_class: IndexMap<String, Vec<Facts>> = IndexMap::new();
    let mut out: IndexMap<String, Vec<Value>> = IndexMap::new();
    let mut stats = DirectiveStats::default();
    for g in gates.iter() {
        if str_of(g.get("gate_kind")).as_deref() != Some("structural") {
            continue;
        }
        let name = str_of(g.get("name")).unwrap_or_default();
        let selector = format!("[{name}]");
        let classes = classes_of_selector.get(&selector).cloned().unwrap_or_default();
        let expr = id_of(g, "expression").and_then(|id| expr_by_id.get(&id).copied());
        let gate_ast = expr.and_then(|e| e.get("ast"));
        for cls in classes.iter().cloned() {
            if !per_class.contains_key(&cls) {
                let found = facts_for(&ctx, &chain_of(&ctx.cidx, &cls));
                if !found.is_empty() {
                    stats.classes += 1;
                    if !found
                        .iter()
                        .any(|f| !f.input_keys.is_empty() && !f.comparisons.is_empty())
                    {
                        stats.inert += 1;
                    }
                }
                per_class.insert(cls.clone(), found);
            }
            let facts = &per_class[&cls];
            if facts.is_empty() {
                continue;
            }
            let keys = literal_keys_of(gate_ast);
            let mut rows = Vec::new();
            for f in facts {
                rows.extend(directive_restriction(f, keys.as_ref()));
            }
            if rows.is_empty() {
                continue;
            }
            stats.gates += 1;
            for r in &rows {
                if str_of(r.get("op")).as_deref() == Some("unknown") {
                    stats.unknown += 1;
                } else {
                    stats.resolved += 1;
                }
            }
            if let Some(id) = id_of(g, "id") {
                out.insert(id, rows);
            }
        }
        // THE MIRROR - the template hands the constant in, see `inputs` - adds to whatever the
        // rule above decided for the same gate, from every class the selector names.
        let mut mirrored = Vec::new();
        for cls in &classes {
            let carried = per_input
                .entry((cls.clone(), name.clone()))
                .or_insert_with(|| inputs::carried(&ctx, &mirror, &chain_of(&ctx.cidx, cls), &name));
            for f in carried.iter() {
                mirrored.extend(inputs::mirror_restriction(&ctx, f, gate_ast));
            }
        }
        if !mirrored.is_empty()
            && let Some(id) = id_of(g, "id")
        {
            stats.gates += 1;
            for r in &mirrored {
                if str_of(r.get("op")).as_deref() == Some("unknown") {
                    stats.unknown += 1;
                } else {
                    stats.resolved += 1;
                }
            }
            out.entry(id).or_default().extend(mirrored);
        }
    }
    (out, stats)
}

#[cfg(test)]
#[allow(non_snake_case)]
#[path = "gate_directives_tests.rs"]
mod tests;
