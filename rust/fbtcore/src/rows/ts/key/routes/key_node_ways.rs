//! WHICH WAY A KEY REACHED EACH NODE (#126): `key_node_ways`, one row per (key, node, route, binding, literal).
//!
//! `key_reach.nodes` names where a key renders as one union over every way in, so a key a typed template literal builds exactly
//! and an untyped literal of the same prefix only might build put their nodes in one list, and the input a key flows through
//! (`[titleTooltipText]="tipKey"` against `[title]`) was nowhere. Each row is one way into one of those nodes:
//!
//! - `route` - as `key_reach.route` names routes (`template`, `template_string`, `pipe`, `field_binding`);
//! - `binding` - the `bindings` row of the template expression that places the key (NULL for text content), `input` its name;
//! - `literal` - the `template_literals` row that built the key on that way: a `tpl:` link of the way, or - for a field or a
//!   method the template reads, whose ways drop those links - a literal `key_builds` has for the key inside that member's lines;
//!   NULL when the key is written whole or the map cannot tell;
//! - `typed` - 1 when every hole of that literal is typed by an enum (`key_builds.enum`), so it builds the key exactly; 0 when a
//!   hole is untyped, so it builds every key its pieces match; NULL without a literal.
//!
//! Only the nodes `key_reach.nodes` keeps are read: a weaker route's site a stronger one replaced is not a way in there.

use super::key_fields::{id_of, text};
use super::key_routes::Route;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;
use std::collections::HashMap;

/// A literal `key_builds` names for a key: its id, file, line, and whether every hole of it is typed.
struct Built {
    literal: String,
    file: String,
    line: i64,
    typed: bool,
}

pub fn rows(store: &Store<'_>, routes: &IndexMap<String, Route>, builds: &[Row]) -> Vec<Row> {
    let mut by_key: IndexMap<String, Vec<Built>> = IndexMap::new();
    for b in builds {
        let (Some(key), Some(literal), Some(file), Some(line)) = (text(b, "key"), text(b, "literal"), text(b, "file"), b.get("line").and_then(Value::as_i64)) else { continue };
        let typed = b.get("enum").is_some_and(|e| !e.is_null());
        let list = by_key.entry(key).or_default();
        match list.iter_mut().find(|x| x.literal == literal) {
            Some(found) => found.typed &= typed,
            None => list.push(Built { literal, file, line, typed }),
        }
    }
    let names: HashMap<String, String> = store.table("bindings").iter().filter_map(|b| Some((id_of(b, "id")?, text(b, "name")?))).collect();
    // A `tpl:` TREE IS THE LITERAL'S `expressions` ROW; its `template_literals` row is the one at its place.
    let at = |r: &Row| Some((text(r, "file")?, r.get("line")?.as_i64()?, r.get("col")?.as_i64()?));
    let literal_at: HashMap<(String, i64, i64), String> = store.table("template_literals").iter().filter_map(|t| Some((at(t)?, id_of(t, "id")?))).collect();
    let trees: IndexSet<&str> = routes.values().flat_map(|r| r.node_via.values().flatten()).flat_map(|v| v.builds.iter().map(String::as_str)).collect();
    let tree_literal: HashMap<String, String> = if trees.is_empty() {
        HashMap::new()
    } else {
        store.table("expressions").iter()
            .filter_map(|e| { let id = id_of(e, "id")?; trees.contains(id.as_str()).then_some(())?; Some((id, literal_at.get(&at(e)?)?.clone())) })
            .collect()
    };
    // THE LINES OF A MEMBER OR FUNCTION a site reads its key from.
    let span = |r: &Row| Some((id_of(r, "id")?, (text(r, "file")?, r.get("line")?.as_i64()?, r.get("end_line")?.as_i64()?)));
    let spans: HashMap<String, (String, i64, i64)> = store.table("members").iter().chain(store.table("functions").iter()).filter_map(span).collect();

    let mut out = Vec::new();
    for (key, hit) in routes {
        let built = by_key.get(key);
        let mut seen: IndexSet<(String, String, Option<String>, Option<String>)> = IndexSet::new();
        let mut nodes: Vec<&String> = hit.nodes.iter().collect();
        nodes.sort();
        for node in nodes {
            for way in hit.node_via.get(node).into_iter().flatten() {
                let mut literals: IndexSet<&str> = way.builds.iter().filter_map(|t| tree_literal.get(t)).map(String::as_str).collect();
                if let Some((file, from, to)) = way.via.reads.as_ref().and_then(|m| spans.get(m)) {
                    literals.extend(built.into_iter().flatten().filter(|b| b.file == *file && (*from..=*to).contains(&b.line)).map(|b| b.literal.as_str()));
                }
                let mut each: Vec<Option<&str>> = literals.into_iter().map(Some).collect();
                if each.is_empty() {
                    each.push(None);
                }
                for literal in each {
                    if !seen.insert((node.clone(), way.route.clone(), way.via.binding.clone(), literal.map(str::to_string))) {
                        continue;
                    }
                    let typed = literal.and_then(|l| built.into_iter().flatten().find(|b| b.literal == l)).map(|b| Value::from(b.typed as i64));
                    let mut row = Row::new();
                    let mut put = |k: &str, v: Value| row.insert(k.to_string(), v);
                    let opt = |v: Option<String>| v.map(Value::String).unwrap_or(Value::Null);
                    put("key", Value::String(key.clone()));
                    put("node", Value::String(node.clone()));
                    put("route", Value::String(way.route.clone()));
                    put("binding", opt(way.via.binding.clone()));
                    put("input", opt(way.via.binding.as_ref().and_then(|b| names.get(b)).cloned()));
                    put("literal", opt(literal.map(str::to_string)));
                    put("typed", typed.unwrap_or(Value::Null));
                    out.push(row);
                }
            }
        }
    }
    out
}
