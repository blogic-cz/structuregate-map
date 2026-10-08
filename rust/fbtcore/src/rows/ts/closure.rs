//! THE RENDER CLOSURE: where every translation key can be travelled to from a routed root.
//!
//! PATHS ARE STORED PER COMPONENT, NOT PER KEY. Keys in one template share one path set,
//! so per-key rows would store the same walk dozens of times over. Keyed by the COMPONENT they
//! all pass through it is tens of thousands of rows on one large workspace - and several
//! times that on another tree, where one component rendered from everywhere is most of
//! them. The shape is the same; the size is the tree's.
//!
//! A KEY IS REACHABLE MORE THAN ONE WAY HALF THE TIME, so there is no "the" gate set.
//! THE UNIT OF ENUMERATION IS (REF, PATH): one ref's own in-template gates joined to the
//! gates of THAT path, never to another site's.
//!
//! The first version of the tool this comes from got it wrong in a way worth stating,
//! because the numbers it produced looked fine: every ref's own chain was collected into
//! ONE flat list and unioned into every path of every component, so the intersection
//! inherited gates from sites the path never passes through and `always_gates` claimed
//! conditions that do not always hold. Read one path, call the key "requires feature X",
//! and you are wrong for a good share of the keys.
//!
//! THERE IS NO PATH CAP. A cap of a few thousand paths per component looks safe, but on a
//! large workspace a single component exceeds it, so the cap hid real paths and bought nothing;
//! on another Angular tree one component rendered from everywhere holds most of the table, and the
//! cap would hide most of its paths. TWO TREES, NOT ONE NUMBER. Dropping the cap is a deliberate divergence from the first version.
//! `truncated` stays as a column because a consumer filters on it, and is always 0.

use std::rc::Rc;
use std::collections::HashMap;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

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

fn string_list(row: &Row, field: &str) -> Vec<String> {
    match row.get(field) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// One site that renders a component, carrying the gate chain already on it.
#[derive(Clone, Debug)]
pub struct Edge {
    pub id: Rc<str>,
    pub parent: Rc<str>,
    pub gates: Vec<Rc<str>>,
}

/// WHERE AN EDGE IS WRITTEN, in the tree's own terms: a path and a position, never a row id.
///
/// The walk is order-sensitive — `render_path.path_index` numbers the paths of one component
/// positionally and a consumer joins on it — so the order the edges are read in decides what
/// the map publishes. Row order was that order, and row order is not a fact about the TREE: a
/// partial run assembles the table as carried-then-produced, and most of the render
/// paths came out at an index holding a different path. The same walks, over the same
/// components, differently numbered.
///
/// A PATH AND NOT AN ID, because two maps of one tree number it differently and still say the
/// same thing. A dynamic edge has no template file of its own, so it falls back to the file of
/// the component that creates it.
type Site = (String, i64, i64, String, String);

fn edge_sites(store: &Store<'_>) -> impl Fn(&Row) -> Site + use<> {
    let mut path_of: IndexMap<String, String> = IndexMap::new();
    for f in store.table("files").iter() {
        if let Some(id) = id_of(f, "id") {
            path_of.insert(id, text(f, "path").unwrap_or_default());
        }
    }
    let mut file_of: IndexMap<String, String> = IndexMap::new();
    for table in ["components", "directives"] {
        for c in store.table(table).iter() {
            if let (Some(id), Some(file)) = (id_of(c, "id"), id_of(c, "file")) {
                file_of.insert(id, file);
            }
        }
    }

    move |r: &Row| {
        let home = id_of(r, "owner_file")
            .or_else(|| id_of(r, "from_component").and_then(|c| file_of.get(&c).cloned()));
        let path = home.and_then(|h| path_of.get(&h).cloned()).unwrap_or_default();
        let number = |name: &str| r.get(name).and_then(|v| v.as_i64()).unwrap_or(-1);
        (
            path,
            number("line"),
            number("col"),
            text(r, "tag").unwrap_or_default(),
            text(r, "kind").unwrap_or_default(),
        )
    }
}

/// EVERY ID HELD ONCE, handed out as a shared handle.
///
/// A path is a list of ids and a large tree has hundreds of thousands of them. Spelled as `String` every hop is its
/// own allocation, so the walk's answer weighed most of a kilobyte a path and the memo that makes the
/// walk cheap could not be afforded - it filled the machine's free memory, paging. As shared
/// handles a path is a fraction of that: cloning one bumps refcounts instead of copying text.
///
/// Interning happens HERE, once, while the DAG is built. The walk itself never looks an id up.
#[derive(Default)]
struct Ids {
    held: std::collections::HashMap<Rc<str>, Rc<str>>,
}

impl Ids {
    fn of(&mut self, name: &str) -> Rc<str> {
        if let Some(held) = self.held.get(name) {
            return Rc::clone(held);
        }
        let held: Rc<str> = Rc::from(name);
        self.held.insert(Rc::clone(&held), Rc::clone(&held));
        held
    }
}

/// The render DAG: child -> the sites that render it.
pub fn load_edges(store: &Store<'_>) -> IndexMap<Rc<str>, Vec<Edge>> {
    let nodes = store.table("template_nodes");
    let mut chains: IndexMap<String, Vec<String>> = IndexMap::new();
    for n in nodes.iter() {
        if matches!(n.get("gate_chain"), Some(Value::Array(_)))
            && let Some(id) = id_of(n, "id")
        {
            chains.insert(id, string_list(n, "gate_chain"));
        }
    }

    // BY WHERE THE EDGE IS WRITTEN, not by the order the rows arrived — see `edge_sites`.
    let site = edge_sites(store);
    let renders = store.table("renders");
    let mut ordered: Vec<&Row> = renders
        .iter()
        .filter(|r| id_of(r, "from_component").is_some())
        .collect();
    ordered.sort_by_key(|r| site(r));

    let mut ids = Ids::default();
    let mut edges: IndexMap<Rc<str>, Vec<Edge>> = IndexMap::new();
    for r in ordered {
        let Some(parent) = id_of(r, "from_component") else { continue };
        let Some(to) = id_of(r, "to") else { continue };
        let gates: Vec<Rc<str>> = id_of(r, "node")
            .and_then(|n| chains.get(&n).cloned())
            .unwrap_or_default()
            .iter()
            .map(|g| ids.of(g))
            .collect();
        let to = ids.of(&to);
        edges.entry(to).or_default().push(Edge {
            id: ids.of(&id_of(r, "id").unwrap_or_default()),
            parent: ids.of(&parent),
            gates,
        });
    }
    edges
}

/// One simple path from a component up to one nothing renders.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPath {
    pub hops: Vec<Rc<str>>,
    pub edges: Vec<Rc<str>>,
    pub gates: Vec<Rc<str>>,
}

