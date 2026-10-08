//! A STRUCTURAL DIRECTIVE THAT RENDERS ONLY WHEN ITS CONFIG'S VALUE IS IN ITS CONFIG'S LIST —
//! read off the directive's own code path, for the occurrence's own literal.
//!
//! `*if-code-in="{selected: code, codes: [A, B]}"`
//! restricts `code` to `[A, B]`, and nothing in the occurrence says so. The directive copies
//! the config into fields (`this._codes = config.codes`), in a BASE
//! class, and hides the element with an early return:
//!
//! ```text
//! if (!sel || (codes && codes.length > 0 && codes.find((x) => x === sel) === undefined)) return;
//! this.show();
//! ```
//!
//! `gate_config` reads a member TESTED in the class; here no member is tested at all, and the
//! restriction exists only once the occurrence's list is known to be non-empty. So the path is
//! evaluated, not matched:
//!
//!   * every RENDER SITE (`createEmbeddedView`, directly or through a `this.` call) reached
//!     from the input setter, through the whole `extends` chain;
//!   * its path condition: the branches around it, the NEGATION of every earlier `return` in
//!     the same method, and the same for the call that reached the method;
//!   * that condition with the occurrence's literal filled in: a field copied from
//!     `config.M` holds `M`'s value, a member the literal omits is `undefined`, a list of
//!     constants is truthy with a known length, and `L.find((x) => x === S) === undefined`,
//!     `L.some(...)`, `L.includes(S)` become membership of `S`'s template expression in `L`.
//!
//! A DIMENSION IS RESTRICTED ONLY WHEN EVERY RENDER SITE THAT CAN HAPPEN REQUIRES IT. A site
//! the literal makes impossible is dropped; a site whose condition cannot be read keeps no
//! condition, which leaves no restriction. A `case` on the path is not read, so it counts as
//! no condition too.

use super::gate_guard_eval::{k, name, necessary, not, Atom, Eval, Sym, F};
use super::gate_config::literal_entries;
use super::gate_values::{constant_of, dimension_of, untyped_dimension, EnumIndex, MemberEnums};
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

fn line(row: &Row) -> i64 {
    match row.get("line") {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        _ => 0,
    }
}

/// One condition on a path: a branch's tree and its polarity, read with the entry's param in
/// scope or not; or several, negated together (an earlier `return`).
#[derive(Debug, Clone)]
enum Cond {
    Branch(String, bool, bool),
    NotAll(Vec<Cond>),
    Unreadable,
}

struct Class {
    branches: IndexMap<String, (Option<String>, bool, Option<String>)>,
    /// Every expression, and where each id sits in it: an AST is decoded when a condition asks for it
    /// (`store::Cell`), never all of them up front - this map used to hold a CLONE of every one.
    exprs: std::rc::Rc<Vec<Row>>,
    ast_at: IndexMap<String, usize>,
    members: IndexMap<String, Vec<(String, Option<String>)>>,
    calls: IndexMap<String, Vec<Row>>,
    returns: IndexMap<String, Vec<Row>>,
}

impl Class {
    fn ast(&self, id: &str) -> Option<&Value> {
        self.exprs.get(*self.ast_at.get(id)?)?.get("ast").filter(|a| !a.is_null())
    }

    /// The branches around a statement, innermost first, each as a condition.
    fn chain(&self, start: Option<String>, entry: bool) -> Vec<Cond> {
        let mut out = Vec::new();
        let mut seen = IndexSet::new();
        let mut current = start;
        while let Some(id) = current {
            if !seen.insert(id.clone()) {
                break;
            }
            let Some((parent, negated, _)) = self.branches.get(&id) else { break };
            out.push(Cond::Branch(id.clone(), *negated, entry));
            current = parent.clone();
        }
        out
    }

