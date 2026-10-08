//! WHAT AN `ngSwitch` ARM RESTRICTS — the `gate_values` row for the comparison NgSwitch
//! writes ITSELF.
//!
//! THE FOURTH SOURCE, and it is split across two gates. A restriction needs a `Binary`
//! node, and neither half of a switch is one: `ngSwitch="tier.TierID"` is a bare Read
//! (the DIMENSION, restricting nothing on its own) and `ngSwitchCase="TierIDs.Gamma"`
//! is a bare Read (the VALUE, naming no dimension). The comparison is written by NgSwitch,
//! in framework code no template carries — so every `ngSwitch` and `ngSwitchCase`
//! gate produced NO value row, and silence there reads as "unrestricted".
//!
//! WHAT IT COSTS without it: a key whose EVERY way in passes an `ngSwitchCase` naming a
//! resolved enum member has that dimension in neither `always_values` nor
//! `unreadable_values`. It asserts "resolved, and no tier
//! restriction" for content one variant alone renders, and a reader that
//! widens on silence takes it as shown for every tier.
//!
//! `ngSwitchDefault` IS STILL INVISIBLE HERE, and cannot be fixed from this file: it takes
//! no value, so it has no `expressions` row and therefore no `gates` row at all (its
//! `bindings` rows, no gates). Its arm is the COMPLEMENT of its siblings — a `not_in` — and
//! stating it needs the extractor to emit a gate for a valueless structural attribute
//! first. Named so the remaining hole is a known one rather than a clean-looking count.

use super::astreads::unwrap;
use super::gate_values::{constant_of, dimension_of, untyped_dimension, EnumIndex, MemberEnums};
use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::{json, Value};

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

/// The dimension one `ngSwitch` node discriminates.
#[derive(Clone, Debug, PartialEq)]
pub struct Dimension {
    /// None when only the case constant will name the enum.
    pub enum_id: Option<String>,
    pub dim: String,
    pub row: Option<String>,
}

/// The nearest enclosing `ngSwitch`'s dimension, or nothing.
///
/// THE SWITCH IS FOUND BY ANCESTRY, NOT BY THE PARENT LINK ALONE. Most cases sit
/// directly under their `ngSwitch` node; the rest are further down — measured to several
/// levels — because desugaring inserts wrappers and templates nest `<ng-container>`s
/// freely. The NEAREST enclosing switch is the one that matters: switches nest, and an
/// outer one discriminates a different dimension.
///
/// Cycle-guarded by id rather than a depth budget: a malformed parent link would otherwise
/// spin forever, and a cap would silently drop the legitimately deep cases this walk
/// exists for.
pub fn enclosing_dimension<'a>(
    node: &str,
    parent: &IndexMap<String, Option<String>>,
    dims: &'a IndexMap<String, Dimension>,
) -> Option<&'a Dimension> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(node.to_string());
    let mut current = parent.get(node).cloned().flatten();
    while let Some(id) = current {
        if seen.contains(&id) {
            return None;
        }
        if let Some(dim) = dims.get(&id) {
            return Some(dim);
        }
        seen.insert(id.clone());
        current = parent.get(&id).cloned().flatten();
    }
    None
}

/// ONE ARM, as a `gate_values` row — the whole decision, given a resolved dimension and
/// constant.
///
/// None only when NEITHER half named an enum: there is then no finite domain to state a
/// restriction over, and a row with no enum cannot be folded. Inventing the enum from the
/// text `reads` already holds is the name-match this tool exists to replace, so the arm is
/// counted and dropped instead.
///
/// AN UNRESOLVABLE ARM IS `unknown`, NEVER ABSENT. `*ngSwitchCase="'grid'"` restricts the
/// dimension to something the map cannot name; recording nothing would publish it as
/// unrestricted.
pub fn switch_arm(dim: &Dimension, con: Option<&(String, String)>) -> Option<Value> {
    let enum_id = dim
        .enum_id
        .clone()
        .or_else(|| con.map(|(e, _)| e.clone()))?;

    // A case whose value resolves to a DIFFERENT enum than the dimension is `unknown` and
    // not a restriction.
    match con {
        Some((found, value)) if *found == enum_id => Some(json!({
            "enum": enum_id, "dim": dim.dim, "row": dim.row,
            "op": "in", "values": [value],
        })),
        _ => Some(json!({"enum": enum_id, "dim": dim.dim, "row": dim.row, "op": "unknown"})),
    }
}

#[derive(Debug, Default)]
pub struct SwitchStats {
    pub switches: usize,
    pub gates: usize,
    pub resolved: usize,
    pub unknown: usize,
    pub unplaced: usize,
    pub nameless: usize,
}