/// THE WALKS ALREADY DONE, shared by every component of one run.
///
/// Without it the walk costs far more than its own output: `walk(parent)` re-enumerates a
/// parent's entire ancestry once per edge into it, and again for every component the run starts
/// from, so a large tree's paths cost minutes to produce. The paths themselves are a great many id
/// pushes - well under a second - and the rest was the same sub-walks over and over.
///
/// WHY A CACHED ANSWER NEEDS NO FURTHER CHECK, which is not obvious and was nearly guarded
/// against twice over:
///
///   * only a walk that cut NO cycle is remembered, so what is stored is the enumeration of
///     `n`'s ancestors with nothing conditioned on the path it was reached by;
///   * `seen` holds the chain from the component the run started at up to `n`, so every member
///     of it is a DESCENDANT of `n`;
///   * for a cached answer to be wrong here, some node would have to be both an ancestor of `n`
///     (reached by the stored walk) and a descendant (on `seen`) - a cycle through `n`;
///   * and had such a cycle existed, `n` would be its own ancestor, the walk that stored it
///     would have hit `seen.contains(n)`, been marked pruned, and never been stored.
///
/// So the rule on the way IN is the whole guarantee. A cheap re-check on the way out was
/// written first, kept a set of every ancestor per entry, and no test could tell it apart from
/// its absence - because nothing can.
/// HOW MANY PATHS THE MEMO MAY HOLD.
///
/// It is bounded because it is not free: remembering everything a large map walks
/// through took the process to the edge of the machine's free memory, and it spent most of
/// its wall clock paging, not walking. The walk it makes cheap
/// is worth nothing if the machine swaps to hold the answer.
///
/// The number is a budget rather than a count of entries because entries are wildly uneven:
/// one component on a large workspace has several times the paths of second place. Spending the
/// budget in arrival order is what keeps the SHARED SPINE - walked first, by every component
/// above it - and drops the rare enormous tail nobody reuses.
///
/// SET AGAINST WHAT A PATH COSTS, not picked. A path is three lists of shared handles - about
/// 200 bytes at the depth a large workspace runs to - so this is roughly 400 MB, which fits beside
/// the payload on a machine with a few GB free. Bounded at 200 000 it was tens of MB, the reuse
/// ran out almost at once, and the walk came out SLOWER than no memo at all, because sharing costs a clone per path where the plain recursion moved it.
const MEMO_PATHS: usize = 2_000_000;