    /// Every render site reachable from `member`, as its path.
    fn walk(
        &self,
        member: &str,
        entry: bool,
        prefix: &[Cond],
        chain: &[String],
        depth: usize,
        visited: &mut IndexSet<String>,
        out: &mut Vec<Vec<Cond>>,
    ) {
        visited.insert(member.to_string());
        if depth > 8 {
            out.push(vec![Cond::Unreadable]);
            return;
        }
        let empty = Vec::new();
        let returns = self.returns.get(member).unwrap_or(&empty);
        for c in self.calls.get(member).unwrap_or(&empty) {
            let mut path = prefix.to_vec();
            if cell(c, "case").is_some() {
                path.push(Cond::Unreadable);
            }
            let own = self.chain(cell(c, "branch"), entry);
            let own_ids: IndexSet<String> = own.iter().filter_map(|x| match x {
                Cond::Branch(id, ..) => Some(id.clone()),
                _ => None,
            }).collect();
            let mut reachable = true;
            for r in returns.iter().filter(|r| line(r) < line(c)) {
                match cell(r, "branch") {
                    None => reachable = false,
                    Some(b) if own_ids.contains(&b) => reachable = false,
                    b => path.push(Cond::NotAll(self.chain(b, entry))),
                }
            }
            if !reachable {
                continue;
            }
            path.extend(own);
            let method = cell(c, "method").unwrap_or_default();
            if method == "createEmbeddedView" {
                out.push(path);
            } else if cell(c, "callee").is_some_and(|x| x == format!("this.{method}"))
                && let Some(next) = self.dispatch(&method, chain)
                && next != member
            {
                self.walk(&next, false, &path, chain, depth + 1, visited, out);
            }
        }
    }

    /// Whether a render the walk did not reach can still run: inside a callback in the chain's
    /// classes, or in a member reachable from another entry Angular calls (`roots`). A method
    /// nothing reaches renders for some OTHER subclass, never for this one.
    fn renders_outside(
        &self,
        chain: &[String],
        visited: &IndexSet<String>,
        spans: &IndexMap<String, (String, i64, i64)>,
        roots: &[String],
    ) -> bool {
        let own: IndexSet<&str> = chain.iter()
            .flat_map(|c| self.members.get(c).into_iter().flatten())
            .filter_map(|(_, id)| id.as_deref())
            .collect();
        let renderers: IndexSet<&str> = chain.iter()
            .flat_map(|c| self.members.get(c).into_iter().flatten())
            .filter(|(_, id)| id.as_ref().and_then(|i| self.calls.get(i)).into_iter().flatten()
                .any(|c| cell(c, "method").as_deref() == Some("createEmbeddedView")))
            .map(|(n, _)| n.as_str())
            .collect();
        let renders = |c: &Row| {
            let method = cell(c, "method").unwrap_or_default();
            method == "createEmbeddedView"
                || (renderers.contains(method.as_str()) && cell(c, "callee").is_some_and(|x| x == format!("this.{method}")))
        };
        let inside = |c: &Row| chain.iter().filter_map(|cls| spans.get(cls)).any(|(file, from, to)| {
            cell(c, "owner_file").as_deref() == Some(file.as_str()) && *from <= line(c) && line(c) <= *to
        });
        let in_callback = self.calls.iter().filter(|(m, _)| !own.contains(m.as_str()))
            .flat_map(|(_, list)| list).any(|c| renders(c) && inside(c));
        let mut reached = IndexSet::new();
        for r in roots {
            self.reach(r, chain, &mut reached);
        }
        in_callback || reached.iter().filter(|m| !visited.contains(*m))
            .any(|m| self.calls.get(m).into_iter().flatten().any(|c| renders(c)))
    }

    /// Every member a `this.` call chain from `member` runs.
    fn reach(&self, member: &str, chain: &[String], out: &mut IndexSet<String>) {
        if !out.insert(member.to_string()) {
            return;
        }
        for c in self.calls.get(member).into_iter().flatten() {
            let method = cell(c, "method").unwrap_or_default();
            if cell(c, "callee").is_some_and(|x| x == format!("this.{method}"))
                && let Some(next) = self.dispatch(&method, chain)
            {
                self.reach(&next, chain, out);
            }
        }
    }

