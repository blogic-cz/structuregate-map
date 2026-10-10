//! The tests of `closure.rs`, apart from it only so that file stays under the line ceiling.
//! It is still `closure::tests`, so `super::*` is `closure`.

use super::*;
use serde_json::{json, Map};

fn store_of(tables: Value) -> Map<String, Value> {
    tables.as_object().cloned().expect("tables")
}

/// `root -> mid -> leaf`, with a gate on each hop.
fn chain_tables() -> Value {
    json!({
        "template_nodes": [{"id": "n:1", "gate_chain": ["g:1"]},
                           {"id": "n:2", "gate_chain": ["g:2"]}],
        "renders": [
            {"id": "r:1", "to": "ng:mid", "from_component": "ng:root", "node": "n:1"},
            {"id": "r:2", "to": "ng:leaf", "from_component": "ng:mid", "node": "n:2"}
        ]
    })
}

/// The ids a path is made of, as the text they stand for.
fn names(ids: &[Rc<str>]) -> Vec<&str> {
    ids.iter().map(|i| &**i).collect()
}

fn edges_of(tables: Value) -> IndexMap<Rc<str>, Vec<Edge>> {
    let map = store_of(tables);
    let store = Store::from_payload(map, "typescript");
    load_edges(&store, &super::super::key_branches::Branches::new(&store))
}

#[test]
fn a_path_runs_from_the_root_down_and_collects_every_hops_gates() {
    let edges = edges_of(chain_tables());
    let (paths, truncated) = paths_to_memo(&edges, "ng:leaf", &mut Memo::default());
    assert!(!truncated);
    assert_eq!(paths.len(), 1);
    assert_eq!(names(&paths[0].hops), vec!["ng:root", "ng:mid", "ng:leaf"]);
    assert_eq!(names(&paths[0].edges), vec!["r:1", "r:2"]);
    assert_eq!(names(&paths[0].gates), vec!["g:1", "g:2"]);
}

#[test]
fn a_component_nothing_renders_is_its_own_single_path() {
    let edges = edges_of(chain_tables());
    let (paths, _) = paths_to_memo(&edges, "ng:root", &mut Memo::default());
    assert_eq!(paths.len(), 1);
    assert_eq!(names(&paths[0].hops), vec!["ng:root"]);
    assert!(paths[0].gates.is_empty());
}

#[test]
fn two_parents_are_two_ways_in() {
    let mut tables = chain_tables();
    tables["renders"] = json!([
        {"id": "r:1", "to": "ng:leaf", "from_component": "ng:a", "node": "n:1"},
        {"id": "r:2", "to": "ng:leaf", "from_component": "ng:b", "node": "n:2"}
    ]);
    let (paths, _) = paths_to_memo(&edges_of(tables), "ng:leaf", &mut Memo::default());
    assert_eq!(paths.len(), 2);
    assert_eq!(names(&paths[0].gates), vec!["g:1"]);
    assert_eq!(names(&paths[1].gates), vec!["g:2"]);
}

#[test]
fn a_self_rendering_component_is_ordinary_rather_than_a_hang() {
    // Several percent of a large tree's components render themselves.
    let mut tables = chain_tables();
    tables["renders"] = json!([
        {"id": "r:1", "to": "ng:x", "from_component": "ng:x", "node": "n:1"},
        {"id": "r:2", "to": "ng:x", "from_component": "ng:root", "node": "n:2"}
    ]);
    let (paths, _) = paths_to_memo(&edges_of(tables), "ng:x", &mut Memo::default());
    assert_eq!(paths.len(), 1, "the self edge is cut, the real parent remains");
    assert_eq!(names(&paths[0].hops), vec!["ng:root", "ng:x"]);
}

#[test]
fn a_two_component_cycle_terminates() {
    let mut tables = chain_tables();
    tables["renders"] = json!([
        {"id": "r:1", "to": "ng:a", "from_component": "ng:b", "node": "n:1"},
        {"id": "r:2", "to": "ng:b", "from_component": "ng:a", "node": "n:2"}
    ]);
    let (paths, _) = paths_to_memo(&edges_of(tables), "ng:a", &mut Memo::default());
    assert_eq!(paths.len(), 1);
    assert_eq!(names(&paths[0].hops), vec!["ng:b", "ng:a"]);
}

