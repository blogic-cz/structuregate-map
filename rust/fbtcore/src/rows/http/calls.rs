//! THE `@angular/common/http` CALLS OF A TYPESCRIPT TREE, each with the url it asks for as a pattern.
//!
//! A PRIMITIVE is a call the checker resolved into that package. Its url is FOLDED: a string is itself, a template its
//! parts around its folded holes, a `this.x` the class member of that name, a local the last value it held at the call,
//! a parameter `{}`, an unknown part `*`. A callable whose primitive's url BEGINS with one of its PARAMETERS is a WRAPPER:
//! it writes no call itself, and every call OF it does, with the url its caller handed in.

use super::{cell_value, files_of};
use crate::rows::dups::{has_columns, text};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashMap;

pub(super) struct Site {
    pub file: String,
    pub call: String,
    pub member: String,
    pub line: i64,
    pub verb: String,
    pub url: String,
    pub via: String,
}

const PRIMITIVES: [&str; 9] = ["get", "post", "put", "delete", "patch", "head", "options", "jsonp", "request"];
const PACKAGE: &str = "@angular/common/http/";
/// The class whose methods send a request; the package's other classes (`HttpHeaders`, `HttpParams`) share its names.
const CLIENT: &str = "HttpClient";
const DEPTH: usize = 8;
const ROUNDS: usize = 5;

struct Call {
    id: String,
    file: String,
    line: i64,
    member: String,
    target_id: String,
    args: Vec<Value>,
    arg_params: Vec<String>,
    primitive: Option<String>,
}

#[derive(Clone)]
struct Wrapper {
    name: String,
    index: usize,
    verb: String,
}

#[derive(Default)]
struct World {
    /// A callable's parameter names and the class its `this` is.
    params: HashMap<String, Vec<String>>,
    class_of: HashMap<String, String>,
    /// (class, name) -> what the member is initialised with.
    members: HashMap<(String, String), Value>,
    /// member -> (name, line, value) of its locals.
    locals: HashMap<String, Vec<(String, i64, Value)>>,
    /// (file, name) -> a module constant's value.
    consts: HashMap<(String, String), Value>,
}

/// Where a value is read: the file, the callable, the class `this` names and the line.
#[derive(Clone)]
struct At<'a> {
    file: &'a str,
    member: &'a str,
    class: Option<&'a str>,
    line: i64,
}