    /// The member a `this.name()` call runs: the most derived class declaring it.
    fn dispatch(&self, method: &str, chain: &[String]) -> Option<String> {
        chain.iter().find_map(|cls| {
            self.members.get(cls)?.iter().find(|(n, _)| n == method).and_then(|(_, id)| id.clone())
        })
    }

    fn read(&self, cond: &Cond, ev: &Eval<'_>, no_param: &Eval<'_>) -> F {
        match cond {
            Cond::Unreadable => F::Unk,
            Cond::Branch(id, negated, entry) => {
                let Some(ast) = self.branches.get(id).and_then(|(_, _, t)| t.as_ref()).and_then(|t| self.ast(t)) else {
                    return F::Unk;
                };
                let f = if *entry { ev.truth(ast) } else { no_param.truth(ast) };
                if *negated { not(f) } else { f }
            }
            Cond::NotAll(xs) => not(F::And(xs.iter().map(|x| self.read(x, ev, no_param)).collect())),
        }
    }
}

/// The occurrence's literal, as what each config member holds.
fn env_of(entries: &IndexMap<String, Value>, mem: &MemberEnums, idx: &EnumIndex) -> IndexMap<String, Sym> {
    let mut out = IndexMap::new();
    for (key, v) in entries {
        let sym = match k(v) {
            Some("Array") => {
                let items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
                let found: Option<Vec<(String, String)>> = items.iter().map(|i| constant_of(i, mem, idx)).collect();
                let one = found.filter(|f| !f.is_empty() && f.iter().all(|(e, _)| *e == f[0].0));
                Sym::List(items.len(), one.map(|f| (f[0].0.clone(), f.into_iter().map(|(_, v)| v).collect())))
            }
            Some("Literal") => match v.get("v") {
                Some(Value::Bool(b)) => Sym::Bool(*b),
                Some(Value::Null) | None => Sym::Absent,
                _ => Sym::Unk,
            },
            _ => Sym::Dim(v.clone()),
        };
        out.insert(key.clone(), sym);
    }
    out
}

