//! A STYLE BINDING JOINED TO WHAT ITS VALUE DOES (`style_hides`).
//!
//! `[style.display]="shown ? 'block' : 'none'"` hides its element by its VALUE, not by a stylesheet: `class_hides`
//! answers class bindings only, and `style_rules.hides` is one declaration of a rule, never a binding. One row per
//! `style` gate says whether the property can hide an element on its own (`alone`: `display`, `visibility`) and, when
//! the value is a literal or a ternary of two literals, WHICH SIDE hides (`when`): `always`, `never`, `then`, `else`.
//! A value that is not decided from its text (a member read, a call, a nested ternary, a pipe) has `when` NULL: it is
//! not a claim either way. `height`, `opacity` and the rest never hide alone - a collapse needs partners.

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

/// The `style_hides` rows: one per `style` gate of the half, in the gates' own order.
pub fn rows(db: &Connection, half: &str) -> Vec<Value> {
    let sql = "SELECT id, expression, template, node, component, name, source FROM gates \
               WHERE half = ?1 AND gate_kind = 'style' ORDER BY rowid";
    let gates = text_rows(db, sql, half, 7);
    if gates.is_empty() {
        return Vec::new();
    }
    // `ast` is an object: its nodes, rebuilt for the few expressions a style gate holds.
    let mut tree = Column::new(db, "expressions", "ast").ok();
    let text = |v: &Option<String>| v.clone().map(Value::String).unwrap_or(Value::Null);
    let mut out = Vec::new();
    for r in gates {
        let Some(name) = r[5].as_deref() else { continue };
        // THE PROPERTY AFTER `style.`, with a unit (`width.px`) as its own column.
        let name = name.strip_prefix("style.").unwrap_or(name);
        let (property, unit) = match name.split_once('.') {
            Some((p, u)) => (p, Some(u.to_string())),
            None => (name, None),
        };
        let alone = matches!(property, "display" | "visibility");
        let decided = if alone {
            match (tree.as_mut(), r[1].as_deref()) {
                (Some(tree), Some(id)) => tree.value(id, &SqlValue::Null).ok().and_then(|ast| when(&ast, property)),
                _ => None,
            }
        } else {
            None
        };
        let mut row = Map::new();
        row.insert("id".into(), Value::String(format!("cssy:{}", out.len() + 1)));
        row.insert("gate".into(), text(&r[0]));
        row.insert("expression".into(), text(&r[1]));
        row.insert("via".into(), Value::String("style".into()));
        row.insert("template".into(), text(&r[2]));
        row.insert("node".into(), text(&r[3]));
        row.insert("component".into(), text(&r[4]));
        row.insert("property".into(), Value::String(property.into()));
        row.insert("unit".into(), unit.map(Value::String).unwrap_or(Value::Null));
        row.insert("condition".into(), text(&r[6]));
        row.insert("alone".into(), Value::from(i64::from(alone)));
        row.insert("hides".into(), Value::from(i64::from(matches!(decided, Some("always" | "then" | "else")))));
        row.insert("when".into(), decided.map(|w| Value::String(w.into())).unwrap_or(Value::Null));
        out.push(Value::Object(row));
    }
    out
}
