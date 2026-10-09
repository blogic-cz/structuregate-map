//! The tests of `keyreach.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `keyreach::tests`, so `super::*` is the pass.
use super::*;
use super::super::closure::RenderPath;
use super::super::gate_features::GateFeatures;

fn path(hops: &[&str], gates: &[&str]) -> RenderPath {
    RenderPath {
        hops: hops.iter().map(|h| std::rc::Rc::from(&**h)).collect(),
        edges: Vec::new(),
        gates: gates.iter().map(|g| std::rc::Rc::from(&**g)).collect(),
    }
}

fn paths_of(entries: &[(&str, Vec<RenderPath>)]) -> Paths {
    let mut by_comp = IndexMap::new();
    let mut rows = 0;
    for (comp, list) in entries {
        rows += list.len();
        by_comp.insert(comp.to_string(), (list.clone(), false));
    }
    Paths { by_comp, rows }
}

fn route_of(sites: &[(&str, Vec<Vec<&str>>)]) -> Route {
    let mut r = Route::default();
    for (comp, chains) in sites {
        r.sites.insert(
            Some(comp.to_string()),
            chains
                .iter()
                .map(|c| c.iter().map(|g| g.to_string()).collect())
                .collect(),
        );
    }
    r
}

fn as_lists(sets: &[IndexSet<String>]) -> Vec<Vec<String>> {
    sets.iter()
        .map(|s| {
            let mut v: Vec<String> = s.iter().cloned().collect();
            v.sort();
            v
        })
        .collect()
}

fn no_reach() -> IndexMap<String, (Vec<String>, Vec<String>)> {
    IndexMap::new()
}

#[test]
fn a_refs_own_gates_join_the_gates_of_THAT_path_and_no_other() {
    // Collected into one flat list instead, the ref's gates were unioned into every path
    // of every component, the intersection inherited gates from sites the path never
    // passes through, and `always_gates` claimed conditions that do not always hold.
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "ng:a"], &["g:pa"])]),
                           ("ng:b", vec![path(&["r2", "ng:b"], &["g:pb"])])]);
    let hit = route_of(&[("ng:a", vec![vec!["g:ra"]]), ("ng:b", vec![vec!["g:rb"]])]);
    let comps = vec!["ng:a".to_string(), "ng:b".to_string()];

    let w = ways_of(&comps, Some(&hit), &paths, &no_reach(), &IndexSet::new());
    assert_eq!(as_lists(&w.sets), vec![vec!["g:pa", "g:ra"], vec!["g:pb", "g:rb"]]);
    // Which is the whole point: nothing holds on both ways in.
    assert!(fold(&w.sets).0.is_empty());
    // Flattened, the same input claims two gates that do not always hold.
    let flat: Vec<IndexSet<String>> = vec![
        ["g:pa", "g:ra", "g:rb"].iter().map(|g| g.to_string()).collect(),
        ["g:pb", "g:ra", "g:rb"].iter().map(|g| g.to_string()).collect(),
    ];
    assert_eq!(fold(&flat).0, vec!["g:ra", "g:rb"], "the defect, stated");
}

#[test]
fn one_ref_per_chain_so_two_refs_in_one_template_are_two_ways() {
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "ng:a"], &["g:p"])])]);
    let hit = route_of(&[("ng:a", vec![vec!["g:one"], vec![]])]);
    let w = ways_of(&["ng:a".to_string()], Some(&hit), &paths, &no_reach(), &IndexSet::new());
    assert_eq!(as_lists(&w.sets), vec![vec!["g:one", "g:p"], vec!["g:p"]]);
    // An unconditional ref makes the path's gates the only thing that always holds.
    assert_eq!(fold(&w.sets).0, vec!["g:p"]);
}

#[test]
fn path_always_gates_exclude_the_refs_own_gates_on_purpose() {
    // It states what the RENDER TREE demands of the component, independent of where in
    // the template the key sits.
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "ng:a"], &["g:p"])])]);
    let hit = route_of(&[("ng:a", vec![vec!["g:own"]])]);
    let w = ways_of(&["ng:a".to_string()], Some(&hit), &paths, &no_reach(), &IndexSet::new());
    assert_eq!(fold(&w.sets).0, vec!["g:own", "g:p"]);
    assert_eq!(fold(&w.path_sets).0, vec!["g:p"]);
}

#[test]
fn a_key_with_no_own_chain_still_renders_by_its_paths() {
    // One empty chain, so the path's own gates are enumerated rather than the whole
    // component dropped and the key reported as reaching nothing.
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "ng:a"], &["g:p"])])]);
    let hit = route_of(&[("ng:a", vec![])]);
    let w = ways_of(&["ng:a".to_string()], Some(&hit), &paths, &no_reach(), &IndexSet::new());
    assert_eq!(as_lists(&w.sets), vec![vec!["g:p"]]);
    assert_eq!(w.depth, Some(1));
}

#[test]
fn a_component_with_no_paths_of_its_own_contributes_nothing_rather_than_panicking() {
    let paths = paths_of(&[]);
    let hit = route_of(&[("ng:gone", vec![vec!["g:own"]])]);
    let w = ways_of(&["ng:gone".to_string()], Some(&hit), &paths, &no_reach(), &IndexSet::new());
    assert!(w.sets.is_empty());
    assert_eq!(w.depth, None, "no path means no depth, not depth zero");
}

