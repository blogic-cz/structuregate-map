//! A ROW'S CELL AS TEXT, the three ways the closure reads one, in one place.
//!
//! Every pass of `rows/ts/` once carried its own copy of these one-liners (some thirty), the same name meaning a
//! different one in different files: a `cell` was `text` in one and `scalar` in the next. A file that calls its reader
//! by its old name imports it under that name (`use crate::rows::ts::rowread::scalar as cell`).

use super::store::{Row, Store};
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// A non-empty STRING cell; any other value, an empty string and a missing cell are None.
pub(crate) fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Any SCALAR cell as text (a number as its digits), an empty string kept; NULL and a missing cell are None.
pub(crate) fn id_of(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// Any SCALAR cell as text, as `id_of`, but an empty string is None too.
pub(crate) fn scalar(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// THE TEMPLATE NODES' SHAPE, built once per derivation and shared by every pass that climbs or walks a template
/// (`Store::shared`, dropped with the store's other memos at `release`): each node's parent, its children in table order,
/// its component and tag, and the names it declares (`variables`).
#[derive(Default)]
pub(crate) struct TemplateTree {
    pub parent: HashMap<String, String>,
    pub children: HashMap<String, Vec<String>>,
    pub component: HashMap<String, String>,
    pub tag: HashMap<String, String>,
    pub declared: HashMap<String, Vec<String>>,
}

impl TemplateTree {
    pub(crate) fn of(store: &Store<'_>) -> Rc<TemplateTree> {
        store.shared("template tree", |s: &Store<'_>| {
            let mut t = TemplateTree::default();
            for n in s.table("template_nodes").iter() {
                let Some(id) = id_of(n, "id") else { continue };
                if let Some(p) = id_of(n, "parent") {
                    t.children.entry(p.clone()).or_default().push(id.clone());
                    t.parent.insert(id.clone(), p);
                }
                if let Some(c) = text(n, "component") {
                    t.component.insert(id.clone(), c);
                }
                if let Some(g) = text(n, "tag") {
                    t.tag.insert(id.clone(), g);
                }
                if let Some(Value::Array(vs)) = n.get("variables") {
                    t.declared.insert(id, vs.iter().filter_map(|v| v.get("name").and_then(Value::as_str).map(str::to_string)).collect());
                }
            }
            t
        })
    }

    /// Whether `node` declares `name` (a loop's item, a `let-` variable): a child declaring it shadows the outer one.
    pub(crate) fn declares(&self, node: &str, name: &str) -> bool {
        self.declared.get(node).is_some_and(|d| d.iter().any(|n| n == name))
    }
}
