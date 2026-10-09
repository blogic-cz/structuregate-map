//! WHICH VALUES A GATE STILL PERMITS — every `===`/`!==` against an enum constant
//! resolved into a restriction over that enum's FINITE domain.
//!
//! `gate_features` answers "which capability must be granted". This answers a different
//! question with the same walk: `shadeID === shadeIDs.Alpha` does not require anything
//! to be switched on, it says the Alpha category is the only one that reaches this. Several
//! times more gates compare against a resolved enum constant than name a feature.
//!
//! THREE THINGS MAKE IT A SEPARATE TABLE:
//!
//! 1. THE POLARITY IS NOT THE SAME. A negated feature contributes nothing. A negated
//!    VALUE contributes plenty: two `!==` remove two of the categories and are a real
//!    restriction, because the domain is finite and the map has it.
//! 2. THE DIMENSION HAS TO BE NAMED. A gate may restrict the category and say nothing
//!    about the tier, so a row is (gate, enum, dimension).
//! 3. SOME COMPARISONS RESOLVE TO NOTHING, AND SILENCE MUST NOT READ AS "UNRESTRICTED".
//!    Comparing two runtime values both typed `TierIDs` gives a known DIMENSION and no
//!    value. That is recorded as `op='unknown'`, never omitted.
//!
//! ENUMS ARE IDENTIFIED BY DECLARATION, NEVER BY NAME: many distinct enum names
//! in a large frontend are declared in more than one file, so a name-keyed lookup would merge
//! two domains and report members the compared enum does not have.

use super::astreads::is_read;
use super::featurewalk::{necessary_either, OrderedSet};
use super::gate_predicates::{predicate_restriction, predicates};
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

const EQ: &[&str] = &["===", "=="];
const NE: &[&str] = &["!==", "!="];
/// Relational operators are recorded as `unknown` rather than turned into a range: an
/// ordering over enum members is not a fact the declaration supports.
const CMP: &[&str] = &[">", "<", ">=", "<="];

fn slash(v: Option<&Value>) -> String {
    match v {
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

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: Option<String>,
    /// The finite domain, in declaration order.
    pub domain: IndexSet<String>,
}

#[derive(Debug, Default)]
pub struct EnumIndex {
    pub by_id: IndexMap<String, EnumInfo>,
    /// Name -> the declarations that carry it, so a shared name never merges two domains.
    pub by_decl: IndexMap<String, Vec<(String, String)>>,
}

pub fn enum_index(store: &Store<'_>) -> EnumIndex {
    let mut file_path: IndexMap<String, Option<String>> = IndexMap::new();
    for f in store.table("files").iter() {
        if let Some(id) = id_of(f, "id") {
            file_path.insert(id, text(f, "path"));
        }
    }

    let mut idx = EnumIndex::default();
    for e in store.table("enums").iter() {
        let Some(Value::Array(members)) = e.get("members") else { continue };
        let domain: IndexSet<String> = members
            .iter()
            .filter_map(|m| m.as_object())
            .filter_map(|m| match m.get("name") {
                Some(Value::String(n)) if !n.is_empty() => Some(n.clone()),
                _ => None,
            })
            .collect();
        if domain.is_empty() {
            continue;
        }
        let path = slash(
            id_of(e, "file")
                .and_then(|f| file_path.get(&f).cloned().flatten())
                .map(Value::String)
                .as_ref(),
        );
        let name = text(e, "name");
        if let Some(id) = id_of(e, "id") {
            idx.by_id.insert(id.clone(), EnumInfo { name: name.clone(), domain });
            if let Some(name) = name {
                idx.by_decl.entry(name).or_default().push((path, id));
            }
        }
    }
    idx
}

/// `type_ref` names the declaring file ABSOLUTELY while `files.path` is FE-relative —
/// matched by suffix.
pub fn enum_of_type_ref(idx: &EnumIndex, name: &str, type_ref: Option<&Value>) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let reference = type_ref?.as_object()?;
    if reference.get("name").and_then(|n| n.as_str()) != Some(name) {
        return None;
    }
    let file = match reference.get("file") {
        Some(Value::String(f)) if !f.is_empty() => f.replace('\\', "/"),
        _ => return None,
    };
    for (path, id) in idx.by_decl.get(name)? {
        if file.ends_with(&format!("/{path}")) {
            return Some(id.clone());
        }
    }
    None
}

