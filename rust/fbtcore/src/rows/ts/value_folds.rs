//! FOLDING GATE VALUES OVER THE WAYS A KEY RENDERS, and the two row emitters.
//!
//! THE FOLD IS A UNION, WHICH IS THE OPPOSITE OF THE FEATURE FOLD, and getting it
//! backwards would invent restrictions that do not hold. A key renders if ANY way in
//! holds, so a value is still possible when ONE way permits it: per (enum, dimension) the
//! allowed sets are intersected ALONG a way — every gate on it must hold — and UNIONED
//! ACROSS ways.

use super::gate_values::EnumIndex;
use super::jsstr::sort_key;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

/// One way in, as the gate ids on it.
pub type Way = IndexSet<String>;
/// Gate id -> the component it sits in and its collapsed restriction rows.
pub type GateMap = IndexMap<String, (Option<String>, Vec<Value>)>;
/// Per way, per `enum#dimension`, the members that way still permits.
pub type PerWay = Vec<IndexMap<String, IndexSet<String>>>;

fn field(row: &Value, name: &str) -> String {
    row.get(name).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn values_of(row: &Value) -> Vec<String> {
    match row.get("values") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn split(k: &str) -> (&str, &str) {
    match k.find('#') {
        Some(i) => (&k[..i], &k[i + 1..]),
        None => (k, ""),
    }
}

/// What every way in still permits, per dimension.
///
/// Intersected ALONG the way: every gate on it must hold, so two restrictions on one
/// dimension both apply. Except two `in`s on a SET, which both hold at once rather than
/// narrow each other (`gate_collapse` says why): the narrower one is kept, and the set's
/// `not_in`s still remove their members from it.
pub fn per_way_allowed(ways: &[Way], gates: &GateMap, idx: &EnumIndex) -> PerWay {
    let mut out = Vec::with_capacity(ways.len());
    for way in ways {
        let mut here: IndexMap<String, IndexSet<String>> = IndexMap::new();
        // Per SET dimension: the one `in` kept, and every member a `not_in` removed.
        let mut set_kept: IndexMap<String, IndexSet<String>> = IndexMap::new();
        let mut set_out: IndexMap<String, IndexSet<String>> = IndexMap::new();
        for g in way {
            let Some((_, rows)) = gates.get(g) else { continue };
            for r in rows {
                let enum_id = field(r, "enum");
                let Some(info) = idx.by_id.get(&enum_id) else { continue };
                let k = format!("{enum_id}#{}", field(r, "dim"));

                // `unknown` is the full domain: it is the honest upper bound.
                let op = field(r, "op");
                if r.get("set") == Some(&Value::Bool(true)) {
                    let vals: IndexSet<String> = values_of(r).into_iter().collect();
                    match op.as_str() {
                        "in" => {
                            let narrower = set_kept.get(&k).is_none_or(|held| vals.len() < held.len());
                            if narrower {
                                set_kept.insert(k.clone(), vals);
                            }
                        }
                        "not_in" => set_out.entry(k.clone()).or_default().extend(vals),
                        _ => {}
                    }
                    let kept = set_kept.get(&k).cloned().unwrap_or_else(|| info.domain.clone());
                    let removed = set_out.get(&k).cloned().unwrap_or_default();
                    here.insert(k, kept.into_iter().filter(|v| !removed.contains(v)).collect());
                    continue;
                }
                let allowed: IndexSet<String> = match op.as_str() {
                    "unknown" => info.domain.clone(),
                    "in" => values_of(r).into_iter().collect(),
                    _ => {
                        let excluded: IndexSet<String> = values_of(r).into_iter().collect();
                        info.domain.iter().filter(|v| !excluded.contains(*v)).cloned().collect()
                    }
                };
                match here.get(&k) {
                    Some(held) => {
                        let narrowed: IndexSet<String> =
                            held.iter().filter(|v| allowed.contains(*v)).cloned().collect();
                        here.insert(k, narrowed);
                    }
                    None => {
                        here.insert(k, allowed);
                    }
                }
            }
        }
        out.push(here);
    }
    out
}

/// Every dimension any way restricts, collected BEFORE the union starts.
///
/// A WAY THAT NEVER MENTIONS THE DIMENSION PERMITS ALL OF IT, so folded per way as they
/// were met, a dimension first restricted on the third way missed the two ways above it
/// and came out looking restricted when those two allow everything.
fn dimensions(per_way: &PerWay) -> IndexSet<String> {
    let mut keys = IndexSet::new();
    for here in per_way {
        for k in here.keys() {
            keys.insert(k.clone());
        }
    }
    keys
}

/// The values every way in still permits between them, where that is less than the domain.
pub fn fold_values(ways: &[Way], gates: &GateMap, idx: &EnumIndex, per_way: Option<&PerWay>) -> Vec<Value> {
    let computed;
    let per_way = match per_way {
        Some(p) => p,
        None => {
            computed = per_way_allowed(ways, gates, idx);
            &computed
        }
    };

    let mut out = Vec::new();
    for k in dimensions(per_way) {
        let (enum_id, dim) = split(&k);
        let Some(info) = idx.by_id.get(enum_id) else { continue };

        let mut union: IndexSet<String> = IndexSet::new();
        for here in per_way {
            // ABSENT AND EMPTY ARE DIFFERENT ANSWERS. A way that never mentions the
            // dimension permits all of it; a way that permits NOTHING permits nothing. An
            // empty `Set` is truthy in the language this is ported from, and an `or`
            // fallback kept the empty one — widening the union to everything and dropping
            // the restriction. It cost dozens of `key_reach` rows their
            // `always_values`.
            match here.get(&k) {
                None => union.extend(info.domain.iter().cloned()),
                Some(allowed) => union.extend(allowed.iter().cloned()),
            }
            // A UNION AS LARGE AS THE DOMAIN ONLY GROWS, and is not published: the ways after it cannot change
            // the answer. Walked to the end, this loop was most of the closure on a large Angular workspace - a key with
            // thousands of ways re-inserted the whole domain once for each.
            if union.len() >= info.domain.len() {
                break;
            }
        }

        // NOTHING PERMITTED ON EVERY WAY IS `in []`, and it is the answer: a key can sit under an outer
        // config whose list does not hold its own member, so it renders for NOBODY. Moved to
        // `unreadable_values`, the dimension would vanish and a reader would take the key as unrestricted.
        if union.len() >= info.domain.len() {
            continue;
        }
        let excluded: Vec<String> =
            info.domain.iter().filter(|v| !union.contains(*v)).cloned().collect();
        let in_side = union.len() <= excluded.len();
        let mut values: Vec<String> =
            if in_side { union.into_iter().collect() } else { excluded };
        values.sort();
        out.push(json!({
            "enum": enum_id, "enum_name": info.name, "dimension": dim,
            "op": if in_side { "in" } else { "not_in" },
            "values": values, "domain": info.domain.len(),
        }));
    }
    sort_by_collation(&mut out);
    out
}

/// THE DIMENSIONS EVERY WAY IN NARROWS THAT NOTHING CAN STATE A VALUE FOR — the complement
/// of `fold_values`, and defined as its complement ON PURPOSE.
///
/// `fold_values` publishes a dimension exactly when the union across ways is SMALLER than
/// the domain. So a dimension restricted on every way whose union is the WHOLE domain is
/// published by neither fold unless this one takes it. Two independent rules left dozens of (key,
/// dimension) pairs in exactly that hole — among them the per-vendor dimension the whole
/// table exists for — because a way can be unresolvable at one gate and PINNED at another.
///
/// THE SILENCE MUST COME FROM UNRESOLVABILITY, not from breadth. If every way knows its own
/// set precisely and the sets merely cover the domain between them, the map is not failing
/// to read anything — the key genuinely renders for everything, and calling that
/// "unreadable" would invent a restriction. So at least one way must permit the whole
/// domain, which is what an `unknown` row contributes.
pub fn unreadable_values(
    ways: &[Way],
    gates: &GateMap,
    idx: &EnumIndex,
    per_way: Option<&PerWay>,
) -> Vec<Value> {
    if ways.is_empty() {
        return Vec::new();
    }
    let computed;
    let per_way = match per_way {
        Some(p) => p,
        None => {
            computed = per_way_allowed(ways, gates, idx);
            &computed
        }
    };

    let mut out = Vec::new();
    for k in dimensions(per_way) {
        let (enum_id, dim) = split(&k);
        let Some(info) = idx.by_id.get(enum_id) else { continue };

        let mut union: IndexSet<String> = IndexSet::new();
        let mut on_every_way = true;
        let mut some_way_knows_nothing = false;
        for here in per_way {
            let Some(allowed) = here.get(&k) else {
                on_every_way = false;
                break;
            };
            if allowed.len() >= info.domain.len() {
                some_way_knows_nothing = true;
            }
            union.extend(allowed.iter().cloned());
        }
        if !on_every_way || !some_way_knows_nothing {
            continue;
        }
        if union.len() < info.domain.len() {
            continue;
        }
        out.push(json!({
            "enum": enum_id, "enum_name": info.name, "dimension": dim,
            "domain": info.domain.len(),
        }));
    }
    sort_by_collation(&mut out);
    out
}

/// BY COLLATION, not by code point. These rows land in `key_reach` columns the 53-table
/// gate compares IN ORDER, so the comparator is the one measured against the runtime.
fn sort_by_collation(rows: &mut [Value]) {
    // STRINGIFIED THE WAY THE RUNTIME DOES IT: a declaration with no name sorts under
    // "None", not under the empty string, so it lands among the N names and not first.
    fn stringly(row: &Value, name: &str) -> String {
        match row.get(name) {
            Some(Value::String(s)) => s.clone(),
            None | Some(Value::Null) => "None".to_string(),
            Some(other) => other.to_string(),
        }
    }
    rows.sort_by(|a, b| {
        let ka = format!("{}{}", stringly(a, "enum_name"), stringly(a, "dimension"));
        let kb = format!("{}{}", stringly(b, "enum_name"), stringly(b, "dimension"));
        sort_key(&ka).cmp(&sort_key(&kb))
    });
}

/// One row per (gate, feature) — a set to intersect, not a JSON blob to parse.
pub fn write_gate_features(
    store: &mut Store<'_>,
    gates: &IndexMap<String, (Option<String>, Vec<String>)>,
) -> usize {
    let mut rows = 0;
    for (gate, (component, features)) in gates {
        for f in features {
            let mut row = Row::new();
            row.insert("gate".into(), Value::String(gate.clone()));
            row.insert("feature".into(), Value::String(f.clone()));
            row.insert(
                "component".into(),
                component.clone().map(Value::String).unwrap_or(Value::Null),
            );
            store.emit("gate_features", row);
            rows += 1;
        }
    }
    rows
}

/// One row per (gate, enum, dimension); `values` is always the SMALLER side, named by `op`.
pub fn write_gate_values(store: &mut Store<'_>, gates: &GateMap, idx: &EnumIndex) -> usize {
    let mut rows = 0;
    for (gate, (component, restrictions)) in gates {
        for r in restrictions {
            let enum_id = field(r, "enum");
            let Some(info) = idx.by_id.get(&enum_id) else { continue };

            let mut row = Row::new();
            row.insert("gate".into(), Value::String(gate.clone()));
            row.insert("enum".into(), Value::String(enum_id.clone()));
            row.insert(
                "enum_name".into(),
                info.name.clone().map(Value::String).unwrap_or(Value::Null),
            );
            row.insert("dimension".into(), Value::String(field(r, "dim")));
            // FALSY, not merely absent: a zero, an empty string and a missing key all
            // land as null here, which is what `or None` does on the other side.
            let dim_row = match r.get("row") {
                Some(Value::String(s)) if !s.is_empty() => Value::String(s.clone()),
                Some(Value::Number(n)) if n.as_f64() != Some(0.0) => Value::Number(n.clone()),
                Some(Value::Bool(true)) => Value::Bool(true),
                _ => Value::Null,
            };
            row.insert("dimension_row".into(), dim_row);
            row.insert("op".into(), Value::String(field(r, "op")));
            // COMPACT, unlike a cell the row store writes: this one names its separators
            // `(",", ":")`, so the space python puts after a comma by default is absent
            // here and serde_json's own compact form is the right one.
            let values = r.get("values").cloned().unwrap_or_else(|| json!([]));
            row.insert(
                "values_json".into(),
                Value::String(serde_json::to_string(&values).unwrap_or_else(|_| "[]".into())),
            );
            row.insert("domain".into(), Value::from(info.domain.len() as i64));
            row.insert(
                "component".into(),
                component.clone().map(Value::String).unwrap_or(Value::Null),
            );
            // LAST, so no column a reader indexes by position moves.
            // WHAT THE SOURCE LISTS, beside what the gate permits: a config object's members for
            // the dimension, resolved whatever the `op` (an exclusion list is `unknown` - the map
            // cannot say whether it hides or disables - and still names its members). Null when
            // no config object lists the dimension. Added, never folded into `values_json`,
            // whose `unknown` has always meant `[]`.
            row.insert(
                "listed_json".into(),
                match r.get("listed") {
                    Some(listed @ Value::Array(_)) => Value::String(
                        serde_json::to_string(listed).unwrap_or_else(|_| "[]".into()),
                    ),
                    _ => Value::Null,
                },
            );
            store.emit("gate_values", row);
            rows += 1;
        }
    }
    rows
}

#[cfg(test)]
#[allow(non_snake_case)]
#[path = "value_folds_tests.rs"]
mod tests;
