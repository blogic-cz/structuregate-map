//! A CLASS BINDING JOINED TO THE RULES THAT HIDE ITS CLASS (`class_hides`).
//!
//! `gates` records `[class.not-visible]="!shown"` as a gate of kind `class`, and a gate's NAME is all it says:
//! whether adding `not-visible` takes the element out of view is written in a stylesheet. This pass joins the
//! two - a class gate, or a key of an `[ngClass]` literal, to every LIVE `style_rules` row whose SUBJECT
//! requires that class on the element itself and whose declaration hides it.
//!
//! A HIDING RULE CAN ASK FOR MORE THAN THE CLASS - another class, an ancestor, `:hover` - and `bare` says when
//! it does not: the whole resolved selector is `.<class>`.
//!
//! WHICH STYLESHEET IT CAME FROM DECIDES WHETHER IT APPLIES, and that is a column, not a filter. Angular
//! encapsulates a component's own styles (`styleUrls`) to its own template, so `scope` says: `own` - the
//! component's own stylesheet; `other` - another component's, which reaches this element only through
//! `::ng-deep` or `ViewEncapsulation.None`; `shared` - a stylesheet no component names, which is the global
//! `styles` of `angular.json` or a partial `@use`d into a component's own (this pass follows no `@use`).

use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

/// Rows of a query whose cells are all read as text; a table the map does not have reads as no rows.
fn text_rows(db: &Connection, sql: &str, half: &str, width: usize) -> Vec<Vec<Option<String>>> {
    let Ok(mut stmt) = db.prepare(sql) else { return Vec::new() };
    let read = stmt.query_map([half], |row| (0..width).map(|i| row.get::<_, Option<String>>(i)).collect());
    match read {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// `dir/./a/../b.scss` as the one path it names, `/`-separated like every `files.path` of this half.
fn join_path(dir: &str, url: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in url.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// `{component id -> the file ids of its own stylesheets}`, from `components.style_urls`, resolved against
/// the folder of the component's own `.ts` as Angular resolves them.
fn own_sheets(db: &Connection, half: &str) -> HashMap<String, BTreeSet<String>> {
    let path_of: HashMap<String, String> = text_rows(db, "SELECT id, path FROM files WHERE half = ?1", half, 2)
        .into_iter()
        .filter_map(|r| Some((r[0].clone()?, r[1].clone()?)))
        .collect();
    let id_of: HashMap<&str, &str> = path_of.iter().map(|(id, path)| (path.as_str(), id.as_str())).collect();
    let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
    for row in text_rows(db, "SELECT id, file, style_urls FROM components WHERE half = ?1", half, 3) {
        let (Some(component), Some(file), Some(urls)) = (&row[0], &row[1], &row[2]) else { continue };
        let Some(path) = path_of.get(file) else { continue };
        let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let urls: Vec<String> = serde_json::from_str(urls).unwrap_or_default();
        let sheets = out.entry(component.clone()).or_default();
        for url in urls {
            if let Some(id) = id_of.get(join_path(dir, &url).as_str()) {
                sheets.insert(id.to_string());
            }
        }
    }
    out
}

/// The classes an `[ngClass]` value puts on the element when they are LITERAL: the keys of an object, a
/// string, or an array of strings. A key holds several classes when it holds spaces, as Angular reads it.
fn ng_classes(ast: &Value, out: &mut Vec<String>) {
    let split = |text: &str, out: &mut Vec<String>| out.extend(text.split_whitespace().map(String::from));
    match ast.get("k").and_then(Value::as_str) {
        Some("Source") => {
            if let Some(inner) = ast.get("ast") {
                ng_classes(inner, out);
            }
        }
        Some("Map") => {
            for key in ast.get("keys").and_then(Value::as_array).into_iter().flatten() {
                if let Some(text) = key.get("key").and_then(Value::as_str) {
                    split(text, out);
                }
            }
        }
        Some("Literal") => {
            if let Some(text) = ast.get("v").and_then(Value::as_str) {
                split(text, out);
            }
        }
        Some("Array") => {
            for item in ast.get("items").and_then(Value::as_array).into_iter().flatten() {
                if item.get("k").and_then(Value::as_str) == Some("Literal") {
                    ng_classes(item, out);
                }
            }
        }
        _ => {}
    }
}

/// One binding that can put a class on an element.
struct Binding {
    gate: Option<String>,
    expression: Option<String>,
    via: &'static str,
    template: Option<String>,
    node: Option<String>,
    component: Option<String>,
    classes: Vec<String>,
    condition: Option<String>,
}

fn bindings(db: &Connection, half: &str) -> Vec<Binding> {
    let mut out = Vec::new();
    let gates = "SELECT id, expression, template, node, component, name, source FROM gates \
                 WHERE half = ?1 AND gate_kind = 'class' ORDER BY rowid";
    for r in text_rows(db, gates, half, 7) {
        let Some(name) = r[5].clone() else { continue };
        out.push(Binding {
            gate: r[0].clone(), expression: r[1].clone(), via: "class", template: r[2].clone(), node: r[3].clone(),
            component: r[4].clone(), classes: vec![name], condition: r[6].clone(),
        });
    }
    let ng = "SELECT id, template, node, component, ast, source FROM expressions \
              WHERE half = ?1 AND name = 'ngClass' ORDER BY rowid";
    for r in text_rows(db, ng, half, 6) {
        let mut classes = Vec::new();
        if let Some(ast) = r[4].as_deref().and_then(|t| serde_json::from_str::<Value>(t).ok()) {
            ng_classes(&ast, &mut classes);
        }
        if classes.is_empty() {
            continue;
        }
        out.push(Binding {
            gate: None, expression: r[0].clone(), via: "ngClass", template: r[1].clone(), node: r[2].clone(),
            component: r[3].clone(), classes, condition: r[5].clone(),
        });
    }
    out
}

/// The `class_hides` rows, over the `style_rules` rows this run derived.
pub fn join(db: &Connection, half: &str, rules: &[Value]) -> Vec<Value> {
    let mut hiding: HashMap<&str, Vec<&Map<String, Value>>> = HashMap::new();
    for rule in rules.iter().filter_map(Value::as_object) {
        let on = |key: &str| rule.get(key).and_then(Value::as_i64) == Some(1);
        if !(on("hides") && on("live") && on("on_element")) {
            continue;
        }
        for class in rule.get("classes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            hiding.entry(class).or_default().push(rule);
        }
    }
    if hiding.is_empty() {
        return Vec::new();
    }
    let own = own_sheets(db, half);
    let any_own: BTreeSet<&String> = own.values().flatten().collect();
    let mut out = Vec::new();
    for binding in bindings(db, half) {
        let mine = binding.component.as_ref().and_then(|c| own.get(c));
        for class in &binding.classes {
            for rule in hiding.get(class.as_str()).into_iter().flatten() {
                let file = rule.get("file").and_then(Value::as_str).unwrap_or("").to_string();
                let scope = if mine.is_some_and(|m| m.contains(&file)) {
                    "own"
                } else if any_own.contains(&file) {
                    "other"
                } else {
                    "shared"
                };
                let mut row = Map::new();
                row.insert("id".into(), Value::String(format!("cssh:{}", out.len() + 1)));
                let text = |v: &Option<String>| v.clone().map(Value::String).unwrap_or(Value::Null);
                row.insert("gate".into(), text(&binding.gate));
                row.insert("expression".into(), text(&binding.expression));
                row.insert("via".into(), Value::String(binding.via.into()));
                row.insert("template".into(), text(&binding.template));
                row.insert("node".into(), text(&binding.node));
                row.insert("component".into(), text(&binding.component));
                row.insert("class".into(), Value::String(class.clone()));
                row.insert("condition".into(), text(&binding.condition));
                row.insert("rule".into(), rule.get("id").cloned().unwrap_or(Value::Null));
                row.insert("file".into(), Value::String(file));
                row.insert("scope".into(), Value::String(scope.into()));
                for key in ["resolved", "subject", "media", "context", "property", "value", "important"] {
                    row.insert(key.into(), rule.get(key).cloned().unwrap_or(Value::Null));
                }
                // THE CLASS ALONE, or the class AND MORE: `.bold.gone` hides an element only when it also has
                // `gone`, and `.panel .item-hidden` only inside a `.panel`. Exact text, so an escaped class name
                // reads as not bare rather than as bare by a guess.
                let bare = rule.get("resolved").and_then(Value::as_str) == Some(format!(".{class}").as_str());
                row.insert("bare".into(), Value::from(i64::from(bare)));
                out.push(Value::Object(row));
            }
        }
    }
    out
}