#[derive(Debug, Default)]
pub struct MemberEnums {
    /// `shadeIDs: typeof ShadeIDs` — the thing a template reads constants off.
    pub alias: IndexMap<String, String>,
    /// `shadeID: ShadeIDs` — the discriminator being compared.
    pub typed: IndexMap<String, String>,
}

/// Interface members are indexed too: `receiver.target.row` is a `tm:` id often in the
/// gate expressions.
pub fn member_enums(store: &Store<'_>, idx: &EnumIndex) -> MemberEnums {
    let mut out = MemberEnums::default();
    for table in ["members", "type_members"] {
        for m in store.table(table).iter() {
            let Some(declared) = text(m, "type") else { continue };
            let is_alias = declared.starts_with("typeof ");
            let name = if is_alias { &declared[7..] } else { declared.as_str() };
            let Some(en) = enum_of_type_ref(idx, name, m.get("type_ref")) else { continue };
            let Some(id) = id_of(m, "id") else { continue };
            if is_alias {
                out.alias.insert(id, en);
            } else {
                out.typed.insert(id, en);
            }
        }
    }
    out
}

/// The dotted path a Read chain spells, which is what a reader recognises the
/// discriminator by.
fn dotted(n: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut current = n;
    while is_read(current) {
        if let Some(Value::String(name)) = current.get("name") {
            parts.insert(0, name.clone());
        }
        match current.get("receiver") {
            Some(next) => current = next,
            None => break,
        }
    }
    parts.join(".")
}

fn target_row(n: &Value) -> Option<String> {
    match n.get("target")?.as_object()?.get("row") {
        Some(Value::String(r)) if !r.is_empty() => Some(r.clone()),
        _ => None,
    }
}

/// A Read off an ALIAS member is an enum CONSTANT: `shadeIDs.Alpha`.
///
/// So is a TypeScript read the resolver tied to the enum it DECLARES a member of
/// (`target.enum`, see `gate_lists::ts_rows`): `TierIDs.Small` has no alias member to go
/// through, and its target lands on the member's own declaration.
pub fn constant_of(n: &Value, mem: &MemberEnums, idx: &EnumIndex) -> Option<(String, String)> {
    if !is_read(n) {
        return None;
    }
    if let Some(Value::String(en)) = n.get("target").and_then(|t| t.get("enum")) {
        let name = n.get("name").and_then(|v| v.as_str())?;
        return idx.by_id.get(en)?.domain.contains(name).then(|| (en.clone(), name.to_string()));
    }
    let receiver = n.get("receiver")?;
    if !is_read(receiver) {
        return None;
    }
    let en = mem.alias.get(&target_row(receiver)?)?;
    let Some(Value::String(name)) = n.get("name") else { return None };
    if idx.by_id.get(en)?.domain.contains(name) {
        Some((en.clone(), name.clone()))
    } else {
        None
    }
}

/// A Read whose own declaration is TYPED by an enum is a discriminator.
pub fn dimension_of(n: &Value, mem: &MemberEnums) -> Option<(String, String, Option<String>)> {
    if !is_read(n) {
        return None;
    }
    let row = target_row(n)?;
    let en = mem.typed.get(&row)?;
    let dim = dotted(n);
    let dim = if dim.is_empty() {
        n.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()
    } else {
        dim
    };
    Some((en.clone(), dim, Some(row)))
}

/// A DIMENSION THE CHECKER COULD NOT TYPE IS STILL A DIMENSION.
///
/// A comparison off a config object resolves its constant perfectly and its left side not
/// at all, and requiring both sides to resolve DROPPED more than half of the comparisons
/// that carry a resolved constant.
///
/// `row` is left null to say the tie is by TEXT and not by declaration. Two identical
/// dotted paths in two components then share a row key, which the union fold can only
/// widen, never overstate.
pub fn untyped_dimension(n: &Value) -> Option<String> {
    if !is_read(n) {
        return None;
    }
    let dim = dotted(n);
    if dim.is_empty() { None } else { Some(dim) }
}

