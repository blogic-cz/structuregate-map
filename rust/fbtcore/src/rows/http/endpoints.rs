//! THE ACTIONS A C# TREE ROUTES BY ATTRIBUTE: `[Route]` on a class and `[Http*]`/`[Route]` on a method, read from the
//! `decorators` rows (their folded argument values, `arg_values`) and the `classes` rows (a class's bases).

use super::matching::endpoint_route;
use crate::rows::dups::{has_columns, owner_sql, scalar_sql, text, Column};
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

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

/// An attribute as the routing names it: `Route`, `HttpGet`, with or without `Attribute`, written dotted or not.
fn attribute(name: &str) -> &str {
    let last = name.rsplit('.').next().unwrap_or(name);
    last.strip_suffix("Attribute").unwrap_or(last)
}

/// The route an attribute is given: its first argument, when that is positional (`Name=text` is a named one).
fn template(values: &Value) -> Option<String> {
    let list = values.as_array().filter(|l| l.iter().all(Value::is_string))?;
    let first = list.first()?.as_str()?.to_string();
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
    // `arg_values` is a list: its nodes.
    let mut values = Column::new(conn, "decorators", "arg_values")?;
    let sql = format!(
        "SELECT d.file, d.line, d.cls, d.name, d.target, d.target_kind, {}, {} FROM decorators d ORDER BY d.file, d.line",
        owner_sql(conn, "decorators", "d"),
        scalar_sql(conn, "decorators", "d", "arg_values")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?, r.get::<_, SqlValue>(7)?)))?;
    let mut decorators = vec![];
    for row in rows {
        let (file, line, cls, name, target, kind, owner, held) = row?;
        let Some(path) = csharp.get(&file) else { continue };
        let template = template(&values.value(&owner, &held)?);
        decorators.push(Decorator { path: path.clone(), file, line: line.parse().unwrap_or(0), cls, name, target, kind, template });
    }
    let (classes, by_symbol) = classes(conn, csharp)?;
    // A CLASS'S OWN ROUTE: its attribute is on the class row's target, and its `cls` is the class AROUND it.
    // EVERY `[Route]` ON A CLASS IS A PREFIX OF ITS OWN (#59): `[Route("api/v1/x")] [Route("api/v2/x")]` serves both.
    let mut routes: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut methods: BTreeMap<(String, String, String), Method> = BTreeMap::new();
    for d in &decorators {
        let name = attribute(&d.name);
        match d.kind.as_str() {
            "class" | "struct" | "record" | "interface" if name == "Route" => {
                routes.entry((d.file.clone(), d.target.clone())).or_default().push(d.template.clone().unwrap_or_default());
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
        let class_routes = class_route(&(file.clone(), cls.clone()), &routes, &classes, &by_symbol, &mut HashSet::new()).unwrap_or_else(|| vec![String::new()]);
        for (verb, own) in verbs {
            let written = own.filter(|t| !t.is_empty()).or_else(|| m.route.clone()).unwrap_or_default();
            let mut made: Vec<String> = Vec::new();
            for class_route in &class_routes {
                let template = combine(class_route, &written, cls, method);
                // an absolute method template ignores every class route: one endpoint, not one per prefix
                if made.contains(&template) {
                    continue;
                }
                made.push(template.clone());
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
    }
    out.sort_by(|a, b| (&a.file, a.line, &a.verb, &a.template).cmp(&(&b.file, b.line, &b.verb, &b.template)));
    Ok(out)
}

type Classes = HashMap<(String, String), Class>;

/// The classes by (file, name), and where a resolved symbol lives.
fn classes(conn: &Connection, csharp: &HashMap<String, String>) -> rusqlite::Result<(Classes, HashMap<String, (String, String)>)> {
    let (mut held, mut by_symbol) = (HashMap::new(), HashMap::new());
    if !has_columns(conn, "classes", &["file", "name", "symbol", "bases"]) {
        return Ok((held, by_symbol));
    }
    let mut list = Column::new(conn, "classes", "bases")?;
    let sql = format!("SELECT k.file, k.name, k.symbol, {}, {} FROM classes k", owner_sql(conn, "classes", "k"), scalar_sql(conn, "classes", "k", "bases"));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, r.get::<_, SqlValue>(4)?)))?;
    for row in rows {
        let (file, name, symbol, owner, cell) = row?;
        if !csharp.contains_key(&file) {
            continue;
        }
        let bases = list.strings(&owner, &cell)?;
        if !symbol.is_empty() {
            by_symbol.entry(symbol.clone()).or_insert((file.clone(), name.clone()));
        }
        held.insert((file, name), Class { bases });
    }
    Ok((held, by_symbol))
}

