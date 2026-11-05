//! WHICH WAY A KEY REACHED EACH NODE (#126): `key_node_ways`, one row per (key, node, route, binding, literal).
//!
//! `key_reach.nodes` names where a key renders as one union over every way in, so a key a typed template literal builds exactly
//! and an untyped literal of the same prefix only might build put their nodes in one list, and the input a key flows through
//! (`[titleTooltipText]="tipKey"` against `[title]`) was nowhere. Each row is one way into one of those nodes:
//!
//! - `route` - as `key_reach.route` names routes (`template`, `template_string`, `pipe`, `field_binding`);
//! - `binding` - the `bindings` row of the template expression that places the key (NULL for text content), `input` its name;
//! - `literal` - the `template_literals` row that built the key on that way: a `tpl:` link of the way, or - for a field or a
//!   method the template reads, whose ways drop those links - a literal `key_builds` has for the key inside that member's lines or those of
//!   any member or function it calls, transitively, of every method assigning or pushing onto a field it reads, and of the parent fields
//!   an `@Input` it reads is bound to (#135: a getter returning a service call; an `*ngFor` collection is the member it reads;
//!   `this.rows = IDS.map(id => ..)` in a method; `[model]="kinds"` on the child's element);
//!   NULL when the key is written whole or the map cannot tell;
//! - `typed` - 1 when every hole of that literal is typed (an enum, a union of literals: `key_builds`), so it builds the key exactly; 0 when
//!   a hole is untyped, or the literal's prefix spells the key but its types say it cannot build it (#132) - an expansion; NULL without a literal.
//!   A method a template calls with literal arguments that fill every hole (`label('time')` over `'demo.' + name`, #137) is 1 whatever the hole's type.
//!
//! Only the nodes `key_reach.nodes` keeps are read: a weaker route's site a stronger one replaced is not a way in there.

