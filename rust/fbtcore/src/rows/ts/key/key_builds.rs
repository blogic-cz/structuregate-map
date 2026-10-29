//! WHICH TEMPLATE LITERAL BUILDS A KEY, AND OVER WHICH ENUM (#123): `key_builds`, one row per (key, literal, hole).
//!
//! `getTranslation(`demo.status.${r.status}`)` reaches `demo.status.Alpha` and `demo.status.Beta` - `key_reach` expands the literal by its
//! parts against the locale keys (`key_branches::value_keys`) - and the rows said nothing of where each key came from, so a consumer could
//! not read `demo.status.*` back as one table indexed by `Demo`. A `template_literals` row whose first part is not empty builds every key
//! its parts spell (`segments`: each part in order, each hole at least one character); each hole is a row: `hole` its position, `value`
//! the text it holds in that key, and - when the extractor typed the hole by an enum (`hole_refs`: a name lookup `${Hue[x]}`, or a hole
//! TYPED by one, rows 112) - `enum`, the `enums` row, and `member`, the member that writes that text; a hole typed by a union of literals
//! (`'alpha' | 'beta'`, rows 116, #131) has no `enum`, its `member` the literal. A typed hole whose text no member
//! writes cannot be what built the key, so the literal builds no such key.
//!
//! A `+` CHAIN FROM A CONSTANT PREFIX (#136) builds the same way: in code the extractor writes it as a `template_literals` row (`form: 'concat'`);
//! in a template expression it is read off the Angular tree here (`concatenated`), `literal` the expression's id.

use super::key_branches::{segments, KeySet};
use super::key_fields::{id_of, text};
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::HashMap;

/// One hole's enum, as `hole_refs` names it: the `enums` row at its place, and member by the text it writes.
struct Typed {
    id: Option<String>,
    member_of: HashMap<String, String>,
}

pub fn rows(store: &Store<'_>) -> Vec<Row> {
    let keys: KeySet = store.table("translations").iter().filter_map(|t| text(t, "key")).collect();
    // A HOLE'S REF NAMES ITS FILE RELATIVE TO THE ROOT (rows 132), as `files.path` does; `abs` too, for a map written before.
    let mut spellings: HashMap<String, Vec<String>> = HashMap::new();
    for f in store.table("files").iter() {
        let Some(id) = id_of(f, "id") else { continue };
        spellings.insert(id, ["path", "abs"].iter().filter_map(|c| text(f, c)).collect());
    }
    let mut enum_at: HashMap<(String, i64), String> = HashMap::new();
    for e in store.table("enums").iter() {
        let (Some(id), Some(file), Some(line)) = (id_of(e, "id"), id_of(e, "file"), e.get("line").and_then(Value::as_i64)) else { continue };
        for path in spellings.get(&file).into_iter().flatten() {
            enum_at.insert((path.clone(), line), id.clone());
        }
    }
    let mut out = Vec::new();
    for t in store.table("template_literals").iter() {
        let Some(Value::Array(parts)) = t.get("parts") else { continue };
        let pieces: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
        if pieces.len() != parts.len() || pieces.first().is_none_or(|p| p.is_empty()) {
            continue;
        }
        let typed: Vec<Option<Typed>> = match t.get("hole_refs") {
            Some(Value::Array(refs)) => refs.iter().map(|r| {
                let r = r.as_object()?;
                let names = r.get("names")?.as_array()?.iter().filter_map(Value::as_str);
                let members = r.get("members")?.as_array()?.iter().filter_map(Value::as_str);
                let member_of = names.zip(members).map(|(n, m)| (n.to_string(), m.to_string())).collect();
                // A UNION OF LITERALS (#131) names no enum: its member is the literal it writes.
                if r.get("kind").and_then(Value::as_str) == Some("literal_union") {
                    return Some(Typed { id: None, member_of });
                }
                let place = (r.get("file")?.as_str()?.to_string(), r.get("line")?.as_i64()?);
                Some(Typed { id: enum_at.get(&place).cloned(), member_of })
            }).collect(),
            _ => Vec::new(),
        };
        let (id, file, line) = (id_of(t, "id"), id_of(t, "file"), t.get("line").cloned().unwrap_or(Value::Null));
        built(&mut out, &keys, &pieces, &typed, &id, &file, &line);
    }
    concatenated(store, &keys, &enum_at, &mut out);
    out
}

/// Every key `pieces` spell with each hole typed as `typed` says, as `key_builds` rows of the literal `id`.
fn built(out: &mut Vec<Row>, keys: &KeySet, pieces: &[&str], typed: &[Option<Typed>], id: &Option<String>, file: &Option<String>, line: &Value) {
    for key in keys.starting_with(pieces[0]) {
        let Some(holes) = segments(key, pieces) else { continue };
        let found: Vec<(&Option<Typed>, Option<&String>)> = holes.iter().enumerate()
            .map(|(i, seg)| {
                let hole = typed.get(i).unwrap_or(&None);
                (hole, hole.as_ref().and_then(|h| h.member_of.get(seg)))
            })
            .collect();
        // A TYPED HOLE NO MEMBER FILLS: this literal never builds that key.
        if found.iter().any(|(hole, member)| hole.is_some() && member.is_none()) {
            continue;
        }
        for (i, (seg, (hole, member))) in holes.iter().zip(found).enumerate() {
            let mut row = Row::new();
            let mut put = |k: &str, v: Value| row.insert(k.to_string(), v);
            put("key", Value::String(key.clone()));
            put("literal", id.clone().map(Value::String).unwrap_or(Value::Null));
            put("file", file.clone().map(Value::String).unwrap_or(Value::Null));
            put("line", line.clone());
            put("prefix", Value::String(pieces[0].to_string()));
            put("hole", Value::from(i as i64));
            put("value", Value::String(seg.clone()));
            put("enum", hole.as_ref().and_then(|h| h.id.clone()).map(Value::String).unwrap_or(Value::Null));
            put("member", member.cloned().map(Value::String).unwrap_or(Value::Null));
            out.push(row);
        }
    }
}

