//! WHICH CLASS AND STYLE GATES ARE A VISIBILITY CONDITION, and on which side - `class_polarity`.
//!
//! Split from `gate_values` (its readers stay there): `gate_values`, `gate_features` and `key_bound` all ask it
//! before they read a class or style gate, and the answer is one of `Polarity`'s three.

use super::gate_values::{id_of, text};
use super::store::Store;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// WHAT A CLASS OR STYLE GATE SAYS ABOUT ITS ELEMENT BEING SHOWN.
#[derive(Clone, Debug)]
pub enum Polarity {
    /// The gate hides its element while TRUE: read on the false side (`!c`).
    Hides,
    /// The gate only styles: no visibility condition.
    Styles,
    /// A style ternary of literals whose one arm hides: the AST of the condition under which the element SHOWS.
    Shown(Value),
}

/// A CLASS GATE IS A VISIBILITY CONDITION ONLY WHEN A STYLESHEET PROVES ITS CLASS HIDES (`class_hides`, scope `own` or
/// `shared`; `other` reaches only through `::ng-deep`/`None` encapsulation, which nothing proves): then the element
/// renders while the binding is FALSE (`Hides`). A class that only styles is no condition (`Styles`). A STYLE gate
/// (`[style.x]`) restyles and is `Styles` too (#52), EXCEPT a `[style.display]`/`[style.visibility]` ternary of two
/// literals whose one arm hides (`style_hides.when` = `then`/`else`, #53): it is `Shown` on its showing side. `always`
/// (never shown) is deliberately NOT claimed here, nor is a non-literal arm. Absent: neither kind of gate.
pub fn class_polarity(store: &Store<'_>) -> HashMap<String, Polarity> {
    let hiding: HashSet<String> = store.table("class_hides").iter()
        .filter(|h| matches!(text(h, "scope").as_deref(), Some("own" | "shared")))
        .filter_map(|h| id_of(h, "gate")).collect();
    let when: HashMap<String, String> = store.table("style_hides").iter()
        .filter_map(|h| Some((id_of(h, "gate")?, text(h, "when")?))).collect();
    let mut out = HashMap::new();
    let mut shown: HashMap<String, String> = HashMap::new();
    for g in store.table("gates").iter() {
        let Some(id) = id_of(g, "id") else { continue };
        match text(g, "gate_kind").as_deref() {
            Some("class") => {
                out.insert(id.clone(), if hiding.contains(&id) { Polarity::Hides } else { Polarity::Styles });
            }
            Some("style") => {
                out.insert(id.clone(), Polarity::Styles);
                if let (Some(w), Some(expression)) = (when.get(&id), id_of(g, "expression"))
                    && matches!(w.as_str(), "then" | "else")
                {
                    shown.insert(expression, id);
                }
            }
            _ => {}
        }
    }
    if shown.is_empty() {
        return out;
    }
    for e in store.table("expressions").iter() {
        let Some(gate) = id_of(e, "id").and_then(|x| shown.get(&x)) else { continue };
        let Some(ast) = e.get("ast").filter(|a| !a.is_null()) else { continue };
        let mut node = ast;
        while node.get("k").and_then(Value::as_str) == Some("Source") {
            let Some(inner) = node.get("ast") else { break };
            node = inner;
        }
        let Some(cond) = node.get("cond").filter(|_| node.get("k").and_then(Value::as_str) == Some("Cond")) else { continue };
        let polarity = match when.get(gate).map(String::as_str) {
            Some("then") => Polarity::Shown(json!({"k": "Not", "expr": cond})),
            _ => Polarity::Shown(cond.clone()),
        };
        out.insert(gate.clone(), polarity);
    }
    out
}