#[derive(Default)]
pub struct Memo {
    held: HashMap<Rc<str>, Rc<Vec<RenderPath>>>,
    /// Paths remembered so far, against `MEMO_PATHS`.
    spent: usize,
}

impl Memo {
    /// Whether one component's walk is remembered. For the tests: the saving is the point, so it
    /// is stated rather than assumed.
    #[cfg(test)]
    pub fn holds(&self, comp: &str) -> bool {
        self.held.contains_key(comp)
    }

    /// A memo with a named budget, so a test can say what happens when it runs out.
    #[cfg(test)]
    pub fn spending(paths: usize) -> Memo {
        Memo { held: HashMap::new(), spent: MEMO_PATHS.saturating_sub(paths) }
    }
}

/// Every simple path from `comp` up to a component nothing renders, reusing what the walks
/// before it already established.
///
/// Cycles are cut on the path set — a self-rendering component is ordinary here (several
/// percent of a large tree), and the cycle guard rather than any depth budget is what terminates the walk.
pub fn paths_to_memo(
    edges: &IndexMap<Rc<str>, Vec<Edge>>,
    comp: &str,
    memo: &mut Memo,
) -> (Vec<RenderPath>, bool) {
    // The starting component may not be a child of anything, so it is not always a key of the
    // DAG; its handle is taken from there when it is and made once when it is not.
    let start: Rc<str> = match edges.get_key_value(comp) {
        Some((held, _)) => Rc::clone(held),
        None => Rc::from(comp),
    };
    let mut seen = IndexSet::new();
    seen.insert(Rc::clone(&start));
    let (paths, _) = walk(edges, &start, &mut seen, memo);
    ((*paths).clone(), false)
}

/// `(the paths up from `current`, whether any parent was cut as a cycle)`.
///
/// THE SECOND HALF IS WHAT MAKES THE MEMO SOUND. A walk that cut a cycle produced an answer
/// true only for the path it was on, so it is never cached.
fn walk(
    edges: &IndexMap<Rc<str>, Vec<Edge>>,
    current: &Rc<str>,
    seen: &mut IndexSet<Rc<str>>,
    memo: &mut Memo,
) -> (Rc<Vec<RenderPath>>, bool) {
    // Only walks that cut no cycle are in here, and those cannot be wrong for this one - see
    // the note on `Memo`.
    if let Some(cached) = memo.held.get(&**current) {
        return (Rc::clone(cached), false);
    }

    let root = || {
        Rc::new(vec![RenderPath {
            hops: vec![Rc::clone(current)],
            edges: Vec::new(),
            gates: Vec::new(),
        }])
    };
    let Some(ups) = edges.get(&**current).filter(|u| !u.is_empty()) else {
        return (root(), false);
    };

    let mut acc = Vec::new();
    let mut pruned = false;
    for u in ups {
        if seen.contains(&u.parent) {
            pruned = true;
            continue;
        }
        seen.insert(Rc::clone(&u.parent));
        let (sub, sub_pruned) = walk(edges, &u.parent, seen, memo);
        pruned = pruned || sub_pruned;
        for p in sub.iter() {
            let mut hops = p.hops.clone();
            hops.push(Rc::clone(current));
            let mut ids = p.edges.clone();
            ids.push(Rc::clone(&u.id));
            let mut gates = p.gates.clone();
            gates.extend(u.gates.iter().cloned());
            acc.push(RenderPath { hops, edges: ids, gates });
        }
        seen.shift_remove(&u.parent);
    }
    // A component whose every parent was already on this path is a root FOR THIS WALK.
    let shared = if acc.is_empty() { root() } else { Rc::new(acc) };

    // A WALK THAT CUT A CYCLE IS NEVER REMEMBERED. Its answer is true only for the path it was
    // found on, and handing it to another walk would quietly shorten that walk's path set.
    //
    // NOR IS ONE THAT WOULD NOT FIT. Forgetting costs time; not fitting costs the machine.
    if !pruned && memo.spent + shared.len() <= MEMO_PATHS {
        memo.spent += shared.len();
        memo.held.insert(Rc::clone(current), Rc::clone(&shared));
    }
    (shared, pruned)
}

