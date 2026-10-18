//! A POSTGRESQL STATEMENT AS THE NORMALIZED TREE `sql_nodes` holds - the kinds the T-SQL half writes
//! (`src/Map/Sql/SqlTree.cs`), so one query, one fingerprint and one column pass read both dialects. The input is
//! libpg_query's JSON of PostgreSQL's own parse tree: a node is `{"Type": {fields}}`, and a field typed as one node
//! type (a RangeVar `relation`, a TypeName, a WindowDef) is its fields alone, so where it sits says what it is.
//!
//! A NODE TYPE NOT MAPPED IS TRANSPARENT: its children hang off the nearest mapped node above. A node no type is
//! (`target`, `on`, `using`, `where`, `partition`, `references`, `inherits`, `attach`) is made from where a part sits.
//! Nodes are numbered in SOURCE preorder, as ScriptDom's are: siblings by the first byte of the source under them,
//! which PostgreSQL's locations give (an operator's is its symbol's, so a node's place is the least of its own and its
//! children's). WITH AN EXPLICIT STACK, never by recursion: a tree is as deep as its statement.

use super::scan::Source;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct PgNode {
    pub parent: Option<usize>,
    pub ord: usize,
    pub kind: String,
    pub value: String,
    pub line: i64,
}

/// What a tree is read against: the text its locations point into, where that text starts in it (`base`), the
/// statement's own byte range in it (`scope`, where a name with no place of its own is looked for), and the PL/pgSQL
/// variables a body's statement may name - a variable is a `param`, never a column.
pub struct Ctx<'s, 'a> {
    pub source: &'s Source<'a>,
    pub base: usize,
    pub scope: (usize, usize),
    pub vars: &'s HashSet<String>,
}

/// How a ResTarget reads where it sits: a select-list entry, an INSERT's column, or one SET assignment.
#[derive(Clone, Copy)]
pub enum Role {
    Item,
    Column,
    Set,
}

/// A part of the statement still to be turned into nodes.
pub enum Item<'v> {
    /// A node as libpg_query writes one: `{"Type": {fields}}`.
    Node(&'v Value),
    /// The fields of a node whose type its place says.
    As(&'static str, &'v Value),
    /// A SelectStmt's fields: its CTEs answer the node above, then its body.
    Select(&'v Value),
    /// A ResTarget's fields, read by where it sits.
    Res(&'v Value, Role),
    /// Every field of an unmapped part, transparent.
    Fields(&'v Value),
    Leaf(&'static str, String, Option<usize>),
    /// A node no type is, made from where a part sits: its place is its children's.
    Wrap(&'static str, Vec<Item<'v>>),
    /// The same with a value - an AND of two, rebuilt from PostgreSQL's flat list.
    Valued(&'static str, String, Vec<Item<'v>>),
}

/// A node the expansion makes: its kind, value and own place (a byte offset into the source).
pub struct Made {
    pub kind: String,
    pub value: String,
    pub at: Option<usize>,
}

pub fn made(kind: &str, value: impl Into<String>, at: Option<usize>) -> Option<Made> {
    Some(Made { kind: kind.into(), value: value.into(), at })
}

/// `{"Type": {fields}}` as `(Type, fields)`.
pub fn typed(v: &Value) -> Option<(&str, &Value)> {
    let map = v.as_object()?;
    if map.len() != 1 {
        return None;
    }
    let (name, body) = map.iter().next()?;
    name.starts_with(|c: char| c.is_ascii_uppercase()).then_some((name.as_str(), body))
}

/// A list field's nodes.
pub fn nodes<'v>(body: &'v Value, field: &str) -> Vec<Item<'v>> {
    body[field].as_array().into_iter().flatten().filter(|v| typed(v).is_some()).map(Item::Node).collect()
}

/// A list of `String` nodes as its values (`A_Star` is `*`).
pub fn strings(list: &Value) -> Vec<String> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| match typed(v) {
            Some(("String", s)) => Some(s["sval"].as_str().unwrap_or("").to_string()),
            Some(("A_Star", _)) => Some("*".to_string()),
            _ => None,
        })
        .collect()
}

pub fn text<'v>(body: &'v Value, field: &str) -> &'v str {
    body[field].as_str().unwrap_or("")
}

impl Ctx<'_, '_> {
    /// A node's place in the source. libpg_query leaves a 0 out, so a type that has a location and shows none is at 0.
    pub fn at(&self, body: &Value) -> Option<usize> {
        match body.get("location").and_then(Value::as_i64) {
            Some(n) if n >= 0 => Some(self.base + n as usize),
            Some(_) => None,
            None => Some(self.base),
        }
    }

    /// A name the parser gives no place of its own, as the statement first spells it.
    pub fn spell(&self, name: &str) -> String {
        self.source.first_in(self.scope.0, self.scope.1, name).unwrap_or_else(|| name.to_string())
    }

