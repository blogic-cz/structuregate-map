//! THE TYPES AN ENDPOINT USES, by resolved symbol: a field table under a section that names an endpoint lists the
//! properties of a type that endpoint takes or returns - not of every class whose property names happen to agree.
//! An endpoint's action (`http_endpoints.func`) is a `functions` row: its RETURN type (`functions.type`, as Roslyn
//! resolved it), each PARAMETER's type (`parameters.type`) and each `[ProducesResponseType(typeof(X))]` (the folded
//! `typeof`, `decorators.arg_values`). Every name in such a type is read - `Task<ActionResult<Response<ItemDto>>>`
//! names `Response` and `ItemDto` - and kept when it is a class the map holds properties of; a kept class's own
//! property types are walked the same way (a visited set ends the walk), so a nested object is the endpoint's too. An
//! action inherited from a generic base has its type parameters bound by the serving controller (`generics.rs`).

use super::apis::same_endpoint;
use crate::rows::dups::{has_columns, items_sql, owner_sql, text};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What a class is to an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Request,
    OutboundRequest,
    Response,
    OutboundResponse,
}

impl Role {
    pub fn word(self) -> &'static str {
        match self {
            Role::Request => "request",
            Role::OutboundRequest => "outbound request",
            Role::Response => "response",
            Role::OutboundResponse => "outbound response",
        }
    }
}

/// Every endpoint (`VERB /template`) with the classes it uses, by `members.class_symbol`.
#[derive(Default)]
pub struct Uses {
    endpoints: Vec<(String, BTreeMap<String, Role>)>,
    /// The routes the tree CALLS (`http_calls`), each with the classes its calls send and get back.
    calls: Vec<(String, BTreeMap<String, Role>)>,
}

/// The endpoints a section names and the classes they use.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bound {
    pub endpoints: Vec<String>,
    pub classes: BTreeMap<String, Role>,
    /// Which of those endpoints use each class.
    pub used_by: BTreeMap<String, Vec<String>>,
}

impl Uses {
    /// Every class an endpoint uses, with each endpoint using it and how (`GET /api/items (response)`), in endpoint order.
    pub fn users(&self) -> HashMap<String, Vec<String>> {
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        for (endpoint, classes) in self.endpoints.iter().chain(&self.calls) {
            for (class, role) in classes {
                out.entry(class.clone()).or_default().push(format!("{endpoint} ({})", role.word()));
            }
        }
        out
    }

    /// The endpoints the routes name (each route against each endpoint, as a link compares them) and their classes;
    /// a class both takes and returns is a response.
    pub fn of(&self, routes: &[String]) -> Bound {
        let mut out = Bound::default();
        let served = self.endpoints.iter().map(|e| (e, false));
        for ((endpoint, classes), called) in served.chain(self.calls.iter().map(|c| (c, true))) {
            if routes.iter().any(|r| if called { same_call(endpoint, r) } else { same_endpoint(endpoint, r) }) {
                out.endpoints.push(endpoint.clone());
                for (class, role) in classes {
                    widen(&mut out.classes, class, *role);
                    out.used_by.entry(class.clone()).or_default().push(endpoint.clone());
                }
            }
        }
        out
    }
}

/// Every endpoint's classes and every called route's, from the rows the C# half stored.
pub fn load(conn: &Connection) -> rusqlite::Result<Uses> {
    if !has_columns(conn, "members", &["kind", "class_symbol", "type"]) {
        return Ok(Uses::default());
    }
    let classes = Classes::read(conn)?;
    Ok(Uses { endpoints: served(conn, &classes)?, calls: called(conn, &classes)? })
}

