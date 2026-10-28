//! A STRUCTURAL DIRECTIVE THAT ALWAYS RENDERS IS NO GATE: its gates leave every `gate_chain` the closure reads.
//!
//! `*ngVar="x as y"` injects `TemplateRef`, so its attribute is a structural `gates` row, but its one way in renders whatever
//! it is handed - `this.vcRef.clear(); this.vcRef.createEmbeddedView(this.templateRef, ctx)` - and a key under it took the
//! gate into `always_gates`, a restriction no render obeys. The `gates` row stays; the chains drop it.
//!
//! PROVED, NEVER NAMED - a directive is inert only when its own class (no `extends`: a base may decide) shows all of:
//!
//!   * a RENDER on no condition, reached on no condition: a `createEmbeddedView` on a field injected as Angular's own
//!     `ViewContainerRef`, with no branch, case or loop and no `return` before it, in an `@Input` setter, `ngOnChanges`,
//!     `ngOnInit` or the constructor - or in a member one of them calls through `this.` on no condition, transitively;
//!   * NO TAKING IT BACK: every `clear`/`remove`/`detach` on that field is in `ngOnDestroy`, or followed in its own member by
//!     such a render.
//!
//! A gate is inert when the one class its `[name]` selector names is. Anything the rows do not show is a gate, as before.

use super::store::{Row, Store};
use crate::rows::{IndexMap, IndexSet};
use serde_json::Value;

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// Where a row starts: line, then column - a `clear(); createEmbeddedView(..)` is often one line.
fn at(row: &Row) -> (i64, i64) {
    let of = |f: &str| row.get(f).and_then(Value::as_i64).unwrap_or(0);
    (of("line"), of("col"))
}

/// Is a `type_ref` Angular's own `ViewContainerRef`? By the declaration it resolved to, as `TemplateRef` is.
fn is_view_container(type_ref: Option<&Value>) -> bool {
    let Some(t) = type_ref else { return false };
    t.get("name").and_then(Value::as_str) == Some("ViewContainerRef")
        && t.get("file").and_then(Value::as_str).is_some_and(|f| f.replace('\\', "/").contains("@angular/core"))
}

/// One class's rows, by member.
#[derive(Default)]
struct Class {
    members: IndexMap<String, String>,
    entries: Vec<String>,
    containers: IndexSet<String>,
}

/// THE GATE IDS whose directive always renders.
pub fn inert_gates(store: &Store<'_>) -> IndexSet<String> {
    let mut classes: IndexMap<String, Class> = IndexMap::default();
    for c in store.table("classes").iter() {
        if let Some(id) = cell(c, "id")
            && c.get("extends").is_none_or(Value::is_null)
        {
            classes.insert(id, Class::default());
        }
    }
    for m in store.table("members").iter() {
        let (Some(cls), Some(id), Some(name)) = (cell(m, "class"), cell(m, "id"), cell(m, "name")) else { continue };
        let Some(class) = classes.get_mut(&cls) else { continue };
        let input = m.get("decorators").and_then(Value::as_array).into_iter().flatten()
            .any(|d| d.get("name").and_then(Value::as_str) == Some("Input"));
        let setter = cell(m, "kind").as_deref() == Some("setter");
        if (setter && input) || matches!(name.as_str(), "ngOnChanges" | "ngOnInit" | "constructor") {
            class.entries.push(id.clone());
        }
        class.members.insert(name, id);
    }
    for d in store.table("di").iter() {
        let Some(class) = cell(d, "class").and_then(|c| classes.get_mut(&c)) else { continue };
        if is_view_container(d.get("type_ref"))
            && let Some(field) = cell(d, "param").or_else(|| cell(d, "member"))
        {
            class.containers.insert(field);
        }
    }
    let mut calls: IndexMap<String, Vec<Row>> = IndexMap::default();
    for c in store.table("calls").iter() {
        if let Some(m) = cell(c, "member") {
            calls.entry(m).or_default().push(c.clone());
        }
    }
    calls.values_mut().for_each(|list| list.sort_by_key(at));
    let mut first_return: IndexMap<String, (i64, i64)> = IndexMap::default();
    for r in store.table("returns").iter() {
        if let Some(m) = cell(r, "member") {
            let first = first_return.entry(m).or_insert((i64::MAX, 0));
            *first = (*first).min(at(r));
        }
    }

    let mut inert: IndexSet<String> = IndexSet::default();
    for (id, class) in &classes {
        if class.containers.is_empty() || class.entries.is_empty() {
            continue;
        }
        let on = |c: &Row, method: &str| {
            cell(c, "method").as_deref() == Some(method)
                && cell(c, "callee").is_some_and(|x| class.containers.iter().any(|f| x == format!("this.{f}.{method}")))
        };
        // ON NO CONDITION: no branch, case or loop around it, and no `return` before it in its member.
        let plain = |c: &Row, member: &str| {
            ["branch", "case", "loop"].iter().all(|f| cell(c, f).is_none())
                && first_return.get(member).is_none_or(|r| *r > at(c))
        };
        let renders_after = |member: &str, from: (i64, i64)| {
            calls.get(member).into_iter().flatten().any(|c| at(c) > from && on(c, "createEmbeddedView") && plain(c, member))
        };
        let mut seen: IndexSet<String> = IndexSet::default();
        let mut todo: Vec<String> = class.entries.clone();
        let mut renders = false;
        while let Some(member) = todo.pop() {
            if !seen.insert(member.clone()) {
                continue;
            }
            for c in calls.get(&member).into_iter().flatten().filter(|c| plain(c, &member)) {
                let method = cell(c, "method").unwrap_or_default();
                if on(c, "createEmbeddedView") {
                    renders = true;
                } else if cell(c, "callee").is_some_and(|x| x == format!("this.{method}"))
                    && let Some(next) = class.members.get(&method)
                {
                    todo.push(next.clone());
                }
            }
        }
        let destroy = class.members.get("ngOnDestroy");
        let taken_back = class.members.values().any(|member| {
            Some(member) != destroy
                && calls.get(member).into_iter().flatten()
                    .any(|c| ["clear", "remove", "detach"].iter().any(|m| on(c, m)) && !renders_after(member, at(c)))
        });
        if renders && !taken_back {
            inert.insert(id.clone());
        }
    }

    let mut classes_of_selector: IndexMap<String, Vec<String>> = IndexMap::default();
    for si in store.table("selector_index").iter() {
        if let (Some(sel), Some(cls)) = (cell(si, "selector"), cell(si, "class")) {
            classes_of_selector.entry(sel).or_default().push(cls);
        }
    }
    store.table("gates").iter()
        .filter(|g| cell(g, "gate_kind").as_deref() == Some("structural"))
        .filter_map(|g| {
            let name = cell(g, "name")?;
            let [cls] = classes_of_selector.get(&format!("[{name}]"))?.as_slice() else { return None };
            inert.contains(cls).then(|| cell(g, "id")).flatten()
        })
        .collect()
}