/// A class's own `[Route]`s, or the nearest base class's - as far up as the bases go. `seen` is every class already
/// asked, so bases that name each other (a broken tree) end the walk instead of looping; no depth cap.
fn class_route(
    key: &(String, String),
    routes: &HashMap<(String, String), Vec<String>>,
    classes: &Classes,
    by_symbol: &HashMap<String, (String, String)>,
    seen: &mut HashSet<(String, String)>,
) -> Option<Vec<String>> {
    if let Some(own) = routes.get(key) {
        return Some(own.clone());
    }
    if !seen.insert(key.clone()) {
        return None;
    }
    let class = classes.get(key)?;
    class.bases.iter().filter_map(|b| by_symbol.get(b)).find_map(|base| class_route(base, routes, classes, by_symbol, seen))
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
    use serde_json::json;

    fn chain(n: usize, cycle: bool) -> (HashMap<(String, String), Vec<String>>, Classes, HashMap<String, (String, String)>) {
        // C0 derives from C1, C1 from C2 .. and the last has the route - or, `cycle`, the last derives from C0 again.
        let key = |i: usize| ("f:1".to_string(), format!("C{i}"));
        let (mut routes, mut classes, mut by_symbol) = (HashMap::new(), HashMap::new(), HashMap::new());
        for i in 0..n {
            by_symbol.insert(format!("Demo.C{i}"), key(i));
            let next = if i + 1 < n { vec![format!("Demo.C{}", i + 1)] } else if cycle { vec!["Demo.C0".to_string()] } else { vec![] };
            classes.insert(key(i), Class { bases: next });
        }
        if !cycle {
            routes.insert(key(n - 1), vec!["api/base".to_string()]);
        }
        (routes, classes, by_symbol)
    }

    #[test]
    fn a_route_is_found_however_far_up_and_bases_in_a_cycle_end() {
        let (routes, classes, by_symbol) = chain(9, false);
        let start = ("f:1".to_string(), "C0".to_string());
        assert_eq!(class_route(&start, &routes, &classes, &by_symbol, &mut HashSet::new()), Some(vec!["api/base".to_string()]));
        let (routes, classes, by_symbol) = chain(4, true);
        assert_eq!(class_route(&start, &routes, &classes, &by_symbol, &mut HashSet::new()), None);
    }

    #[test]
    fn an_endpoint_is_read_from_the_decorators_and_bases_nodes() {
        let conn = Connection::open_in_memory().unwrap();
        let rows = |table: &str, rows: Vec<Value>| crate::rows::store::insert_rows(&conn, table, &rows).unwrap();
        rows("classes", vec![
            json!({"id": "c:1", "file": "f:1", "name": "OrdersController", "symbol": "Demo.OrdersController", "bases": ["Demo.ApiBase"]}),
            json!({"id": "c:2", "file": "f:1", "name": "ApiBase", "symbol": "Demo.ApiBase", "bases": []}),
        ]);
        rows("decorators", vec![
            json!({"id": "d:1", "file": "f:1", "line": 2, "cls": "", "name": "Route", "target": "ApiBase", "target_kind": "class", "arg_values": ["api/[controller]"]}),
            json!({"id": "d:2", "file": "f:1", "line": 9, "cls": "OrdersController", "name": "HttpGet", "target": "List", "target_kind": "method", "arg_values": ["list"]}),
        ]);
        let csharp = HashMap::from([("f:1".to_string(), "Orders.cs".to_string())]);
        let found = load(&conn, &csharp).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].verb.as_str(), found[0].template.as_str()), ("GET", "api/Orders/list"));
    }

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
        assert_eq!(template(&json!(["api/x"])).as_deref(), Some("api/x"));
        assert_eq!(template(&json!(["Name=x"])), None);
        assert_eq!(template(&json!(["{id=5}"])).as_deref(), Some("{id=5}"));
        assert_eq!(template(&json!([])), None);
        assert_eq!(attribute("Http.HttpGetAttribute"), "HttpGet");
    }
}