/// THE ROUTES THE TREE CALLS: each `http_calls` row's call (`calls`, through a wrapper the row it was written in)
/// returns its response type and takes its parameters' (`calls.type`, `calls.signature`, as Roslyn bound them -
/// `PostAsync<ReqA, RespA>(string, ReqA)` gives `ReqA` and `RespA`). A url is kept from its path: a scheme and host
/// and the holes it starts with (a base url the run time supplies) are no part of it; a url nothing could fold is none.
fn called(conn: &Connection, classes: &Classes) -> rusqlite::Result<Vec<(String, BTreeMap<String, Role>)>> {
    let mut out: Vec<(String, BTreeMap<String, Role>)> = Vec::new();
    if !has_columns(conn, "http_calls", &["call", "verb", "url"]) || !has_columns(conn, "calls", &["id", "type", "signature"]) {
        return Ok(out);
    }
    let mut stmt = conn.prepare("SELECT h.verb, h.url, c.type, c.signature FROM http_calls h JOIN calls c ON c.id = h.call ORDER BY h.id")?;
    for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?)))? {
        let (verb, url, returns, signature) = row?;
        let Some(route) = call_route(&verb, &url) else { continue };
        let at = match out.iter().position(|(r, _)| *r == route) {
            Some(at) => at,
            None => {
                out.push((route, BTreeMap::new()));
                out.len() - 1
            }
        };
        for class in classes.reached(&returns) {
            widen(&mut out[at].1, &class, Role::OutboundResponse);
        }
        // THE PARAMETER TYPES of the bound signature: what follows its name's `(`.
        let parameters = signature.find('(').map_or("", |i| &signature[i..]);
        for class in classes.reached(parameters) {
            widen(&mut out[at].1, &class, Role::OutboundRequest);
        }
    }
    Ok(out)
}

/// A called url as a route: `VERB /path`, without its scheme, host and the holes it starts with; None when a part of it
/// could not be folded (`*`) or no path is left.
fn call_route(verb: &str, url: &str) -> Option<String> {
    if url.contains('*') {
        return None;
    }
    let path = match url.split_once("://") {
        Some((_, rest)) => &rest[rest.find('/')?..],
        None => url,
    };
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).skip_while(|s| *s == "{}").collect();
    (!segments.is_empty()).then(|| format!("{verb} /{}", segments.join("/")))
}

/// A documented route against a called one: the same verb when both name one, and the shorter path the END of the
/// longer one - a base path the tree adds at run time (`/api`) is in the document and not in the code - on at least
/// two segments, a parameter on either side any one segment.
pub fn same_call(call: &str, fact: &str) -> bool {
    let ((call_verb, call_path), (fact_verb, fact_path)) = (super::apis::route_parts(call), super::apis::route_parts(fact));
    if let (Some(a), Some(b)) = (&call_verb, &fact_verb)
        && a != b
    {
        return false;
    }
    let n = call_path.len().min(fact_path.len());
    let (c, f) = (&call_path[call_path.len() - n..], &fact_path[fact_path.len() - n..]);
    let same = |a: &String, b: &String| a == b || a.starts_with("{}") || b.starts_with("{}");
    c.iter().zip(f).all(|(a, b)| same(a, b)) && c.iter().zip(f).filter(|(a, b)| a == b && !a.starts_with("{}")).count() >= 2
}

