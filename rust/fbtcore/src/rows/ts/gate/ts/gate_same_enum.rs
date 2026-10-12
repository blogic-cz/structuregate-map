//! TWO ENUM DECLARATIONS THAT SAY THE SAME THING, and a NUMBER written where a member is meant.
//!
//! Enums are identified by declaration (`gate_values`), so a dimension typed by one copy of an
//! enum and a constant read off ANOTHER copy of it never met: `client.PartyType === PartyType.Company`
//! came out `unknown` when the model declared the enum in one folder and the component imported the
//! twin from another. Same NAME and the same members with the same values is one enum for a
//! comparison; a copy with a different member or value stays a different enum, and stays `unknown`.
//!
//! A NUMBER COMPARED WITH AN ENUM-TYPED VALUE is the member whose value it equals (`w.Type === 0`).
//! A number no member has can never be equal, so the gate renders for nobody: `in []`. An enum with a
//! member whose value the compiler could not fold says nothing about a number, and stays `unknown`.

use super::gate_values::EnumIndex;
use serde_json::{json, Value};

/// Whether enums `a` and `b` are one declaration or two with the same name, members and values.
pub fn same_enum(idx: &EnumIndex, a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (Some(x), Some(y)) = (idx.by_id.get(a), idx.by_id.get(b)) else { return false };
    x.name.is_some() && x.name == y.name && x.domain == y.domain && idx.values.get(a) == idx.values.get(b)
}

fn number(n: &Value) -> Option<f64> {
    if n.get("k").and_then(Value::as_str) != Some("Literal") {
        return None;
    }
    n.get("v").filter(|v| v.is_number()).and_then(Value::as_f64)
}

/// The restriction `<enum-typed dimension> === <number>` proves, or nothing when it cannot be said.
pub fn number_restriction(sides: &[&Value; 2], idx: &EnumIndex, en: &str, dim: &str, row: &Option<String>, eq: bool) -> Option<Value> {
    let lit = sides.iter().find_map(|s| number(s))?;
    let values = idx.values.get(en)?;
    // A member whose value is not a number (a string, or none folded) makes the enum unreadable here.
    if values.is_empty() || values.values().any(|v| !v.is_number()) {
        return None;
    }
    let named = values.iter().find(|(_, v)| v.as_f64() == Some(lit)).map(|(n, _)| n.clone());
    let op = if eq { "in" } else { "not_in" };
    Some(match named {
        Some(member) => json!({"enum": en, "dim": dim, "row": row, "op": op, "value": member}),
        None => json!({"enum": en, "dim": dim, "row": row, "op": op, "values": []}),
    })
}

#[cfg(test)]
mod tests {
    use super::super::gate_values::EnumInfo;
    use super::*;

    fn idx_of(copies: &[(&str, &[(&str, i64)])]) -> EnumIndex {
        let mut idx = EnumIndex::default();
        for (id, members) in copies {
            let domain = members.iter().map(|(n, _)| n.to_string()).collect();
            idx.by_id.insert(id.to_string(), EnumInfo { name: Some("Kind".into()), domain });
            idx.values.insert(id.to_string(), members.iter().map(|(n, v)| (n.to_string(), json!(v))).collect());
        }
        idx
    }

    #[test]
    fn a_twin_with_the_same_members_and_values_is_the_same_enum_and_another_is_not() {
        let idx = idx_of(&[("e:1", &[("A", 1), ("B", 2)]), ("e:2", &[("A", 1), ("B", 2)]), ("e:3", &[("A", 1), ("B", 3)]), ("e:4", &[("A", 1)])]);
        assert!(same_enum(&idx, "e:1", "e:2"));
        assert!(!same_enum(&idx, "e:1", "e:3"), "a different value");
        assert!(!same_enum(&idx, "e:1", "e:4"), "a different member");
    }

    #[test]
    fn a_number_is_the_member_it_equals_and_one_no_member_has_is_in_nothing() {
        let idx = idx_of(&[("e:1", &[("Info", 0), ("Warn", 1)])]);
        let lit = |v: i64| json!({"k": "Literal", "v": v});
        let dim = json!({"k": "Read", "name": "Type"});
        let got = number_restriction(&[&dim, &lit(1)], &idx, "e:1", "w.Type", &None, true).expect("a member");
        assert_eq!((got["op"].clone(), got["value"].clone()), (json!("in"), json!("Warn")));
        let none = number_restriction(&[&lit(9), &dim], &idx, "e:1", "w.Type", &None, true).expect("no member");
        assert_eq!((none["op"].clone(), none["values"].clone()), (json!("in"), json!([])));
        assert!(number_restriction(&[&dim, &dim], &idx, "e:1", "w.Type", &None, true).is_none());
    }
}
