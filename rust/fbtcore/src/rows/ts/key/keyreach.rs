//! WHERE EVERY TRANSLATION KEY CAN BE TRAVELLED TO FROM A ROUTED ROOT — the four derived
//! tables, written into the store that already holds the rows they come from.
//!
//! IT RUNS HERE AND NOT IN NODE, and that is the whole point of the move: it is derived
//! from EVERY other table, so it needs every row. Keeping the store alive in node is fine
//! on a full run and impossible on an incremental one — node cannot read this database, and
//! handing the rows back costs about as much JSON as a `JSON.parse` can hold at
//! all.
//!
//! A KEY IS REACHABLE MORE THAN ONE WAY HALF THE TIME, so there is no "the" gate set, and
//! three columns say different things about it. THE UNIT OF ENUMERATION IS (REF, PATH): one
//! ref's own in-template gates joined to the gates of THAT path, never to another site's.
//!   `always_gates`       intersection over every (ref, path) — the ONLY column a consumer
//!                        may read as NECESSARY
//!   `maybe_gates`        union over the same — holds on SOME way in, not a requirement
//!   `path_always_gates`  intersection over the PATHS alone, own gates excluded — what the
//!                        render tree demands of the component, independent of where in the
//!                        template the key sits
//!
//! The first version of the tool this comes from got it wrong in a way worth stating,
//! because the numbers it produced looked fine: every ref's own chain was collected into ONE
//! flat list and unioned into every path of every component, so the intersection inherited
//! gates from sites the path never passes through and `always_gates` claimed conditions that
//! do not always hold. Read one path, call the key "requires feature X", and you are wrong
//! for more than one key in four.
//!
//! Two more columns say the same thing about MEANING rather than row identity, because an
//! intersection over ids is blind to ONE CONDITION WRITTEN TWICE:
//! `always_features`/`maybe_features` fold `gate_features` over the SAME (ref, path) sets.
//!
//! A KEY ASSIGNED IN TYPESCRIPT CARRIES THE `if`/`else` AROUND ITS WRITE (`key_branches`).
//! Those branches ride in the same (ref, path) sets, so the value and feature folds read
//! them like any gate, but they are published apart — `always_branches`/`maybe_branches`,
//! holding `branches` and `switch_cases` ids — because a `gates` column names `gates` rows
//! and nothing else.
//!
//! THERE IS NO PATH CAP. A cap of a few thousand paths per component looks safe, but on a
//! large workspace a single component exceeds it, so the cap hid real paths and bought nothing;
//! on another Angular tree one component rendered from everywhere holds most of the table, and the
//! cap would hide most of its paths. TWO TREES, NOT ONE NUMBER. Dropping the cap is a deliberate divergence from the first version.
//! `truncated` stays a column because a consumer filters on it, and is now always 0.

use super::closure::{fold, load_edges, locale_index, reach_index, render_paths, Paths};
use super::gate_cases::case_values;
use super::gate_config;
use super::gate_directives;
use super::gate_features::gate_features_with;
use super::gate_switch;
use super::gate_values::{enum_index, gate_values_with, member_enums, EnumIndex};
use super::key_branches::Branches;
use super::key_fields::field_sites;
use super::key_literals::literal_gates;
use super::key_routes::{key_routes, Route};
use super::sourcetrace::{source_trace, Trace};
use super::store::{Row, Store};
use super::value_folds::{
    fold_values, per_way_allowed, unreadable_values, write_gate_features, write_gate_values,
};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;
use std::path::Path;

/// WHICH FILES ARE CARRIED AS TEXT. The same set the row writer carries, stated in both
/// places because neither can see the other's.
const TEXT_EXTS: &[&str] = &[
    "ts", "html", "scss", "js", "json", "css", "md", "yaml", "xml", "properties", "config", "sh",
];