#[test]
fn the_edge_order_is_the_row_order_because_the_walk_is_order_sensitive() {
    let mut tables = chain_tables();
    tables["renders"] = json!([
        {"id": "r:z", "to": "ng:leaf", "from_component": "ng:z", "node": "n:1"},
        {"id": "r:a", "to": "ng:leaf", "from_component": "ng:a", "node": "n:2"}
    ]);
    let edges = edges_of(tables);
    assert_eq!(
        edges["ng:leaf"].iter().map(|e| &*e.id).collect::<Vec<_>>(),
        vec!["r:z", "r:a"],
        "not sorted - the extraction order is the answer"
    );
}

#[test]
fn a_render_with_no_node_carries_no_gates() {
    let mut tables = chain_tables();
    tables["renders"] = json!([{"id": "r:1", "to": "ng:leaf", "from_component": "ng:root"}]);
    let (paths, _) = paths_to_memo(&edges_of(tables), "ng:leaf", &mut Memo::default());
    assert!(paths[0].gates.is_empty());
}

// ---------------------------------------------------------------------------
// The fold, which is where the (ref, path) rule lives
// ---------------------------------------------------------------------------

fn set(items: &[&str]) -> IndexSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_intersection_holds_only_what_every_way_in_requires() {
    let (always, maybe) = fold(&[set(&["g:1", "g:2"]), set(&["g:2", "g:3"])]);
    assert_eq!(always, vec!["g:2"]);
    assert_eq!(maybe, vec!["g:1", "g:2", "g:3"]);
}

#[test]
fn one_unguarded_way_in_empties_the_intersection() {
    // A key that renders with no condition on one path requires nothing always.
    let (always, maybe) = fold(&[set(&["g:1"]), set(&[])]);
    assert!(always.is_empty());
    assert_eq!(maybe, vec!["g:1"]);
}

#[test]
fn no_ways_in_at_all_folds_to_nothing_rather_than_to_everything() {
    let (always, maybe) = fold(&[]);
    assert!(always.is_empty());
    assert!(maybe.is_empty());
}

#[test]
fn a_refs_own_gates_join_only_the_path_they_were_found_on() {
    // THE TRAP. Two components, each with its own in-template gate. Collected into one
    // flat list and unioned into every path, the intersection inherits gates from a
    // site the path never passes through - and `always_gates` then claims a condition
    // that does not always hold. Wrong for more than one key in four.
    //
    // Path A carries g:pa and the ref there carries g:ra; path B carries g:pb and its
    // ref carries g:rb. Nothing is common to both ways in.
    let per_ref_path = [set(&["g:pa", "g:ra"]), set(&["g:pb", "g:rb"])];
    let (always, _) = fold(&per_ref_path);
    assert!(always.is_empty(), "the two ways in share no condition");

    // The flattened version would have unioned both refs' chains into both paths,
    // leaving the refs' gates in the intersection.
    let flattened = [set(&["g:pa", "g:ra", "g:rb"]), set(&["g:pb", "g:ra", "g:rb"])];
    let (wrong, _) = fold(&flattened);
    assert_eq!(wrong, vec!["g:ra", "g:rb"], "which is the defect, stated");
}

#[test]
fn the_path_only_fold_answers_a_different_question() {
    // `path_always_gates` states what the RENDER TREE demands of the component,
    // independent of where in the template the key sits - so the ref's own gates are
    // not in it.
    let paths_only = [set(&["g:pa"]), set(&["g:pa", "g:pb"])];
    let (path_always, _) = fold(&paths_only);
    assert_eq!(path_always, vec!["g:pa"]);
}

#[test]
fn locales_are_listed_in_the_order_they_were_read_and_deduplicated() {
    let tables = store_of(json!({
        "translations": [
            {"key": "a.b", "locale": "en", "referenced": 0},
            {"key": "a.b", "locale": "de", "referenced": 1},
            {"key": "a.b", "locale": "en", "referenced": 0}
        ]
    }));
    let store = Store::from_payload(tables, "typescript");
    let idx = locale_index(&store);
    assert_eq!(idx["a.b"].list, vec!["en", "de"]);
    assert_eq!(idx["a.b"].referenced, 1, "the flag is the maximum any row carried");
}

