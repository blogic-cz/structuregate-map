//! WHICH TEMPLATE LITERAL BUILDS A KEY, AND OVER WHICH ENUM (#123): `key_builds`, one row per (key, literal, hole).
//!
//! `getTranslation(`demo.status.${r.status}`)` reaches `demo.status.Alpha` and `demo.status.Beta` - `key_reach` expands the literal by its
//! parts against the locale keys (`key_branches::value_keys`) - and the rows said nothing of where each key came from, so a consumer could
//! not read `demo.status.*` back as one table indexed by `Demo`. A `template_literals` row whose first part is not empty builds every key
//! its parts spell (`segments`: each part in order, each hole at least one character); each hole is a row: `hole` its position, `value`
//! the text it holds in that key, and - when the extractor typed the hole by an enum (`hole_refs`: a name lookup `${Hue[x]}`, or a hole
//! TYPED by one, rows 112) - `enum`, the `enums` row, and `member`, the member that writes that text. A typed hole whose text no member
//! writes cannot be what built the key, so the literal builds no such key.

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
    let abs: HashMap<String, String> = store.table("files").iter().filter_map(|f| Some((id_of(f, "id")?, text(f, "abs")?))).collect();
    let mut enum_at: HashMap<(String, i64), String> = HashMap::new();
    for e in store.table("enums").iter() {
        let (Some(id), Some(file), Some(line)) = (id_of(e, "id"), id_of(e, "file"), e.get("line").and_then(Value::as_i64)) else { continue };
        if let Some(path) = abs.get(&file) {
            enum_at.insert((path.clone(), line), id);
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
                let place = (r.get("file")?.as_str()?.to_string(), r.get("line")?.as_i64()?);
                let names = r.get("names")?.as_array()?.iter().filter_map(Value::as_str);
                let members = r.get("members")?.as_array()?.iter().filter_map(Value::as_str);
                Some(Typed { id: enum_at.get(&place).cloned(), member_of: names.zip(members).map(|(n, m)| (n.to_string(), m.to_string())).collect() })
            }).collect(),
            _ => Vec::new(),
        };
        let (id, file, line) = (id_of(t, "id"), id_of(t, "file"), t.get("line").cloned().unwrap_or(Value::Null));
        for key in keys.starting_with(pieces[0]) {
            let Some(holes) = segments(key, &pieces) else { continue };
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
    out
}
