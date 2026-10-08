//! WHAT A TYPESCRIPT `case` RESTRICTS — the value its discriminant must hold for a statement
//! in that case group to run.
//!
//! The sibling of `gate_switch`, which reads the same comparison off an `ngSwitch` in a
//! template, and it states each arm the same way (`switch_arm`). Neither half of a `switch`
//! is a `Binary` the polarity walk can read, so the restriction is built here, per case
//! GROUP: `case A: case B:` holds for A or B, which is an `in` of both — not two `in`s that
//! would intersect to nothing.
//!
//! A `default` IS EVERY VALUE ITS SIBLINGS DO NOT NAME, a `not_in`. The siblings are the cases
//! whose `discriminant_expr` is the same row: that row is one per switch STATEMENT, so two
//! switches over one discriminant in one method are never merged. A group that is `default`
//! AND names labels permits those labels too, so they are left out of the `not_in`.
//!
//! AN UNRESOLVABLE LABEL MAKES ITS ARM `unknown`, never absent — and a default whose siblings
//! cannot all be read is `unknown` too: a `not_in` missing one member claims more than it knows.

use super::astreads::unwrap;
use super::gate_switch::{switch_arm, Dimension};
use super::gate_tsrows::TsRows;
use super::gate_values::{constant_of, dimension_of, untyped_dimension, EnumIndex, MemberEnums};
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// One case group: its id, its labels read as constants (None where one cannot be), and
/// whether it is the `default`.
struct Group {
    id: String,
    labels: Vec<Option<(String, String)>>,
    default: bool,
}

/// The restriction of one case group, given its switch's dimension and its siblings.
fn arm_of(g: &Group, dim: &Dimension, siblings: &[&Group]) -> Option<Value> {
    if !g.default {
        let arms: Vec<Value> = g.labels.iter().filter_map(|c| switch_arm(dim, c.as_ref())).collect();
        let first = arms.first()?;
        let en = first["enum"].clone();
        let all_in = arms.iter().all(|a| a["op"] == json!("in") && a["enum"] == en);
        if !all_in || arms.len() != g.labels.len() {
            return Some(json!({"enum": en, "dim": dim.dim, "row": dim.row, "op": "unknown"}));
        }
        let mut values: IndexSet<Value> = IndexSet::new();
        for a in &arms {
            values.extend(a["values"].as_array().into_iter().flatten().cloned());
        }
        return Some(json!({"enum": en, "dim": dim.dim, "row": dim.row, "op": "in",
                           "values": values.into_iter().collect::<Vec<Value>>()}));
    }
    let named: Vec<&Option<(String, String)>> = siblings.iter().flat_map(|s| s.labels.iter()).collect();
    let en = dim.enum_id.clone().or_else(|| named.iter().find_map(|c| c.as_ref().map(|(e, _)| e.clone())))?;
    let own: IndexSet<&str> = g.labels.iter().flatten().map(|(_, v)| v.as_str()).collect();
    let mut values: IndexSet<String> = IndexSet::new();
    for c in named {
        match c {
            Some((e, v)) if *e == en => {
                if !own.contains(v.as_str()) {
                    values.insert(v.clone());
                }
            }
            _ => return Some(json!({"enum": en, "dim": dim.dim, "row": dim.row, "op": "unknown"})),
        }
    }
    (!values.is_empty()).then(|| json!({"enum": en, "dim": dim.dim, "row": dim.row, "op": "not_in",
                                         "values": values.into_iter().collect::<Vec<String>>()}))
}

