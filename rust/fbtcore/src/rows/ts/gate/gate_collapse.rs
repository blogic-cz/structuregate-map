//! ONE RESTRICTION PER (GATE, ENUM, DIMENSION), from every source that proves one.
//!
//! Split out of `gate_values`, which feeds it the comparisons, when a fifth source arrived
//! that restricts a SET rather than a value — see `gate_predicates`.

use super::gate_values::EnumIndex;
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

#[derive(Debug, Default)]
struct Slot {
    enum_id: String,
    dim: String,
    row: Option<String>,
    ins: Vec<Vec<String>>,
    nots: IndexSet<String>,
    unknown: bool,
    /// The dimension is a COLLECTION the members are tested against, not one value.
    set: bool,
    /// The members a config object WRITES for the dimension, whatever its `op` - see
    /// `gate_config`. Unioned, since each source that lists is one the gate may bind.
    listed: Option<IndexSet<String>>,
}

/// Restrictions collapse to ONE per (gate, enum, dimension).
///
/// `in` sets intersect (both must hold), `not_in` sets union, and any `unknown` wins
/// outright: a gate that restricts a dimension in a way the map cannot read restricts it,
/// and reporting the half it could read would understate the condition.
///
/// A SET DIMENSION DOES NOT INTERSECT ITS `in`s. `isItemActive([A]) &&
/// isItemActive([B])` needs A AND B active, which no single `in` states, and the
/// intersection would state that nothing renders. Both hold, and no single row states both,
/// so ONE IS KEPT: the narrower, the first on a tie - each alone is true, and giving up on the
/// dimension instead dropped a restriction the gate proves. Its `not_in`s still union, and an
/// `in` still loses what a `not_in` removes: "A or B is active, and B is not" is "A is active".
pub fn collapse(items: &[Value], idx: &EnumIndex) -> Vec<Value> {
    let mut by_dim: IndexMap<String, Slot> = IndexMap::new();

    for r in items {
        let Some(object) = r.as_object() else { continue };
        let en = object.get("enum").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let dim = object.get("dim").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let key = format!("{en}#{dim}");
        let slot = by_dim.entry(key).or_insert_with(|| Slot {
            enum_id: en.clone(),
            dim: dim.clone(),
            row: object.get("row").and_then(|v| v.as_str()).map(|s| s.to_string()),
            ..Default::default()
        });

        // A CONFIG OBJECT STATES A SET, not one value: `allowIDs: [Beta, Alpha, …]`
        // permits any of several. Pushed as separate `in` restrictions the intersection
        // below would permit NOTHING, which is the opposite of what the list says.
        let vals: Vec<String> = match object.get("values") {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => object
                .get("value")
                .and_then(|v| v.as_str())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default(),
        };

        slot.set |= object.get("set") == Some(&Value::Bool(true));
        if let Some(Value::Array(listed)) = object.get("listed") {
            let held = slot.listed.get_or_insert_with(IndexSet::new);
            held.extend(listed.iter().filter_map(|v| v.as_str().map(|s| s.to_string())));
        }
        match object.get("op").and_then(|v| v.as_str()) {
            Some("unknown") => slot.unknown = true,
            Some("in") => slot.ins.push(vals),
            _ => {
                for v in vals {
                    slot.nots.insert(v);
                }
            }
        }
    }

    let mut out = Vec::new();
    for mut slot in by_dim.into_values() {
        let Some(info) = idx.by_id.get(&slot.enum_id) else { continue };
        let domain = &info.domain;
        if slot.set && slot.ins.len() > 1 {
            let keep = slot.ins.iter().min_by_key(|vs| vs.len()).cloned().unwrap_or_default();
            slot.ins = vec![keep];
        }
        let listed = slot.listed.take().map(|l| {
            let mut l: Vec<String> = l.into_iter().collect();
            l.sort();
            l
        });
        if slot.unknown {
            let mut row = json!({"enum": slot.enum_id, "dim": slot.dim, "row": slot.row,
                                 "op": "unknown", "values": [], "set": slot.set});
            if let Some(listed) = listed {
                row["listed"] = json!(listed);
            }
            out.push(row);
            continue;
        }
        // EACH `in` INTERSECTS, it does not add: every restriction here reached the same
        // gate through `&&`, so `x === A && x === B` permits NOTHING.
        let mut allowed: IndexSet<String> = domain.clone();
        for vs in &slot.ins {
            allowed = allowed.into_iter().filter(|v| vs.contains(v)).collect();
        }
        for v in &slot.nots {
            allowed.shift_remove(v);
        }
        if allowed.len() == domain.len() {
            continue;
        }
        let excluded: Vec<String> = domain.iter().filter(|v| !allowed.contains(*v)).cloned().collect();
        let in_side = allowed.len() <= excluded.len();
        // `values` is always the SMALLER side, named by `op`.
        let mut values: Vec<String> =
            if in_side { allowed.into_iter().collect() } else { excluded };
        values.sort();
        let mut row = json!({"enum": slot.enum_id, "dim": slot.dim, "row": slot.row,
                             "op": if in_side { "in" } else { "not_in" }, "values": values,
                             "set": slot.set});
        if let Some(listed) = listed {
            row["listed"] = json!(listed);
        }
        out.push(row);
    }
    out
}

