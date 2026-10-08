//! The TypeScript half: the rows it stores and the closure it derives from them.
//!
//! Ported from earlier python and node implementations of the same passes, and before that
//! from the predecessor tool. The bar was the map those passes produced, table for table and
//! row for row, and it was met before the earlier ends were deleted.
//!

pub mod astreads;
pub mod closure;
pub mod feature_writes;
pub mod featurewalk;
// TWO FOLDERS BY `#[path]`, NOT TWO MODULES. The folder held 19 files against a limit of 15 the day
// rust was first measured; moving the files kept every module where it was, so no `super::` in any
// of them had to change. `gate/` is the gate passes, `key/` the four key passes.
#[path = "gate/ts/gate_calls.rs"]
pub mod gate_calls;
#[path = "gate/ts/gate_cases.rs"]
pub mod gate_cases;
#[path = "gate/gate_collapse.rs"]
pub mod gate_collapse;
#[path = "gate/gate_config.rs"]
pub mod gate_config;
#[path = "gate/gate_directives.rs"]
pub mod gate_directives;
#[path = "gate/gate_features.rs"]
pub mod gate_features;
#[path = "gate/ts/gate_guard_eval.rs"]
pub mod gate_guard_eval;
#[path = "gate/ts/gate_guards.rs"]
pub mod gate_guards;
#[path = "gate/ts/gate_lists.rs"]
pub mod gate_lists;
#[path = "gate/gate_lookups.rs"]
pub mod gate_lookups;
#[path = "gate/gate_predicates.rs"]
pub mod gate_predicates;
#[path = "gate/ts/gate_props.rs"]
pub mod gate_props;
#[path = "gate/ts/gate_input_flags.rs"]
pub mod gate_input_flags;
#[path = "gate/gate_switch.rs"]
pub mod gate_switch;
#[path = "gate/ts/gate_tsrows.rs"]
pub mod gate_tsrows;
#[path = "gate/gate_values.rs"]
pub mod gate_values;
pub mod jsstr;
#[path = "key/key_branches.rs"]
pub mod key_branches;
#[path = "key/key_built.rs"]
pub mod key_built;
#[path = "key/key_fields.rs"]
pub mod key_fields;
#[path = "key/keyreach.rs"]
pub mod keyreach;
#[path = "key/key_literals.rs"]
pub mod key_literals;
#[path = "key/key_routes.rs"]
pub mod key_routes;
pub mod sourcescan;
pub mod sourcetrace;
pub mod store;
pub mod value_folds;

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::path::Path;

/// The route table this half derives, as JSON, for holding against the python pass.
///
/// NOTHING IS HANDED IN. The proven ternary writes come from `gate_features`, which is
/// ported too, so the whole chain behind the route table is this crate's own work.
pub fn routes_json(db: &Path, half: &str, checks: &[String], enum_name: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db)?;
    let store = store::Store::from_db(&conn, half)?;
    let gf = gate_features::gate_features(&store, checks, enum_name, &mut |_| {});
    let lit_gates = key_literals::literal_gates(&store, &gf.ternary_writes);
    let (fields, stats) = key_fields::field_sites(&store);
    eprintln!(
        "field sites: {} field(s), {} node(s), {} site(s)",
        stats.fields, stats.nodes, stats.sites
    );
    let routes = key_routes::key_routes(&store, &lit_gates, &fields);

    let mut out = Map::new();
    for (key, hit) in &routes {
        let mut sites = Map::new();
        for (comp, gate_lists) in &hit.sites {
            let name = comp.clone().unwrap_or_else(|| "\u{0}none".to_string());
            sites.insert(name, json!(gate_lists));
        }
        out.insert(
            key.clone(),
            json!({
                "route": hit.route,
                "refs": hit.refs,
                "sites": Value::Object(sites),
                "nodes": hit.nodes.iter().collect::<Vec<_>>(),
            }),
        );
    }
    Ok(serde_json::to_string(&Value::Object(out))?)
}

/// The gates a bare `.ts` key literal renders under, as JSON, for holding against the
/// python pass.
///
/// The proven ternary writes come from `gate_features` rather than from the caller: that
/// pass is ported, so there is nothing left to hand in.
pub fn literal_gates_json(db: &Path, half: &str, checks: &[String], enum_name: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db)?;
    let store = store::Store::from_db(&conn, half)?;
    let gf = gate_features::gate_features(&store, checks, enum_name, &mut |_| {});
    let gates = key_literals::literal_gates(&store, &gf.ternary_writes);
    let mut out = Map::new();
    for (key, ids) in gates {
        out.insert(key, json!(ids));
    }
    Ok(serde_json::to_string(&Value::Object(out))?)
}


/// What every template gate necessarily requires, as JSON, for holding against the python
/// pass. Nothing is handed in here: this is the pass that PRODUCES `ternary_writes`.
pub fn gate_features_json(db: &Path, half: &str, checks: &[String], enum_name: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db)?;
    let store = store::Store::from_db(&conn, half)?;
    let mut notes: Vec<String> = Vec::new();
    let gf = gate_features::gate_features(&store, checks, enum_name, &mut |n| {
        notes.push(n.to_string())
    });

    let mut map = Map::new();
    for (gate, (component, features)) in &gf.map {
        map.insert(
            gate.clone(),
            json!({"component": component, "features": features}),
        );
    }
    let mut writes = Map::new();
    for (id, row) in &gf.ternary_writes {
        writes.insert(id.clone(), Value::Object(row.to_map()));
    }
    Ok(serde_json::to_string(&json!({
        "map": Value::Object(map),
        "checks": gf.checks.len(),
        "props": gf.props,
        "direct": gf.direct,
        "callbacks": gf.callbacks,
        "ternaries": gf.ternaries,
        "ternary_writes": Value::Object(writes),
        "notes": notes,
    }))?)
}

