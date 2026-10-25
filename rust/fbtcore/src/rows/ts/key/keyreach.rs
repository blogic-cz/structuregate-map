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
use super::key_bound::{bind_links, bound_conds, Bound};
use super::key_branches::Branches;
use super::key_fields::field_sites;
use super::key_literals::literal_gates;
use super::key_routes::{key_routes, Route};
use super::key_scopes::Scopes;
use super::sourcetrace::{source_trace, Trace};
use super::store::{Row, Store};
use super::value_folds::{
    fold_sided, merge_aliased, per_way_keyed, relabel, unreadable_values, write_gate_features, write_gate_values, Aliases, Classes, Labels, Renames, SetSides,
};
use crate::trace::retained;
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
/// carrying tens of MB through a payload. It reads the DISK and never `file_text`, so a map
/// built with `--map-no-text` traces the same (`rows/cells/text.rs`).
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
    /// `control_options` rows: the option keys form controls are offered (#117).
    pub option_rows: usize,
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
pub fn extra_restrictions(
    store: &Store<'_>,
    idx: &EnumIndex,
) -> (IndexMap<String, Vec<Value>>, IndexMap<String, Vec<Value>>) {
    let step = crate::trace::stage("restrictions: member enums");
    let mem = retained!("member enums (restrictions)", member_enums(store, idx));
    crate::trace::metrics_mark!("restrictions: member enums");
    drop(step);
    let step = crate::trace::stage("restrictions: configs");
    let (cfg, per_loop, _) = retained!("config restrictions", gate_config::config_restrictions_all(store, &mem, idx));
    crate::trace::metrics_mark!("restrictions: configs");
    drop(step);
    let step = crate::trace::stage("restrictions: switches");
    let (sw, _) = retained!("switch restrictions", gate_switch::switch_restrictions(store, &mem, idx));
    crate::trace::metrics_mark!("restrictions: switches");
    drop(step);
    let step = crate::trace::stage("restrictions: directives");
    let (dir, _) = retained!("directive restrictions", gate_directives::directive_restrictions(store, &mem, idx));
    crate::trace::metrics_mark!("restrictions: directives");
    drop(step);
    let _step = crate::trace::stage("restrictions: guards");
    let guards = retained!("guard restrictions", super::gate_guards::guard_restrictions(store, &mem, idx));
    crate::trace::metrics_mark!("restrictions: guards");
    let mut extra = cfg;
    for (gate, rows) in sw.into_iter().chain(dir).chain(guards) {
        extra.entry(gate).or_default().extend(rows);
    }
    crate::trace::metrics_gone!("member enums (restrictions)");
    (extra, per_loop)
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
    let step = crate::trace::stage("branches: index");
    // BUILT ONCE FOR EVERY PASS THAT ASKS (`Store::shared`): six passes made their own, each over every row.
    let shared_branches = retained!("branches", store.shared("branches", Branches::new));
    crate::trace::metrics_mark!("branches: index");
    let branches: &Branches = &shared_branches;
    let idx0 = retained!("enum index", enum_index(store));
    crate::trace::metrics_mark!("branches: enum index");
    drop(step);
    let step = crate::trace::stage("branches: edges");
    let edges = retained!("edges", load_edges(store, &branches));
    crate::trace::metrics_mark!("branches: edges");
    drop(step);
    let conds = retained!("branch conds", branches.conds(store, &idx0));
    crate::trace::metrics_mark!("closure: branches");
    drop(stage);
    let stage = crate::trace::stage("closure: gate_features");
    // RESOLVED MEANING, folded over the SAME (ref, path) sets as the ids. It runs BEFORE the
    // routes because the `literal` route needs the ternary writes it proves.
    let gf = retained!("gate features", gate_features_with(store, checks, enum_name, &conds, note));
    crate::trace::metrics_mark!("closure: gate_features");
    drop(stage);
    let stage = crate::trace::stage("closure: routes");
    let step = crate::trace::stage("routes: literal gates");
    let lit_gates = retained!("literal gates", literal_gates(store, &gf.ternary_writes));
    crate::trace::metrics_mark!("routes: literal gates");
    drop(step);
    let step = crate::trace::stage("routes: field sites");
    let (fields, _) = retained!("field sites", field_sites(store));
    crate::trace::metrics_mark!("routes: field sites");
    drop(step);
    let step = crate::trace::stage("routes: key routes");
    let routes = retained!("routes", key_routes(store, &lit_gates, &fields));
    drop(step);
    let step = crate::trace::stage("routes: locales");
    let locales = retained!("locales", locale_index(store));
    drop(step);

    // Paths per COMPONENT, computed once for every component any key reaches, then shared by
    // all its keys. Summed over the keyed render sites the paths come to several times as many as
    // keyed by the COMPONENT they all pass through.
    let mut needed: IndexSet<String> = IndexSet::new();
    for hit in routes.values() {
        for comp in hit.sites.keys().flatten() {
            needed.insert(comp.clone());
        }
    }
    crate::trace::metrics_mark!("closure: routes");
    drop(stage);
    // THE ROUTES READ EVERY CALL'S ARGUMENTS AND EVERY RETURN'S VALUE, the passes after them a few (`Store::forget_decoded`).
    // The other trees they read, the restrictions read again - every one of them - so those stay.
    store.forget_decoded(&[("calls", "args"), ("returns", "value")]);
    crate::give_back();
    let stage = crate::trace::stage("closure: render_paths");
    let paths = retained!("render paths", render_paths(store, &edges, &needed));
    crate::trace::metrics_mark!("closure: render_paths");
    drop(stage);
    let stage = crate::trace::stage("closure: gate_values");

    // EVERY STEP A SPAN: most of this stage was in none of its children.
    let step = crate::trace::stage("gate_values: write gate_features");
    let gf_rows = retained!("emitted gate_features", write_gate_features(store, &gf.map));
    drop(step);
    let step = crate::trace::stage("gate_values: config restrictions");
    let (extra, per_loop) = retained!("extra restrictions", extra_restrictions(store, &idx0));
    drop(step);
    // A FLAG THE PARENTS BIND DIFFERENTLY IS ONE CONDITION PER RENDER EDGE (`key_bound`).
    let step = crate::trace::stage("gate_values: bound flags");
    let bound = retained!("bound flags", bound_conds(store));
    drop(step);
    let mut conds = conds;
    conds.extend(bound.conds.clone());
    // A CALLEE'S BRANCH CARRIED THROUGH A CALL, its parameters read as the caller's arguments (`key_args`).
    let step = crate::trace::stage("gate_values: carried arguments");
    let carried = routes.values().flat_map(|h| h.sites.values().flatten().flatten());
    let (arg_found, arg_params) = retained!("carried arguments", super::key_args::arg_conds(store, &branches, carried, &idx0));
    drop(step);
    conds.append(arg_found);
    let (gv, idx) = retained!("gate values", gate_values_with(store, &extra, &conds));
    let step = crate::trace::stage("gate_values: write gate_values");
    let gv_rows = retained!("emitted gate_values", write_gate_values(store, &gv.map, &idx));
    drop(step);
    // THE FOLDS READ GATES AND BRANCHES ALIKE; only the published tables are gates alone.
    let mut feature_map = gf.map.clone();
    feature_map.extend(gf.conds.iter().map(|(id, f)| (id.clone(), (None, f.clone()))));
    let mut value_map = gv.map.clone();
    value_map.extend(gv.conds.clone());
    crate::trace::metrics_mark!("closure: gate_values");
    drop(stage);
    let stage = crate::trace::stage("closure: values");
    let mem = member_enums(store, &idx);
    value_map.extend(case_values(store, &mem, &idx));
    // SEVERAL CASES RETURNING ONE CLASS: either of them - see `key_returned::either_link`.
    let either = super::key_returned::either_values(store, &branches, &value_map, &idx);
    value_map.extend(either);
    // A KEY BUILT FROM AN ENUM MEMBER'S NAME requires that member - see `key_built`.
    let links: IndexSet<String> = routes.values()
        .flat_map(|h| h.sites.values().flatten().flatten())
        .filter(|id| id.starts_with("tpl:"))
        .cloned()
        .collect();
    value_map.extend(super::key_built::built_values(store, &links, &mem, &idx));
    // ONE ELEMENT OF AN `*ngFor` AT A TIME: see `gate_config::config_restrictions_all`.
    value_map.extend(per_loop.into_iter().map(|(link, rows)| (link, (None, rows))));

    crate::trace::metrics_mark!("closure: values");
    drop(stage);
    let stage = crate::trace::stage("closure: reach and trace");
    let reach = retained!("reach index", reach_index(store));
    let dead = retained!("dead gates", super::key_dead::dead_gates(store));
    let trace = retained!("source trace", source_trace(store, &read_texts(store, root)));
    crate::trace::metrics_mark!("closure: reach and trace");

    drop(stage);
    let _keys = crate::trace::stage("closure: key_reach");
    let mut counts: IndexMap<String, usize> =
        ROUTES.iter().map(|r| (r.to_string(), 0usize)).collect();
    let mut traces: IndexMap<String, usize> =
        TRACES.iter().map(|t| (t.to_string(), 0usize)).collect();

    // EVERY PASS THAT READS THE ROWS IS DONE: what follows folds what they derived. The tables they read, and
    // every tree decoded on the way, were held to the end and were most of the closure's peak.
    // WHAT EACH FORM CONTROL IS OFFERED (#117), from the lists the loops were found over - before they go with the store.
    let options = super::key_options::rows(store);
    let option_rows = options.len();
    for row in options {
        store.emit("control_options", row);
    }
    store.release();
    crate::give_back();
    crate::trace::metrics_mark!("closure: released");
    let scopes = Scopes::new(store);
    // EVERY KEY ON EVERY CORE: a key's row is folded from what the passes above derived and nothing else, so the
    // keys are folded apart and written in the locales' order - the order the rows were always written in.
    use rayon::prelude::*;
    let keyed: Vec<_> = locales.iter().collect();
    let folded: Vec<(String, Option<String>, Vec<(String, Value)>)> = keyed.par_iter().map(|&(key, loc)| {
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

        let (tr, tr_file) = if route == "none" {
            let (how, file) = trace.of(key);
            (Some(how.name().to_string()), file)
        } else {
            (None, None)
        };

        let mut comps: Vec<String> =
            hit.map(|h| h.sites.keys().flatten().cloned().collect()).unwrap_or_default();
        comps.sort();

        let ways = ways_of(&comps, hit, &paths, &reach, &dead, &bound, &scopes);
        let Ways { sets, path_sets, renames, aliases, roots, mods, rts, depth, cut, on } = ways;
        let mut on = on;

        let (always_ids, maybe_ids) = fold(&sets);
        let (always, always_br) = split_branches(always_ids, &branches);
        let (maybe, maybe_br) = split_branches(maybe_ids, &branches);
        let (path_always, _) = fold(&path_sets);
        let feature_sets: Vec<IndexSet<String>> =
            sets.iter().map(|s| features_of(s, &feature_map)).collect();
        let (always_f, maybe_f) = fold(&feature_sets);
        // ONE WALK OVER THE WAYS, read by both folds — they ask the same question of the
        // same sets, and computing them twice per key was nearly twice the walks one does.
        let ways: Vec<IndexSet<String>> = sets.clone();
        let mut labels = Labels::new();
        let mut classes = Classes::new();
        let mut sides = SetSides::new();
        let per_way = per_way_keyed(&ways, &value_map, &idx, Some(&renames), Some(&aliases), Some(&arg_params), &mut labels, &mut classes, &mut sides);
        debug_assert_eq!(aliases.len(), ways.len());
        let mut values = fold_sided(&per_way, &idx, &sides);
        merge_aliased(&mut values, &classes);
        // The dimensions the gates narrow but the map cannot read. BESIDE the resolved ones,
        // never inside them: an `unknown` row folds to the whole domain and would otherwise
        // leave no trace at all.
        let mut unreadable = unreadable_values(&ways, &value_map, &idx, Some(&per_way));
        relabel(&mut values, &labels);
        relabel(&mut unreadable, &labels);

        let mut row: Vec<(String, Value)> = Vec::with_capacity(24);
        let mut put = |k: &str, v: Value| row.push((k.to_string(), v));
        put("key", Value::String(key.clone()));
        put("locales", Value::from(loc.list.clone()));
        put("route", Value::String(route.clone()));
        put("n_refs", Value::from(hit.map(|h| h.refs as i64).unwrap_or(0)));
        put("components", Value::from(comps.clone()));
        // THE TEMPLATE NODES THE KEY RENDERS AT (#122), whatever route took it there - `{{ statusKey | translate }}` over a member a
        // literal was assigned to as much as a key written in the template: each `template_nodes` id, sorted.
        let mut nodes: Vec<String> = hit.map(|h| h.nodes.iter().cloned().collect()).unwrap_or_default();
        nodes.sort();
        put("nodes", Value::from(nodes));
        put("n_paths", Value::from(sets.len() as i64));
        // EACH WAY NAMED (#53): its `render_path.id` and what it adds to that row's gates/branches, sorted like every list here.
        debug_assert_eq!(on.len(), sets.len());
        on.sort();
        put("paths", Value::Array(on.into_iter()
            .map(|(id, own)| serde_json::json!({ "path": id, "own": own })).collect()));
        put("min_depth", depth.map(Value::from).unwrap_or(Value::Null));
        put("roots", Value::from(sorted(roots)));
        put("modules", Value::from(sorted(mods)));
        put("routes", Value::from(sorted(rts)));
        put("always_gates", Value::from(always));
        put("maybe_gates", Value::from(maybe));
        put("path_always_gates", Value::from(path_always));
        put("always_branches", Value::from(always_br));
        // THE SIDE OF EACH, spelled out: the id alone says it only as a `!` prefix on a ternary's
        // `else` arm, and a branch's sense is a lookup away in another table.
        let senses: serde_json::Map<String, Value> =
            maybe_br.iter().map(|id| (id.clone(), Value::from(branches.sense(id)))).collect();
        put("maybe_branches", Value::from(maybe_br));
        put("branch_senses", Value::Object(senses));
        put("always_features", Value::from(always_f));
        put("maybe_features", Value::from(maybe_f));
        put("always_values", Value::Array(values));
        put("unreadable_values", Value::Array(unreadable));
        put("trace", tr.clone().map(Value::String).unwrap_or(Value::Null));
        put("trace_file", tr_file.map(Value::String).unwrap_or(Value::Null));
        put("truncated", Value::from(i64::from(cut)));
        (route, tr, row)
    }).collect();
    for (route, how, cells) in folded {
        *counts.entry(route).or_default() += 1;
        if let Some(how) = how {
            *traces.entry(how).or_default() += 1;
        }
        let mut row = Row::new();
        for (k, v) in cells {
            row.insert(k, v);
        }
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
        option_rows,
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
    /// Per way (parallel to `sets`): the `render_path.id` it runs over and the ids it adds to that row's
    /// `gates`/`branches` - the site's own chain and its links. Published as `key_reach.paths` (#53).
    pub on: Vec<(i64, Vec<String>)>,
    pub path_sets: Vec<IndexSet<String>>,
    /// Per way (parallel to `sets`): the renames each gate or link's dimension goes through.
    pub renames: Renames,
    /// Per way: the bindings on its edges, one variable under two spellings (`key_scopes::pairs`).
    pub aliases: Aliases,
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
    bound: &Bound,
    scopes: &Scopes,
) -> Ways {
    let mut w = Ways::default();
    for c in comps {
        let Some((comp_paths, truncated)) = paths.by_comp.get(c) else { continue };
        let first = paths.first_id.get(c).copied().unwrap_or(0);
        w.cut = w.cut || *truncated;
        let own: &[Vec<String>] = match hit.and_then(|h| h.sites.get(&Some(c.clone()))) {
            Some(chains) if !chains.is_empty() => chains,
            // A KEY WITH NO OWN CHAIN STILL RENDERS: one empty chain, so the path's own
            // gates are enumerated rather than the whole component dropped.
            _ => std::slice::from_ref(&EMPTY_CHAIN),
        };
        for (i, p) in comp_paths.iter().enumerate() {
            // A WAY THROUGH A LITERAL `false` IS NO WAY (`key_dead`): not a path, not a root, not counted.
            // NOR IS ONE THROUGH AN EDGE THAT LEAVES AN INPUT FLAG AT A LITERAL `false` (`key_bound`).
            let Some(plinks) = bind_links(bound, p, p.gates.iter().map(|g| &**g)) else { continue };
            // AN ELEMENT A CHILD'S INPUT GOT THROUGH ONE RENDER EDGE (`edge:<render>`, `key_inputs`) holds on a path through it alone.
            let chains: Vec<(&Vec<String>, Vec<String>)> = own.iter()
                .filter(|c| !c.iter().any(|g| dead.contains(g)))
                .filter(|c| c.iter().filter_map(|g| g.strip_prefix("edge:")).all(|e| p.edges.iter().any(|x| &**x == e)))
                // A SUBTYPE'S KEY READ THROUGH ITS BASE (`via:<component>|..`, `key_classes::add_reads`) holds on a path through
                // a component that builds it.
                .filter(|c| c.iter().filter_map(|g| g.strip_prefix("via:")).all(|v| v.split('|').any(|b| p.hops.iter().any(|h| &**h == b))))
                .filter_map(|c| Some((c, bind_links(bound, p, c.iter().map(String::as_str))?)))
                .collect();
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
            for (chain, links) in chains {
                // A case a factory returned a child under (`key_returned`) holds on this path like a
                // gate does, and folds with the key's own branches.
                let mut one: IndexSet<String> =
                    p.gates.iter().chain(p.branches.iter()).map(|g| g.to_string()).collect();
                one.extend(chain.iter().filter(|g| !g.starts_with("edge:") && !g.starts_with("via:")).cloned());
                one.extend(plinks.iter().chain(links.iter()).cloned());
                // A CHILD'S INPUT READS AS THE PARENT'S EXPRESSION through the edge that binds it (`key_scopes`).
                let mut named: IndexMap<String, Vec<super::key_scopes::Step>> = IndexMap::new();
                let sites = chain.iter().filter(|g| !g.starts_with("edge:") && !g.starts_with("via:") && !g.starts_with("bind:"));
                let owned = p.gates.iter().filter_map(|g| Some((&**g, scopes.gate_comp.get(&**g)?.as_str())));
                w.aliases.push(scopes.pairs(p));
                for (id, at) in owned.chain(sites.map(|g| (g.as_str(), c.as_str()))) {
                    let steps = scopes.steps(p, at);
                    if steps.first().is_some_and(|s| !s.is_empty()) {
                        named.entry(id.to_string()).or_insert(steps);
                    }
                }
                w.renames.push(named);
                let mut adds: Vec<String> = one.iter()
                    .filter(|g| !p.gates.iter().chain(p.branches.iter()).any(|x| &**x == g.as_str()))
                    .cloned().collect();
                adds.sort();
                w.on.push((first + i as i64, adds));
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