/// A DISJUNCTION, as what BOTH of its sides prove.
///
/// `a || b` permits whatever either side permits, so a dimension is restricted only when BOTH sides
/// restrict it, and then to the UNION of what each permits: `isCompact || isWide` is the one
/// size or one of the others. A side silent about a dimension may be the one that held, with any value
/// there, so the dimension is not a row at all; `x.IsEdited || productID === A` restricts nothing.
/// `unknown` on either side stays `unknown` - the gate restricts the dimension, the map cannot say how.
///
/// A SET dimension (`gate_predicates`) unions its `in`s - "some of A is present, or some of B" is "some
/// of A or B" - and anything else on one is `unknown`: "none of A, or none of B" is no single set.
pub fn either(left: &[Value], right: &[Value], idx: &EnumIndex) -> Vec<Value> {
    let right = collapse(right, idx);
    let key = |v: &Value| (v["enum"].clone(), v["dim"].clone());
    let op = |v: &Value| v["op"].as_str().unwrap_or("unknown").to_string();
    let mut out = Vec::new();
    for a in collapse(left, idx) {
        let Some(b) = right.iter().find(|b| key(b) == key(&a)) else { continue };
        let Some(info) = a["enum"].as_str().and_then(|e| idx.by_id.get(e)) else { continue };
        let set = a["set"] == Value::Bool(true) || b["set"] == Value::Bool(true);
        let row = if a["row"] == b["row"] { a["row"].clone() } else { Value::Null };
        let (oa, ob) = (op(&a), op(b));
        if oa == "unknown" || ob == "unknown" || (set && (oa != "in" || ob != "in")) {
            out.push(json!({"enum": a["enum"], "dim": a["dim"], "row": row, "op": "unknown", "set": set}));
            continue;
        }
        // Each side's PERMITTED members: its `in`, or the domain less its `not_in`.
        let permitted = |v: &Value, o: &str| -> IndexSet<String> {
            let named: IndexSet<String> = v["values"].as_array().into_iter().flatten()
                .filter_map(|m| m.as_str().map(str::to_string)).collect();
            if o == "in" { named } else { info.domain.iter().filter(|m| !named.contains(*m)).cloned().collect() }
        };
        let mut union = permitted(&a, &oa);
        union.extend(permitted(b, &ob));
        let values: Vec<String> = info.domain.iter().filter(|m| union.contains(*m)).cloned().collect();
        out.push(json!({"enum": a["enum"], "dim": a["dim"], "row": row, "op": "in", "values": values, "set": set}));
    }
    out
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use super::super::gate_values::EnumInfo;

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

    fn one(op: &str, value: &str) -> Value {
        json!({"enum": "e:1", "dim": "shadeID", "row": null, "op": op, "value": value})
    }

    fn many(op: &str, values: &[&str]) -> Value {
        json!({"enum": "e:1", "dim": "shadeID", "row": null, "op": op, "values": values})
    }

    /// What a predicate call proves: the members tested against a collection.
    fn tested(op: &str, values: &[&str]) -> Value {
        json!({"enum": "e:1", "dim": "active", "row": null, "op": op, "values": values,
               "set": true})
    }

    fn collapsed(items: &[Value], domain: &[&str]) -> Vec<Value> {
        collapse(items, &idx_of(domain))
    }

    const FIVE: &[&str] = &["Alpha", "Beta", "Gamma", "Delta", "Other"];

    #[test]
    fn one_equality_permits_exactly_that_member() {
        let out = collapsed(&[one("in", "Alpha")], FIVE);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["op"], json!("in"));
        assert_eq!(out[0]["values"], json!(["Alpha"]));
    }

    #[test]
    fn two_equalities_intersect_and_permit_nothing() {
        // Every restriction here reached the same gate through `&&`, so `x === A && x === B`
        // permits NOTHING. Filtering the domain by the LIST of ins instead unioned them and
        // published two different equalities as "either" - the opposite of what the gate says.
        let out = collapsed(&[one("in", "Alpha"), one("in", "Beta")], FIVE);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["op"], json!("in"));
        assert_eq!(out[0]["values"], json!([]), "nothing reaches this gate");
    }

    #[test]
    fn a_config_list_is_ONE_restriction_permitting_any_of_its_members() {
        // Pushed as separate `in` restrictions these would intersect to nothing.
        let out = collapsed(&[many("in", &["Alpha", "Beta"])], FIVE);
        assert_eq!(out[0]["op"], json!("in"));
        assert_eq!(out[0]["values"], json!(["Alpha", "Beta"]));
    }

    #[test]
    fn negated_comparisons_union_and_remove_each_member() {
        let out = collapsed(&[one("not_in", "Alpha"), one("not_in", "Beta")], FIVE);
        assert_eq!(out[0]["op"], json!("not_in"));
        assert_eq!(out[0]["values"], json!(["Alpha", "Beta"]));
    }

    #[test]
    fn an_unknown_wins_outright_over_anything_readable() {
        // A gate that restricts a dimension in a way the map cannot read restricts it, and
        // reporting the half it could read would understate the condition.
        let out = collapsed(
            &[one("in", "Alpha"), json!({"enum": "e:1", "dim": "shadeID", "row": null,
                                       "op": "unknown"})],
            FIVE,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["op"], json!("unknown"));
        assert_eq!(out[0]["values"], json!([]));
    }

    #[test]
    fn a_restriction_that_removes_nothing_is_not_a_restriction() {
        // Excluding a member the enum does not have leaves the whole domain allowed.
        let out = collapsed(&[one("not_in", "NotAMember")], FIVE);
        assert!(out.is_empty());
    }

    #[test]
    fn the_values_column_always_carries_the_SMALLER_side() {
        // One member excluded out of five: naming the four allowed would be the larger side.
        let out = collapsed(&[one("not_in", "Alpha")], FIVE);
        assert_eq!(out[0]["op"], json!("not_in"));
        assert_eq!(out[0]["values"], json!(["Alpha"]));

        // And two allowed out of five names the two.
        let out = collapsed(&[many("in", &["Alpha", "Beta"])], FIVE);
        assert_eq!(out[0]["op"], json!("in"));
        assert_eq!(out[0]["values"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_tie_on_size_is_reported_as_the_in_side() {
        // Two allowed and two excluded out of four: `in` wins the tie, which is what
        // `len(allowed) <= len(excluded)` says.
        let out = collapsed(&[many("in", &["A", "B"])], &["A", "B", "C", "D"]);
        assert_eq!(out[0]["op"], json!("in"));
    }

    #[test]
    fn two_dimensions_of_one_enum_collapse_separately() {
        let items = vec![
            one("in", "Alpha"),
            json!({"enum": "e:1", "dim": "tierID", "row": null, "op": "not_in", "value": "Beta"}),
        ];
        let out = collapsed(&items, FIVE);
        assert_eq!(out.len(), 2, "a gate may restrict one dimension and not the other");
    }

    #[test]
    fn an_enum_the_index_does_not_hold_is_dropped_rather_than_guessed() {
        let items = vec![json!({"enum": "e:missing", "dim": "x", "row": null,
                                "op": "in", "value": "A"})];
        assert!(collapse(&items, &idx_of(FIVE)).is_empty());
    }

    #[test]
    fn one_predicate_call_is_an_in_over_its_arguments() {
        let out = collapsed(&[tested("in", &["Alpha", "Beta"])], FIVE);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["op"], json!("in"));
        assert_eq!(out[0]["values"], json!(["Alpha", "Beta"]));
        assert_eq!(out[0]["set"], json!(true), "the fold along a way needs to know");
    }

    #[test]
    fn two_predicate_calls_on_one_set_keep_the_narrower_and_never_nothing() {
        // `A active && (B or C) active` needs both. The value-dimension intersection says
        // NOTHING renders, which is the opposite of the truth; either one alone is true.
        let out = collapsed(&[tested("in", &["Beta", "Gamma"]), tested("in", &["Alpha"])], FIVE);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0]["op"].clone(), out[0]["values"].clone()), (json!("in"), json!(["Alpha"])));
    }

    #[test]
    fn a_negated_predicate_call_still_removes_its_members_from_a_set() {
        // "Alpha or Beta is active, and Beta is not" is "Alpha is active".
        let out = collapsed(&[tested("in", &["Alpha", "Beta"]), tested("not_in", &["Beta"])], FIVE);
        assert_eq!(out[0]["op"], json!("in"));
        assert_eq!(out[0]["values"], json!(["Alpha"]));
    }
    #[test]
    fn a_disjunction_permits_the_union_of_what_both_sides_permit() {
        // `isCompact || isWide`: one equality, and the two members of the other side.
        let out = either(&[one("in", "Alpha")], &[many("in", &["Beta", "Gamma"])], &idx_of(FIVE));
        assert_eq!((out[0]["op"].clone(), out[0]["values"].clone()), (json!("in"), json!(["Alpha", "Beta", "Gamma"])));
        // ...and a `not_in` side permits the rest of the domain: `x !== Alpha || x === Alpha` permits all.
        let out = either(&[one("not_in", "Alpha")], &[one("in", "Beta")], &idx_of(FIVE));
        let folded = collapsed(&out, FIVE);
        assert_eq!((folded[0]["op"].clone(), folded[0]["values"].clone()), (json!("not_in"), json!(["Alpha"])));
        let out = either(&[one("not_in", "Alpha")], &[one("in", "Alpha")], &idx_of(FIVE));
        assert!(collapsed(&out, FIVE).is_empty(), "the whole domain restricts nothing");
    }

    #[test]
    fn a_disjunction_restricts_nothing_one_side_is_silent_about() {
        // `x.IsEdited || productID === A`: the first side may hold with any product.
        assert!(either(&[], &[one("in", "Alpha")], &idx_of(FIVE)).is_empty());
        let other = json!({"enum": "e:1", "dim": "tierID", "row": null, "op": "in", "value": "Alpha"});
        assert!(either(&[other], &[one("in", "Alpha")], &idx_of(FIVE)).is_empty());
    }

    #[test]
    fn a_disjunction_with_an_unknown_side_or_a_set_it_cannot_union_is_unknown() {
        let unread = json!({"enum": "e:1", "dim": "shadeID", "row": null, "op": "unknown"});
        let out = either(&[unread], &[one("in", "Alpha")], &idx_of(FIVE));
        assert_eq!(out[0]["op"], json!("unknown"));
        // Two sets' `in`s union; a `not_in` on a set is "none of these", and two of those are no one set.
        let out = either(&[tested("in", &["Alpha"])], &[tested("in", &["Beta"])], &idx_of(FIVE));
        assert_eq!((out[0]["values"].clone(), out[0]["set"].clone()), (json!(["Alpha", "Beta"]), json!(true)));
        let out = either(&[tested("not_in", &["Alpha"])], &[tested("in", &["Beta"])], &idx_of(FIVE));
        assert_eq!(out[0]["op"], json!("unknown"));
    }

    #[test]
    fn what_a_config_lists_rides_through_whatever_the_op() {
        // An `unknown` keeps `values` empty and still names the members the source wrote.
        let mut row = many("unknown", &[]);
        row["listed"] = json!(["Alpha", "Beta"]);
        let out = collapsed(&[row], FIVE);
        assert_eq!((out[0]["op"].clone(), out[0]["listed"].clone()),
                   (json!("unknown"), json!(["Alpha", "Beta"])));
        // A long `in` flips to its `not_in` side; `listed` is still what the source wrote.
        let mut row = many("in", &["Alpha", "Beta", "Gamma", "Delta"]);
        row["listed"] = json!(["Alpha", "Beta", "Delta", "Gamma"]);
        let out = collapsed(&[row], FIVE);
        assert_eq!((out[0]["op"].clone(), out[0]["listed"].clone()),
                   (json!("not_in"), json!(["Alpha", "Beta", "Delta", "Gamma"])));
        assert!(collapsed(&[one("in", "Alpha")], FIVE)[0].get("listed").is_none());
    }
}
