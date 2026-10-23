//! THE `@angular/common/http` CALLS OF A TYPESCRIPT TREE, each with the url it asks for as a pattern.
//!
//! A PRIMITIVE is a call the checker resolved into that package. Its url is FOLDED: a string is itself, a template its
//! parts around its folded holes, a `this.x` the class member of that name, a local the last value it held at the call,
//! a parameter `{}`, an unknown part `*`. A callable whose primitive's url BEGINS with one of its PARAMETERS is a WRAPPER:
//! it writes no call itself, and every call OF it does, with the url its caller handed in. The url BEGINS with a parameter
//! when it is that parameter, a template opening with one, a local holding any of these, EITHER ARM of a ternary
//! (`q = params ? createQueryUrl(url, params) : url`), or a call whose FIRST argument is one AND whose callee returns that argument
//! (`createQueryUrl(url, params)` is the parameter and an unknown suffix: the suffix is the query a caller's url never carries,
//! `finished` cuts it). The callee's own `returns` say so (`calls_helpers.rs`): `` `${url}${query}` `` does, `` `${this.root}x` `` over an
//! id parameter does not - that call is an unknown piece (`*`) and the literal after it stays. A CALLER's own url folds through such a
//! helper to its first argument too (`createQueryUrl(`${base}/count`, options)` is `${base}/count`).
//!
//! A CALL IS OF A WRAPPER BY `calls.callee_id` (rows 81), the callable that runs - never `target_id`, which is the checker's declaration and may be
//! a property of an object literal (`{ get }`) whose value is the function. A helper hands its url back through a `+` or a `new URL(url)` too (`calls_helpers.rs`).

use super::files_of;
use super::inputs::read_of;
use crate::rows::dups::{has_columns, owner_sql, scalar_sql, text, Column};
// A whole cell spelled `null` is read as the null it stood for before #85 wrote it as `{$literal: 'null'}`.
use crate::rows::ts::store::null_literal;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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

struct Call {
    id: String,
    file: String,
    line: i64,
    member: String,
    callee_id: String,
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
    /// (abs path, name) -> every callable of that name, and each callable's `returns` (`calls_helpers.rs`).
    callees: HashMap<(String, String), Vec<String>>,
    returns: HashMap<String, Vec<Value>>,
}

/// Where a value is read: the file, the callable, the class `this` names and the line.
#[derive(Clone)]
struct At<'a> {
    file: &'a str,
    member: &'a str,
    class: Option<&'a str>,
    line: i64,
}

fn names(cell: Value) -> Vec<String> {
    match cell {
        Value::Array(items) => items
            .iter()
            .filter_map(|p| p.get("name").and_then(Value::as_str).or_else(|| p.as_str()))
            .map(str::to_string)
            .collect(),
        _ => vec![],
    }
}

/// Four text columns of a table, the owner its node rows are keyed by, and up to two cells as stored.
type Read = (String, String, String, String, String, SqlValue, SqlValue);

fn read(conn: &Connection, table: &str, picked: &str, cells: &[&str], order: &str) -> rusqlite::Result<Vec<Read>> {
    let cell = |i: usize| cells.get(i).map_or("NULL".to_string(), |c| scalar_sql(conn, table, "t", c));
    let picked: Vec<String> = picked.split(", ").map(|c| if c.starts_with('\'') { c.to_string() } else { format!("t.{c}") }).collect();
    let sql = format!("SELECT {}, {}, {}, {} FROM \"{table}\" t {order}", picked.join(", "), owner_sql(conn, table, "t"), cell(0), cell(1));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, r.get(5)?, r.get(6)?)))?;
    rows.collect()
}

/// A cell no row filled reads as the empty text it read as when every cell was text.
fn blank(value: Value) -> Value {
    if value.is_null() { Value::String(String::new()) } else { value }
}