#[test]
fn a_root_with_no_reach_row_is_reported_with_empty_lists() {
    let tables = store_of(json!({"component_reach": [
        {"component": "ng:1", "areas": ["ng:m1"], "routes": ["rt:1"]}
    ]}));
    let store = Store::from_payload(tables, "typescript");
    let idx = reach_index(&store);
    assert_eq!(idx.get("ng:1"), Some(&(vec!["ng:m1".to_string()], vec!["rt:1".to_string()])));
    assert_eq!(idx.get("ng:missing"), None, "the caller supplies the empty answer");
}

/// THE WALK AS IT WAS BEFORE THE MEMO, kept here as the oracle.
///
/// The memo is only allowed to make the same walk cheaper. Comparing against the plain
/// recursion is the only way to say that and mean it - a test that asserted a hand-written
/// expected list would be asserting what I believe, not what the walk did.
fn naive(edges: &IndexMap<Rc<str>, Vec<Edge>>, comp: &str) -> Vec<RenderPath> {
    fn walk(
        edges: &IndexMap<Rc<str>, Vec<Edge>>,
        current: &Rc<str>,
        seen: &mut IndexSet<Rc<str>>,
    ) -> Vec<RenderPath> {
        let root = || {
            vec![RenderPath {
                hops: vec![Rc::clone(current)],
                edges: Vec::new(),
                gates: Vec::new(),
                branches: Vec::new(),
            }]
        };
        let Some(ups) = edges.get(&**current).filter(|u| !u.is_empty()) else {
            return root();
        };
        let mut acc = Vec::new();
        for u in ups {
            if seen.contains(&u.parent) {
                continue;
            }
            seen.insert(Rc::clone(&u.parent));
            for q in walk(edges, &u.parent, seen) {
                let mut hops = q.hops;
                hops.push(Rc::clone(current));
                let mut ids = q.edges;
                ids.push(Rc::clone(&u.id));
                let mut gates = q.gates;
                gates.extend(u.gates.iter().cloned());
                acc.push(RenderPath { hops, edges: ids, gates, branches: Vec::new() });
            }
            seen.shift_remove(&u.parent);
        }
        if acc.is_empty() { root() } else { acc }
    }
    let start: Rc<str> = match edges.get_key_value(comp) {
        Some((held, _)) => Rc::clone(held),
        None => Rc::from(comp),
    };
    let mut seen = IndexSet::new();
    seen.insert(Rc::clone(&start));
    walk(edges, &start, &mut seen)
}

/// A render graph with the shapes that matter: a shared spine (so the memo is used), two
/// diamonds (so one parent is reached by more than one route), a pair of components rendered
/// by each other (so a cycle is cut), a self-render, and two edges between the SAME pair -
/// which is what multiplies the real map's paths.
fn shapes() -> IndexMap<Rc<str>, Vec<Edge>> {
    let mut edges: IndexMap<Rc<str>, Vec<Edge>> = IndexMap::new();
    let mut add = |child: &str, parent: &str, id: &str, gate: Option<&str>| {
        edges.entry(Rc::from(child)).or_default().push(Edge {
            id: Rc::from(id),
            parent: Rc::from(parent),
            gates: gate.map(|g| vec![Rc::from(g)]).unwrap_or_default(),
            branches: Vec::new(),
        });
    };
    add("spine", "root", "e1", Some("g1"));
    add("mid", "spine", "e2", None);
    add("mid", "spine", "e3", Some("g2"));
    add("left", "mid", "e4", None);
    add("right", "mid", "e5", Some("g3"));
    add("join", "left", "e6", None);
    add("join", "right", "e7", None);
    add("deep", "join", "e8", None);
    add("deep", "spine", "e9", None);
    add("ping", "pong", "e10", None);
    add("pong", "ping", "e11", None);
    add("ping", "root", "e12", None);
    add("self", "self", "e13", None);
    add("self", "join", "e14", None);
    edges
}