/// The endpoints the tree SERVES, each with the classes its action takes and returns.
fn served(conn: &Connection, classes: &Classes) -> rusqlite::Result<Vec<(String, BTreeMap<String, Role>)>> {
    if !has_columns(conn, "http_endpoints", &["verb", "template", "func"])
        || !has_columns(conn, "functions", &["id", "file", "name", "qualname", "line", "end_line", "type"])
    {
        return Ok(Vec::new());
    }
    let mut by_func: HashMap<String, Vec<(String, Role)>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT e.func, f.type FROM http_endpoints e JOIN functions f ON f.id = e.func")?;
    for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))? {
        let (func, returns) = row?;
        by_func.entry(func).or_default().push((returns, Role::Response));
    }
    if has_columns(conn, "parameters", &["file", "qualname", "line", "type"]) {
        let mut stmt = conn.prepare(
            "SELECT f.id, p.type FROM functions f JOIN parameters p ON p.file = f.file AND p.qualname = f.qualname \
             AND p.line BETWEEN f.line AND f.end_line WHERE f.id IN (SELECT func FROM http_endpoints)",
        )?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))? {
            let (func, typed) = row?;
            by_func.entry(func).or_default().push((typed, Role::Request));
        }
    }
    if has_columns(conn, "decorators", &["file", "line", "name", "target", "arg_values"]) {
        let (args, owner) = (items_sql(conn, "decorators", "arg_values", &["value"]), owner_sql(conn, "decorators", "d"));
        let mut stmt = conn.prepare(&format!(
            "SELECT f.id, a.value FROM functions f JOIN decorators d ON d.file = f.file AND d.target = f.name \
             AND d.line BETWEEN f.line AND f.end_line JOIN {args} a ON a.owner = {owner} \
             WHERE f.id IN (SELECT func FROM http_endpoints) AND (d.name LIKE '%ProducesResponseType' OR d.name LIKE '%ProducesResponseTypeAttribute')"
        ))?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))? {
            let (func, typed) = row?;
            by_func.entry(func).or_default().push((typed, Role::Response));
        }
    }
    // AN INHERITED ACTION'S TYPES are its base's, with the base's type parameters bound by the class serving it.
    let hierarchy = super::generics::Hierarchy::read(conn)?;
    let declaring = if has_columns(conn, "functions", &["cls"]) { "coalesce(f.file, ''), coalesce(f.cls, '')" } else { "'', ''" };
    let mut stmt = conn.prepare(&format!(
        "SELECT e.verb, e.template, e.func, e.file, e.cls, {declaring} FROM http_endpoints e LEFT JOIN functions f ON f.id = e.func ORDER BY e.id"
    ))?;
    let mut endpoints = Vec::new();
    for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?)))? {
        let (verb, template, func, path, server, file, owner) = row?;
        let bound = hierarchy.bindings((&path, &server), (&file, &owner));
        let mut used: BTreeMap<String, Role> = BTreeMap::new();
        for (typed, role) in by_func.get(&func).into_iter().flatten() {
            for class in classes.reached(&super::generics::substitute(typed, &bound)) {
                widen(&mut used, &class, *role);
            }
        }
        endpoints.push((format!("{verb} /{}", template.trim_start_matches(['~', '/'])), used));
    }
    Ok(endpoints)
}

/// A class's role, once more: one an endpoint both takes and returns is a response.
fn widen(classes: &mut BTreeMap<String, Role>, class: &str, role: Role) {
    let slot = classes.entry(class.to_string()).or_insert(role);
    *slot = (*slot).max(role);
}

/// The classes the map holds properties of: each by its symbol without type parameters, and its property types.
struct Classes {
    symbol: HashMap<String, Vec<String>>,
    property_types: HashMap<String, Vec<String>>,
}

impl Classes {
    fn read(conn: &Connection) -> rusqlite::Result<Classes> {
        let mut stmt = conn.prepare("SELECT class_symbol, type FROM members WHERE kind = 'property' AND coalesce(class_symbol, '') <> ''")?;
        let (mut symbol, mut property_types): (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) = Default::default();
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))? {
            let (class, typed) = row?;
            let bare = names(&class).into_iter().next().unwrap_or_default();
            let known = symbol.entry(bare).or_default();
            if !known.contains(&class) {
                known.push(class.clone());
            }
            property_types.entry(class).or_default().push(typed);
        }
        Ok(Classes { symbol, property_types })
    }

    /// Every class a type names, and every class their properties name in turn.
    fn reached(&self, typed: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut todo: Vec<String> = vec![typed.to_string()];
        while let Some(t) = todo.pop() {
            for name in names(&t) {
                for class in self.symbol.get(&name).into_iter().flatten() {
                    if seen.insert(class.clone()) {
                        todo.extend(self.property_types.get(class).into_iter().flatten().cloned());
                    }
                }
            }
        }
        seen
    }
}

