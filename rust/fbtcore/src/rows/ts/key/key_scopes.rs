//! A CHILD'S INPUT IS THE PARENT'S EXPRESSION ON A WAY THROUGH THE EDGE THAT BINDS IT.
//!
//! ALIAS ON THE WAY (`pairs`), RENAME NOTHING: every row keeps its own text, and on a way the bound spellings
//! (`vm.kind`, `item.kind`) are one variable (`value_folds::unify_present`). Renaming either into the other split one
//! variable wherever only some rows of a way were renamed, and the key lost its restriction. `renamed` only labels.
//!
//! RENAME ON THE WAY, BEFORE A GATE IS KEYED BY ITS DIMENSION TEXT. `[item]="vm"` makes the child's
//! `item.kind` and the parent's `vm.kind` ONE variable on a way through that render edge, but the two
//! texts never met, so each was a dimension of its own and `value_folds::fold_enums` UNIONED them
//! where one dimension's rows would have intersected: a key read as permitting more members than
//! either gate lets through.
//!
//! ONLY A PLAIN READ CHAIN (`vm`, `ctx.context`) THE CHILD NEVER WRITES. A call, an index or an
//! operator is no variable; and a child that writes the member itself (`this.item = {..}`, a setter)
//! holds something else than what the parent bound, so renaming it would claim a restriction that
//! does not hold.

use super::closure::RenderPath;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;
use std::sync::Arc;

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// One step of a rename: the child's input member -> the parent's read chain on that edge. SHARED: a way holds the
/// steps of every gate on it, and a deep copy per gate per way was most of the time `keyreach::ways_of` took.
pub type Step = Arc<IndexMap<String, String>>;

#[derive(Default)]
pub struct Scopes {
    /// Render id -> (child input member -> parent source).
    binds: IndexMap<String, Step>,
    /// Gate id -> the component it sits in.
    pub gate_comp: IndexMap<String, String>,
    /// The step of an edge with no bindings.
    empty: Step,
}

/// A literal (`1`, `true`, `null`), or a chain through the parent's `typeof X` alias field (`DemoIDs.Home`), is a CONSTANT:
/// no variable to rename into, and renaming into it left no spelling common to the ways.
fn constant(source: &str, parent: &str, aliases: &IndexSet<(String, String)>) -> bool {
    let root = source.split('.').next().unwrap_or("");
    matches!(root, "true" | "false" | "null" | "undefined")
        || root.chars().next().is_some_and(|c| c.is_ascii_digit())
        || aliases.contains(&(parent.to_string(), root.to_string()))
}

/// Whether a source text is one plain chain of names: no call, index or operator, no stray `.`.
fn plain_chain(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
}

impl Scopes {
    pub fn new(store: &Store<'_>) -> Scopes {
        // (class, member) the class writes itself: an assignment on `this`, or a setter.
        let mut written: IndexSet<(String, String)> = IndexSet::new();
        for a in store.table("assignments").iter() {
            if cell(a, "scope").as_deref() != Some("this") {
                continue;
            }
            if let Some(class) = cell(a, "class") {
                for f in ["target", "holder"] {
                    if let Some(t) = cell(a, f) {
                        written.insert((class.clone(), t));
                    }
                }
            }
        }
        for m in store.table("members").iter() {
            if cell(m, "kind").as_deref() == Some("setter")
                && let (Some(class), Some(name)) = (cell(m, "class"), cell(m, "name"))
            {
                written.insert((class, name));
            }
        }
        // (class, name) of every `typeof X` field: an enum alias the template reads constants through.
        let mut aliases: IndexSet<(String, String)> = IndexSet::new();
        for m in store.table("members").iter() {
            if cell(m, "type").is_some_and(|t| t.starts_with("typeof "))
                && let (Some(class), Some(name)) = (cell(m, "class"), cell(m, "name"))
            {
                aliases.insert((class, name));
            }
        }
        let mut member_of: IndexMap<String, String> = IndexMap::new();
        for io in store.table("io").iter() {
            if let (Some(id), Some(member)) = (cell(io, "id"), cell(io, "member")) {
                member_of.insert(id, member);
            }
        }
        // (node, to_class) -> the render ids of it, each with the parent class that renders.
        let mut renders: IndexMap<(String, String), Vec<(String, String)>> = IndexMap::new();
        for r in store.table("renders").iter() {
            if let (Some(id), Some(node), Some(class)) = (cell(r, "id"), cell(r, "node"), cell(r, "to_class")) {
                renders.entry((node, class)).or_default().push((id, cell(r, "from_class").unwrap_or_default()));
            }
        }
        let mut binds: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        for iu in store.table("input_usage").iter() {
            let (Some(io), Some(node), Some(class), Some(source)) =
                (cell(iu, "io"), cell(iu, "node"), cell(iu, "to_class"), cell(iu, "source"))
            else {
                continue;
            };
            let Some(member) = member_of.get(&io) else { continue };
            if !plain_chain(&source) || written.contains(&(class.clone(), member.clone())) {
                continue;
            }
            for (id, from_class) in renders.get(&(node, class)).into_iter().flatten() {
                if constant(&source, from_class, &aliases) {
                    continue;
                }
                binds.entry(id.clone()).or_default().insert(member.clone(), source.clone());
            }
        }
        let mut gate_comp = IndexMap::new();
        for g in store.table("gates").iter() {
            if let (Some(id), Some(c)) = (cell(g, "id"), cell(g, "component")) {
                gate_comp.insert(id, c);
            }
        }
        let binds = binds.into_iter().map(|(id, step)| (id, Arc::new(step))).collect();
        Scopes { binds, gate_comp, empty: Step::default() }
    }

    /// The renames a way through `p` applies to a text read in `comp`, child edge first.
    ///
    /// AN EDGE WITH NO BINDINGS IS AN EMPTY STEP, and it stops the rename: past it the name is not known - so a child
    /// edge with none renames nothing, and no steps are made at all.
    pub fn steps(&self, p: &RenderPath, comp: &str) -> Vec<Step> {
        let Some(j) = p.hops.iter().rposition(|h| &**h == comp) else { return Vec::new() };
        let edges = &p.edges[..j.min(p.edges.len())];
        if edges.last().and_then(|e| self.binds.get(&**e)).is_none_or(|s| s.is_empty()) {
            return Vec::new();
        }
        edges.iter().rev().map(|e| Arc::clone(self.binds.get(&**e).unwrap_or(&self.empty))).collect()
    }

    /// Every binding on the way's edges as (parent source, child member): one variable under two spellings on this way.
    pub fn pairs(&self, p: &RenderPath) -> Vec<(String, String)> {
        p.edges.iter().flat_map(|e| self.binds.get(&**e).into_iter().flat_map(|s| s.iter())).map(|(m, x)| (x.clone(), m.clone())).collect()
    }
}

/// A dimension text read through the steps, from the child edge outward.
pub fn renamed(dim: &str, steps: &[Step]) -> String {
    let mut dim = dim.to_string();
    for step in steps {
        let root = dim.split('.').next().unwrap_or("").to_string();
        match step.get(&root) {
            Some(src) => dim = format!("{src}{}", &dim[root.len()..]),
            None => break,
        }
    }
    dim
}