fn is_empty(value: &Value) -> bool {
    value.is_null() || value.as_str() == Some("")
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

impl World {
    fn load(conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<World> {
        let mut w = World::default();
        if has_columns(conn, "members", read_of("members")) {
            let (mut params, mut value) = (Column::new(conn, "members", "params")?, Column::new(conn, "members", "value")?);
            for (id, file, class, name, owner, held_params, held_value) in read(conn, "members", "id, file, class, name", &["params", "value"], "")? {
                if !typescript.contains_key(&file) {
                    continue;
                }
                w.params.insert(id.clone(), names(params.value(&owner, &held_params)?));
                if !class.is_empty() {
                    w.class_of.insert(id.clone(), class.clone());
                    let value = null_literal(value.value(&owner, &held_value)?);
                    if !is_empty(&value) {
                        w.members.insert((class, name), value);
                    }
                }
            }
        }
        if has_columns(conn, "functions", read_of("functions")) {
            let mut params = Column::new(conn, "functions", "params")?;
            for (id, file, _, _, owner, held, _) in read(conn, "functions", "id, file, '', ''", &["params"], "")? {
                if typescript.contains_key(&file) {
                    w.params.insert(id, names(params.value(&owner, &held)?));
                }
            }
        }
        if has_columns(conn, "locals", read_of("locals")) {
            let mut value = Column::new(conn, "locals", "value")?;
            for (file, member, name, line, owner, held, _) in read(conn, "locals", "file, member, name, line", &["value"], "ORDER BY t.line")? {
                if typescript.contains_key(&file) && !member.is_empty() {
                    let value = blank(null_literal(value.value(&owner, &held)?));
                    w.locals.entry(member).or_default().push((name, line.parse().unwrap_or(0), value));
                }
            }
        }
        if has_columns(conn, "consts", read_of("consts")) {
            let mut value = Column::new(conn, "consts", "value")?;
            for (file, name, _, _, owner, held, _) in read(conn, "consts", "file, name, '', ''", &["value"], "")? {
                if typescript.contains_key(&file) {
                    let value = blank(null_literal(value.value(&owner, &held)?));
                    w.consts.insert((file, name), value);
                }
            }
        }
        w.load_helpers(conn, typescript)?;
        Ok(w)
    }

    fn is_param(&self, at: &At, name: &str) -> bool {
        self.params.get(at.member).is_some_and(|p| p.iter().any(|p| p == name))
    }

    /// The last local of that name the callable held at the line.
    fn local(&self, at: &At, name: &str) -> Option<&(String, i64, Value)> {
        self.locals.get(at.member)?.iter().filter(|(n, line, _)| n == name && *line <= at.line).last()
    }

    /// A value as the url pattern it spells. `seen` is every name being folded on the way here: a name that reads
    /// itself (`let u = u + '/x'`, a member initialised from itself) is a cycle and folds to `*` - no depth cap.
    fn fold(&self, value: &Value, at: &At, seen: &mut HashSet<String>) -> String {
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
                        out.push_str(&self.fold(hole, at, seen));
                        out.push_str(part.as_str().unwrap_or(""));
                    }
                    return out;
                }
                // A CALL OF A HELPER THAT RETURNS ITS FIRST ARGUMENT is that argument (`calls_helpers.rs`): its tail is the query `finished` cuts.
                if o.get("$call").is_some_and(Value::is_string) && self.forwards_first(o, true) {
                    return o.get("$args").and_then(Value::as_array).and_then(|a| a.first()).map_or("*".into(), |a| self.fold(a, at, seen));
                }
                match o.get("$expr").and_then(Value::as_str) {
                    Some(expr) => self.expression(expr, at, seen),
                    None => "*".into(),
                }
            }
            _ => "*".into(),
        }
    }

    /// `value` folded at `at` under the name `key`, unless that name is already being folded.
    fn through(&self, key: String, value: &Value, at: &At, seen: &mut HashSet<String>) -> String {
        if !seen.insert(key.clone()) {
            return "*".into();
        }
        let out = self.fold(value, at, seen);
        seen.remove(&key);
        out
    }

    fn expression(&self, expr: &str, at: &At, seen: &mut HashSet<String>) -> String {
        if let Some(name) = expr.strip_prefix("this.").filter(|n| is_name(n)) {
            let held = at.class.and_then(|c| self.members.get(&(c.to_string(), name.to_string())));
            let member = At { member: "", line: i64::MAX, ..at.clone() };
            let key = format!("this\0{}\0{name}", at.class.unwrap_or(""));
            return held.map_or("*".into(), |v| self.through(key, v, &member, seen));
        }
        if is_name(expr) {
            if let Some((_, line, value)) = self.local(at, expr) {
                let key = format!("local\0{}\0{expr}\0{line}", at.member);
                return self.through(key, value, &At { line: *line, ..at.clone() }, seen);
            }
            return if self.is_param(at, expr) { "{}".into() } else { "*".into() };
        }
        if let Some((object, key)) = expr.split_once('.').filter(|(o, k)| is_name(o) && is_name(k)) {
            if let Some(Value::Object(fields)) = self.consts.get(&(at.file.to_string(), object.to_string())) {
                if !fields.contains_key("$expr") && !fields.contains_key("$template") {
                    let name = format!("const\0{}\0{object}\0{key}", at.file);
                    return fields.get(key).map_or("*".into(), |v| self.through(name, v, &At { member: "", line: i64::MAX, ..at.clone() }, seen));
                }
            }
        }
        "*".into()
    }

    /// The parameter a url BEGINS with: the value is that parameter, a template whose first hole is one, or a local
    /// that is either. A parameter in the middle of a path (`/orders/${id}`) is a segment, not the whole url.
    /// A local followed back to itself is a cycle `seen` stops, never a depth.
    fn parameter(&self, value: &Value, at: &At, seen: &mut HashSet<String>) -> Option<String> {
        let Value::Object(o) = value else { return None };
        if let (Some(Value::Array(parts)), Some(Value::Array(holes))) = (o.get("$template"), o.get("$hole_values")) {
            let open = parts.first().and_then(Value::as_str) == Some("");
            return holes.first().filter(|_| open).and_then(|h| self.parameter(h, at, seen));
        }
        // A TERNARY IS ITS CONDITION'S TWO ARMS, and it carries its own text as `$expr` too - so it is read before the name.
        if o.contains_key("$cond") {
            return ["$then", "$else"].iter().filter_map(|arm| o.get(*arm)).find_map(|arm| self.parameter(arm, at, seen));
        }
        // A CALL OF THE PARAMETER'S FIRST ARGUMENT is the parameter followed by whatever the callee appends - when the callee's own returns say it
        // returns that argument (`calls_helpers.rs`); a callee that turns it into a url (an id handed to a url builder) makes the call an unknown piece.
        if o.get("$call").is_some_and(Value::is_string) {
            if !self.forwards_first(o, false) {
                return None;
            }
            return o.get("$args").and_then(Value::as_array).and_then(|a| a.first()).and_then(|a| self.parameter(a, at, seen));
        }
        let name = o.get("$expr").and_then(Value::as_str).filter(|n| is_name(n))?;
        if self.is_param(at, name) {
            return Some(name.to_string());
        }
        let (_, line, local) = self.local(at, name)?;
        let key = format!("{}\0{name}\0{line}", at.member);
        if !seen.insert(key.clone()) {
            return None;
        }
        let found = self.parameter(local, &At { line: *line, ..at.clone() }, seen);
        seen.remove(&key);
        found
    }

    /// `fold` from nothing yet seen.
    fn url(&self, value: &Value, at: &At) -> String {
        self.fold(value, at, &mut HashSet::new())
    }

    /// `parameter` from nothing yet seen.
    fn base(&self, value: &Value, at: &At) -> Option<String> {
        self.parameter(value, at, &mut HashSet::new())
    }
}