/// Case id -> its restriction, in `gate_values`' own map shape.
pub fn case_values(
    store: &Store<'_>,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> IndexMap<String, (Option<String>, Vec<Value>)> {
    let rows = TsRows::new(store);
    let none = IndexSet::new();
    let expressions = store.table("expressions");
    // BY ROW, NOT BY TREE: an AST is decoded when it is read (`store::Cell`), so the map holds rows and only the
    // trees a lookup asks for are ever decoded.
    let mut ast_of: IndexMap<String, &Row> = IndexMap::new();
    for e in expressions.iter() {
        if let Some(id) = cell(e, "id") {
            ast_of.insert(id, e);
        }
    }
    let read = |id: Option<&str>| {
        id.and_then(|i| ast_of.get(i)).and_then(|r| r.get("ast")).filter(|a| !a.is_null()).map(|a| rows.resolve(unwrap(a), &none, idx))
    };

    let mut switches: IndexMap<String, Vec<Group>> = IndexMap::new();
    for c in store.table("switch_cases").iter() {
        let (Some(id), Some(switch)) = (cell(c, "id"), cell(c, "discriminant_expr")) else { continue };
        let labels = match c.get("label_exprs") {
            Some(Value::Array(items)) => items
                .iter()
                .map(|x| read(x.as_str()).and_then(|ast| constant_of(&ast, mem, idx)))
                .collect(),
            _ => Vec::new(),
        };
        let default = c.get("is_default") == Some(&Value::Bool(true));
        switches.entry(switch).or_default().push(Group { id, labels, default });
    }

    let mut out = IndexMap::new();
    for (switch, groups) in &switches {
        let Some(ast) = read(Some(switch)) else { continue };
        let dim = match dimension_of(&ast, mem) {
            Some((en, dim, row)) => Dimension { enum_id: Some(en), dim, row },
            None => match untyped_dimension(&ast) {
                Some(dim) => Dimension { enum_id: None, dim, row: None },
                None => continue,
            },
        };
        for g in groups {
            let siblings: Vec<&Group> = groups.iter().filter(|s| s.id != g.id).collect();
            if let Some(arm) = arm_of(g, &dim, &siblings) {
                out.insert(g.id.clone(), (None, vec![arm]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim() -> Dimension {
        Dimension { enum_id: Some("e:1".into()), dim: "kind".into(), row: None }
    }

    fn group(id: &str, labels: &[Option<&str>], default: bool) -> Group {
        Group {
            id: id.into(),
            labels: labels.iter().map(|l| l.map(|v| ("e:1".to_string(), v.to_string()))).collect(),
            default,
        }
    }

    #[test]
    fn a_group_of_two_labels_is_one_in_of_both_and_not_two_that_intersect() {
        let g = group("sc:1", &[Some("A"), Some("B")], false);
        let arm = arm_of(&g, &dim(), &[]).expect("an arm");
        assert_eq!((arm["op"].clone(), arm["values"].clone()), (json!("in"), json!(["A", "B"])));
    }

    #[test]
    fn one_unreadable_label_makes_the_arm_unknown() {
        let g = group("sc:1", &[Some("A"), None], false);
        assert_eq!(arm_of(&g, &dim(), &[]).expect("an arm")["op"], json!("unknown"));
    }

    #[test]
    fn a_default_is_every_value_its_siblings_do_not_name() {
        let a = group("sc:1", &[Some("A")], false);
        let b = group("sc:2", &[Some("B")], false);
        let d = group("sc:3", &[], true);
        let arm = arm_of(&d, &dim(), &[&a, &b]).expect("an arm");
        assert_eq!((arm["op"].clone(), arm["values"].clone()), (json!("not_in"), json!(["A", "B"])));
    }

    #[test]
    fn a_default_that_also_names_a_label_permits_it() {
        let a = group("sc:1", &[Some("A")], false);
        let d = group("sc:2", &[Some("A")], true);
        assert!(arm_of(&d, &dim(), &[&a]).is_none(), "A is permitted, so nothing is excluded");
    }

    #[test]
    fn a_default_whose_siblings_cannot_all_be_read_is_unknown() {
        let a = group("sc:1", &[None], false);
        let d = group("sc:2", &[], true);
        assert_eq!(arm_of(&d, &dim(), &[&a]).expect("an arm")["op"], json!("unknown"));
    }

    #[test]
    fn two_switches_over_one_discriminant_are_never_siblings() {
        let tables = json!({
            "enums": [{"id": "e:1", "name": "K", "members": [{"name": "A"}, {"name": "B"}, {"name": "C"}]}],
            "expressions": [
                {"id": "x:s1", "ast": {"k": "Read", "name": "kind", "receiver": {"k": "This"}}},
                {"id": "x:s2", "ast": {"k": "Read", "name": "kind", "receiver": {"k": "This"}}},
                {"id": "x:a", "ast": {"k": "Read", "name": "A", "target": {"enum": "e:1"},
                                      "receiver": {"k": "Read", "name": "K", "receiver": {"k": "Implicit"}}}},
                {"id": "x:b", "ast": {"k": "Read", "name": "B", "target": {"enum": "e:1"},
                                      "receiver": {"k": "Read", "name": "K", "receiver": {"k": "Implicit"}}}},
            ],
            "switch_cases": [
                {"id": "sc:1", "discriminant_expr": "x:s1", "label_exprs": ["x:a"], "is_default": false},
                {"id": "sc:2", "discriminant_expr": "x:s1", "label_exprs": [], "is_default": true},
                {"id": "sc:3", "discriminant_expr": "x:s2", "label_exprs": ["x:b"], "is_default": false},
            ],
        });
        let store = Store::from_payload(tables.as_object().cloned().unwrap(), "typescript");
        let idx = super::super::gate_values::enum_index(&store);
        let out = case_values(&store, &MemberEnums::default(), &idx);
        assert_eq!(out["sc:2"].1[0]["values"], json!(["A"]), "B belongs to the other switch");
        assert_eq!(out["sc:3"].1[0]["values"], json!(["B"]));
    }
}