    /// A dotted name as the source spells it at `at`, where its first part starts - or, with no place, as the
    /// statement first spells each part.
    pub fn dotted_at(&self, at: Option<usize>, parts: &[String]) -> String {
        match at.and_then(|a| self.source.parts_at(a, parts)) {
            Some((spelled, _)) => spelled.join("."),
            None => parts.iter().map(|p| if p == "*" { p.clone() } else { self.spell(p) }).collect::<Vec<_>>().join("."),
        }
    }

    /// A RangeVar's parts (`catalog`, `schema`, `name`, each empty when not written) and alias, spelled.
    pub fn rel_parts(&self, rv: &Value) -> ([String; 3], String) {
        let fields = ["catalogname", "schemaname", "relname"];
        let written: Vec<String> = fields.iter().map(|f| text(rv, f).to_string()).filter(|p| !p.is_empty()).collect();
        let alias = text(&rv["alias"], "aliasname");
        let (spelled, after) = match self.at(rv).and_then(|a| self.source.parts_at(a, &written)) {
            Some((spelled, after)) => (spelled, Some(after)),
            None => (written.iter().map(|p| self.spell(p)).collect(), None),
        };
        let alias = match (alias.is_empty(), after.and_then(|i| self.source.alias_at(i, alias))) {
            (true, _) => String::new(),
            (false, Some(found)) => found,
            (false, None) => self.spell(alias),
        };
        let mut parts: [String; 3] = Default::default();
        let mut spelled = spelled.into_iter();
        for (slot, field) in parts.iter_mut().zip(fields) {
            if !text(rv, field).is_empty() {
                *slot = spelled.next().unwrap_or_default();
            }
        }
        (parts, alias)
    }

    /// A RangeVar as `schema.name`, spelled.
    pub fn relation(&self, rv: &Value) -> String {
        let (parts, _) = self.rel_parts(rv);
        parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(".")
    }
}

struct Draft {
    parent: Option<usize>,
    kind: String,
    value: String,
    at: Option<usize>,
}

/// The tree of a statement: its root `kind` at `at` (where its first token is; none - at its first node's), then
/// `roots` under it.
pub fn build(ctx: &Ctx, kind: &str, at: Option<usize>, roots: Vec<Item<'_>>) -> Vec<PgNode> {
    built(ctx, kind, at, roots).0
}

/// A tree whose root has no place of its own, and the place its first node gives it.
pub fn build_first(ctx: &Ctx, kind: &str, roots: Vec<Item<'_>>) -> (Vec<PgNode>, Option<usize>) {
    built(ctx, kind, None, roots)
}

fn built(ctx: &Ctx, kind: &str, at: Option<usize>, roots: Vec<Item<'_>>) -> (Vec<PgNode>, Option<usize>) {
    let mut drafts = vec![Draft { parent: None, kind: kind.into(), value: String::new(), at }];
    let mut stack: Vec<(Item, usize)> = roots.into_iter().rev().map(|item| (item, 0)).collect();
    while let Some((item, parent)) = stack.pop() {
        let (node, children) = super::exprs::expand(ctx, item);
        let under = match node {
            Some(m) => {
                drafts.push(Draft { parent: Some(parent), kind: m.kind, value: m.value, at: m.at });
                drafts.len() - 1
            }
            None => parent,
        };
        for child in children.into_iter().rev() {
            stack.push((child, under));
        }
    }
    numbered(ctx, &drafts)
}

/// Source preorder: a node's place is the least of its own and its children's (children are made after their
/// parent, so one backward pass gives it); a node with none takes its parent's, and keeps the order it was made in.
fn numbered(ctx: &Ctx, drafts: &[Draft]) -> (Vec<PgNode>, Option<usize>) {
    let mut key: Vec<Option<usize>> = drafts.iter().map(|d| d.at).collect();
    for i in (1..drafts.len()).rev() {
        if let (Some(k), Some(p)) = (key[i], drafts[i].parent) {
            key[p] = Some(key[p].map_or(k, |q| q.min(k)));
        }
    }
    for i in 1..drafts.len() {
        if key[i].is_none() {
            key[i] = drafts[i].parent.and_then(|p| key[p]);
        }
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); drafts.len()];
    for (i, d) in drafts.iter().enumerate() {
        if let Some(p) = d.parent {
            children[p].push(i);
        }
    }
    for list in &mut children {
        list.sort_by_key(|&c| (key[c].unwrap_or(usize::MAX), c));
    }
    let root_line = key[0].map_or(1, |k| ctx.source.line(k));
    let mut nodes = Vec::with_capacity(drafts.len());
    let mut stack: Vec<(usize, Option<usize>, usize)> = vec![(0, None, 0)];
    while let Some((d, parent, ord)) = stack.pop() {
        let n = nodes.len();
        nodes.push(PgNode {
            parent,
            ord,
            kind: drafts[d].kind.clone(),
            value: drafts[d].value.clone(),
            line: key[d].map_or(root_line, |k| ctx.source.line(k)),
        });
        for (o, &c) in children[d].iter().enumerate().rev() {
            stack.push((c, Some(n), o));
        }
    }
    (nodes, key[0])
}
