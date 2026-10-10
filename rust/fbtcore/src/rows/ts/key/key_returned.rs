//! THE `case` A FACTORY RETURNS A COMPONENT UNDER — the condition a dynamic render edge holds
//! that no template node states.
//!
//! `switch (kind) { case Kind.A: return ADetail; }` behind `*ngComponentOutlet="detail"` renders
//! `ADetail` only when the case holds, and the outlet's node has no gate saying so: every key in
//! `ADetail` read as shown for every kind. The extractor records each way the class was returned
//! (`renders.return_ways`, the `returns` rows on it, `TsDerive/TsDynRender.mjs`), and every one of
//! those rows names the branch and the case it sits in - the chain `key_branches` already reads for a
//! key written in TypeScript. This joins the two.
//!
//! THE EDGE HOLDS WHAT EVERY WAY HOLDS. A class returned under two cases, or once under a case and
//! once with none, renders under either, and one edge carries one chain: the intersection is the
//! only part of it that is true of the edge, the same rule `always_gates` folds by. A way through
//! several returns (a registry returning another registry's answer) holds every return's chain.
//!
//! A branch rides the path AS A BRANCH, never as a gate: `render_path.gates` names `gates` rows and
//! nothing else, so the ids get `render_path.branches`, and `key_reach` folds them with the key's own.

use super::key_branches::Branches;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// `renders.id` -> the branches, cases and arms every way its class was returned under, in the
/// order the first way names them. A render with no `return_ways`, or with nothing every way
/// holds, has no entry.
pub fn returned_under(store: &Store<'_>, branches: &Branches) -> IndexMap<String, Vec<String>> {
    let mut out = IndexMap::new();
    let renders = store.table("renders");
    let wanted: Vec<(&Row, &Vec<Value>)> = renders
        .iter()
        .filter_map(|r| match r.get("return_ways") {
            Some(Value::Array(ways)) if !ways.is_empty() => Some((r, ways)),
            _ => None,
        })
        .collect();
    if wanted.is_empty() {
        return out;
    }
    let returns = store.table("returns");
    let by_id: IndexMap<String, &Row> =
        returns.iter().filter_map(|r| cell(r, "id").map(|id| (id, r))).collect();
    for (render, ways) in wanted {
        let Some(id) = cell(render, "id") else { continue };
        let mut held: Option<IndexSet<String>> = None;
        for way in ways {
            let mut chain = IndexSet::new();
            for rv in way.as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
                // A ROW THE MAP DOES NOT HOLD PROVES NOTHING, so the way holds nothing either -
                // never fewer conditions than the rows say, never more.
                let Some(row) = by_id.get(rv) else {
                    chain.clear();
                    break;
                };
                chain.extend(branches.chain_of(row));
            }
            held = Some(match held {
                None => chain,
                Some(h) => h.into_iter().filter(|b| chain.contains(b)).collect(),
            });
        }
        let held: Vec<String> = held.unwrap_or_default().into_iter().collect();
        if !held.is_empty() {
            out.insert(id, held);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store(renders: Value) -> Store<'static> {
        let tables = json!({
            "switch_cases": [{"id": "sc:1"}, {"id": "sc:2"}],
            "returns": [
                {"id": "rv:1", "case": "sc:1"},
                {"id": "rv:2", "case": "sc:2"},
                {"id": "rv:3"},
            ],
            "renders": renders,
        });
        Store::from_payload(tables.as_object().cloned().unwrap(), "typescript")
    }

    #[test]
    fn a_class_returned_under_one_case_holds_that_case() {
        let s = store(json!([{"id": "rd:1", "return_ways": [["rv:1"]]}]));
        let out = returned_under(&s, &Branches::new(&s));
        assert_eq!(out["rd:1"], vec!["sc:1".to_string()]);
    }

    #[test]
    fn a_class_returned_under_two_cases_or_none_holds_nothing() {
        let s = store(json!([
            {"id": "rd:1", "return_ways": [["rv:1"], ["rv:2"]]},
            {"id": "rd:2", "return_ways": [["rv:1"], []]},
            {"id": "rd:3", "return_ways": [["rv:3"]]},
        ]));
        let out = returned_under(&s, &Branches::new(&s));
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_way_through_two_returns_holds_both_and_a_missing_row_holds_nothing() {
        let s = store(json!([
            {"id": "rd:1", "return_ways": [["rv:1", "rv:2"]]},
            {"id": "rd:2", "return_ways": [["rv:1", "rv:9"]]},
        ]));
        let out = returned_under(&s, &Branches::new(&s));
        assert_eq!(out["rd:1"], vec!["sc:1".to_string(), "sc:2".to_string()]);
        assert!(!out.contains_key("rd:2"));
    }
}