/// The type names a resolved type is written with: `A.B<C.D, E[]>?` names `A.B`, `C.D` and `E`.
fn names(typed: &str) -> Vec<String> {
    typed
        .split(|c: char| matches!(c, '<' | '>' | ',' | '(' | ')' | '[' | ']' | '?' | '*') || c.is_whitespace())
        .map(|s| s.trim_start_matches("global::").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoints_routes_name_bring_their_classes_and_one_both_taken_and_returned_is_a_response() {
        let roles = |pairs: &[(&str, Role)]| {
            let mut m = BTreeMap::new();
            pairs.iter().for_each(|(c, r)| widen(&mut m, c, *r));
            m
        };
        assert_eq!(roles(&[("Demo.A", Role::Request), ("Demo.A", Role::Response), ("Demo.B", Role::Response), ("Demo.B", Role::Request)])["Demo.A"], Role::Response);
        assert_eq!(roles(&[("Demo.B", Role::Response), ("Demo.B", Role::Request)])["Demo.B"], Role::Response);
        let uses = Uses {
            endpoints: vec![
                ("POST /api/items".into(), roles(&[("Demo.A", Role::Request)])),
                ("GET /api/items/{id}".into(), roles(&[("Demo.A", Role::Response), ("Demo.B", Role::Response)])),
                ("GET /api/other".into(), roles(&[("Demo.C", Role::Response)])),
            ],
            calls: vec![("POST /demo/clients/redacted".into(), roles(&[("Demo.RespA", Role::OutboundResponse), ("Demo.RespB", Role::OutboundResponse)]))],
        };
        let bound = uses.of(&["POST /api/items".into(), "/api/items/7".into()]);
        assert_eq!(bound.endpoints, ["POST /api/items", "GET /api/items/{id}"]);
        assert_eq!(bound.classes, roles(&[("Demo.A", Role::Response), ("Demo.B", Role::Response)]));
        assert_eq!(bound.used_by["Demo.B"], ["GET /api/items/{id}"]);
        // A ROUTE THE TREE CALLS binds by the end of the documented path: the document's `/api` is the caller's base url.
        let called = uses.of(&["POST /api/demo/clients/redacted".into()]);
        assert_eq!(called.endpoints, ["POST /demo/clients/redacted"]);
        assert_eq!(called.classes, roles(&[("Demo.RespA", Role::OutboundResponse), ("Demo.RespB", Role::OutboundResponse)]));
        assert_eq!(uses.users()["Demo.RespB"], ["POST /demo/clients/redacted (outbound response)"]);
    }

    #[test]
    fn a_called_url_is_its_path_and_matches_a_documented_route_by_its_end() {
        assert_eq!(call_route("POST", "https://x.example/api/demo/clients/redacted").as_deref(), Some("POST /api/demo/clients/redacted"));
        assert_eq!(call_route("GET", "{}/{}/demo/items/{}").as_deref(), Some("GET /demo/items/{}"));
        assert_eq!((call_route("GET", "{}/*/items"), call_route("GET", "https://x.example"), call_route("GET", "{}")), (None, None, None));
        assert!(same_call("POST /demo/clients/redacted", "POST /api/demo/clients/redacted"));
        assert!(same_call("GET /demo/items/{}", "/api/demo/items/7"));
        assert!(!same_call("POST /demo/clients/redacted", "GET /api/demo/clients/redacted"));
        assert!(!same_call("POST /demo/clients/redacted", "POST /api/other/clients/redacted"));
        assert!(!same_call("GET /items", "GET /api/items"));
        assert!(!same_call("GET /{}/{}", "GET /api/x/y"));
    }

    #[test]
    fn a_resolved_type_names_each_type_it_is_written_with() {
        assert_eq!(names("System.Threading.Tasks.Task<Microsoft.AspNetCore.Mvc.ActionResult<Demo.Response<Demo.ItemDto>>>"), [
            "System.Threading.Tasks.Task",
            "Microsoft.AspNetCore.Mvc.ActionResult",
            "Demo.Response",
            "Demo.ItemDto"
        ]);
        assert_eq!(names("Demo.Pair<Demo.A, Demo.B[]>?"), ["Demo.Pair", "Demo.A", "Demo.B"]);
    }

    #[test]
    fn a_class_reaches_the_classes_its_properties_name_and_a_cycle_ends() {
        let classes = Classes {
            symbol: HashMap::from([
                ("Demo.Response".into(), vec!["Demo.Response<T>".into()]),
                ("Demo.ItemDto".into(), vec!["Demo.ItemDto".into()]),
                ("Demo.Part".into(), vec!["Demo.Part".into()]),
                ("Demo.Other".into(), vec!["Demo.Other".into()]),
            ]),
            property_types: HashMap::from([
                ("Demo.Response<T>".into(), vec!["T".into(), "int".into()]),
                ("Demo.ItemDto".into(), vec!["System.Collections.Generic.List<Demo.Part>".into()]),
                ("Demo.Part".into(), vec!["Demo.ItemDto".into()]),
            ]),
        };
        let reached: Vec<String> = classes.reached("System.Threading.Tasks.Task<Demo.Response<Demo.ItemDto>>").into_iter().collect();
        assert_eq!(reached, ["Demo.ItemDto", "Demo.Part", "Demo.Response<T>"]);
    }
}
