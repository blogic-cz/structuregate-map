//! THE POLARITY WALK AND THE VALUE VOCABULARY, in one place because three passes and a
//! second table read them.
//!
//! A walk that is copied is a walk that drifts — the whole reason `astreads` exists one
//! level down.

use super::jsstr;
use indexmap::IndexMap;
use serde_json::Value;

/// A JavaScript `Set`, for the one place both of its behaviours are needed at once.
///
/// `necessary` collects into it from two callers: the feature pass adds feature-code
/// STRINGS, where adding the same code from two leaves must collapse to one, and the
/// value pass adds restriction OBJECTS, where every one is a fresh object and a
/// JavaScript `Set` therefore never collapses any of them.
///
/// Insertion order is preserved, because both callers iterate the result into rows.
#[derive(Debug, Default, Clone)]
pub struct OrderedSet {
    seen: std::collections::HashSet<String>,
    items: Vec<Value>,
}

impl OrderedSet {
    pub fn new() -> OrderedSet {
        OrderedSet::default()
    }

    pub fn add(&mut self, value: Value) {
        // A scalar is hashable and collapses; an object or a list is not, and a
        // JavaScript Set would not collapse those either.
        match &value {
            Value::Array(_) | Value::Object(_) => self.items.push(value),
            scalar => {
                let key = scalar.to_string();
                if self.seen.insert(key) {
                    self.items.push(value);
                }
            }
        }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Value> {
        self.items.iter()
    }

    /// Read by the test that states a javascript `Set` holds every fresh object, which is
    /// the rule the value pass depends on.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The members as strings, for the callers that only ever add strings.
    pub fn strings(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect()
    }
}

/// Every object a serialised value holds, at any depth.
pub fn walk_value(v: &Value, fn_: &mut dyn FnMut(&serde_json::Map<String, Value>)) {
    let mut stack: Vec<&Value> = vec![v];
    while let Some(n) = stack.pop() {
        match n {
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(object) => {
                fn_(object);
                stack.extend(object.values());
            }
            _ => continue,
        }
    }
}

/// Every `<root>.X` a serialised value names, at any depth.
pub fn codes_in(value: &Value, root: &str) -> IndexMap<String, bool> {
    let mut out = IndexMap::new();
    let prefix = format!("{root}.");
    walk_value(value, &mut |n| {
        if let Some(Value::String(member)) = n.get("$member")
            && let Some(rest) = member.strip_prefix(prefix.as_str())
        {
            out.insert(rest.to_string(), true);
        }
    });
    out
}

/// THE POLARITY WALK, over an ANGULAR AST.
///
/// Only `&&`, `||` and `!` are interpreted: everything else is a LEAF handed to `resolve`
/// with the polarity that reaches it, and what a leaf MEANS is the caller's business. A
/// `===` against an enum constant is a leaf, not a connective.
///
/// `resolve(node, negated)` takes the polarity because the two callers need it
/// differently: a NEGATED feature contributes nothing — "renders when the feature is OFF"
/// is not a grantable gate — while a negated `!==` is an ordinary `===` and restricts as
/// much as one.
pub fn necessary(
    node: &Value,
    negated: bool,
    resolve: &mut dyn FnMut(&Value, bool) -> Vec<Value>,
    out: &mut OrderedSet,
) {
    walk(node, negated, resolve, &mut None, out);
}

/// What one side of a DISJUNCTION proved, and what the other did, as what the whole proves.
pub type Either<'a> = &'a mut dyn FnMut(Vec<Value>, Vec<Value>) -> Vec<Value>;

/// `necessary`, where a DISJUNCTION is read as well - an `||` un-negated, an `&&` under a `!`.
///
/// Neither side of `a || b` is necessary, so `necessary` proves nothing from it, and that is right
/// for a FEATURE: "this or that is switched on" grants neither. A VALUE is different: `x === A ||
/// x === B` permits exactly A and B, a fact neither side states alone. The walk proves
/// each side on its own and hands both to `either`, which keeps only what BOTH sides restrict - a
/// dimension one side says nothing of may hold any value when that side is the one that held.
pub fn necessary_either(
    node: &Value,
    negated: bool,
    resolve: &mut dyn FnMut(&Value, bool) -> Vec<Value>,
    either: Either<'_>,
    out: &mut OrderedSet,
) {
    walk(node, negated, resolve, &mut Some(either), out);
}

fn walk(
    node: &Value,
    negated: bool,
    resolve: &mut dyn FnMut(&Value, bool) -> Vec<Value>,
    either: &mut Option<Either<'_>>,
    out: &mut OrderedSet,
) {
    let Some(object) = node.as_object() else { return };
    let kind = object.get("k").and_then(|k| k.as_str());

    // A PROPERTY TRUE ONLY BECAUSE ITS ONE WRITE HELD (`gate_props`): true proves the write's
    // condition, false proves nothing - the property may never have been written.
    if kind == Some("Implied") {
        if !negated && let Some(inner) = object.get("expr") {
            walk(inner, negated, resolve, either, out);
        }
        return;
    }
    if kind == Some("Source") {
        if let Some(inner) = object.get("ast") {
            walk(inner, negated, resolve, either, out);
        }
        return;
    }
    if kind == Some("Not") {
        if let Some(expr) = object.get("expr") {
            walk(expr, !negated, resolve, either, out);
        }
        return;
    }
    if kind == Some("Binary") {
        let op = object.get("op").and_then(|o| o.as_str());
        if op == Some("&&") || op == Some("||") {
            let sides = [object.get("left"), object.get("right")];
            // An `&&` under no negation, or an `||` under one, is the only shape whose
            // parts are each necessary.
            if (op == Some("&&") && !negated) || (op == Some("||") && negated) {
                for side in sides.into_iter().flatten() {
                    walk(side, negated, resolve, either, out);
                }
            } else if either.is_some() {
                let mut proved = [OrderedSet::new(), OrderedSet::new()];
                for (side, into) in sides.into_iter().zip(proved.iter_mut()) {
                    if let Some(side) = side {
                        walk(side, negated, resolve, either, into);
                    }
                }
                let [l, r] = proved;
                if let Some(join) = either.as_mut() {
                    for x in join(l.items, r.items) {
                        out.add(x);
                    }
                }
            }
            return;
        }
    }
    for x in resolve(node, negated) {
        out.add(x);
    }
}

/// A CONDITION'S NECESSARY NODES, over the serialised TypeScript that `branches.condition`
/// carries.
///
/// `necessary` above cannot be reused: a template gate is an Angular AST (`k`/`op`/`expr`)
/// and a branch condition is this map's `$` value vocabulary (`$logic`/`$operands`/
/// `$kind`). Only the SHAPE differs — the rule is the same one, `&&` descends and
/// `||`/`??` prove nothing.
///
/// A NEGATION IS OPAQUE HERE RATHER THAN DESCENDED. `PrefixUnaryExpression` is serialised
/// as a LEAF that still lists the negated name in `$reads`, so reading through it would
/// report `!x` as requiring `x`. Proving nothing from it is the conservative direction: it
/// can only ever drop an attribution.
pub fn necessary_nodes(node: &Value) -> Vec<&serde_json::Map<String, Value>> {
    let mut out = Vec::new();
    let mut stack: Vec<&Value> = vec![node];
    while let Some(n) = stack.pop() {
        match n {
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(object) => {
                match object.get("$logic").and_then(|l| l.as_str()) {
                    Some("&&") => {
                        if let Some(Value::Array(operands)) = object.get("$operands") {
                            stack.extend(operands.iter());
                        }
                        continue;
                    }
                    Some("||") | Some("??") => continue,
                    _ => {}
                }
                if object.get("$kind").and_then(|k| k.as_str()) == Some("PrefixUnaryExpression") {
                    continue;
                }
                out.push(object);
            }
            _ => continue,
        }
    }
    out
}

/// The identifiers among them, which is the shape the branch guard asks for.
///
/// This replaced a helper that unioned `$reads` at any depth, and that helper was the
/// single worst bug in the file it comes from: a callback's parameter read as PROVEN
/// whenever the name appeared ANYWHERE in the value, so `= !this.isHidden ||
/// !isEnabled` counted. A handful of rows, and the worst kind is a property truthy only
/// when the feature is OFF — the row then told a caller to switch on the very thing that
/// hides the content.
pub fn necessary_idents(node: &Value) -> IndexMap<String, bool> {
    let mut out = IndexMap::new();
    for n in necessary_nodes(node) {
        if n.get("$kind").and_then(|k| k.as_str()) == Some("Identifier")
            && let Some(Value::String(expr)) = n.get("$expr")
        {
            out.insert(jsstr::trim(expr).to_string(), true);
        }
    }
    out
}

/// A VALUE THAT SUPPLIES NOTHING — `null`, `0`, `false` or `""`, and nothing else.
///
/// STRICT, the way `===` is. In python `0 == False`, so a loose comparison would fold the
/// number and the boolean into one another; the source tests them as four separate
/// identities and so does this.
pub fn is_blank(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !*b,
        Value::Number(n) => n.as_f64().map(|f| f == 0.0).unwrap_or(false),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// THE TWO READINGS OF "supplies nothing", which are not the same question.
///
/// `[]` is TRUTHY in JavaScript. An `*ngIf="entries"` over it RENDERS; an `*ngFor` over
/// it renders nothing. So a ternary whose else branch is `[]` proves a feature necessary
/// for the ITERATION gate and proves nothing for an `ngIf` on the same property, and one
/// predicate serving both would attribute the feature to a gate that does not require it —
/// demanding a capability the element does not need, which HIDES content from callers
/// entitled to it.
pub fn supplies_nothing(v: &Value, iterated: bool) -> bool {
    is_blank(v) || (iterated && matches!(v, Value::Array(a) if a.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A resolver that reports every leaf it is given, with its polarity.
    fn leaves(seen: &mut Vec<(String, bool)>) -> impl FnMut(&Value, bool) -> Vec<Value> + '_ {
        move |node, negated| {
            let name = node
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("?")
                .to_string();
            seen.push((name.clone(), negated));
            vec![Value::String(name)]
        }
    }

    fn walk(ast: &Value) -> (Vec<String>, Vec<(String, bool)>) {
        let mut seen = Vec::new();
        let mut out = OrderedSet::new();
        {
            let mut resolve = leaves(&mut seen);
            necessary(ast, false, &mut resolve, &mut out);
        }
        (out.strings(), seen)
    }

    fn leaf(name: &str) -> Value {
        json!({"k": "PropertyRead", "name": name})
    }

    #[test]
    fn an_and_makes_both_sides_necessary() {
        let ast = json!({"k": "Binary", "op": "&&", "left": leaf("a"), "right": leaf("b")});
        assert_eq!(walk(&ast).0, vec!["a", "b"]);
    }

    #[test]
    fn an_or_proves_nothing_because_either_side_may_be_the_one_that_held() {
        let ast = json!({"k": "Binary", "op": "||", "left": leaf("a"), "right": leaf("b")});
        assert!(walk(&ast).0.is_empty());
    }

    #[test]
    fn a_disjunction_hands_both_sides_to_either_and_keeps_what_it_returns() {
        // `a || b`, and `!(a && b)` - which is `!a || !b` - are the two shapes `either` is asked about.
        let or = json!({"k": "Binary", "op": "||", "left": leaf("a"), "right": leaf("b")});
        let nand = json!({"k": "Not", "expr": {"k": "Binary", "op": "&&", "left": leaf("a"), "right": leaf("b")}});
        for ast in [or, nand] {
            let mut asked = Vec::new();
            let mut out = OrderedSet::new();
            let mut resolve = |n: &Value, _: bool| vec![n["name"].clone()];
            let mut either = |l: Vec<Value>, r: Vec<Value>| {
                asked.push((l.clone(), r.clone()));
                vec![json!("joined")]
            };
            necessary_either(&ast, false, &mut resolve, &mut either, &mut out);
            assert_eq!(asked, vec![(vec![json!("a")], vec![json!("b")])]);
            assert_eq!(out.strings(), vec!["joined"]);
        }
        // An `&&` still proves each side, and never asks.
        let and = json!({"k": "Binary", "op": "&&", "left": leaf("a"), "right": leaf("b")});
        let mut out = OrderedSet::new();
        let mut resolve = |n: &Value, _: bool| vec![n["name"].clone()];
        let mut either = |_: Vec<Value>, _: Vec<Value>| -> Vec<Value> { panic!("an && is no disjunction") };
        necessary_either(&and, false, &mut resolve, &mut either, &mut out);
        assert_eq!(out.strings(), vec!["a", "b"]);
    }

    #[test]
    fn negation_swaps_which_connective_proves_anything() {
        // !(a || b) means neither held, so both are necessary.
        let ast = json!({"k": "Not", "expr": {"k": "Binary", "op": "||",
                                              "left": leaf("a"), "right": leaf("b")}});
        let (got, seen) = walk(&ast);
        assert_eq!(got, vec!["a", "b"]);
        assert!(seen.iter().all(|(_, negated)| *negated), "both leaves arrive negated");

        // !(a && b) proves nothing: only one of them need be false.
        let ast = json!({"k": "Not", "expr": {"k": "Binary", "op": "&&",
                                              "left": leaf("a"), "right": leaf("b")}});
        assert!(walk(&ast).0.is_empty());
    }

    #[test]
    fn the_source_wrapper_is_walked_through() {
        let ast = json!({"k": "Source", "ast": leaf("a"), "text": "a"});
        assert_eq!(walk(&ast).0, vec!["a"]);
    }

    #[test]
    fn a_comparison_is_a_leaf_and_not_a_connective() {
        // `===` against an enum constant is the caller's business, not the walk's.
        let ast = json!({"k": "Binary", "op": "===", "left": leaf("step"), "right": leaf("TWO")});
        let (got, seen) = walk(&ast);
        assert_eq!(seen.len(), 1, "the whole comparison is handed over once");
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn the_same_code_from_two_leaves_collapses_to_one() {
        let ast = json!({"k": "Binary", "op": "&&", "left": leaf("a"), "right": leaf("a")});
        assert_eq!(walk(&ast).0, vec!["a"]);
    }

    #[test]
    fn two_restriction_objects_never_collapse_however_alike() {
        // A javascript Set holds every fresh object; the value pass depends on that.
        let mut set = OrderedSet::new();
        set.add(json!({"enum": "e:1", "member": "TWO"}));
        set.add(json!({"enum": "e:1", "member": "TWO"}));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn a_blank_value_is_exactly_four_things() {
        for v in [json!(null), json!(false), json!(0), json!(0.0), json!("")] {
            assert!(is_blank(&v), "{v} supplies nothing");
        }
        // STRICT: `true` is not `1`, and a non-empty string is not blank.
        for v in [json!(true), json!(1), json!("x"), json!([]), json!({})] {
            assert!(!is_blank(&v), "{v} supplies something");
        }
    }

    #[test]
    fn an_empty_list_supplies_nothing_only_to_an_iteration_gate() {
        // `[]` is TRUTHY, so an *ngIf over it renders and an *ngFor over it does not.
        assert!(supplies_nothing(&json!([]), true));
        assert!(!supplies_nothing(&json!([]), false));
        // And a non-empty list supplies something either way.
        assert!(!supplies_nothing(&json!([1]), true));
    }

    #[test]
    fn a_branch_condition_descends_an_and_and_stops_at_an_or() {
        let node = json!({"$logic": "&&", "$operands": [
            {"$kind": "Identifier", "$expr": "isEnabled"},
            {"$logic": "||", "$operands": [{"$kind": "Identifier", "$expr": "hidden"}]},
        ]});
        let idents = necessary_idents(&node);
        assert!(idents.contains_key("isEnabled"));
        assert!(!idents.contains_key("hidden"), "an || proves nothing");
    }

    #[test]
    fn a_negation_in_a_branch_condition_is_opaque_rather_than_read_through() {
        // The worst bug in the file this comes from: `!isEnabled` reported as REQUIRING
        // isEnabled, telling a caller to switch on the very thing that hides the content.
        let node = json!({"$kind": "PrefixUnaryExpression", "$expr": "!isEnabled",
                          "$reads": ["isEnabled"]});
        assert!(necessary_idents(&node).is_empty());
    }

    #[test]
    fn codes_are_read_out_of_a_serialised_value_at_any_depth() {
        let value = json!({"$then": {"$member": "FlagCodes.ALPHA"},
                           "$else": [{"$member": "FlagCodes.BETA"},
                                     {"$member": "Other.GAMMA"}]});
        let codes = codes_in(&value, "FlagCodes");
        assert!(codes.contains_key("ALPHA"));
        assert!(codes.contains_key("BETA"));
        assert!(!codes.contains_key("GAMMA"), "another enum is not this one");
    }
}
