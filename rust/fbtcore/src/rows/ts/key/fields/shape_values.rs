//! THE VALUE AN OBJECT LITERAL'S PROPERTY IS WRITTEN WITH, for a template read that lands on it (#117).
//!
//! `*ngIf="viewModel$ | async as vm"` over a selector whose projector returns `{ criteria: { yesNoOptions: getYesNoOptions() } }`
//! makes `[options]="vm.criteria.yesNoOptions"` a read whose `target.row` is that property's `shape_members` row - a
//! declaration, with no value of its own. The value is in the row the literal is written in: the property's shape is a
//! nested object of its parent property (same file, same last line, the property starting last before it does), up to the
//! root object, and the root is the value of the innermost `returns`, `assignments`, `locals`, `consts` or `members` row
//! whose lines hold it. The path of property names from the root is walked down that row's value tree; a root that is no
//! object there (a literal handed to a call, an element of an array) has no value here.

use super::astreads::read_rows;
use super::key_fields::{id_of, text};
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The `shape_members` rows a template input binding reads (every read of one), each with the value it is written with.
pub(super) fn bound_shape_values(store: &Store<'_>) -> HashMap<String, Value> {
    let mut wanted: HashSet<String> = HashSet::new();
    for e in store.table("expressions").iter() {
        if text(e, "binding").is_none() {
            continue;
        }
        let Some(ast) = e.get("ast").filter(|a| !a.is_null()) else { continue };
        // EVERY READ OF ONE: a pipe's argument (`vm.hues | labelled : vm.huePath`) is a second read of the same view model.
        wanted.extend(read_rows(super::astreads::unwrap(ast)).into_iter().filter(|r| r.starts_with("sm:")));
    }
    if wanted.is_empty() {
        return HashMap::new();
    }
    values_of(store, &wanted)
}

/// A span of lines in one file.
#[derive(Clone, Copy)]
struct Span {
    line: i64,
    end: i64,
}

fn span(r: &Row) -> Option<Span> {
    Some(Span { line: r.get("line")?.as_i64()?, end: r.get("end_line")?.as_i64()? })
}

/// Each wanted `shape_members` row's value, when the walk finds one.
fn values_of(store: &Store<'_>, wanted: &HashSet<String>) -> HashMap<String, Value> {
    let members = store.table("shape_members");
    let shapes = store.table("shapes");
    let shape_by_id: HashMap<String, &Row> = shapes.iter().filter_map(|s| Some((id_of(s, "id")?, s))).collect();
    let member_by_id: HashMap<String, &Row> = members.iter().filter_map(|m| Some((id_of(m, "id")?, m))).collect();
    // (file, last line) -> the property rows ending there: a nested object's parent property ends where it does.
    let mut ending: HashMap<(String, i64), Vec<&Row>> = HashMap::new();
    for m in members.iter().filter(|m| text(m, "kind").as_deref() == Some("property")) {
        if let (Some(file), Some(s)) = (text(m, "file"), span(m)) {
            ending.entry((file, s.end)).or_default().push(m);
        }
    }
    // file -> every row holding a value tree, with its lines.
    let tables: Vec<_> = ["returns", "assignments", "locals", "consts", "members"].iter().map(|t| store.table(t)).collect();
    let mut holders: HashMap<String, Vec<(Span, &Value)>> = HashMap::new();
    for table in &tables {
        for r in table.iter() {
            let (Some(file), Some(s), Some(value)) = (text(r, "file"), span(r), r.get("value").filter(|v| v.is_object())) else { continue };
            holders.entry(file).or_default().push((s, value));
        }
    }
    let mut out = HashMap::new();
    for id in wanted {
        let Some(m) = member_by_id.get(id) else { continue };
        let Some(name) = text(m, "name") else { continue };
        let mut path = vec![name];
        let mut shape = id_of(m, "shape").and_then(|s| shape_by_id.get(&s).copied());
        let mut root = None;
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(s) = shape {
            let (Some(sid), Some(file), Some(sp)) = (id_of(s, "id"), text(s, "file"), span(s)) else { break };
            if !seen.insert(sid.clone()) {
                break;
            }
            // THE PARENT PROPERTY: ends on the object's last line, starts on or before its first, and is no member of it.
            // Of those, the one that STARTS LAST before the object does: a sibling on the same line ends before it begins.
            let start = (sp.line, s.get("col").and_then(Value::as_i64).unwrap_or(0));
            let parent = ending.get(&(file.clone(), sp.end)).into_iter().flatten()
                .filter_map(|p| Some((p, (span(p)?.line, p.get("col").and_then(Value::as_i64).unwrap_or(0)))))
                .filter(|(p, at)| *at < start && id_of(p, "shape").as_deref() != Some(sid.as_str()))
                .max_by_key(|(_, at)| *at)
                .map(|(p, _)| p);
            match parent {
                Some(p) => {
                    let Some(pname) = text(p, "name") else { break };
                    path.push(pname);
                    shape = id_of(p, "shape").and_then(|x| shape_by_id.get(&x).copied());
                }
                None => {
                    root = Some((file, sp));
                    break;
                }
            }
        }
        let Some((file, sp)) = root else { continue };
        // THE INNERMOST ROW WHOSE LINES HOLD THE ROOT OBJECT is the one it is written in.
        let Some(&(_, value)) = holders.get(&file).into_iter().flatten()
            .filter(|(hs, _)| hs.line <= sp.line && hs.end >= sp.end)
            .max_by_key(|(hs, _)| (hs.line, -hs.end)) else { continue };
        let mut at = value;
        let mut found = true;
        for name in path.iter().rev() {
            match at.as_object().filter(|o| o.keys().all(|k| !k.starts_with('$'))).and_then(|o| o.get(name)) {
                Some(next) => at = next,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if found {
            out.insert(id.clone(), at.clone());
        }
    }
    out
}