/// Gate id -> the restrictions its directive's render path proves, in `gate_values`' shape.
pub fn guard_restrictions(store: &Store<'_>, mem: &MemberEnums, idx: &EnumIndex) -> IndexMap<String, Vec<Value>> {
    let mut files: IndexMap<String, String> = IndexMap::new();
    for f in store.table("files").iter() {
        if let (Some(id), Some(p)) = (cell(f, "id"), cell(f, "path")) {
            files.insert(id, p.replace('\\', "/"));
        }
    }
    let mut class_at: IndexMap<(String, String), String> = IndexMap::new();
    let mut spans: IndexMap<String, (String, i64, i64)> = IndexMap::new();
    let mut parent_of: IndexMap<String, (String, String)> = IndexMap::new();
    for c in store.table("classes").iter() {
        let Some(id) = cell(c, "id") else { continue };
        if let Some(file) = cell(c, "file") {
            let end = c.get("end_line").and_then(|v| v.as_i64()).unwrap_or(0);
            spans.insert(id.clone(), (file, line(c), end));
        }
        if let (Some(n), Some(p)) = (cell(c, "name"), cell(c, "file").and_then(|f| files.get(&f).cloned())) {
            class_at.insert((n, p), id.clone());
        }
        if let Some(ext) = c.get("extends")
            && let (Some(n), Some(p)) = (name(ext), ext.get("file").and_then(|f| f.as_str()))
        {
            parent_of.insert(id, (n.to_string(), p.replace('\\', "/")));
        }
    }
    let chain_of = |cls: &str| -> Vec<String> {
        let mut out = vec![cls.to_string()];
        while let Some(p) = parent_of.get(out.last().expect("never empty")).and_then(|k| class_at.get(k)) {
            if out.contains(p) {
                break;
            }
            out.push(p.clone());
        }
        out
    };

    let mut ctx = Class {
        branches: IndexMap::new(),
        exprs: store.table("expressions"),
        ast_at: IndexMap::new(),
        members: IndexMap::new(),
        calls: IndexMap::new(),
        returns: IndexMap::new(),
    };
    for b in store.table("branches").iter() {
        if let Some(id) = cell(b, "id") {
            let negated = cell(b, "sense").as_deref() == Some("else");
            ctx.branches.insert(id, (cell(b, "parent"), negated, cell(b, "condition_expr")));
        }
    }
    for (at, e) in ctx.exprs.clone().iter().enumerate() {
        if let Some(id) = cell(e, "id") {
            ctx.ast_at.insert(id, at);
        }
    }
    let mut params: IndexMap<String, Vec<String>> = IndexMap::new();
    let mut decorated: IndexSet<String> = IndexSet::new();
    let mut setters: Vec<(String, String, String, String)> = Vec::new();
    for m in store.table("members").iter() {
        let (Some(cls), Some(n)) = (cell(m, "class"), cell(m, "name")) else { continue };
        let id = cell(m, "id");
        ctx.members.entry(cls.clone()).or_default().push((n.clone(), id.clone()));
        let names: Vec<String> = m.get("params").and_then(|p| p.as_array()).into_iter().flatten()
            .filter_map(|p| name(p).map(str::to_string)).collect();
        if let Some(id) = &id {
            params.insert(id.clone(), names.clone());
            if m.get("decorators").and_then(|d| d.as_array()).is_some_and(|d| !d.is_empty()) {
                decorated.insert(id.clone());
            }
        }
        // THE INPUT IS FOUND BY ITS DECORATOR'S ALIAS, which is the attribute the gate carries.
        let input = m.get("decorators").and_then(|d| d.as_array()).into_iter().flatten()
            .find(|d| name(d) == Some("Input"))
            .map(|d| d.get("args").and_then(|a| a.get(0)).and_then(|a| a.as_str()).unwrap_or(&n).to_string());
        if let (Some(alias), Some(id), true, [p]) = (input, id, cell(m, "kind").as_deref() == Some("setter"), names.as_slice()) {
            setters.push((cls, alias, id, p.clone()));
        }
    }
    for c in store.table("calls").iter() {
        if let Some(m) = cell(c, "member") {
            ctx.calls.entry(m).or_default().push(c.clone());
        }
    }
    for r in store.table("returns").iter() {
        if let Some(m) = cell(r, "member") {
            ctx.returns.entry(m).or_default().push(r.clone());
        }
    }
    for list in ctx.calls.values_mut() {
        list.sort_by_key(line);
    }

    // A FIELD IS A CONFIG MEMBER ONLY WHEN EVERY WRITE TO IT COPIES THAT ONE MEMBER off a
    // parameter of the method writing it. One other write and it is a field of its own.
    let mut aliases_by_class: IndexMap<String, IndexMap<String, Option<String>>> = IndexMap::new();
    for a in store.table("assignments").iter() {
        let (Some(cls), Some(target)) = (cell(a, "class"), cell(a, "target")) else { continue };
        if cell(a, "scope").as_deref() != Some("this") {
            continue;
        }
        let copied = a.get("value").and_then(|v| v.get("$expr")).and_then(|e| e.as_str())
            .and_then(|e| e.split_once('.'))
            .filter(|(p, m)| !m.contains('.') && cell(a, "member").and_then(|w| params.get(&w)).is_some_and(|ps| ps.iter().any(|x| x == p)))
            .map(|(_, m)| m.to_string());
        let slot = aliases_by_class.entry(cls).or_default();
        let merged = match (slot.get(&target), copied) {
            (None, c) => c,
            (Some(Some(held)), Some(c)) if *held == c => Some(c),
            _ => None,
        };
        slot.insert(target, merged);
    }

    let mut entry_of: IndexMap<String, Vec<(String, String, String)>> = IndexMap::new();
    for (cls, alias, id, p) in setters {
        entry_of.entry(cls).or_default().push((alias, id, p));
    }
    let mut classes_of_selector: IndexMap<String, Vec<String>> = IndexMap::new();
    for si in store.table("selector_index").iter() {
        if let (Some(sel), Some(cls)) = (cell(si, "selector"), cell(si, "class")) {
            classes_of_selector.entry(sel).or_default().push(cls);
        }
    }

    let mut out: IndexMap<String, Vec<Value>> = IndexMap::new();
    for g in store.table("gates").iter() {
        let (Some(gid), Some(gname)) = (cell(g, "id"), cell(g, "name")) else { continue };
        let Some(entries) = cell(g, "expression").and_then(|x| ctx.ast(&x)).and_then(|a| literal_entries(Some(a))) else { continue };
        let Some([cls]) = classes_of_selector.get(&format!("[{gname}]")).map(|v| v.as_slice()) else { continue };
        let chain = chain_of(cls);
        let Some((_, setter, param)) = chain.iter().filter_map(|c| entry_of.get(c)).flatten().find(|(a, ..)| *a == gname) else { continue };

        let mut aliases: IndexMap<String, String> = IndexMap::new();
        for c in chain.iter().rev() {
            for (f, m) in aliases_by_class.get(c).into_iter().flatten() {
                match m {
                    Some(m) => { aliases.insert(f.clone(), m.clone()); }
                    None => { aliases.shift_remove(f); }
                }
            }
        }
        let env = env_of(&entries, mem, idx);
        let ev = Eval { env: &env, aliases: &aliases, param: Some(param) };
        let no_param = Eval { env: &env, aliases: &aliases, param: None };

        let mut paths = Vec::new();
        let mut visited = IndexSet::new();
        ctx.walk(setter, true, &[], &chain, 0, &mut visited, &mut paths);
        // A RENDER THE WALK DID NOT REACH IS A WAY IN IT CANNOT SEE: a lifecycle hook, a
        // subscription callback. Reading the reached sites alone would then claim a condition
        // one way in does not have, so the whole directive is refused.
        let roots: Vec<String> = chain.iter().flat_map(|c| ctx.members.get(c).into_iter().flatten())
            .filter(|(n, id)| id.as_deref() != Some(setter.as_str()) && (n.starts_with("ng") || id.as_ref().is_some_and(|i| decorated.contains(i))))
            .filter_map(|(_, id)| id.clone())
            .collect();
        if ctx.renders_outside(&chain, &visited, &spans, &roots) {
            continue;
        }
        let mut sites: Vec<Vec<Atom>> = Vec::new();
        for p in &paths {
            let f = F::And(p.iter().map(|c| ctx.read(c, &ev, &no_param)).collect());
            if let Some(atoms) = necessary(&f) {
                sites.push(atoms.into_iter().filter(|a| a.holds).collect());
            }
        }
        let Some(first) = sites.first() else { continue };
        let mut rows = Vec::new();
        for a in first {
            let mut union: IndexSet<String> = a.values.iter().cloned().collect();
            let on_every = sites[1..].iter().all(|s| {
                s.iter().find(|b| b.enum_id == a.enum_id && b.dim == a.dim).map(|b| union.extend(b.values.iter().cloned())).is_some()
            });
            if !on_every {
                continue;
            }
            let (dim, row, typed) = match dimension_of(&a.dim, mem) {
                Some((en, d, r)) => (d, r, Some(en)),
                None => match untyped_dimension(&a.dim) {
                    Some(d) => (d, None, None),
                    None => continue,
                },
            };
            if typed.as_ref().is_some_and(|t| *t != a.enum_id) {
                rows.push(json!({"enum": typed, "dim": dim, "row": row, "op": "unknown"}));
                continue;
            }
            let mut values: Vec<String> = union.into_iter().collect();
            values.sort();
            rows.push(json!({"enum": a.enum_id, "dim": dim, "row": row, "op": "in", "values": values}));
        }
        if !rows.is_empty() {
            out.insert(gid, rows);
        }
    }
    out
}

#[cfg(test)]
#[path = "gate_guards_tests.rs"]
mod tests;