/// A TEMPLATE'S `+` (#136): `{{ 'demo.region.' + place.region | translate }}` builds as a code concatenation does, `literal` the template
/// expression's `expressions` id. A hole is a read whose `target` row (`members`, `shape_members`, `type_members`) has a `type_ref` at an enum.
fn concatenated(store: &Store<'_>, keys: &KeySet, enum_at: &HashMap<(String, i64), String>, out: &mut Vec<Row>) {
    let mut members: HashMap<String, HashMap<String, String>> = HashMap::new();
    for e in store.table("enums").iter() {
        let Some(id) = id_of(e, "id") else { continue };
        let named = e.get("members").and_then(Value::as_array).into_iter().flatten().filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let text = match m.get("value") { Some(Value::String(s)) => s.clone(), Some(Value::Number(n)) => n.to_string(), _ => name.clone() };
            Some((text, name))
        });
        members.insert(id, named.collect());
    }
    let table = store.table("expressions");
    let mut expressions = table.iter().filter(|x| x.get("template").is_some_and(|t| !t.is_null())).peekable();
    if expressions.peek().is_none() {
        return;
    }
    let mut enum_of_row: HashMap<String, String> = HashMap::new();
    for table in ["members", "shape_members", "type_members"] {
        for r in store.table(table).iter() {
            let Some(id) = id_of(r, "id") else { continue };
            let Some(t) = r.get("type_ref").and_then(Value::as_object) else { continue };
            let place = t.get("file").and_then(Value::as_str).zip(t.get("line").and_then(Value::as_i64));
            if let Some(e) = place.and_then(|(f, l)| enum_at.get(&(f.to_string(), l))) {
                enum_of_row.insert(id, e.clone());
            }
        }
    }
    for x in expressions {
        // A TREE IS PEEKED, never kept decoded, and only where the source has a `+` at all.
        let Some(id) = id_of(x, "id").filter(|_| text(x, "source").is_some_and(|s| s.contains('+'))) else { continue };
        let file = id_of(x, "owner_file");
        x.peek("ast", |ast| {
            let mut chains = Vec::new();
            if let Some(ast) = ast {
                plus_chains(ast, false, &mut chains);
            }
            for operands in chains {
                let mut pieces = vec![String::new()];
                let mut typed: Vec<Option<Typed>> = Vec::new();
                for o in operands {
                    match (o.get("k").and_then(Value::as_str), o.get("v")) {
                        (Some("Literal"), Some(Value::String(s))) => pieces.last_mut().expect("one piece").push_str(s),
                        _ => {
                            let row = o.get("target").and_then(|t| t.get("row")).and_then(Value::as_str);
                            let e = row.and_then(|r| enum_of_row.get(r));
                            typed.push(e.map(|e| Typed { id: Some(e.clone()), member_of: members.get(e).cloned().unwrap_or_default() }));
                            pieces.push(String::new());
                        }
                    }
                }
                if pieces[0].is_empty() || typed.is_empty() {
                    continue;
                }
                let pieces: Vec<&str> = pieces.iter().map(String::as_str).collect();
                built(out, keys, &pieces, &typed, &Some(id.clone()), &file, &Value::Null);
            }
        });
    }
}

/// The operands of each outermost `+` in an Angular expression tree, in order.
fn plus_chains<'a>(v: &'a Value, under_plus: bool, out: &mut Vec<Vec<&'a Value>>) {
    let is_plus = |v: &Value| v.get("k").and_then(Value::as_str) == Some("Binary") && v.get("op").and_then(Value::as_str) == Some("+");
    match v {
        Value::Object(o) if is_plus(v) => {
            if !under_plus {
                let mut operands = Vec::new();
                flatten(v, &mut operands);
                out.push(operands);
            }
            for side in ["left", "right"] {
                if let Some(x) = o.get(side) {
                    plus_chains(x, true, out);
                }
            }
        }
        Value::Object(o) => o.values().for_each(|x| plus_chains(x, false, out)),
        Value::Array(a) => a.iter().for_each(|x| plus_chains(x, false, out)),
        _ => {}
    }
    fn flatten<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        if v.get("k").and_then(Value::as_str) == Some("Binary") && v.get("op").and_then(Value::as_str) == Some("+") {
            for side in ["left", "right"] {
                if let Some(x) = v.get(side) {
                    flatten(x, out);
                }
            }
        } else {
            out.push(v);
        }
    }
}