fn names(cell: &str) -> Vec<String> {
    match cell_value(cell) {
        Value::Array(items) => items
            .iter()
            .filter_map(|p| p.get("name").and_then(Value::as_str).or_else(|| p.as_str()))
            .map(str::to_string)
            .collect(),
        _ => vec![],
    }
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

impl World {
    fn load(conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<World> {
        let mut w = World::default();
        if has_columns(conn, "members", &["id", "file", "class", "name", "params", "value"]) {
            let mut stmt = conn.prepare("SELECT id, file, class, name, params, value FROM members")?;
            let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?)))?;
            for row in rows {
                let (id, file, class, name, params, value) = row?;
                if !typescript.contains_key(&file) {
                    continue;
                }
                w.params.insert(id.clone(), names(&params));
                if !class.is_empty() {
                    w.class_of.insert(id, class.clone());
                    if !value.is_empty() {
                        w.members.insert((class, name), cell_value(&value));
                    }
                }
            }
        }
        if has_columns(conn, "functions", &["id", "file", "params"]) {
            let mut stmt = conn.prepare("SELECT id, file, params FROM functions")?;
            let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?)))?;
            for row in rows {
                let (id, file, params) = row?;
                if typescript.contains_key(&file) {
                    w.params.insert(id, names(&params));
                }
            }
        }
        if has_columns(conn, "locals", &["file", "member", "name", "line", "value"]) {
            let mut stmt = conn.prepare("SELECT file, member, name, line, value FROM locals ORDER BY line")?;
            let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?)))?;
            for row in rows {
                let (file, member, name, line, value) = row?;
                if typescript.contains_key(&file) && !member.is_empty() {
                    w.locals.entry(member).or_default().push((name, line.parse().unwrap_or(0), cell_value(&value)));
                }
            }
        }
        if has_columns(conn, "consts", &["file", "name", "value"]) {
            let mut stmt = conn.prepare("SELECT file, name, value FROM consts")?;
            let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?)))?;
            for row in rows {
                let (file, name, value) = row?;
                if typescript.contains_key(&file) {
                    w.consts.insert((file, name), cell_value(&value));
                }
            }
        }
        Ok(w)
    }

    fn is_param(&self, at: &At, name: &str) -> bool {
        self.params.get(at.member).is_some_and(|p| p.iter().any(|p| p == name))
    }

    /// The last local of that name the callable held at the line.
    fn local(&self, at: &At, name: &str) -> Option<&(String, i64, Value)> {
        self.locals.get(at.member)?.iter().filter(|(n, line, _)| n == name && *line <= at.line).last()
    }

    /// A value as the url pattern it spells.
    fn fold(&self, value: &Value, at: &At, depth: usize) -> String {
        if depth >= DEPTH {
            return "*".into();
        }
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Object(o) => {
                if let (Some(Value::Array(parts)), Some(Value::Array(holes))) = (o.get("$template"), o.get("$hole_values")) {
                    if parts.len() != holes.len() + 1 {
                        return "*".into();
                    }
                    let mut out = parts[0].as_str().unwrap_or("").to_string();
                    for (hole, part) in holes.iter().zip(&parts[1..]) {
                        out.push_str(&self.fold(hole, at, depth + 1));
                        out.push_str(part.as_str().unwrap_or(""));
                    }
                    return out;
                }
                match o.get("$expr").and_then(Value::as_str) {
                    Some(expr) => self.expression(expr, at, depth),
                    None => "*".into(),
                }
            }
            _ => "*".into(),
        }
    }

    fn expression(&self, expr: &str, at: &At, depth: usize) -> String {
        if let Some(name) = expr.strip_prefix("this.").filter(|n| is_name(n)) {
            let held = at.class.and_then(|c| self.members.get(&(c.to_string(), name.to_string())));
            let member = At { member: "", line: i64::MAX, ..at.clone() };
            return held.map_or("*".into(), |v| self.fold(v, &member, depth + 1));
        }
        if is_name(expr) {
            if let Some((_, line, value)) = self.local(at, expr) {
                return self.fold(value, &At { line: *line, ..at.clone() }, depth + 1);
            }
            return if self.is_param(at, expr) { "{}".into() } else { "*".into() };
        }
        if let Some((object, key)) = expr.split_once('.').filter(|(o, k)| is_name(o) && is_name(k)) {
            if let Some(Value::Object(fields)) = self.consts.get(&(at.file.to_string(), object.to_string())) {
                if !fields.contains_key("$expr") && !fields.contains_key("$template") {
                    return fields.get(key).map_or("*".into(), |v| self.fold(v, &At { member: "", line: i64::MAX, ..at.clone() }, depth + 1));
                }
            }
        }
        "*".into()
    }

    /// The parameter a url BEGINS with: the value is that parameter, a template whose first hole is one, or a local
    /// that is either. A parameter in the middle of a path (`/orders/${id}`) is a segment, not the whole url.
    fn parameter(&self, value: &Value, at: &At, depth: usize) -> Option<String> {
        let Value::Object(o) = value else { return None };
        if let (Some(Value::Array(parts)), Some(Value::Array(holes))) = (o.get("$template"), o.get("$hole_values")) {
            let open = parts.first().and_then(Value::as_str) == Some("");
            return holes.first().filter(|_| open).and_then(|h| self.parameter(h, at, depth + 1));
        }
        let name = o.get("$expr").and_then(Value::as_str).filter(|n| is_name(n))?;
        if self.is_param(at, name) {
            return Some(name.to_string());
        }
        let (_, line, local) = self.local(at, name).filter(|_| depth < DEPTH)?;
        self.parameter(local, &At { line: *line, ..at.clone() }, depth + 1)
    }
}

