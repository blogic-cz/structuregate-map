//! A STYLE BINDING JOINED TO WHAT ITS VALUE DOES (`style_hides`).
//!
//! `[style.display]="shown ? 'block' : 'none'"` hides its element by its VALUE, not by a stylesheet: `class_hides`
//! answers class bindings only, and `style_rules.hides` is one declaration of a rule, never a binding. One row per
//! `style` gate says whether the property can hide an element on its own (`alone`: `display`, `visibility`) and, when
//! the value is a literal or a ternary of two literals, WHICH SIDE hides (`when`): `always`, `never`, `then`, `else`.
//! A value that is not decided from its text (a member read, a call, a nested ternary, a pipe) has `when` NULL: it is
//! not a claim either way. `height`, `opacity` and the rest never hide alone - a collapse needs partners.
//!
//! `unit` is Angular's (`[style.height.px]` is `height` in `px`), carried by the gate row (rows 62). A LITERAL KEY of an
//! `[ngStyle]` object is a row of its own (`via = 'ngStyle'`, `gate` NULL, `expression` the binding's row), its value
//! read as a `[style.x]` value is (#54); nothing restricts through one - the closure asks gates, and it is none.

use crate::rows::dups::Column;
use rusqlite::Connection;
use rusqlite::types::Value as SqlValue;
use serde_json::{Map, Value};

/// Rows of a query whose cells are all read as text; a table the map does not have reads as no rows.
fn text_rows(db: &Connection, sql: &str, half: &str, width: usize) -> Vec<Vec<Option<String>>> {
    let Ok(mut stmt) = db.prepare(sql) else { return Vec::new() };
    let read = stmt.query_map([half], |row| (0..width).map(|i| row.get::<_, Option<String>>(i)).collect());
    match read {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// The expression under its `Source` wrapper, as `ng_classes` unwraps it.
fn unwrap(ast: &Value) -> &Value {
    match (ast.get("k").and_then(Value::as_str), ast.get("ast")) {
        (Some("Source"), Some(inner)) => unwrap(inner),
        _ => ast,
    }
}

/// Whether the node is a literal that hides under `property`: `none` for display, `hidden`/`collapse` for visibility.
/// Anything else - another literal, JSON null (Angular removes the style) - does not. `None` when it is no literal.
fn literal_hides(node: &Value, property: &str) -> Option<bool> {
    if node.get("k").and_then(Value::as_str) != Some("Literal") {
        return None;
    }
    let text = node.get("v").and_then(Value::as_str).map(|s| s.trim().to_ascii_lowercase());
    Some(match (property, text.as_deref()) {
        ("display", Some("none")) => true,
        ("visibility", Some("hidden" | "collapse")) => true,
        _ => false,
    })
}

/// `always`, `never`, `then`, `else`, or `None` when the value is not decided from its text.
fn when(ast: &Value, property: &str) -> Option<&'static str> {
    let node = unwrap(ast);
    if let Some(hides) = literal_hides(node, property) {
        return Some(if hides { "always" } else { "never" });
    }
    if node.get("k").and_then(Value::as_str) != Some("Cond") {
        return None;
    }
    let then = literal_hides(node.get("then")?, property)?;
    let other = literal_hides(node.get("else")?, property)?;
    Some(match (then, other) {
        (true, true) => "always",
        (true, false) => "then",
        (false, true) => "else",
        (false, false) => "never",
    })
}

/// Whether the property can hide an element on its own.
fn alone(property: &str) -> bool {
    matches!(property, "display" | "visibility")
}

/// The property and unit a style name spells: `height.px` is `height` in `px`, as Angular splits it.
fn split_unit(name: &str) -> (&str, Option<String>) {
    match name.split_once('.') {
        Some((p, u)) => (p, Some(u.to_string())),
        None => (name, None),
    }
}

/// What one binding of one property says, before it is a row: `place` is (template, node, component, condition).
struct Styled<'a> {
    gate: Option<String>,
    expression: Option<String>,
    via: &'static str,
    place: &'a [Option<String>],
    property: &'a str,
    unit: Option<String>,
    value: Option<&'a Value>,
}