/// One comparison, under the polarity that reached it, as a restriction.
///
/// `!==` under a `!` is an `===` and is treated as one — the whole reason the walk passes
/// its polarity down.
pub fn restriction_of(
    node: &Value,
    negated: bool,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> Option<Value> {
    let object = node.as_object()?;
    if object.get("k").and_then(|k| k.as_str()) != Some("Binary") {
        return None;
    }
    let op = object.get("op").and_then(|o| o.as_str())?;
    let relational = CMP.contains(&op);
    if !EQ.contains(&op) && !NE.contains(&op) && !relational {
        return None;
    }

    let null = Value::Null;
    let sides = [object.get("left").unwrap_or(&null), object.get("right").unwrap_or(&null)];
    let con = sides.iter().find_map(|s| constant_of(s, mem, idx));
    let typed_dim = sides.iter().find_map(|s| dimension_of(s, mem));

    // The OTHER side of a resolved constant is the dimension even when nothing types it.
    let loose = con.as_ref().and_then(|_| {
        sides
            .iter()
            .find(|s| constant_of(s, mem, idx).is_none())
            .and_then(|s| untyped_dimension(s))
    });

    let (en, dim, row) = match (typed_dim, &con, loose) {
        (Some(d), _, _) => d,
        (None, Some((en, _)), Some(dim)) => (en.clone(), dim, None),
        _ => return None,
    };

    if relational || con.is_none() || con.as_ref().map(|(e, _)| e) != Some(&en) {
        return Some(json!({"enum": en, "dim": dim, "row": row, "op": "unknown"}));
    }
    let (_, value) = con.expect("checked above");
    let eq = EQ.contains(&op) != negated;
    Some(json!({
        "enum": en, "dim": dim, "row": row,
        "op": if eq { "in" } else { "not_in" },
        "value": value,
    }))
}

/// Restrictions collapse to ONE per (gate, enum, dimension) — see `gate_collapse`.
pub use super::gate_collapse::collapse;

#[derive(Debug, Default)]
pub struct GateValues {
    /// Gate id -> the component it sits in and its collapsed restrictions.
    pub map: IndexMap<String, (Option<String>, Vec<Value>)>,
    /// The same, for the conditions handed in that are not template gates.
    pub conds: IndexMap<String, (Option<String>, Vec<Value>)>,
}

/// Gate id -> its collapsed restrictions.
///
/// `extra` carries the restrictions the other three sources prove — a structural directive
/// binding a constant, a config object listing permitted members, and an NgSwitch arm.
/// None of those is a `Binary` the walk can read, and a consumer cannot tell which source
/// proved a restriction. A CALL to a proven membership predicate is read by the walk
/// itself, since its polarity is the walk's: see `gate_predicates`.
pub fn gate_values(
    store: &Store<'_>,
    extra: &IndexMap<String, Vec<Value>>,
) -> (GateValues, EnumIndex) {
    gate_values_with(store, extra, &IndexMap::new())
}

/// `gate_values`, plus the restrictions each of `conds` proves — a TypeScript condition
/// read by the SAME resolver, so an `if` and an `*ngIf` over one comparison agree.
pub fn gate_values_with(
    store: &Store<'_>,
    extra: &IndexMap<String, Vec<Value>>,
    conds: &IndexMap<String, (Value, bool)>,
) -> (GateValues, EnumIndex) {
    let stage = crate::trace::stage("gate_values: enums and predicates");
    let idx = enum_index(store);
    let mem = member_enums(store, &idx);
    let preds = predicates(store, &idx);
    drop(stage);
    let stage = crate::trace::stage("gate_values: lists");
    let lists = super::gate_lists::lists(store, &idx);
    let selectors = super::gate_selectors::Selectors::new(store, &idx);
    drop(stage);
    // A BARE BOOLEAN PROPERTY IS READ AS WHAT IT IS ASSIGNED, and a call to a function of one
    // return as that return - see `gate_props` - unless a reader above already reads the call.
    let claimed = |row: &str| preds.contains_key(row) || lists.claims(row);
    // THE PASSES v1.5.16 AND AFTER ADDED (calls through a return, directive lists, input flags) run in here.
    let stage = crate::trace::stage("gate_values: props");
    let props = super::gate_props::Props::new(store, &idx, &claimed);
    drop(stage);
    let mut leaf = |n: &Value, negated: bool| -> Vec<Value> {
        restriction_of(n, negated, &mem, &idx)
            .or_else(|| predicate_restriction(n, negated, &preds, &mem, &idx))
            .or_else(|| lists.restriction(n, negated, &mem, &idx))
            .or_else(|| selectors.restriction(n, negated))
            .into_iter()
            .collect()
    };
    let mut either = |l: Vec<Value>, r: Vec<Value>| super::gate_collapse::either(&l, &r, &idx);

    let expressions = store.table("expressions");
    let mut expr_by_id: IndexMap<String, &Row> = IndexMap::new();
    for e in expressions.iter() {
        if let Some(id) = id_of(e, "id") {
            expr_by_id.insert(id, e);
        }
    }

    let mut out = GateValues::default();
    let stage = crate::trace::stage("gate_values: gates");
    for g in store.table("gates").iter() {
        let Some(e) = id_of(g, "expression").and_then(|x| expr_by_id.get(&x)) else { continue };
        let Some(ast) = e.get("ast").filter(|a| !a.is_null()) else { continue };

        let mut found = OrderedSet::new();
        necessary_either(&props.expand(ast), false, &mut leaf, &mut either, &mut found);
        if let Some(id) = id_of(g, "id")
            && let Some(more) = extra.get(&id)
        {
            for r in more {
                found.add(r.clone());
            }
        }

        let items: Vec<Value> = found.iter().cloned().collect();
        let rows = collapse(&items, &idx);
        if !rows.is_empty()
            && let Some(id) = id_of(g, "id")
        {
            out.map.insert(id, (text(g, "component"), rows));
        }
    }
    drop(stage);
    let _conds = crate::trace::stage("gate_values: branch conditions");
    for (id, (ast, negated)) in conds {
        let mut found = OrderedSet::new();
        necessary_either(&props.expand(ast), *negated, &mut leaf, &mut either, &mut found);
        let rows = collapse(&found.iter().cloned().collect::<Vec<Value>>(), &idx);
        if !rows.is_empty() {
            out.conds.insert(id.clone(), (None, rows));
        }
    }
    (out, idx)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    /// An enum of five members, so "the smaller side" is decidable.
    fn idx_of(domain: &[&str]) -> EnumIndex {
        let mut idx = EnumIndex::default();
        idx.by_id.insert(
            "e:1".to_string(),
            EnumInfo {
                name: Some("ShadeIDs".to_string()),
                domain: domain.iter().map(|s| s.to_string()).collect(),
            },
        );
        idx
    }

    const FIVE: &[&str] = &["Alpha", "Beta", "Gamma", "Delta", "Other"];

    // ---------------------------------------------------------------------------
    // restriction_of, over the shapes a template actually spells
    // ---------------------------------------------------------------------------

    fn mem_of() -> MemberEnums {
        let mut mem = MemberEnums::default();
        mem.alias.insert("m:alias".to_string(), "e:1".to_string());
        mem.typed.insert("m:typed".to_string(), "e:1".to_string());
        mem
    }

    /// `shadeIDs.Alpha`
    fn constant(name: &str) -> Value {
        json!({"k": "Read", "name": name,
               "receiver": {"k": "Read", "name": "shadeIDs", "target": {"row": "m:alias"}}})
    }

    /// `shadeID`, declared with an enum type
    fn discriminator() -> Value {
        json!({"k": "Read", "name": "shadeID", "target": {"row": "m:typed"}})
    }

    fn binary(op: &str, left: Value, right: Value) -> Value {
        json!({"k": "Binary", "op": op, "left": left, "right": right})
    }

    #[test]
    fn an_equality_against_a_resolved_constant_is_an_in_restriction() {
        let node = binary("===", discriminator(), constant("Alpha"));
        let r = restriction_of(&node, false, &mem_of(), &idx_of(FIVE)).expect("a restriction");
        assert_eq!(r["op"], json!("in"));
        assert_eq!(r["value"], json!("Alpha"));
        assert_eq!(r["dim"], json!("shadeID"));
    }

    #[test]
    fn polarity_turns_an_inequality_into_an_equality() {
        // `!==` under a `!` is an `===`, which is the whole reason the walk passes its
        // polarity down.
        let node = binary("!==", discriminator(), constant("Alpha"));
        let plain = restriction_of(&node, false, &mem_of(), &idx_of(FIVE)).expect("a restriction");
        assert_eq!(plain["op"], json!("not_in"));
        let negated = restriction_of(&node, true, &mem_of(), &idx_of(FIVE)).expect("a restriction");
        assert_eq!(negated["op"], json!("in"));
    }

    #[test]
    fn a_relational_operator_is_unknown_rather_than_a_range() {
        // An ordering over enum members is not a fact the declaration supports.
        let node = binary(">", discriminator(), constant("Alpha"));
        let r = restriction_of(&node, false, &mem_of(), &idx_of(FIVE)).expect("a restriction");
        assert_eq!(r["op"], json!("unknown"));
    }

    #[test]
    fn a_known_dimension_with_no_resolvable_value_is_unknown_and_never_omitted() {
        // Two runtime values both typed by the enum: known dimension, no value. Treating
        // that as no restriction is the same defect as the id intersection.
        let node = binary("===", discriminator(), discriminator());
        let r = restriction_of(&node, false, &mem_of(), &idx_of(FIVE)).expect("a restriction");
        assert_eq!(r["op"], json!("unknown"));
    }

    #[test]
    fn a_constant_against_an_untyped_dimension_still_resolves() {
        // Requiring both sides to resolve dropped more than half of the comparisons carrying a
        // resolved constant. The enum comes from the CONSTANT; the dimension is the text.
        let untyped = json!({"k": "Read", "name": "id",
                             "receiver": {"k": "Read", "name": "config"}});
        let node = binary("===", untyped, constant("Alpha"));
        let r = restriction_of(&node, false, &mem_of(), &idx_of(FIVE)).expect("a restriction");
        assert_eq!(r["op"], json!("in"));
        assert_eq!(r["dim"], json!("config.id"));
        assert_eq!(r["row"], json!(null), "the tie is by text, not by declaration");
    }

    #[test]
    fn a_comparison_against_a_member_the_enum_does_not_have_is_not_a_constant() {
        let node = binary("===", discriminator(), constant("NotAMember"));
        let r = restriction_of(&node, false, &mem_of(), &idx_of(FIVE)).expect("a restriction");
        assert_eq!(r["op"], json!("unknown"), "the dimension is known, the value is not");
    }

    #[test]
    fn something_that_is_not_a_comparison_is_not_a_restriction() {
        assert!(restriction_of(&discriminator(), false, &mem_of(), &idx_of(FIVE)).is_none());
        assert!(restriction_of(&json!("text"), false, &mem_of(), &idx_of(FIVE)).is_none());
        let plus = binary("+", discriminator(), constant("Alpha"));
        assert!(restriction_of(&plus, false, &mem_of(), &idx_of(FIVE)).is_none());
    }

    #[test]
    fn an_enum_name_declared_twice_never_merges_two_domains() {
        // Many distinct enum names in a large frontend are declared in more than one file.
        let mut idx = EnumIndex::default();
        idx.by_id.insert("e:a".to_string(), EnumInfo {
            name: Some("Dup".to_string()), 
            domain: ["A"].iter().map(|s| s.to_string()).collect(),
        });
        idx.by_id.insert("e:b".to_string(), EnumInfo {
            name: Some("Dup".to_string()), 
            domain: ["B"].iter().map(|s| s.to_string()).collect(),
        });
        idx.by_decl.insert("Dup".to_string(), vec![
            ("one/dup.ts".to_string(), "e:a".to_string()),
            ("two/dup.ts".to_string(), "e:b".to_string()),
        ]);

        let tables: Map<String, Value> = json!({
            "members": [{"id": "m:1", "type": "Dup",
                         "type_ref": {"name": "Dup", "file": "/repo/two/dup.ts"}}],
            "type_members": []
        }).as_object().cloned().unwrap();
        let store = Store::from_payload(tables, "typescript");
        let mem = member_enums(&store, &idx);
        assert_eq!(mem.typed.get("m:1"), Some(&"e:b".to_string()),
                   "the declaring FILE decides, never the name");
    }
}