#[test]
fn the_memo_walks_to_the_same_paths_as_the_plain_recursion() {
    let edges = shapes();
    let mut names: Vec<String> = edges.keys().map(|k| k.to_string()).collect();
    names.push("root".to_string());
    names.sort();

    // ONE memo across every component, which is how the run uses it: the point is that a
    // later component reuses what an earlier one established.
    let mut memo = Memo::default();
    for name in &names {
        let (got, truncated) = paths_to_memo(&edges, name, &mut memo);
        let want = naive(&edges, name);
        assert!(!truncated);
        assert_eq!(got, want, "the paths of {name} differ once the memo is warm");
    }
}

#[test]
fn the_memo_does_not_reorder_the_paths() {
    // ORDER IS PART OF THE ANSWER: `path_index` is written from it, and `render_path` rows
    // are compared against the old tool's by it.
    let edges = shapes();
    let mut memo = Memo::default();
    // Warm the memo from the deepest component first, then ask for one of its ancestors -
    // the order a cold walk would have produced must survive.
    let _ = paths_to_memo(&edges, "deep", &mut memo);
    let (got, _) = paths_to_memo(&edges, "join", &mut memo);
    assert_eq!(got, naive(&edges, "join"));
    // Two parents (left, right), each reaching the spine by both of the two edges into
    // `mid` - four routes, and the order they come out in is the oracle's.
    let mine: Vec<Vec<&str>> = got.iter().map(|p| names(&p.hops)).collect();
    let oracle = naive(&edges, "join");
    let theirs: Vec<Vec<&str>> = oracle.iter().map(|p| names(&p.hops)).collect();
    assert_eq!(mine, theirs, "the same paths in the same order");
    assert_eq!(mine.len(), 4, "the diamond, doubled by the repeated edge");
}

#[test]
fn a_walk_that_cut_a_cycle_is_never_reused() {
    // A pruned answer is true only for the path it was found on. Caching it would hand a
    // shortened path set to a walk that was entitled to the whole thing.
    let edges = shapes();
    let mut memo = Memo::default();
    // `ping` and `pong` render each other, so walking from either cuts the other.
    let (from_ping, _) = paths_to_memo(&edges, "ping", &mut memo);
    let (from_pong, _) = paths_to_memo(&edges, "pong", &mut memo);
    assert_eq!(from_ping, naive(&edges, "ping"));
    assert_eq!(from_pong, naive(&edges, "pong"));
    // And asking again, with the memo now warm, still agrees.
    let (again, _) = paths_to_memo(&edges, "ping", &mut memo);
    assert_eq!(again, from_ping, "a second ask is the same ask");
}

#[test]
fn the_memo_is_what_makes_a_shared_spine_cheap() {
    // The reason it exists: every component below the spine would otherwise re-derive it.
    // Counting the edge visits is how the saving is stated rather than assumed.
    let edges = shapes();
    let mut memo = Memo::default();
    let names: Vec<String> = ["deep", "join", "left", "right", "mid"]
        .iter()
        .map(|n| (*n).to_string())
        .collect();
    for name in &names {
        let (got, _) = paths_to_memo(&edges, name, &mut memo);
        assert_eq!(got, naive(&edges, name), "{name}");
    }
    // `spine` was derived once and reused by everything above it.
    assert!(memo.holds("spine"), "the spine is remembered");
    assert!(memo.holds("mid"), "and so is everything acyclic above it");
    assert!(!memo.holds("ping"), "but nothing that had a cycle cut");
}

#[test]
fn the_memo_stops_growing_rather_than_taking_the_machine_with_it() {
    // A walk it cannot afford to remember is walked again. The ANSWER must not change.
    let edges = shapes();
    let mut memo = Memo::spending(0);
    let mut names: Vec<String> = edges.keys().map(|k| k.to_string()).collect();
    names.push("root".to_string());
    names.sort();
    for name in &names {
        let (got, _) = paths_to_memo(&edges, name, &mut memo);
        assert_eq!(got, naive(&edges, name), "{name} with nothing remembered");
    }
    assert!(!memo.holds("spine"), "nothing was affordable");
}