fn row_of(n: usize, s: Styled<'_>) -> Value {
    let text = |v: Option<String>| v.map(Value::String).unwrap_or(Value::Null);
    let alone = alone(s.property);
    let decided = if alone { s.value.and_then(|v| when(v, s.property)) } else { None };
    let mut row = Map::new();
    row.insert("id".into(), Value::String(format!("cssy:{n}")));
    row.insert("gate".into(), text(s.gate));
    row.insert("expression".into(), text(s.expression));
    row.insert("via".into(), Value::String(s.via.into()));
    row.insert("template".into(), text(s.place[0].clone()));
    row.insert("node".into(), text(s.place[1].clone()));
    row.insert("component".into(), text(s.place[2].clone()));
    row.insert("property".into(), Value::String(s.property.into()));
    row.insert("unit".into(), text(s.unit));
    row.insert("condition".into(), text(s.place[3].clone()));
    row.insert("alone".into(), Value::from(i64::from(alone)));
    row.insert("hides".into(), Value::from(i64::from(matches!(decided, Some("always" | "then" | "else")))));
    row.insert("when".into(), decided.map(|w| Value::String(w.into())).unwrap_or(Value::Null));
    Value::Object(row)
}

/// AN `[ngStyle]` LITERAL MAP, one `(key, value)` per key written as a name or a string: `{ 'height.px': h }` is the
/// key `height.px`. Anything but an object literal (a member read, a call) has no keys to give, nor has a spread.
fn ng_style(ast: &Value) -> Vec<(&str, &Value)> {
    let node = unwrap(ast);
    if node.get("k").and_then(Value::as_str) != Some("Map") {
        return Vec::new();
    }
    let keys = node.get("keys").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let values = node.get("values").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    keys.iter().zip(values).filter_map(|(k, v)| Some((k.get("key").and_then(Value::as_str)?, v))).collect()
}

/// The `style_hides` rows: one per `style` gate of the half, in the gates' own order, then one per literal key of
/// each `[ngStyle]` object (`via = 'ngStyle'`, no gate - as `class_hides` files an `[ngClass]` key).
pub fn rows(db: &Connection, half: &str) -> Vec<Value> {
    // `unit` is on a style gate since rows 62; a map stored before has no such column, and the name's split stands in.
    let unit = if db.prepare("SELECT unit FROM gates LIMIT 0").is_ok() { "unit" } else { "NULL" };
    let sql = format!(
        "SELECT template, node, component, source, id, expression, name, {unit} FROM gates \
         WHERE half = ?1 AND gate_kind = 'style' ORDER BY rowid"
    );
    let gates = text_rows(db, &sql, half, 8);
    let ng = "SELECT template, node, component, source, id FROM expressions \
              WHERE half = ?1 AND name = 'ngStyle' ORDER BY rowid";
    let styles = text_rows(db, ng, half, 5);
    if gates.is_empty() && styles.is_empty() {
        return Vec::new();
    }
    // `ast` is an object: its nodes, rebuilt for the few expressions a style binding holds.
    let mut tree = Column::new(db, "expressions", "ast").ok();
    let mut ast_of = |id: Option<&str>| match (tree.as_mut(), id) {
        (Some(tree), Some(id)) => tree.value(id, &SqlValue::Null).ok(),
        _ => None,
    };
    let mut out = Vec::new();
    for r in &gates {
        let Some(name) = r[6].as_deref() else { continue };
        // THE PROPERTY AFTER `style.`, its unit from the gate row (or the name, for a row stored without one).
        let name = name.strip_prefix("style.").unwrap_or(name);
        let (property, split) = split_unit(name);
        let ast = if alone(property) { ast_of(r[5].as_deref()) } else { None };
        let s = Styled {
            gate: r[4].clone(), expression: r[5].clone(), via: "style", place: &r[..4], property,
            unit: r[7].clone().or(split), value: ast.as_ref(),
        };
        out.push(row_of(out.len() + 1, s));
    }
    for r in &styles {
        let Some(ast) = ast_of(r[4].as_deref()) else { continue };
        for (key, value) in ng_style(&ast) {
            let (property, unit) = split_unit(key);
            let s = Styled {
                gate: None, expression: r[4].clone(), via: "ngStyle", place: &r[..4], property, unit, value: Some(value),
            };
            out.push(row_of(out.len() + 1, s));
        }
    }
    out
}