#[test]
fn min_depth_is_the_SHORTEST_way_in_over_every_component() {
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "x", "ng:a"], &[]),
                                         path(&["r2", "ng:a"], &[])])]);
    let hit = route_of(&[("ng:a", vec![vec![]])]);
    let w = ways_of(&["ng:a".to_string()], Some(&hit), &paths, &no_reach(), &IndexSet::new());
    assert_eq!(w.depth, Some(1));
    assert_eq!(w.roots.len(), 2);
}

#[test]
fn the_area_and_the_route_come_from_the_ROOT_of_each_path() {
    let mut reach = IndexMap::new();
    reach.insert("r1".to_string(),
                 (vec!["Admin".to_string()], vec!["/admin".to_string()]));
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "ng:a"], &[]),
                                         path(&["r9", "ng:a"], &[])])]);
    let hit = route_of(&[("ng:a", vec![vec![]])]);
    let w = ways_of(&["ng:a".to_string()], Some(&hit), &paths, &reach, &IndexSet::new());
    assert_eq!(sorted(w.mods), vec!["Admin"]);
    assert_eq!(sorted(w.rts), vec!["/admin"]);
    // A root with no reach row is reported as a root, not dropped.
    assert_eq!(sorted(w.roots), vec!["r1", "r9"]);
}

#[test]
fn a_way_through_a_literal_false_is_no_way_so_a_key_behind_only_that_renders_nowhere() {
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "ng:a"], &["g:p"]),
                                         path(&["r2", "ng:a"], &["g:off"])])]);
    let dead: IndexSet<String> = ["g:off".to_string()].into_iter().collect();
    let hit = route_of(&[("ng:a", vec![vec!["g:own"], vec!["g:off"]])]);
    let w = ways_of(&["ng:a".to_string()], Some(&hit), &paths, &no_reach(), &dead);
    assert_eq!(as_lists(&w.sets), vec![vec!["g:own", "g:p"]], "the dead chain and the dead path are gone");
    assert_eq!(sorted(w.roots), vec!["r1"]);
    let hit = route_of(&[("ng:a", vec![vec!["g:off"]])]);
    let w = ways_of(&["ng:a".to_string()], Some(&hit), &paths, &no_reach(), &dead);
    assert!(w.sets.is_empty() && w.path_sets.is_empty() && w.roots.is_empty());
}

#[test]
fn a_key_the_routes_never_located_takes_its_gates_from_nowhere() {
    let paths = paths_of(&[("ng:a", vec![path(&["r1", "ng:a"], &["g:p"])])]);
    let w = ways_of(&[], None, &paths, &no_reach(), &IndexSet::new());
    assert!(w.sets.is_empty());
    assert!(w.path_sets.is_empty());
}

#[test]
fn the_feature_fold_reads_MEANING_so_one_condition_written_twice_survives_it() {
    // An intersection over IDS is blind to it: two different gate ids that require the
    // same feature intersect to nothing while the feature holds on both ways.
    let mut gf = GateFeatures::default();
    gf.map.insert("g:1".into(), (None, vec!["F_A".into()]));
    gf.map.insert("g:2".into(), (None, vec!["F_A".into()]));
    let sets: Vec<IndexSet<String>> = vec![
        ["g:1"].iter().map(|g| g.to_string()).collect(),
        ["g:2"].iter().map(|g| g.to_string()).collect(),
    ];
    assert!(fold(&sets).0.is_empty(), "the ids share nothing");
    let feats: Vec<IndexSet<String>> = sets.iter().map(|s| features_of(s, &gf.map)).collect();
    assert_eq!(fold(&feats).0, vec!["F_A"], "the meaning holds on both");
}

#[test]
fn a_file_whose_path_escapes_the_root_is_not_this_applications_source() {
    // Scanning the compiler's own library told the trace that `confidence` and
    // `temporary` are words the workspace uses. Some keys read `leaf` where the truth
    // is `absent`.
    let dir = std::env::temp_dir().join("fbt_read_texts_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("here.ts"), "const here = 1;").unwrap();

    let mut tables = serde_json::Map::new();
    tables.insert("files".into(), serde_json::json!([
        {"id": "f:1", "path": "here.ts", "ext": "ts"},
        {"id": "f:2", "path": "../node_modules/typescript/lib/lib.dom.d.ts", "ext": "ts"},
        {"id": "f:3", "path": "logo.png", "ext": "png"},
        {"id": "f:4", "path": "gone.ts", "ext": "ts"}
    ]));
    let store = Store::from_payload(tables, "typescript");
    let texts = read_texts(&store, &dir);

    assert_eq!(texts.keys().cloned().collect::<Vec<String>>(), vec!["here.ts"]);
    // A file that cannot be read contributes NOTHING rather than an empty string: "" is
    // a real answer for an empty file and must not also mean "unreadable".
    assert!(!texts.contains_key("gone.ts"));
    std::fs::remove_dir_all(&dir).ok();
}
