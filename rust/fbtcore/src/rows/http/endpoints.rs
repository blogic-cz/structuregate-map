//! THE ACTIONS A C# TREE ROUTES BY ATTRIBUTE: `[Route]` on a class and `[Http*]`/`[Route]` on a method, read from the
//! `decorators` rows (their folded argument values, `arg_values`) and the `classes` rows (a class's bases).

use super::matching::endpoint_route;
use crate::rows::dups::{has_columns, text};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};

pub(super) struct Endpoint {
    pub file: String,
    pub cls: String,
    pub method: String,
    pub line: i64,
    pub verb: String,
    pub template: String,
    pub route: String,
}

const VERBS: [(&str, &str); 7] =
    [("HttpGet", "GET"), ("HttpPost", "POST"), ("HttpPut", "PUT"), ("HttpDelete", "DELETE"), ("HttpPatch", "PATCH"), ("HttpHead", "HEAD"), ("HttpOptions", "OPTIONS")];

/// How far up a class's bases its route is looked for.
const LEVELS: usize = 5;

/// An attribute as the routing names it: `Route`, `HttpGet`, with or without `Attribute`, written dotted or not.
fn attribute(name: &str) -> &str {
    let last = name.rsplit('.').next().unwrap_or(name);
    last.strip_suffix("Attribute").unwrap_or(last)
}

/// The route an attribute is given: its first argument, when that is positional (`Name=text` is a named one).
fn template(values: &str) -> Option<String> {
    let list: Vec<String> = serde_json::from_str(values).ok()?;
    let first = list.into_iter().next()?;
    let named = first.split_once('=').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_'));
    (!named).then_some(first)
}

struct Decorator {
    file: String,
    path: String,
    line: i64,
    cls: String,
    name: String,
    target: String,
    kind: String,
    template: Option<String>,
}

#[derive(Default)]
struct Method {
    line: i64,
    verbs: Vec<(&'static str, Option<String>)>,
    route: Option<String>,
}

struct Class {
    bases: Vec<String>,
}

pub(super) fn load(conn: &Connection, csharp: &HashMap<String, String>) -> rusqlite::Result<Vec<Endpoint>> {
    if !has_columns(conn, "decorators", &["file", "line", "cls", "name", "target", "target_kind", "arg_values"]) {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare("SELECT file, line, cls, name, target, target_kind, arg_values FROM decorators ORDER BY file, line")?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?)))?;
    let mut decorators = vec![];
    for row in rows {
        let (file, line, cls, name, target, kind, values) = row?;
        let Some(path) = csharp.get(&file) else { continue };
        decorators.push(Decorator { path: path.clone(), file, line: line.parse().unwrap_or(0), cls, name, target, kind, template: template(&values) });
    }
    let (classes, by_symbol) = classes(conn, csharp)?;
    // A CLASS'S OWN ROUTE: its attribute is on the class row's target, and its `cls` is the class AROUND it.
    let mut routes: HashMap<(String, String), String> = HashMap::new();
    let mut methods: BTreeMap<(String, String, String), Method> = BTreeMap::new();
    for d in &decorators {
        let name = attribute(&d.name);
        match d.kind.as_str() {
            "class" | "struct" | "record" | "interface" if name == "Route" => {
                routes.insert((d.file.clone(), d.target.clone()), d.template.clone().unwrap_or_default());
            }
            "method" => {
                let slot = methods.entry((d.file.clone(), d.cls.clone(), d.target.clone())).or_default();
                if slot.line == 0 {
                    slot.line = d.line;
                }
                if name == "Route" {
                    slot.route = Some(d.template.clone().unwrap_or_default());
                } else if let Some((_, verb)) = VERBS.iter().find(|(attr, _)| *attr == name) {
                    slot.verbs.push((verb, d.template.clone()));
                }
            }
            _ => {}
        }
    }
    let paths: HashMap<&String, &String> = decorators.iter().map(|d| (&d.file, &d.path)).collect();
    let mut out = vec![];
    for ((file, cls, method), m) in &methods {
        let verbs: Vec<(&str, Option<String>)> = match (m.verbs.is_empty(), &m.route) {
            (true, Some(_)) => vec![("ANY", None)],
            (true, None) => continue,
            _ => m.verbs.clone(),
        };
        let class_route = class_route(&(file.clone(), cls.clone()), &routes, &classes, &by_symbol, 0).unwrap_or_default();
        for (verb, own) in verbs {
            let written = own.filter(|t| !t.is_empty()).or_else(|| m.route.clone()).unwrap_or_default();
            let template = combine(&class_route, &written, cls, method);
            out.push(Endpoint {
                file: paths.get(file).map(|p| p.to_string()).unwrap_or_default(),
                cls: cls.clone(),
                method: method.clone(),
                line: m.line,
                verb: verb.to_string(),
                route: endpoint_route(&template),
                template,
            });
        }
    }
    out.sort_by(|a, b| (&a.file, a.line, &a.verb).cmp(&(&b.file, b.line, &b.verb)));
    Ok(out)
}

