//! THE TYPES AN ENDPOINT USES, by resolved symbol: a field table under a section that names an endpoint lists the
//! properties of a type that endpoint takes or returns - not of every class whose property names happen to agree.
//! An endpoint's action (`http_endpoints.func`) is a `functions` row: its RETURN type (`functions.type`, as Roslyn
//! resolved it), each PARAMETER's type (`parameters.type`) and each `[ProducesResponseType(typeof(X))]` (the folded
//! `typeof`, `decorators.arg_values`). Every name in such a type is read - `Task<ActionResult<Response<ItemDto>>>`
//! names `Response` and `ItemDto` - and kept when it is a class the map holds properties of; a kept class's own
//! property types are walked the same way (a visited set ends the walk), so a nested object is the endpoint's too.

use super::apis::same_endpoint;
use crate::rows::dups::{has_columns, items_sql, owner_sql, text};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What a class is to an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Request,
    Response,
}

impl Role {
    pub fn word(self) -> &'static str {
        match self {
            Role::Request => "request",
            Role::Response => "response",
        }
    }
}

/// Every endpoint (`VERB /template`) with the classes it uses, by `members.class_symbol`.
#[derive(Default)]
pub struct Uses {
    endpoints: Vec<(String, BTreeMap<String, Role>)>,
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
    /// The endpoints the routes name (each route against each endpoint, as a link compares them) and their classes;
    /// a class both takes and returns is a response.
    pub fn of(&self, routes: &[String]) -> Bound {
        let mut out = Bound::default();
        for (endpoint, classes) in &self.endpoints {
            if routes.iter().any(|r| same_endpoint(endpoint, r)) {
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

/// Every endpoint's classes, from the rows the C# half stored; nothing when the map has no endpoint.
pub fn load(conn: &Connection) -> rusqlite::Result<Uses> {
    if !has_columns(conn, "http_endpoints", &["verb", "template", "func"])
        || !has_columns(conn, "functions", &["id", "file", "name", "qualname", "line", "end_line", "type"])
        || !has_columns(conn, "members", &["kind", "class_symbol", "type"])
    {
        return Ok(Uses::default());
    }
    let classes = Classes::read(conn)?;
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
    let mut stmt = conn.prepare("SELECT verb, template, func FROM http_endpoints ORDER BY id")?;
    let mut endpoints = Vec::new();
    for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?)))? {
        let (verb, template, func) = row?;
        let mut used: BTreeMap<String, Role> = BTreeMap::new();
        for (typed, role) in by_func.get(&func).into_iter().flatten() {
            for class in classes.reached(typed) {
                widen(&mut used, &class, *role);
            }
        }
        endpoints.push((format!("{verb} /{}", template.trim_start_matches(['~', '/'])), used));
    }
    Ok(Uses { endpoints })
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
        };
        let bound = uses.of(&["POST /api/items".into(), "/api/items/7".into()]);
        assert_eq!(bound.endpoints, ["POST /api/items", "GET /api/items/{id}"]);
        assert_eq!(bound.classes, roles(&[("Demo.A", Role::Response), ("Demo.B", Role::Response)]));
        assert_eq!(bound.used_by["Demo.B"], ["GET /api/items/{id}"]);
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