/// THE SOURCE ITSELF, for the one question the rows cannot answer.
///
/// The dead-key trace reads raw text, and the rows do not carry it. `file_text` is written
/// by the row writer, which reads the same files off disk; re-reading here costs less than
/// carrying tens of MB through a payload.
///
/// NEWLINE TRANSLATION IS OFF — the bytes have to be what the file holds, because every
/// offset and every `\r` strip in the scan depends on it. This language does not translate
/// on read, so nothing has to be turned off; it is stated because the language it was
/// ported from does.
pub fn read_texts(store: &Store<'_>, root: &Path) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    for f in store.table("files").iter() {
        let ext = match f.get("ext") {
            Some(Value::String(s)) => s.to_lowercase(),
            _ => String::new(),
        };
        let Some(Value::String(path)) = f.get("path") else { continue };
        if path.is_empty() || !TEXT_EXTS.contains(&ext.as_str()) {
            continue;
        }
        // A FILE OUTSIDE THE WORKSPACE IS NOT THIS APPLICATION'S SOURCE. The compiler's own
        // library is a program file and therefore a `files` row, reached through a borrowed
        // node_modules with a path that escapes the root — and scanning it told the
        // dead-key trace that `confidence` and `temporary` are words the workspace uses,
        // when the only place they appear is `lib.dom.d.ts`. Some keys read `leaf` where
        // the truth is `absent`.
        if path.starts_with("..") {
            continue;
        }
        let full = root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        // A file that cannot be read contributes NOTHING rather than an empty string: "" is
        // a real answer for an empty file and must not also mean "unreadable".
        if let Ok(bytes) = std::fs::read(&full) {
            out.insert(path.clone(), String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    out
}

/// What each pass found, because a rule that silently stops matching looks exactly like a
/// frontend that stopped using the shape it matches.
#[derive(Debug, Default)]
pub struct Closure {
    pub path_rows: usize,
    pub components: usize,
    pub keys: usize,
    /// Route name -> how many keys took it.
    pub counts: IndexMap<String, usize>,
    /// Trace name -> how many dead keys it explained.
    pub traces: IndexMap<String, usize>,
    pub gate_feature_rows: usize,
    pub gate_value_rows: usize,
}

const ROUTES: &[&str] = &[
    "template",
    "template_string",
    "field_binding",
    "pipe",
    "ts_call",
    "route_path",
    "literal",
    "referenced",
    "none",
];
const TRACES: &[&str] = &["full_literal", "unparsed_file", "prefix", "leaf", "absent"];

/// The three restriction sources that are not a comparison, MERGED BY GATE.
///
/// A gate may carry a config row, a switch arm and a directive at once, and the collapse
/// folds them together exactly as it folds two comparisons — a consumer cannot tell which
/// source proved a restriction, and does not need to.
pub fn extra_restrictions(store: &Store<'_>, idx: &EnumIndex) -> IndexMap<String, Vec<Value>> {
    let mem = member_enums(store, idx);
    let (cfg, _) = gate_config::config_restrictions(store, &mem, idx);
    let (sw, _) = gate_switch::switch_restrictions(store, &mem, idx);
    let (dir, _) = gate_directives::directive_restrictions(store, &mem, idx);
    let guards = super::gate_guards::guard_restrictions(store, &mem, idx);
    let mut extra = cfg;
    for (gate, rows) in sw.into_iter().chain(dir).chain(guards) {
        extra.entry(gate).or_default().extend(rows);
    }
    extra
}

/// What the closure derived beyond its rows: the trace, read by the differential.
pub struct Derived {
    pub trace: Trace,
}

pub fn build_closure(
    store: &mut Store<'_>,
    checks: &[String],
    enum_name: &str,
    root: &Path,
    note: &mut dyn FnMut(&str),
) -> (Closure, Derived) {
    let stage = crate::trace::stage("closure: branches");
    let step = crate::trace::stage("branches: edges");
    let edges = load_edges(store);
    drop(step);
    let step = crate::trace::stage("branches: index");
    let branches = Branches::new(store);
    let idx0 = enum_index(store);
    drop(step);
    let conds = branches.conds(store, &idx0);
    drop(stage);
    let stage = crate::trace::stage("closure: gate_features");
    // RESOLVED MEANING, folded over the SAME (ref, path) sets as the ids. It runs BEFORE the
    // routes because the `literal` route needs the ternary writes it proves.
    let gf = gate_features_with(store, checks, enum_name, &conds, note);
    drop(stage);
    let stage = crate::trace::stage("closure: routes");
    let lit_gates = literal_gates(store, &gf.ternary_writes);
    let (fields, _) = field_sites(store);
    let routes = key_routes(store, &lit_gates, &fields);
    let locales = locale_index(store);

    // Paths per COMPONENT, computed once for every component any key reaches, then shared by
    // all its keys. Summed over the keyed render sites the paths come to several times as many as
    // keyed by the COMPONENT they all pass through.
    let mut needed: IndexSet<String> = IndexSet::new();
    for hit in routes.values() {
        for comp in hit.sites.keys().flatten() {
            needed.insert(comp.clone());
        }
    }
    drop(stage);
    let stage = crate::trace::stage("closure: render_paths");
    let paths = render_paths(store, &edges, &needed);
    drop(stage);
    let stage = crate::trace::stage("closure: gate_values");

    let gf_rows = write_gate_features(store, &gf.map);
    let extra = extra_restrictions(store, &idx0);
    let (gv, idx) = gate_values_with(store, &extra, &conds);
    let gv_rows = write_gate_values(store, &gv.map, &idx);
    // THE FOLDS READ GATES AND BRANCHES ALIKE; only the published tables are gates alone.
    let mut feature_map = gf.map.clone();
    feature_map.extend(gf.conds.iter().map(|(id, f)| (id.clone(), (None, f.clone()))));
    let mut value_map = gv.map.clone();
    value_map.extend(gv.conds.clone());
    drop(stage);
    let stage = crate::trace::stage("closure: values");
    let mem = member_enums(store, &idx);
    value_map.extend(case_values(store, &mem, &idx));
    // A KEY BUILT FROM AN ENUM MEMBER'S NAME requires that member - see `key_built`.
    let links: IndexSet<String> = routes.values()
        .flat_map(|h| h.sites.values().flatten().flatten())
        .filter(|id| id.starts_with("tpl:"))
        .cloned()
        .collect();
    value_map.extend(super::key_built::built_values(store, &links, &mem, &idx));

    drop(stage);
    let stage = crate::trace::stage("closure: reach and trace");
    let reach = reach_index(store);
    let dead = super::key_dead::dead_gates(store);
    let trace = source_trace(store, &read_texts(store, root));

    drop(stage);
    let _keys = crate::trace::stage("closure: key_reach");
    let mut counts: IndexMap<String, usize> =
        ROUTES.iter().map(|r| (r.to_string(), 0usize)).collect();
    let mut traces: IndexMap<String, usize> =
        TRACES.iter().map(|t| (t.to_string(), 0usize)).collect();

    for (key, loc) in &locales {
        let hit = routes.get(key);
        // The map publishes its OWN `referenced` flag over the whole source, counted by an
        // exact literal join. A key it calls referenced that none of the routes located is
        // referenced somewhere this closure cannot place — a service, a guard, a model — so
        // it is `referenced`, never `none`. Calling it dead would contradict the map on its
        // own evidence, and taking only the narrowest route made thousands of referenced keys
        // look dead.
        let route = match hit {
            Some(h) => h.route.clone(),
            None if loc.referenced > 0 => "referenced".to_string(),
            None => "none".to_string(),
        };
        *counts.entry(route.clone()).or_default() += 1;

        let (tr, tr_file) = if route == "none" {
            let (how, file) = trace.of(key);
            *traces.entry(how.name().to_string()).or_default() += 1;
            (Some(how.name().to_string()), file)
        } else {
            (None, None)
        };

        let mut comps: Vec<String> =
            hit.map(|h| h.sites.keys().flatten().cloned().collect()).unwrap_or_default();
        comps.sort();

        let ways = ways_of(&comps, hit, &paths, &reach, &dead);
        let Ways { sets, path_sets, roots, mods, rts, depth, cut } = ways;

        let (always, always_br) = split_branches(fold(&sets).0, &branches);
        let (maybe, maybe_br) = split_branches(fold(&sets).1, &branches);
        let (path_always, _) = fold(&path_sets);
        let feature_sets: Vec<IndexSet<String>> =
            sets.iter().map(|s| features_of(s, &feature_map)).collect();
        let (always_f, maybe_f) = fold(&feature_sets);
        // ONE WALK OVER THE WAYS, read by both folds — they ask the same question of the
        // same sets, and computing them twice per key was nearly twice the walks one does.
        let ways: Vec<IndexSet<String>> = sets.clone();
        let per_way = per_way_allowed(&ways, &value_map, &idx);
        let values = fold_values(&ways, &value_map, &idx, Some(&per_way));
        // The dimensions the gates narrow but the map cannot read. BESIDE the resolved ones,
        // never inside them: an `unknown` row folds to the whole domain and would otherwise
        // leave no trace at all.
        let unreadable = unreadable_values(&ways, &value_map, &idx, Some(&per_way));

        let mut row = Row::new();
        row.insert("key".into(), Value::String(key.clone()));
        row.insert("locales".into(), Value::from(loc.list.clone()));
        row.insert("route".into(), Value::String(route));
        row.insert("n_refs".into(), Value::from(hit.map(|h| h.refs as i64).unwrap_or(0)));
        row.insert("components".into(), Value::from(comps.clone()));
        row.insert("n_paths".into(), Value::from(sets.len() as i64));
        row.insert("min_depth".into(), depth.map(Value::from).unwrap_or(Value::Null));
        row.insert("roots".into(), Value::from(sorted(roots)));
        row.insert("modules".into(), Value::from(sorted(mods)));
        row.insert("routes".into(), Value::from(sorted(rts)));
        row.insert("always_gates".into(), Value::from(always));
        row.insert("maybe_gates".into(), Value::from(maybe));
        row.insert("path_always_gates".into(), Value::from(path_always));
        row.insert("always_branches".into(), Value::from(always_br));
        // THE SIDE OF EACH, spelled out: the id alone says it only as a `!` prefix on a ternary's
        // `else` arm, and a branch's sense is a lookup away in another table.
        let senses: serde_json::Map<String, Value> =
            maybe_br.iter().map(|id| (id.clone(), Value::from(branches.sense(id)))).collect();
        row.insert("maybe_branches".into(), Value::from(maybe_br));
        row.insert("branch_senses".into(), Value::Object(senses));
        row.insert("always_features".into(), Value::from(always_f));
        row.insert("maybe_features".into(), Value::from(maybe_f));
        row.insert("always_values".into(), Value::Array(values));
        row.insert("unreadable_values".into(), Value::Array(unreadable));
        row.insert("trace".into(), tr.map(Value::String).unwrap_or(Value::Null));
        row.insert("trace_file".into(), tr_file.map(Value::String).unwrap_or(Value::Null));
        row.insert("truncated".into(), Value::from(i64::from(cut)));
        store.emit("key_reach", row);
    }

    let stats = Closure {
        path_rows: paths.rows,
        components: needed.len(),
        keys: locales.len(),
        counts,
        traces,
        gate_feature_rows: gf_rows,
        gate_value_rows: gv_rows,
    };
    (stats, Derived { trace })
}

/// TWO COLLECTIONS, because they answer different questions. `sets` is one entry per
/// (REF, PATH) — the ref's own in-template gates joined to the gates of THAT path, which is
/// what "every way this key renders" actually enumerates. `path_sets` is one entry per PATH
/// alone, so the intersection over it states what the RENDER TREE requires of the component,
/// independent of where in the template the key sits.
#[derive(Default)]
pub struct Ways {
    pub sets: Vec<IndexSet<String>>,
    pub path_sets: Vec<IndexSet<String>>,
    pub roots: IndexSet<String>,
    pub mods: IndexSet<String>,
    pub rts: IndexSet<String>,
    pub depth: Option<i64>,
    pub cut: bool,
}

/// Every way one key renders, enumerated as (REF, PATH) pairs.
///
/// THE PAIRING IS THE RULE. Collected into one flat list instead, the ref's gates were
/// unioned into every path of every component, so the intersection inherited gates from
/// sites the path never passes through and `always_gates` claimed conditions that do not
/// always hold — wrong for more than one key in four.
pub fn ways_of(
    comps: &[String],
    hit: Option<&Route>,
    paths: &Paths,
    reach: &IndexMap<String, (Vec<String>, Vec<String>)>,
    dead: &IndexSet<String>,
) -> Ways {
    let mut w = Ways::default();
    for c in comps {
        let Some((comp_paths, truncated)) = paths.by_comp.get(c) else { continue };
        w.cut = w.cut || *truncated;
        let own: &[Vec<String>] = match hit.and_then(|h| h.sites.get(&Some(c.clone()))) {
            Some(chains) if !chains.is_empty() => chains,
            // A KEY WITH NO OWN CHAIN STILL RENDERS: one empty chain, so the path's own
            // gates are enumerated rather than the whole component dropped.
            _ => std::slice::from_ref(&EMPTY_CHAIN),
        };
        for p in comp_paths {
            // A WAY THROUGH A LITERAL `false` IS NO WAY (`key_dead`): not a path, not a root, not counted.
            let chains: Vec<&Vec<String>> =
                own.iter().filter(|c| !c.iter().any(|g| dead.contains(g))).collect();
            if chains.is_empty() || p.gates.iter().any(|g| dead.contains(&**g)) {
                continue;
            }
            // The ids are shared handles inside the walk - see `closure::load_edges`. The
            // answers this builds are named, so the text is taken here, once per path.
            let root = p.hops[0].to_string();
            w.roots.insert(root.clone());
            if let Some((modules, routes_of)) = reach.get(&root) {
                w.mods.extend(modules.iter().cloned());
                w.rts.extend(routes_of.iter().cloned());
            }
            let hops = p.hops.len() as i64 - 1;
            w.depth = Some(match w.depth {
                None => hops,
                Some(d) => d.min(hops),
            });
            w.path_sets.push(p.gates.iter().map(|g| g.to_string()).collect());
            for chain in chains {
                let mut one: IndexSet<String> =
                    p.gates.iter().map(|g| g.to_string()).collect();
                one.extend(chain.iter().cloned());
                w.sets.push(one);
            }
        }
    }
    w
}

/// The one empty chain a site with no gates of its own contributes.
static EMPTY_CHAIN: Vec<String> = Vec::new();

fn sorted(set: IndexSet<String>) -> Vec<String> {
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort();
    out
}

/// A folded id list, as (the gates, the branches) — each keeps the fold's order.
fn split_branches(ids: Vec<String>, branches: &Branches) -> (Vec<String>, Vec<String>) {
    ids.into_iter().partition(|id| !branches.contains(id))
}

/// AN INTERSECTION OVER IDS IS BLIND TO ONE CONDITION WRITTEN TWICE, so the feature columns
/// fold the MEANING of the same (ref, path) sets rather than their row identity.
fn features_of(
    gates: &IndexSet<String>,
    map: &IndexMap<String, (Option<String>, Vec<String>)>,
) -> IndexSet<String> {
    let mut out = IndexSet::new();
    for g in gates {
        if let Some((_, features)) = map.get(g) {
            out.extend(features.iter().cloned());
        }
    }
    out
}

#[cfg(test)]
#[allow(non_snake_case)]
#[path = "keyreach_tests.rs"]
mod tests;
