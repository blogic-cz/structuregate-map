//! WHERE A BARE `.ts` KEY LITERAL ACTUALLY RENDERS — the gate that reads the property
//! its value flows into.
//!
//! The `literal` route attaches NO gates, and cannot on its own: a string literal in a
//! `.ts` file has no template node, so nothing ties it to a position in a template and
//! `key_reach` gives it only the gates of the render path INTO the component. For a key
//! DECLARED in TypeScript and rendered through a property the template iterates, that
//! drops the one condition that matters. Without this pass such
//! keys are `route='literal'` with the path's gates and no features, even when every one of
//! them renders only under `*ngFor="let entry of entries"` with `entries` assigned
//! from a feature-guarded ternary.
//!
//! THE CHAIN IS ID JOINS END TO END, never a name match:
//!   * `string_literals.file` + `.line` → the `returns`/`assignments` row whose span
//!     encloses it, INNERMOST
//!   * `returns.member` → `calls.target_id` — the member is CALLED somewhere
//!   * a `calls.line` inside a PROVEN ternary assignment's span → that assignment's target
//!   * `gates.reads` names the property, in the literal's own component → the gate
//!
//! The proof that the ternary is feature-guarded is NOT redone here: `gate_features`
//! already made it and its `ternary_writes` are handed in, so the soundness rule lives in
//! exactly one place.

use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::Value;

/// One assignment row the feature pass proved to be a guarded ternary write.
pub type TernaryWrites = IndexMap<String, Row>;

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