fn load_calls(conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<Vec<Call>> {
    if !has_columns(conn, "calls", read_of("calls")) {
        return Ok(vec![]);
    }
    // `target` is an object, `args` and `arg_params` lists: each read back from its nodes.
    let (mut targets, mut args, mut arg_params) =
        (Column::new(conn, "calls", "target")?, Column::new(conn, "calls", "args")?, Column::new(conn, "calls", "arg_params")?);
    let scalar = |c: &str| scalar_sql(conn, "calls", "c", c);
    let sql = format!(
        "SELECT c.id, c.file, c.line, c.member, c.callee_id, {}, {}, {}, {} FROM calls c",
        owner_sql(conn, "calls", "c"),
        scalar("target"),
        scalar("args"),
        scalar("arg_params")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, r.get::<_, SqlValue>(6)?, r.get::<_, SqlValue>(7)?, r.get::<_, SqlValue>(8)?))
    })?;
    let mut out = vec![];
    for row in rows {
        let (id, file, line, member, callee_id, owner, target, held_args, held_params) = row?;
        if !typescript.contains_key(&file) {
            continue;
        }
        let target = targets.value(&owner, &target)?;
        // THE OWNER, NOT THE NAME: `HttpHeaders.get` sits in the same `.d.ts` as `HttpClient.get`.
        let primitive = (target.get("file").and_then(Value::as_str).is_some_and(|f| f.contains(PACKAGE))
            && target.get("owner").and_then(Value::as_str) == Some(CLIENT))
            .then(|| target.get("name").and_then(Value::as_str))
            .flatten()
            .filter(|n| PRIMITIVES.contains(n))
            .map(str::to_string);
        let Value::Array(args) = args.value(&owner, &held_args)? else { continue };
        let arg_params = names(arg_params.value(&owner, &held_params)?);
        out.push(Call { id, file, line: line.parse().unwrap_or(0), member, callee_id, args, arg_params, primitive });
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
        if let Some(param) = c.args.get(url).and_then(|v| world.base(v, &place(&world, c))) {
            let index = world.params.get(&c.member).and_then(|p| p.iter().position(|p| *p == param)).unwrap_or(0);
            wrappers.entry(c.member.clone()).or_insert(Wrapper { name: param, index, verb });
        }
    }
    // A CALLABLE THAT HANDS ITS OWN PARAMETER TO A WRAPPER IS ONE: the verb is the inner one's.
    let position = |w: &Wrapper, c: &Call| c.arg_params.iter().position(|p| *p == w.name).unwrap_or(w.index);
    // UNTIL NOTHING NEW IS FOUND: each round only adds callables not yet wrappers, so it ends - no round cap.
    loop {
        let mut found: Vec<(String, Wrapper)> = vec![];
        for c in &calls {
            let Some(w) = wrappers.get(&c.callee_id).filter(|_| !c.callee_id.is_empty()) else { continue };
            if wrappers.contains_key(&c.member) {
                continue;
            }
            if let Some(param) = c.args.get(position(w, c)).and_then(|v| world.base(v, &place(&world, c))) {
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
        let (verb, via, value) = match (verbs.get(&n), wrappers.get(&c.callee_id).filter(|_| !c.callee_id.is_empty())) {
            (Some((verb, url)), _) => (verb.clone(), String::new(), c.args.get(*url)),
            (None, Some(w)) => (w.verb.clone(), c.callee_id.clone(), c.args.get(position(w, c))),
            _ => continue,
        };
        let value = value.unwrap_or(&none);
        if world.base(value, &place(&world, c)).is_some() {
            continue;
        }
        sites.push(Site {
            file: paths.get(&c.file).cloned().unwrap_or_default(),
            call: c.id.clone(),
            member: c.member.clone(),
            line: c.line,
            verb,
            url: finished(&world.url(value, &place(&world, c))),
            via,
        });
    }
    sites.sort_by(|a, b| (&a.file, a.line, &a.call).cmp(&(&b.file, b.line, &b.call)));
    Ok(sites)
}

#[path = "calls_helpers.rs"]
mod calls_helpers;

#[cfg(test)]
#[path = "calls_wrapper_tests.rs"]
mod wrapper_tests;

#[cfg(test)]
#[path = "calls_helpers_tests.rs"]
mod helpers_tests;

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
        assert_eq!(w.url(&hole("this.base"), &at(5)), "*api//z");
        assert_eq!(w.url(&hole("id"), &at(5)), "{}/z");
        assert_eq!(w.url(&hole("u"), &at(5)), "/early/z");
        assert_eq!(w.url(&hole("u"), &at(12)), "/late/z");
        assert_eq!(w.url(&hole("API.list"), &at(5)), "/x/list/z");
        assert_eq!(w.url(&hole("g()"), &at(5)), "*/z");
        assert_eq!(finished("*api/x?a=1"), "*/api/x");
    }

    #[test]
    fn a_parameter_is_found_straight_or_through_a_local() {
        let mut w = world();
        w.locals.insert("m:1".into(), vec![("u".into(), 3, json!({"$expr": "id"}))]);
        assert_eq!(w.base(&json!({"$expr": "id"}), &at(5)).as_deref(), Some("id"));
        assert_eq!(w.base(&json!({"$expr": "u"}), &at(5)).as_deref(), Some("id"));
        assert_eq!(w.base(&json!({"$template": ["", "/x"], "$hole_values": [{"$expr": "id"}]}), &at(5)).as_deref(), Some("id"));
        // A PARAMETER INSIDE THE PATH is a segment, not a base.
        assert_eq!(w.base(&json!({"$template": ["/a/", ""], "$hole_values": [{"$expr": "id"}]}), &at(5)), None);
        assert_eq!(w.base(&json!("/plain"), &at(5)), None);
    }

    #[test]
    fn a_name_that_reads_itself_is_a_cycle_and_a_long_chain_is_followed_to_its_end() {
        let mut w = world();
        // `let u = `${u}/x`` - and a member initialised from itself: each folds to `*` where it loops, and ends.
        w.locals.insert("m:1".into(), vec![("u".into(), 3, json!({"$template": ["", "/x"], "$hole_values": [{"$expr": "u"}]}))]);
        w.members.insert(("c:1".into(), "self".into()), json!({"$expr": "this.self"}));
        assert_eq!(w.url(&json!({"$expr": "u"}), &at(5)), "*/x");
        assert_eq!(w.url(&json!({"$expr": "this.self"}), &at(5)), "*");
        assert_eq!(w.base(&json!({"$expr": "u"}), &at(5)), None);
        // TWELVE locals deep, each the one before it: past the old cap of eight, and still folded whole.
        let mut chain = vec![("v0".to_string(), 1, json!("/root"))];
        for i in 1..12 {
            chain.push((format!("v{i}"), 1 + i as i64, json!({"$template": ["", "/s"], "$hole_values": [{"$expr": format!("v{}", i - 1)}]})));
        }
        w.locals.insert("m:1".into(), chain);
        assert_eq!(w.url(&json!({"$expr": "v11"}), &at(20)), format!("/root{}", "/s".repeat(11)));
        // The same name twice in one url is no cycle.
        w.members.insert(("c:1".into(), "a".into()), json!("/a"));
        let twice = json!({"$template": ["", "", ""], "$hole_values": [{"$expr": "this.a"}, {"$expr": "this.a"}]});
        assert_eq!(w.url(&twice, &at(5)), "/a/a");
    }

    #[test]
    fn the_calls_are_read_from_their_node_rows() {
        let conn = Connection::open_in_memory().unwrap();
        let rows = |table: &str, rows: Vec<Value>| crate::rows::store::insert_rows(&conn, table, &rows).unwrap();
        rows("files", vec![json!({"id": "f:1", "path": "a.service.ts", "lang": "typescript"})]);
        rows("members", vec![
            json!({"id": "m:1", "file": "f:1", "class": "c:1", "name": "load", "params": [{"name": "id"}], "value": ""}),
            json!({"id": "m:2", "file": "f:1", "class": "c:1", "name": "base", "params": [], "value": "/api"}),
        ]);
        rows("calls", vec![json!({
            "id": "call:1", "file": "f:1", "line": 4, "member": "m:1", "callee_id": "",
            "target": {"file": "node_modules/@angular/common/http/index.d.ts", "owner": "HttpClient", "name": "get"},
            "args": [{"$template": ["", "/orders"], "$hole_values": [{"$expr": "this.base"}]}], "arg_params": [],
        })]);
        let typescript = files_of(&conn, "typescript").unwrap();
        let sites = load(&conn, &typescript).unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!((sites[0].verb.as_str(), sites[0].url.as_str(), sites[0].file.as_str()), ("GET", "/api/orders", "a.service.ts"));
    }
}