fn load_calls(conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<Vec<Call>> {
    if !has_columns(conn, "calls", &["id", "file", "line", "member", "target", "target_id", "args", "arg_params"]) {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare("SELECT id, file, line, member, target, target_id, args, arg_params FROM calls")?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?, text(r, 7)?)))?;
    let mut out = vec![];
    for row in rows {
        let (id, file, line, member, target, target_id, args, arg_params) = row?;
        if !typescript.contains_key(&file) {
            continue;
        }
        let target = cell_value(&target);
        // THE OWNER, NOT THE NAME: `HttpHeaders.get` sits in the same `.d.ts` as `HttpClient.get`.
        let primitive = (target.get("file").and_then(Value::as_str).is_some_and(|f| f.contains(PACKAGE))
            && target.get("owner").and_then(Value::as_str) == Some(CLIENT))
            .then(|| target.get("name").and_then(Value::as_str))
            .flatten()
            .filter(|n| PRIMITIVES.contains(n))
            .map(str::to_string);
        let Value::Array(args) = cell_value(&args) else { continue };
        out.push(Call { id, file, line: line.parse().unwrap_or(0), member, target_id, args, arg_params: names(&arg_params), primitive });
    }
    Ok(out)
}

/// The url as stored: cut at the query, an unknown base made its own leading segment.
fn finished(url: &str) -> String {
    let cut = url.split('?').next().unwrap_or("");
    if cut.len() > 1 && cut.starts_with('*') && !cut.starts_with("*/") { format!("*/{}", &cut[1..]) } else { cut.to_string() }
}

fn place<'a>(world: &'a World, c: &'a Call) -> At<'a> {
    At { file: &c.file, member: &c.member, class: world.class_of.get(&c.member).map(String::as_str), line: c.line }
}