use super::key_fields::{id_of, text};
use super::key_routes::Route;
use super::store::{Row, Store};
use crate::rows::{IndexMap, IndexSet};
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
    let mut by_key: IndexMap<String, Vec<Built>> = IndexMap::default();
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
    // ...AND OF EVERY MEMBER OR FUNCTION IT CALLS, transitively (#135): a getter returning `this.texts.of(code)` builds its key in `of`.
    let mut callees: HashMap<String, IndexSet<String>> = HashMap::new();
    for c in store.table("calls").iter() {
        if let (Some(from), Some(to)) = (id_of(c, "member"), id_of(c, "callee_id")) {
            callees.entry(from).or_default().insert(to);
        }
    }
    // ...AND OF EVERY METHOD THAT ASSIGNS OR PUSHES ONTO THE FIELD the site reads: `this.rows = IDS.map(id => ({ name: t(`demo.item_${id}`) }))`
    // in a method - the field's own initializer is often only `[]` - or `this.rows.push({ label: this.labelOf(code) })`; and, for an
    // `@Input`, of the fields its parents bind it to (`[model]="kinds"`), and so on up.
    let feeders = feeders(store);
    let reached = |start: &str| -> Vec<&(String, i64, i64)> {
        let mut seen: IndexSet<String> = IndexSet::from_iter([start.to_string()]);
        let mut i = 0;
        while let Some(m) = seen.get_index(i).cloned() {
            i += 1;
            seen.extend(feeders.get(&m).into_iter().flatten().cloned());
            seen.extend(callees.get(&m).into_iter().flatten().cloned());
        }
        seen.iter().filter_map(|m| spans.get(m)).collect()
    };
    // EVERY TEMPLATE LITERAL WITH TEXT OF ITS OWN, by file (#132): one in a member a site reads whose text spells the key, but which `key_builds`
    // says cannot build it, is still the literal that put the key there - an over-expansion, `typed` 0. Its text is matched part by part,
    // in order: a literal opening with its hole (`${message}2`, #135, filled by a call two hops up) spells the key by its suffix.
    let mut prefixed: HashMap<String, Vec<(String, i64, Vec<String>)>> = HashMap::new();
    for t in store.table("template_literals").iter() {
        let (Some(id), Some(file), Some(line)) = (id_of(t, "id"), text(t, "file"), t.get("line").and_then(Value::as_i64)) else { continue };
        let parts: Vec<String> = t.get("parts").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        if parts.iter().any(|p| !p.is_empty()) {
            prefixed.entry(file).or_default().push((id, line, parts));
        }
    }

    let mut out = Vec::new();
    for (key, hit) in routes {
        let built = by_key.get(key);
        let mut seen: IndexSet<(String, String, Option<String>, Option<String>)> = IndexSet::default();
        let mut nodes: Vec<&String> = hit.nodes.iter().collect();
        nodes.sort();
        for node in nodes {
            for way in hit.node_via.get(node).into_iter().flatten() {
                let mut literals: IndexSet<&str> = way.builds.iter().filter_map(|t| tree_literal.get(t)).map(String::as_str).collect();
                if let Some(read) = way.via.reads.as_ref() {
                    let within = reached(read);
                    let inside = |file: &str, line: i64| within.iter().any(|(f, from, to)| f == file && (*from..=*to).contains(&line));
                    literals.extend(built.into_iter().flatten().filter(|b| inside(&b.file, b.line)).map(|b| b.literal.as_str()));
                    if literals.is_empty() {
                        for (file, _, _) in &within {
                            literals.extend(prefixed.get(file).into_iter().flatten().filter(|(_, l, p)| inside(file, *l) && spells(key, p)).map(|(id, _, _)| id.as_str()));
                        }
                    }
                }
                let mut each: Vec<Option<&str>> = literals.into_iter().map(Some).collect();
                if each.is_empty() {
                    each.push(None);
                }
                for literal in each {
                    if !seen.insert((node.clone(), way.route.clone(), way.via.binding.clone(), literal.map(str::to_string))) {
                        continue;
                    }
                    // NOT BUILT BY IT (no `key_builds` row for the pair): an expansion of its prefix, never exact.
                    // ...UNLESS THE TEMPLATE'S CALL SPELLED IT (#137): `label('time')` over `'demo.' + name` builds that one key.
                    let typed = literal.map(|l| Value::from((way.via.spelled || built.into_iter().flatten().find(|b| b.literal == l).is_some_and(|b| b.typed)) as i64));
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

/// Whether a template literal's `parts` (the text between its holes) can spell `key`: the first part opens it, the last closes it, and
/// every part between follows in order, each hole standing for text of any length. A literal with no hole spells what it starts.
fn spells(key: &str, parts: &[String]) -> bool {
    let (Some(first), Some(last)) = (parts.first(), parts.last()) else { return false };
    if parts.len() == 1 {
        return key.starts_with(first.as_str());
    }
    let Some(mut rest) = key.strip_prefix(first.as_str()) else { return false };
    let Some(inner) = rest.strip_suffix(last.as_str()) else { return false };
    rest = inner;
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part.as_str()) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// field member id -> the members and functions that fill it: those assigning it (`assignments.target_id`), those pushing onto it
/// (`key_pushes`), and - for an `@Input`, or the getter of an input setter - the members every element rendering its class reads in the
/// binding of that input.
fn feeders(store: &Store<'_>) -> HashMap<String, IndexSet<String>> {
    let mut out: HashMap<String, IndexSet<String>> = HashMap::new();
    for a in store.table("assignments").iter() {
        if let (Some(field), Some(writer)) = (id_of(a, "target_id"), id_of(a, "member")) {
            out.entry(field).or_default().insert(writer);
        }
    }
    let (calls, members, functions) = (store.table("calls"), store.table("members"), store.table("functions"));
    let field_id: HashMap<(String, String), String> = members.iter().filter_map(|m| Some(((text(m, "class")?, text(m, "name")?), id_of(m, "id")?))).collect();
    for ((class, field), pushed) in super::key_pushes::pushes(&calls, &members, &functions).field {
        let Some(id) = field_id.get(&(class, field)) else { continue };
        out.entry(id.clone()).or_default().extend(pushed.iter().filter_map(|c| id_of(c, "member")));
    }
    // (class, input name) -> the members its parents' bindings of that input read.
    let mut rendered: HashMap<String, Vec<String>> = HashMap::new();
    for r in store.table("renders").iter() {
        if let (Some(node), Some(class)) = (id_of(r, "node"), id_of(r, "to_class")) {
            rendered.entry(node).or_default().push(class);
        }
    }
    let mut bound: HashMap<(String, String), IndexSet<String>> = HashMap::new();
    for e in store.table("expressions").iter() {
        if text(e, "kind").as_deref() != Some("input") {
            continue;
        }
        let (Some(node), Some(name)) = (id_of(e, "node"), text(e, "name")) else { continue };
        let Some(classes) = rendered.get(&node) else { continue };
        let mut read = IndexSet::default();
        if let Some(ast) = e.get("ast") {
            own_reads(ast, &mut read);
        }
        for class in classes {
            bound.entry((class.clone(), name.clone())).or_default().extend(read.iter().cloned());
        }
    }
    // AN INPUT SETTER'S GETTER is fed too: `@Input() set model(items) { this._model = items; }` beside `get model() { return this._model; }`,
    // which is what the template reads - the member of that name, whichever of the pair it is.
    let alias_of: HashMap<(String, String), String> = members.iter()
        .filter_map(|m| Some(((text(m, "class")?, text(m, "name")?), super::gate_input_flags::input_alias(m)?)))
        .collect();
    for m in members.iter() {
        let (Some(id), Some(class), Some(name)) = (id_of(m, "id"), text(m, "class"), text(m, "name")) else { continue };
        let Some(alias) = alias_of.get(&(class.clone(), name)) else { continue };
        if let Some(read) = bound.get(&(class, alias.clone())) {
            out.entry(id).or_default().extend(read.iter().cloned());
        }
    }
    out
}

/// The members a template expression reads off its own component (`kinds`, `this.kinds`): every `Read` on the implicit receiver.
fn own_reads(ast: &Value, out: &mut IndexSet<String>) {
    match ast {
        Value::Object(o) => {
            let implicit = o.get("receiver").and_then(|r| r.get("k")).and_then(Value::as_str) == Some("Implicit");
            if implicit && o.get("k").and_then(Value::as_str) == Some("Read") {
                if let Some(row) = o.get("target").and_then(|t| t.get("row")).and_then(Value::as_str) {
                    out.insert(row.to_string());
                }
            }
            o.values().for_each(|v| own_reads(v, out));
        }
        Value::Array(a) => a.iter().for_each(|v| own_reads(v, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::spells;

    fn parts(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_literal_spells_a_key_by_its_text_in_order() {
        assert!(spells("demo.pid.adultWarning2", &parts(&["", "2"])));
        assert!(spells("demo.a.x.b", &parts(&["demo.", ".x.", ""])));
        assert!(!spells("demo.a.b", &parts(&["demo.", ".x.", ""])), "a middle part the key lacks");
        assert!(!spells("demo.x.a", &parts(&["demo.", "", ".x"])), "the last part closes the key");
        assert!(!spells("demox", &parts(&["demox", "x"])), "the first and last parts may not overlap");
        assert!(spells("demo.title", &parts(&["demo."])), "a literal with no hole spells what it starts");
    }
}