fn number(row: &Row, field: &str) -> Option<i64> {
    match row.get(field) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// `class + " " + target`, the property a write fills.
fn property_of(row: &Row) -> String {
    format!(
        "{} {}",
        id_of(row, "class").unwrap_or_else(|| "None".to_string()),
        id_of(row, "target").unwrap_or_else(|| "None".to_string())
    )
}

/// File -> its component, or nothing when the file declares more than one and the literal
/// names neither.
fn component_by_file(store: &Store<'_>) -> IndexMap<Option<String>, Option<String>> {
    let mut out: IndexMap<Option<String>, Option<String>> = IndexMap::new();
    for c in store.table("components").iter() {
        let file = text(c, "file");
        if out.contains_key(&file) {
            out.insert(file, None);
        } else {
            out.insert(file, id_of(c, "id"));
        }
    }
    out
}

/// Member -> `class + " " + property`, for members whose EVERY call site sits inside a
/// proven ternary write.
///
/// EVERY CALL SITE MUST BE GUARDED, not merely one. A member whose list is also built into
/// an ungated property is reachable that way too, and attributing the guarded site's
/// condition to the key would demand a capability one of its ways in does not need — which
/// HIDES content from callers entitled to it.
fn member_properties(store: &Store<'_>, ternary_writes: &TernaryWrites) -> IndexMap<String, String> {
    let mut by_member: IndexMap<String, Vec<&Row>> = IndexMap::new();
    for a in ternary_writes.values() {
        let Some(member) = id_of(a, "member") else { continue };
        by_member.entry(member).or_default().push(a);
    }

    let calls = store.table("calls");
    let mut sites: IndexMap<String, Vec<&Row>> = IndexMap::new();
    for c in calls.iter() {
        let (Some(target), Some(_)) = (id_of(c, "target_id"), id_of(c, "member")) else {
            continue;
        };
        sites.entry(target).or_default().push(c);
    }

    let mut out = IndexMap::new();
    for (target, call_sites) in sites {
        let mut prop: Option<String> = None;
        let mut ok = true;
        for c in call_sites {
            let member = id_of(c, "member").unwrap_or_default();
            let line = number(c, "line");
            let found = by_member.get(&member).and_then(|writes| {
                writes.iter().find(|x| {
                    let (Some(line), Some(start), Some(end)) =
                        (line, number(x, "line"), number(x, "end_line"))
                    else {
                        return false;
                    };
                    start <= line && line <= end
                })
            });
            let Some(found) = found else {
                ok = false;
                break;
            };
            let k = property_of(found);
            if prop.as_ref().is_some_and(|p| *p != k) {
                ok = false;
                break;
            }
            prop = Some(k);
        }
        if ok && let Some(prop) = prop {
            out.insert(target, prop);
        }
    }
    out
}

/// `class + " " + property` -> the ONE gate that reads it in that class's component, or
/// nothing if several.
///
/// ONE GATE OR NONE. Gates collected for one ref are ANDed, so attaching two that read the
/// same property says the key needs BOTH, which is wrong the moment they are alternative
/// renders in different subtrees.
fn gate_by_property(store: &Store<'_>) -> IndexMap<String, Option<String>> {
    let mut class_of_component: IndexMap<String, Option<String>> = IndexMap::new();
    for c in store.table("components").iter() {
        if let Some(id) = id_of(c, "id") {
            class_of_component.insert(id, id_of(c, "class"));
        }
    }

    let mut out: IndexMap<String, Option<String>> = IndexMap::new();
    for g in store.table("gates").iter() {
        let Some(Value::Array(reads)) = g.get("reads") else { continue };
        let Some(component) = id_of(g, "component") else { continue };
        let Some(class) = class_of_component.get(&component) else { continue };
        let class = class.clone().unwrap_or_else(|| "None".to_string());
        for p in reads {
            let name = match p {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let k = format!("{class} {name}");
            if out.contains_key(&k) {
                out.insert(k, None);
            } else {
                out.insert(k, id_of(g, "id"));
            }
        }
    }
    out
}

struct Span {
    line: Option<i64>,
    end: Option<i64>,
    prop: String,
}

/// File -> the enclosing spans a key literal can sit in, each already resolved to the
/// property it fills.
fn spans_by_file(
    store: &Store<'_>,
    ternary_writes: &TernaryWrites,
    member_prop: &IndexMap<String, String>,
) -> IndexMap<String, Vec<Span>> {
    let mut class_file: IndexMap<String, Option<String>> = IndexMap::new();
    for r in store.table("classes").iter() {
        if let Some(id) = id_of(r, "id") {
            class_file.insert(id, text(r, "file"));
        }
    }

    let mut out: IndexMap<String, Vec<Span>> = IndexMap::new();
    let mut add = |file: Option<String>, span: Span| {
        if let Some(file) = file.filter(|f| !f.is_empty()) {
            out.entry(file).or_default().push(span);
        }
    };

    for r in store.table("returns").iter() {
        let (Some(class), Some(member)) = (id_of(r, "class"), id_of(r, "member")) else {
            continue;
        };
        let Some(prop) = member_prop.get(&member) else { continue };
        let file = class_file.get(&class).cloned().flatten();
        add(file, Span { line: number(r, "line"), end: number(r, "end_line"), prop: prop.clone() });
    }

    // The literal can also sit in the guarded ternary's OWN then-branch — a getter
    // returning the array inline — in which case there is no member hop to make and the
    // property is the write's own target.
    for a in ternary_writes.values() {
        let file = id_of(a, "class").and_then(|c| class_file.get(&c).cloned().flatten());
        add(file, Span { line: number(a, "line"), end: number(a, "end_line"), prop: property_of(a) });
    }
    out
}

/// `key + " " + component` -> the gate ids that key's literal renders under.
///
/// Keyed by component because the route pairs every ref with the component it was found
/// in, and a gate from one component may never be unioned into another's path.
pub fn literal_gates(
    store: &Store<'_>,
    ternary_writes: &TernaryWrites,
) -> std::collections::HashMap<String, Vec<String>> {
    let member_prop = member_properties(store, ternary_writes);
    let spans = spans_by_file(store, ternary_writes, &member_prop);
    let gate_for = gate_by_property(store);
    let comp_of = component_by_file(store);

    let keys: std::collections::HashSet<String> = store
        .table("translations")
        .iter()
        .filter_map(|t| text(t, "key"))
        .collect();

    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for s in store.table("string_literals").iter() {
        let Some(value) = text(s, "value") else { continue };
        if !keys.contains(&value) {
            continue;
        }
        let Some(Some(comp)) = comp_of.get(&text(s, "file")) else { continue };

        let mut inner: Option<&Span> = None;
        let line = number(s, "line");
        if let Some(file) = text(s, "file")
            && let Some(list) = spans.get(&file)
        {
            for sp in list {
                let (Some(line), Some(start), Some(end)) = (line, sp.line, sp.end) else {
                    continue;
                };
                if line < start || line > end {
                    continue;
                }
                // INNERMOST wins: the tightest span is the one that actually encloses it.
                let tighter = inner
                    .map(|held| end - start < held.end.unwrap_or(0) - held.line.unwrap_or(0))
                    .unwrap_or(true);
                if tighter {
                    inner = Some(sp);
                }
            }
        }
        let Some(inner) = inner else { continue };
        let Some(Some(gate)) = gate_for.get(&inner.prop) else { continue };

        let k = format!("{value} {comp}");
        // TWO LITERALS OF ONE KEY IN ONE COMPONENT UNDER DIFFERENT GATES are two ways in,
        // not one requirement. ANDing them would claim a condition neither way alone
        // imposes, so the pair resolves to no gate.
        match out.get(&k) {
            Some(held) if held.first() != Some(gate) => {
                out.insert(k, Vec::new());
            }
            Some(_) => {}
            None => {
                out.insert(k, vec![gate.clone()]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn writes_of(rows: Value) -> TernaryWrites {
        let mut out = TernaryWrites::new();
        for r in rows.as_array().expect("rows") {
            let row = Row::Built(r.as_object().cloned().expect("a row"));
            out.insert(id_of(&row, "id").unwrap_or_default(), row);
        }
        out
    }

    fn run(tables: Value, writes: Value) -> std::collections::HashMap<String, Vec<String>> {
        let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
        let store = Store::from_payload(map, "typescript");
        literal_gates(&store, &writes_of(writes))
    }

    /// A key literal inside a getter whose member is called only from a guarded ternary.
    fn chain() -> (Value, Value) {
        let tables = json!({
            "translations": [{"key": "list.item1"}],
            "components": [{"id": "ng:1", "file": "f:1", "class": "c:1"}],
            "classes": [{"id": "c:1", "file": "f:1"}],
            "string_literals": [{"value": "list.item1", "file": "f:1", "line": 20}],
            "returns": [{"class": "c:1", "member": "m:build", "line": 15, "end_line": 25}],
            "calls": [{"target_id": "m:build", "member": "m:build", "line": 50}],
            "gates": [{"id": "g:1", "component": "ng:1", "reads": ["entries"]}],
        });
        let writes = json!([
            {"id": "a:1", "member": "m:build", "class": "c:1", "target": "entries",
             "line": 45, "end_line": 55},
        ]);
        (tables, writes)
    }

    #[test]
    fn a_literal_reached_only_through_a_guarded_ternary_carries_that_gate() {
        let (tables, writes) = chain();
        let out = run(tables, writes);
        assert_eq!(out.get("list.item1 ng:1"), Some(&vec!["g:1".to_string()]));
    }

    #[test]
    fn a_member_with_one_unguarded_call_site_proves_nothing() {
        // The list is also built into an ungated property, so the key is reachable that
        // way too; claiming the guarded condition would hide content from callers
        // entitled to it.
        let (mut tables, writes) = chain();
        tables["calls"] = json!([
            {"target_id": "m:build", "member": "m:build", "line": 50},
            {"target_id": "m:build", "member": "m:build", "line": 900},
        ]);
        assert!(run(tables, writes).is_empty());
    }

    #[test]
    fn call_sites_resolving_to_different_properties_prove_nothing() {
        let (mut tables, _) = chain();
        tables["calls"] = json!([
            {"target_id": "m:build", "member": "m:build", "line": 50},
            {"target_id": "m:build", "member": "m:build", "line": 70},
        ]);
        let writes = json!([
            {"id": "a:1", "member": "m:build", "class": "c:1", "target": "entries",
             "line": 45, "end_line": 55},
            {"id": "a:2", "member": "m:build", "class": "c:1", "target": "other",
             "line": 65, "end_line": 75},
        ]);
        assert!(run(tables, writes).is_empty());
    }

    #[test]
    fn a_property_read_by_two_gates_resolves_to_none_rather_than_a_guess() {
        // Gates collected for one ref are ANDed, so two would say the key needs BOTH -
        // wrong the moment they are alternative renders in different subtrees.
        let (mut tables, writes) = chain();
        tables["gates"] = json!([
            {"id": "g:1", "component": "ng:1", "reads": ["entries"]},
            {"id": "g:2", "component": "ng:1", "reads": ["entries"]},
        ]);
        assert!(run(tables, writes).is_empty());
    }

    #[test]
    fn a_file_declaring_two_components_names_neither() {
        let (mut tables, writes) = chain();
        tables["components"] = json!([
            {"id": "ng:1", "file": "f:1", "class": "c:1"},
            {"id": "ng:2", "file": "f:1", "class": "c:2"},
        ]);
        assert!(run(tables, writes).is_empty());
    }

    #[test]
    fn a_literal_outside_every_span_carries_no_gate() {
        let (mut tables, writes) = chain();
        tables["string_literals"] = json!([{"value": "list.item1", "file": "f:1", "line": 999}]);
        assert!(run(tables, writes).is_empty());
    }

    #[test]
    fn the_innermost_span_wins() {
        let (mut tables, _) = chain();
        tables["returns"] = json!([
            {"class": "c:1", "member": "m:build", "line": 1, "end_line": 100},
            {"class": "c:1", "member": "m:tight", "line": 18, "end_line": 22},
        ]);
        tables["calls"] = json!([
            {"target_id": "m:build", "member": "m:build", "line": 50},
            {"target_id": "m:tight", "member": "m:tight", "line": 46},
        ]);
        tables["gates"] = json!([
            {"id": "g:wide", "component": "ng:1", "reads": ["entries"]},
            {"id": "g:tight", "component": "ng:1", "reads": ["inner"]},
        ]);
        let writes = json!([
            {"id": "a:1", "member": "m:build", "class": "c:1", "target": "entries",
             "line": 45, "end_line": 55},
            {"id": "a:2", "member": "m:tight", "class": "c:1", "target": "inner",
             "line": 44, "end_line": 48},
        ]);
        assert_eq!(run(tables, writes).get("list.item1 ng:1"), Some(&vec!["g:tight".to_string()]));
    }

    #[test]
    fn two_literals_of_one_key_under_different_gates_resolve_to_no_gate() {
        // Two ways in, not one requirement: ANDing would claim a condition neither
        // imposes on its own.
        let (mut tables, _) = chain();
        tables["string_literals"] = json!([
            {"value": "list.item1", "file": "f:1", "line": 20},
            {"value": "list.item1", "file": "f:1", "line": 70},
        ]);
        tables["returns"] = json!([
            {"class": "c:1", "member": "m:build", "line": 15, "end_line": 25},
            {"class": "c:1", "member": "m:two", "line": 65, "end_line": 75},
        ]);
        tables["calls"] = json!([
            {"target_id": "m:build", "member": "m:build", "line": 50},
            {"target_id": "m:two", "member": "m:two", "line": 46},
        ]);
        tables["gates"] = json!([
            {"id": "g:1", "component": "ng:1", "reads": ["entries"]},
            {"id": "g:2", "component": "ng:1", "reads": ["other"]},
        ]);
        let writes = json!([
            {"id": "a:1", "member": "m:build", "class": "c:1", "target": "entries",
             "line": 45, "end_line": 55},
            {"id": "a:2", "member": "m:two", "class": "c:1", "target": "other",
             "line": 44, "end_line": 48},
        ]);
        assert_eq!(run(tables, writes).get("list.item1 ng:1"), Some(&Vec::<String>::new()));
    }

    #[test]
    fn the_same_gate_found_twice_is_still_that_gate() {
        let (mut tables, writes) = chain();
        tables["string_literals"] = json!([
            {"value": "list.item1", "file": "f:1", "line": 20},
            {"value": "list.item1", "file": "f:1", "line": 21},
        ]);
        assert_eq!(run(tables, writes).get("list.item1 ng:1"), Some(&vec!["g:1".to_string()]));
    }

    #[test]
    fn with_no_proven_ternary_write_there_is_nothing_to_attach() {
        let (tables, _) = chain();
        assert!(run(tables, json!([])).is_empty());
    }
}