pub(super) fn load(conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<Vec<Site>> {
    let paths = files_of(conn, "typescript")?;
    let calls = load_calls(conn, typescript)?;
    let world = World::load(conn, typescript)?;
    let none = Value::Null;

    // THE PRIMITIVES: the verb and the url's value, and which of them are wrappers.
    let mut wrappers: HashMap<String, Wrapper> = HashMap::new();
    let mut verbs: HashMap<usize, (String, usize)> = HashMap::new();
    for (n, c) in calls.iter().enumerate() {
        let Some(name) = &c.primitive else { continue };
        let (verb, url) = if name == "request" {
            let verb = match c.args.first() {
                Some(Value::String(v)) => v.to_uppercase(),
                _ => "ANY".into(),
            };
            (verb, 1)
        } else {
            (name.to_uppercase(), 0)
        };
        verbs.insert(n, (verb.clone(), url));
        if let Some(param) = c.args.get(url).and_then(|v| world.parameter(v, &place(&world, c), 0)) {
            let index = world.params.get(&c.member).and_then(|p| p.iter().position(|p| *p == param)).unwrap_or(0);
            wrappers.entry(c.member.clone()).or_insert(Wrapper { name: param, index, verb });
        }
    }
    // A CALLABLE THAT HANDS ITS OWN PARAMETER TO A WRAPPER IS ONE: the verb is the inner one's.
    let position = |w: &Wrapper, c: &Call| c.arg_params.iter().position(|p| *p == w.name).unwrap_or(w.index);
    for _ in 0..ROUNDS {
        let mut found: Vec<(String, Wrapper)> = vec![];
        for c in &calls {
            let Some(w) = wrappers.get(&c.target_id).filter(|_| !c.target_id.is_empty()) else { continue };
            if wrappers.contains_key(&c.member) {
                continue;
            }
            if let Some(param) = c.args.get(position(w, c)).and_then(|v| world.parameter(v, &place(&world, c), 0)) {
                let index = world.params.get(&c.member).and_then(|p| p.iter().position(|p| *p == param)).unwrap_or(0);
                found.push((c.member.clone(), Wrapper { name: param, index, verb: w.verb.clone() }));
            }
        }
        if found.is_empty() {
            break;
        }
        for (member, w) in found {
            wrappers.entry(member).or_insert(w);
        }
    }
    // THE ROWS: a primitive nobody parameterises, and every call of a wrapper that hands it a url of its own.
    let mut sites = vec![];
    for (n, c) in calls.iter().enumerate() {
        let (verb, via, value) = match (verbs.get(&n), wrappers.get(&c.target_id).filter(|_| !c.target_id.is_empty())) {
            (Some((verb, url)), _) => (verb.clone(), String::new(), c.args.get(*url)),
            (None, Some(w)) => (w.verb.clone(), c.target_id.clone(), c.args.get(position(w, c))),
            _ => continue,
        };
        let value = value.unwrap_or(&none);
        if world.parameter(value, &place(&world, c), 0).is_some() {
            continue;
        }
        sites.push(Site {
            file: paths.get(&c.file).cloned().unwrap_or_default(),
            call: c.id.clone(),
            member: c.member.clone(),
            line: c.line,
            verb,
            url: finished(&world.fold(value, &place(&world, c), 0)),
            via,
        });
    }
    sites.sort_by(|a, b| (&a.file, a.line, &a.call).cmp(&(&b.file, b.line, &b.call)));
    Ok(sites)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn world() -> World {
        let mut w = World::default();
        w.params.insert("m:1".into(), vec!["id".into()]);
        w.class_of.insert("m:1".into(), "c:1".into());
        w.members.insert(("c:1".into(), "base".into()), json!({"$template": ["", "api/"], "$hole_values": [{"$expr": "this.root"}]}));
        w.consts.insert(("f:1".into(), "API".into()), json!({"list": "/x/list"}));
        w.locals.insert("m:1".into(), vec![("u".into(), 3, json!("/early")), ("u".into(), 9, json!("/late"))]);
        w
    }

    fn at(line: i64) -> At<'static> {
        At { file: "f:1", member: "m:1", class: Some("c:1"), line }
    }

    #[test]
    fn a_value_folds_through_members_locals_parameters_and_constants() {
        let w = world();
        let hole = |e: &str| json!({"$template": ["", "/z"], "$hole_values": [{"$expr": e}]});
        assert_eq!(w.fold(&hole("this.base"), &at(5), 0), "*api//z");
        assert_eq!(w.fold(&hole("id"), &at(5), 0), "{}/z");
        assert_eq!(w.fold(&hole("u"), &at(5), 0), "/early/z");
        assert_eq!(w.fold(&hole("u"), &at(12), 0), "/late/z");
        assert_eq!(w.fold(&hole("API.list"), &at(5), 0), "/x/list/z");
        assert_eq!(w.fold(&hole("g()"), &at(5), 0), "*/z");
        assert_eq!(finished("*api/x?a=1"), "*/api/x");
    }

    #[test]
    fn a_parameter_is_found_straight_or_through_a_local() {
        let mut w = world();
        w.locals.insert("m:1".into(), vec![("u".into(), 3, json!({"$expr": "id"}))]);
        assert_eq!(w.parameter(&json!({"$expr": "id"}), &at(5), 0).as_deref(), Some("id"));
        assert_eq!(w.parameter(&json!({"$expr": "u"}), &at(5), 0).as_deref(), Some("id"));
        assert_eq!(w.parameter(&json!({"$template": ["", "/x"], "$hole_values": [{"$expr": "id"}]}), &at(5), 0).as_deref(), Some("id"));
        // A PARAMETER INSIDE THE PATH is a segment, not a base.
        assert_eq!(w.parameter(&json!({"$template": ["/a/", ""], "$hole_values": [{"$expr": "id"}]}), &at(5), 0), None);
        assert_eq!(w.parameter(&json!("/plain"), &at(5), 0), None);
    }
}