/// Gate id -> the restriction its switch arm states, in `gate_values`' own row shape.
pub fn switch_restrictions(
    store: &Store<'_>,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> (IndexMap<String, Vec<Value>>, SwitchStats) {
    let expressions = store.table("expressions");
    let mut expr_by_id: IndexMap<String, &Row> = IndexMap::new();
    for e in expressions.iter() {
        if let Some(id) = id_of(e, "id") {
            expr_by_id.insert(id, e);
        }
    }

    let nodes = store.table("template_nodes");
    let mut parent: IndexMap<String, Option<String>> = IndexMap::new();
    for n in nodes.iter() {
        if let Some(id) = id_of(n, "id") {
            parent.insert(id, id_of(n, "parent"));
        }
    }

    let gates = store.table("gates");
    let ast_of = |g: &Row| -> Option<&Value> {
        let e = id_of(g, "expression").and_then(|x| expr_by_id.get(&x).copied())?;
        let ast = e.get("ast").filter(|a| !a.is_null())?;
        Some(unwrap(ast))
    };

    // THE ENUM COMES FROM WHICHEVER SIDE PROVES IT: the switch's own declaration when the
    // checker typed it, otherwise the case constant, with `row` left null to say the tie
    // is by TEXT.
    let mut dims: IndexMap<String, Dimension> = IndexMap::new();
    for g in gates.iter().filter(|g| text(g, "name").as_deref() == Some("ngSwitch")) {
        let Some(ast) = ast_of(g) else { continue };
        let Some(node) = id_of(g, "node") else { continue };
        if let Some((enum_id, dim, row)) = dimension_of(ast, mem) {
            dims.insert(node, Dimension { enum_id: Some(enum_id), dim, row });
        } else if let Some(dim) = untyped_dimension(ast) {
            dims.insert(node, Dimension { enum_id: None, dim, row: None });
        }
    }

    let mut out: IndexMap<String, Vec<Value>> = IndexMap::new();
    let mut stats = SwitchStats { switches: dims.len(), ..Default::default() };

    for g in gates.iter().filter(|g| text(g, "name").as_deref() == Some("ngSwitchCase")) {
        let Some(node) = id_of(g, "node") else {
            stats.unplaced += 1;
            continue;
        };
        let Some(dim) = enclosing_dimension(&node, &parent, &dims) else {
            stats.unplaced += 1;
            continue;
        };
        let con = ast_of(g).and_then(|ast| constant_of(ast, mem, idx));
        let Some(arm) = switch_arm(dim, con.as_ref()) else {
            stats.nameless += 1;
            continue;
        };
        stats.gates += 1;
        if arm["op"] == json!("in") {
            stats.resolved += 1;
        } else {
            stats.unknown += 1;
        }
        if let Some(id) = id_of(g, "id") {
            out.insert(id, vec![arm]);
        }
    }
    (out, stats)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use super::super::gate_values::{enum_index, member_enums};
    use serde_json::Map;

    fn dim_of(enum_id: Option<&str>) -> Dimension {
        Dimension {
            enum_id: enum_id.map(String::from),
            dim: "tier.TierID".to_string(),
            row: None,
        }
    }

    #[test]
    fn an_arm_naming_the_dimensions_own_enum_is_an_in_restriction() {
        let con = ("e:1".to_string(), "Gamma".to_string());
        let arm = switch_arm(&dim_of(Some("e:1")), Some(&con)).expect("an arm");
        assert_eq!(arm["op"], json!("in"));
        assert_eq!(arm["values"], json!(["Gamma"]));
    }

    #[test]
    fn an_arm_whose_value_does_not_resolve_is_unknown_and_never_absent() {
        // `*ngSwitchCase="'grid'"` restricts the dimension to something the map cannot
        // name; recording nothing would publish it as unrestricted.
        let arm = switch_arm(&dim_of(Some("e:1")), None).expect("an arm");
        assert_eq!(arm["op"], json!("unknown"));
    }

    #[test]
    fn an_arm_resolving_to_a_DIFFERENT_enum_is_unknown() {
        let con = ("e:OTHER".to_string(), "Something".to_string());
        let arm = switch_arm(&dim_of(Some("e:1")), Some(&con)).expect("an arm");
        assert_eq!(arm["op"], json!("unknown"));
    }

    #[test]
    fn the_case_constant_may_supply_the_enum_the_switch_could_not() {
        let con = ("e:1".to_string(), "Gamma".to_string());
        let arm = switch_arm(&dim_of(None), Some(&con)).expect("an arm");
        assert_eq!(arm["enum"], json!("e:1"));
        assert_eq!(arm["op"], json!("in"));
    }

    #[test]
    fn neither_half_naming_an_enum_is_dropped_rather_than_invented() {
        // There is no finite domain to state a restriction over, and inventing the enum
        // from the text is the name-match this tool exists to replace.
        assert!(switch_arm(&dim_of(None), None).is_none());
    }

    fn parents(pairs: &[(&str, Option<&str>)]) -> IndexMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(String::from)))
            .collect()
    }

    #[test]
    fn a_case_several_levels_below_its_switch_still_finds_it() {
        // Some cases are not direct children - measured to several levels - because
        // desugaring inserts wrappers and templates nest containers freely.
        let parent = parents(&[("n:case", Some("n:w1")), ("n:w1", Some("n:w2")),
                               ("n:w2", Some("n:switch")), ("n:switch", None)]);
        let mut dims = IndexMap::new();
        dims.insert("n:switch".to_string(), dim_of(Some("e:1")));
        assert_eq!(enclosing_dimension("n:case", &parent, &dims), Some(&dim_of(Some("e:1"))));
    }

    #[test]
    fn the_NEAREST_enclosing_switch_wins_because_switches_nest() {
        let parent = parents(&[("n:case", Some("n:inner")), ("n:inner", Some("n:outer")),
                               ("n:outer", None)]);
        let inner = Dimension { enum_id: Some("e:inner".into()), dim: "b".into(), row: None };
        let outer = Dimension { enum_id: Some("e:outer".into()), dim: "a".into(), row: None };
        let mut dims = IndexMap::new();
        dims.insert("n:outer".to_string(), outer);
        dims.insert("n:inner".to_string(), inner.clone());
        assert_eq!(enclosing_dimension("n:case", &parent, &dims), Some(&inner));
    }

    #[test]
    fn a_cycle_in_the_parent_links_ends_the_walk_rather_than_spinning() {
        let parent = parents(&[("n:a", Some("n:b")), ("n:b", Some("n:a"))]);
        assert_eq!(enclosing_dimension("n:a", &parent, &IndexMap::new()), None);
    }

    #[test]
    fn a_case_under_no_switch_at_all_is_unplaced() {
        let parent = parents(&[("n:case", None)]);
        assert_eq!(enclosing_dimension("n:case", &parent, &IndexMap::new()), None);
    }

    #[test]
    fn the_whole_pass_reads_a_switch_and_its_arm_out_of_a_store() {
        let tables: Map<String, Value> = json!({
            "files": [{"id": "fl:1", "path": "app/t.ts"}],
            "enums": [{"id": "e:1", "name": "TierIDs", "file": "fl:1",
                       "members": [{"name": "Gamma"}, {"name": "Other"}]}],
            "members": [
                {"id": "m:alias", "type": "typeof TierIDs",
                 "type_ref": {"name": "TierIDs", "file": "/repo/app/t.ts"}},
                {"id": "m:typed", "type": "TierIDs",
                 "type_ref": {"name": "TierIDs", "file": "/repo/app/t.ts"}}
            ],
            "type_members": [],
            "template_nodes": [{"id": "n:switch", "parent": null},
                               {"id": "n:case", "parent": "n:switch"}],
            "expressions": [
                {"id": "x:s", "ast": {"k": "Read", "name": "TierID",
                                      "target": {"row": "m:typed"}}},
                {"id": "x:c", "ast": {"k": "Read", "name": "Gamma",
                                      "receiver": {"k": "Read", "name": "tierIDs",
                                                   "target": {"row": "m:alias"}}}}
            ],
            "gates": [{"id": "g:s", "name": "ngSwitch", "node": "n:switch", "expression": "x:s"},
                      {"id": "g:c", "name": "ngSwitchCase", "node": "n:case", "expression": "x:c"}]
        })
        .as_object()
        .cloned()
        .unwrap();

        let store = Store::from_payload(tables, "typescript");
        let idx = enum_index(&store);
        let mem = member_enums(&store, &idx);
        let (out, stats) = switch_restrictions(&store, &mem, &idx);

        assert_eq!(stats.switches, 1);
        assert_eq!(stats.gates, 1);
        assert_eq!(stats.resolved, 1);
        let arm = &out["g:c"][0];
        assert_eq!(arm["op"], json!("in"));
        assert_eq!(arm["values"], json!(["Gamma"]));
        assert_eq!(arm["dim"], json!("TierID"));
    }
}