/// A node's `gate_chain`, its inert gates left out - what every reader of a chain reads.
pub fn chain(store: &Store<'_>, row: &Row) -> Vec<String> {
    let inert = store.shared("inert gates", inert_gates);
    match row.get("gate_chain") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .filter(|g| !inert.contains(g))
            .collect(),
        _ => Vec::new(),
    }
}

/// WHAT AN INERT DIRECTIVE'S ALIAS STANDS FOR, by the template node it sits on: `*ngVar="E as v"` binds `v` to the directive's own
/// input, so a condition under it reading `v` (`v ? ('k' | translate) : ..`) is `E`. Its gate is no condition, and the alias
/// was the only way to the one that holds. Only the `as` form - a variable whose value names the gate's input.
fn aliases(store: &Store<'_>) -> IndexMap<String, IndexMap<String, Value>> {
    let inert = store.shared("inert gates", inert_gates);
    let expressions = store.table("expressions");
    let ast_of: IndexMap<String, &Row> = expressions.iter().filter_map(|e| Some((cell(e, "id")?, e))).collect();
    let mut bound: IndexMap<String, Vec<(String, Value)>> = IndexMap::default();
    for g in store.table("gates").iter() {
        let (Some(id), Some(node), Some(name)) = (cell(g, "id"), cell(g, "node"), cell(g, "name")) else { continue };
        if !inert.contains(&id) {
            continue;
        }
        let ast = cell(g, "expression").and_then(|x| ast_of.get(&x).copied()).and_then(|e| e.get("ast").cloned());
        if let Some(ast) = ast.map(|a| a.get("ast").cloned().unwrap_or(a)) {
            bound.entry(node).or_default().push((name, ast));
        }
    }
    let mut out: IndexMap<String, IndexMap<String, Value>> = IndexMap::default();
    if bound.is_empty() {
        return out;
    }
    for n in store.table("template_nodes").iter() {
        let Some(id) = cell(n, "id") else { continue };
        let Some(inputs) = bound.get(&id) else { continue };
        for v in n.get("variables").and_then(Value::as_array).into_iter().flatten() {
            let (Some(var), Some(value)) = (v.get("name").and_then(Value::as_str), v.get("value").and_then(Value::as_str)) else { continue };
            if let Some((_, ast)) = inputs.iter().find(|(input, _)| input == value) {
                out.entry(id.clone()).or_default().insert(var.to_string(), ast.clone());
            }
        }
    }
    out
}

/// A template tree on `node`, every bare read of an inert directive's alias in scope (on the node or an ancestor) put back as
/// the expression it stands for.
pub fn unaliased(store: &Store<'_>, node: Option<&str>, tree: &Value) -> Value {
    let held = store.shared("inert aliases", aliases);
    if held.is_empty() {
        return tree.clone();
    }
    let parents = store.shared("template parents", |s: &Store<'_>| -> IndexMap<String, String> {
        s.table("template_nodes").iter().filter_map(|n| Some((cell(n, "id")?, cell(n, "parent")?))).collect()
    });
    let mut scope: IndexMap<String, Value> = IndexMap::default();
    let mut at = node.map(str::to_string);
    let mut seen: IndexSet<String> = IndexSet::default();
    while let Some(id) = at {
        if !seen.insert(id.clone()) {
            break;
        }
        // THE INNERMOST BINDING WINS: an alias an inner node declares again hides the outer one.
        for (var, ast) in held.get(&id).into_iter().flatten() {
            scope.entry(var.clone()).or_insert_with(|| ast.clone());
        }
        at = parents.get(&id).cloned();
    }
    if scope.is_empty() {
        return tree.clone();
    }
    fn put(v: &Value, scope: &IndexMap<String, Value>) -> Value {
        match v {
            Value::Object(o) => {
                // A READ THE CHECKER RESOLVED (`target`) is a member, never the alias.
                let bare = o.get("k").and_then(Value::as_str) == Some("Read") && o.get("target").is_none()
                    && o.get("receiver").and_then(|r| r.get("k")).and_then(Value::as_str) == Some("Implicit");
                if bare && let Some(ast) = o.get("name").and_then(Value::as_str).and_then(|n| scope.get(n)) {
                    return ast.clone();
                }
                Value::Object(o.iter().map(|(k, x)| (k.clone(), put(x, scope))).collect())
            }
            Value::Array(xs) => Value::Array(xs.iter().map(|x| put(x, scope)).collect()),
            other => other.clone(),
        }
    }
    put(tree, &scope)
}