type Classes = HashMap<(String, String), Class>;

/// The classes by (file, name), and where a resolved symbol lives.
fn classes(conn: &Connection, csharp: &HashMap<String, String>) -> rusqlite::Result<(Classes, HashMap<String, (String, String)>)> {
    let (mut held, mut by_symbol) = (HashMap::new(), HashMap::new());
    if !has_columns(conn, "classes", &["file", "name", "symbol", "bases"]) {
        return Ok((held, by_symbol));
    }
    let mut stmt = conn.prepare("SELECT file, name, symbol, bases FROM classes")?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?)))?;
    for row in rows {
        let (file, name, symbol, bases) = row?;
        if !csharp.contains_key(&file) {
            continue;
        }
        let bases: Vec<String> = serde_json::from_str(&bases).unwrap_or_default();
        if !symbol.is_empty() {
            by_symbol.entry(symbol.clone()).or_insert((file.clone(), name.clone()));
        }
        held.insert((file, name), Class { bases });
    }
    Ok((held, by_symbol))
}

/// A class's own `[Route]`, or the nearest base class's.
fn class_route(key: &(String, String), routes: &HashMap<(String, String), String>, classes: &Classes, by_symbol: &HashMap<String, (String, String)>, depth: usize) -> Option<String> {
    if let Some(own) = routes.get(key) {
        return Some(own.clone());
    }
    if depth >= LEVELS {
        return None;
    }
    let class = classes.get(key)?;
    class.bases.iter().filter_map(|b| by_symbol.get(b)).find_map(|base| class_route(base, routes, classes, by_symbol, depth + 1))
}

/// The class route and the method's, with the tokens a route may carry replaced.
fn combine(class: &str, method: &str, cls: &str, name: &str) -> String {
    let absolute = method.strip_prefix("~/").or_else(|| method.strip_prefix('/'));
    let joined = match absolute {
        Some(rest) => rest.trim_matches('/').to_string(),
        None => [class, method].iter().map(|p| p.trim_matches('/')).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("/"),
    };
    joined
        .replace("[controller]", cls.strip_suffix("Controller").unwrap_or(cls))
        .replace("[action]", name.strip_suffix("Async").unwrap_or(name))
        .replace("[area]", "*")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_combine_and_tokens_are_replaced() {
        assert_eq!(combine("api/calculations", "save", "C", "Save"), "api/calculations/save");
        assert_eq!(combine("api/", "/x/", "C", "Save"), "x");
        assert_eq!(combine("api/[controller]", "[action]", "OrdersController", "ListAsync"), "api/Orders/List");
        assert_eq!(combine("[area]/x", "", "C", "M"), "*/x");
        assert_eq!(combine("api", "~/abs/y", "C", "M"), "abs/y");
    }

    #[test]
    fn a_named_argument_is_no_template() {
        assert_eq!(template(r#"["api/x"]"#).as_deref(), Some("api/x"));
        assert_eq!(template(r#"["Name=x"]"#), None);
        assert_eq!(template(r#"["{id=5}"]"#).as_deref(), Some("{id=5}"));
        assert_eq!(template("[]"), None);
        assert_eq!(attribute("Http.HttpGetAttribute"), "HttpGet");
    }
}
