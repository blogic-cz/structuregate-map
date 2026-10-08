//! THE THREE THINGS EVERY WALK OVER A STORED AST NEEDS, in one place.
//!
//! `expressions.ast` is the map's own serialised tree, and four passes walk it: what a
//! gate compares, what a directive class proves, what a config object states, and where a
//! bound field renders. Each had its own copy of the same two one-liners in the tool this
//! is ported from, and two of them a hand-written copy of the SAME Read-target walk with
//! the predicate flipped. A vocabulary that is copied is a vocabulary that drifts: the day
//! `Source` gains a sibling wrapper, five files have to learn it and the one that does not
//! silently walks a tree it thinks is empty.

use indexmap::IndexSet;
use serde_json::Value;

/// A stored tree is wrapped in a `Source` node carrying the original text; the walk wants
/// what is inside.
pub fn unwrap(ast: &Value) -> &Value {
    if let Some(object) = ast.as_object()
        && object.get("k").and_then(|k| k.as_str()) == Some("Source")
    {
        return object.get("ast").unwrap_or(&Value::Null);
    }
    ast
}

/// Both property reads the template vocabulary has — `a.b` and `a?.b`.
pub fn is_read(n: &Value) -> bool {
    matches!(
        n.as_object().and_then(|o| o.get("k")).and_then(|k| k.as_str()),
        Some("Read") | Some("SafeRead")
    )
}

/// Python truthiness, which is what the conditions this replaces were written against.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn row_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Every declaration row a tree's reads RESOLVE to, at any depth, IN THE ORDER THEY WERE
/// FOUND.
///
/// The row is the checker's own answer (`Read.target.row`), which is why this is an id
/// join and not a name match: a same-named field on another class carries a different row,
/// so it cannot be mistaken for this one. Depth matters — a key may be bound inside a
/// ternary or behind a pipe.
///
/// AN ORDERED SET, because the caller iterates it and turns each row into an output row.
/// An unordered one would publish the same field sites in an order that changes between
/// runs, which is exactly the kind of difference the 53-table gate reports as changed rows
/// carrying no changed fact.
pub fn read_rows(ast: &Value) -> IndexSet<String> {
    let mut out = IndexSet::new();
    let mut stack: Vec<&Value> = vec![ast];

    while let Some(n) = stack.pop() {
        match n {
            // A list is descended into, and is never itself a read.
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(object) => {
                if is_read(n)
                    && let Some(target) = object.get("target").and_then(|t| t.as_object())
                    && let Some(row) = target.get("row")
                    && truthy(row)
                {
                    out.insert(row_text(row));
                }
                stack.extend(object.values());
            }
            _ => continue,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_source_wrapper_is_unwrapped_and_nothing_else_is() {
        let wrapped = json!({"k": "Source", "ast": {"k": "Read"}, "text": "a.b"});
        assert_eq!(unwrap(&wrapped), &json!({"k": "Read"}));
        let bare = json!({"k": "Read"});
        assert_eq!(unwrap(&bare), &bare);
    }

    #[test]
    fn both_property_reads_the_vocabulary_has_are_reads() {
        assert!(is_read(&json!({"k": "Read"})));
        assert!(is_read(&json!({"k": "SafeRead"})));
        assert!(!is_read(&json!({"k": "KeyedRead"})));
        assert!(!is_read(&json!("Read")));
    }

    #[test]
    fn a_read_resolves_to_the_declaration_row_the_checker_gave_it() {
        let ast = json!({"k": "Read", "name": "x", "target": {"row": "m:7"}});
        assert_eq!(read_rows(&ast).into_iter().collect::<Vec<_>>(), vec!["m:7"]);
    }

    #[test]
    fn a_read_with_no_resolved_row_contributes_nothing() {
        // An unresolved read is the honest empty answer, not a name to match on.
        assert!(read_rows(&json!({"k": "Read", "name": "x"})).is_empty());
        assert!(read_rows(&json!({"k": "Read", "target": {}})).is_empty());
        assert!(read_rows(&json!({"k": "Read", "target": {"row": ""}})).is_empty());
        assert!(read_rows(&json!({"k": "Read", "target": {"row": null}})).is_empty());
    }

    #[test]
    fn depth_matters_because_a_key_may_be_bound_behind_a_pipe_or_a_ternary() {
        let ast = json!({
            "k": "Conditional",
            "condition": {"k": "Read", "target": {"row": "m:1"}},
            "trueExp": {"k": "Pipe", "exp": {"k": "SafeRead", "target": {"row": "m:2"}}},
            "falseExp": [{"k": "Read", "target": {"row": "m:3"}}],
        });
        let rows = read_rows(&ast);
        assert_eq!(rows.len(), 3);
        for want in ["m:1", "m:2", "m:3"] {
            assert!(rows.contains(want), "{want} was not reached");
        }
    }

    #[test]
    fn the_same_row_read_twice_is_one_entry() {
        let ast = json!({
            "a": {"k": "Read", "target": {"row": "m:1"}},
            "b": {"k": "Read", "target": {"row": "m:1"}},
        });
        assert_eq!(read_rows(&ast).len(), 1);
    }

    #[test]
    fn the_order_rows_are_found_in_is_stable() {
        // The caller turns each row into an output row, so a walk that reordered would
        // publish the same sites differently on every run.
        let ast = json!({
            "one": {"k": "Read", "target": {"row": "m:1"}},
            "two": {"k": "Read", "target": {"row": "m:2"}},
            "three": {"k": "Read", "target": {"row": "m:3"}},
        });
        let first: Vec<String> = read_rows(&ast).into_iter().collect();
        let again: Vec<String> = read_rows(&ast).into_iter().collect();
        assert_eq!(first, again);
        assert_eq!(first.len(), 3);
    }

    #[test]
    fn a_tree_that_is_not_an_object_is_walked_without_complaint() {
        assert!(read_rows(&Value::Null).is_empty());
        assert!(read_rows(&json!("text")).is_empty());
        assert!(read_rows(&json!([])).is_empty());
    }
}