/// Which values every template gate still permits, as JSON, for holding against the python
/// pass.
///
/// ALL FOUR SOURCES ARE LIVE: the comparison walk, the CONFIG OBJECT, the NGSWITCH ARM and
/// the STRUCTURAL DIRECTIVE. Each reports its own counts, because a rule that silently stops
/// matching looks exactly like a frontend that stopped using the shape it matches.
pub fn gate_values_json(db: &Path, half: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db)?;
    let store = store::Store::from_db(&conn, half)?;
    let idx0 = gate_values::enum_index(&store);
    let mem = gate_values::member_enums(&store, &idx0);
    let (cfg, cfg_stats) = gate_config::config_restrictions(&store, &mem, &idx0);
    let (sw, sw_stats) = gate_switch::switch_restrictions(&store, &mem, &idx0);
    let (dir, dir_stats) = gate_directives::directive_restrictions(&store, &mem, &idx0);

    // The sources merge by gate: a gate may carry a config row, a switch arm and a directive
    // at once, and the collapse folds them together exactly as it folds two comparisons.
    let guards = gate_guards::guard_restrictions(&store, &mem, &idx0);
    let mut extra = cfg;
    for (gate, rows) in sw.into_iter().chain(dir).chain(guards) {
        extra.entry(gate).or_default().extend(rows);
    }
    let (gv, idx) = gate_values::gate_values(&store, &extra);

    let mut map = Map::new();
    for (gate, (component, rows)) in &gv.map {
        map.insert(gate.clone(), json!({"component": component, "rows": rows}));
    }
    Ok(serde_json::to_string(&json!({
        "map": Value::Object(map),
        "enums": idx.by_id.len(),
        "config": {"gates": cfg_stats.gates, "resolved": cfg_stats.resolved,
                   "unknown": cfg_stats.unknown, "classes": cfg_stats.classes},
        "switch": {"switches": sw_stats.switches, "gates": sw_stats.gates,
                   "resolved": sw_stats.resolved, "unknown": sw_stats.unknown,
                   "unplaced": sw_stats.unplaced, "nameless": sw_stats.nameless},
        "directives": {"classes": dir_stats.classes, "inert": dir_stats.inert,
                       "gates": dir_stats.gates, "resolved": dir_stats.resolved,
                       "unknown": dir_stats.unknown},
    }))?)
}

/// The `render_path` rows — every way in to every component any key reaches — as JSON, for
/// holding against the python pass.
///
/// This is the largest table the closure derives (hundreds of thousands of rows on a large workspace)
/// and the one the `(ref, path)` enumeration is built on, so it is compared row for row
/// and IN ORDER: `render_path.id` is positional, and `key_reach` joins against it.
/// The `key_reach` rows — the whole closure — as JSON, for holding against the python pass.
///
/// This is the table the other three exist to fill, so it is the last differential and the
/// one that covers every column. `root` is the workspace the dead-key trace reads its text
/// from; without it the trace answers `absent` for everything.
fn tally(counts: &indexmap::IndexMap<String, usize>) -> Value {
    let mut out = Map::new();
    for (name, n) in counts {
        out.insert(name.clone(), Value::from(*n as i64));
    }
    Value::Object(out)
}

pub fn key_reach_json(
    db: &Path,
    half: &str,
    checks: &[String],
    enum_name: &str,
    root: &Path,
) -> Result<String> {
    let conn = rusqlite::Connection::open(db)?;
    let mut store = store::Store::from_db(&conn, half)?;
    let (stats, derived) =
        keyreach::build_closure(&mut store, checks, enum_name, root, &mut |_| {});

    let rows: Vec<Value> = store
        .emitted
        .get("key_reach")
        .map(|rows| rows.iter().map(|r| Value::Object(r.to_map())).collect())
        .unwrap_or_default();

    Ok(serde_json::to_string(&json!({
        "rows": rows,
        "path_rows": stats.path_rows,
        "components": stats.components,
        "keys": stats.keys,
        "counts": tally(&stats.counts),
        "traces": tally(&stats.traces),
        "gate_features": stats.gate_feature_rows,
        "gate_values": stats.gate_value_rows,
        "scanned": derived.trace.scanned,
        "comment_lines": derived.trace.comment_lines,
        "locales": derived.trace.locales,
    }))?)
}

pub fn render_paths_json(db: &Path, half: &str, checks: &[String], enum_name: &str) -> Result<String> {
    let conn = rusqlite::Connection::open(db)?;
    let store = store::Store::from_db(&conn, half)?;

    let gf = gate_features::gate_features(&store, checks, enum_name, &mut |_| {});
    let lit_gates = key_literals::literal_gates(&store, &gf.ternary_writes);
    let (fields, _) = key_fields::field_sites(&store);
    let routes = key_routes::key_routes(&store, &lit_gates, &fields);

    // Paths per COMPONENT, computed once for every component any key reaches.
    let mut needed: indexmap::IndexSet<String> = indexmap::IndexSet::new();
    for hit in routes.values() {
        for comp in hit.sites.keys().flatten() {
            needed.insert(comp.clone());
        }
    }

    let edges = closure::load_edges(&store);
    let mut emitting = store::Store::from_db(&conn, half)?;
    let paths = closure::render_paths(&mut emitting, &edges, &needed);

    let rows: Vec<Value> = emitting
        .emitted
        .get("render_path")
        .map(|rows| rows.iter().map(|r| Value::Object(r.to_map())).collect())
        .unwrap_or_default();

    Ok(serde_json::to_string(&json!({
        "rows": rows,
        "components": paths.by_comp.len(),
        "path_rows": paths.rows,
        "needed": needed.len(),
    }))?)
}