/// The NgModule areas and routes a root component is reached through.
///
/// A root without a reach row is reported with empty lists, not dropped.
pub fn reach_index(store: &Store<'_>) -> IndexMap<String, (Vec<String>, Vec<String>)> {
    let mut out = IndexMap::new();
    for r in store.table("component_reach").iter() {
        if let Some(component) = id_of(r, "component") {
            out.insert(component, (string_list(r, "areas"), string_list(r, "routes")));
        }
    }
    out
}

/// What the locale files declare about one key.
#[derive(Clone, Debug, Default)]
pub struct LocaleEntry {
    /// The locales that define it, in the order they were read.
    pub list: Vec<String>,
    /// The map's own flag, over the whole source, from an exact literal join.
    pub referenced: i64,
}

pub fn locale_index(store: &Store<'_>) -> IndexMap<String, LocaleEntry> {
    let mut acc: IndexMap<String, LocaleEntry> = IndexMap::new();
    let mut seen: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for t in store.table("translations").iter() {
        let Some(key) = text(t, "key") else { continue };
        let entry = acc.entry(key.clone()).or_default();
        let locales = seen.entry(key).or_default();
        if let Some(locale) = text(t, "locale")
            && locales.insert(locale.clone())
        {
            entry.list.push(locale);
        }
        let referenced = match t.get("referenced") {
            Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
            Some(Value::String(s)) => s.parse().unwrap_or(0),
            _ => 0,
        };
        entry.referenced = entry.referenced.max(referenced);
    }
    acc
}

/// The intersection and the union of a list of sets, both sorted — the shape every gate
/// column takes.
pub fn fold(sets: &[IndexSet<String>]) -> (Vec<String>, Vec<String>) {
    let mut inter: Option<IndexSet<String>> = None;
    let mut uni: IndexSet<String> = IndexSet::new();
    for s in sets {
        for g in s {
            uni.insert(g.clone());
        }
        inter = Some(match inter {
            None => s.clone(),
            Some(held) => held.into_iter().filter(|g| s.contains(g)).collect(),
        });
    }
    let mut a: Vec<String> = inter.unwrap_or_default().into_iter().collect();
    let mut b: Vec<String> = uni.into_iter().collect();
    a.sort();
    b.sort();
    (a, b)
}

/// The paths of every component any key reaches, computed once and shared by its keys.
pub struct Paths {
    pub by_comp: IndexMap<String, (Vec<RenderPath>, bool)>,
    pub rows: usize,
}

/// Emit `render_path` for every component a key reaches.
pub fn render_paths(
    store: &mut Store<'_>,
    edges: &IndexMap<Rc<str>, Vec<Edge>>,
    needed: &IndexSet<String>,
) -> Paths {
    let mut ordered: Vec<String> = needed.iter().cloned().collect();
    ordered.sort();

    let mut by_comp = IndexMap::new();
    let mut rows = 0usize;
    let mut memo = Memo::default();
    for comp in ordered {
        let (paths, truncated) = paths_to_memo(edges, &comp, &mut memo);
        for (i, p) in paths.iter().enumerate() {
            let mut row = super::store::Row::new();
            row.insert("id".into(), Value::from(rows as i64 + 1));
            row.insert("component".into(), Value::String(comp.clone()));
            row.insert("path_index".into(), Value::from(i as i64));
            let text = |ids: &[Rc<str>]| -> Value {
                Value::from(ids.iter().map(|i| i.to_string()).collect::<Vec<String>>())
            };
            row.insert("root".into(), Value::String(p.hops[0].to_string()));
            row.insert("depth".into(), Value::from(p.hops.len() as i64 - 1));
            row.insert("hops".into(), text(&p.hops));
            row.insert("edges".into(), text(&p.edges));
            row.insert("gates".into(), text(&p.gates));
            row.insert("truncated".into(), Value::from(i64::from(truncated)));
            store.emit("render_path", row);
            rows += 1;
        }
        by_comp.insert(comp, (paths, truncated));
    }
    Paths { by_comp, rows }
}

#[cfg(test)]
#[path = "closure_tests.rs"]
mod tests;
