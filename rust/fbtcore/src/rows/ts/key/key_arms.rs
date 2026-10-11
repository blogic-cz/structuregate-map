//! A KEY A TEMPLATE EXPRESSION CHOOSES - the template half of `key_branches`' choice links.

use serde_json::Value;

/// Every literal a TEMPLATE expression holds, with the arms that choose it, and every arm's condition.
///
/// A KEY A TEMPLATE CHOOSES TAKES THE ARM IT SITS IN, as a TypeScript value does (`key_branches::value_keys`): in
/// `[text]="c ? 'a' : 'b'"` the literal `a` holds `c` and `b` holds `!c`; `x && 'k'` holds `x`,
/// `x || 'k'` holds `!x`, `??` nothing. A template tree has no `expressions` row per condition, so the
/// link is `arm:<expression>#<n>`, the n-th such node of that tree in walk order, and the condition's
/// subtree is handed to the gate readers under it - unresolved, as a `gates` tree is. A literal inside a
/// call or a pipe is found too: it is evaluated only when its arm is.
pub fn template_arms<'a>(expr: &str, ast: &'a Value) -> (Vec<(String, Vec<String>)>, Vec<(String, &'a Value)>) {
    fn walk<'a>(
        v: &'a Value,
        expr: &str,
        chain: &[String],
        n: &mut usize,
        out: &mut (Vec<(String, Vec<String>)>, Vec<(String, &'a Value)>),
    ) {
        let o = match v {
            Value::Array(items) => return items.iter().for_each(|i| walk(i, expr, chain, n, out)),
            Value::Object(o) => o,
            _ => return,
        };
        let with = |link: String| -> Vec<String> {
            let mut c = chain.to_vec();
            c.push(link);
            c
        };
        let (kind, op) = (o.get("k").and_then(Value::as_str), o.get("op").and_then(Value::as_str));
        let (test, arms): (&str, [(&str, Option<bool>); 2]) = match (kind, op) {
            (Some("Literal"), _) => {
                if let Some(Value::String(s)) = o.get("v") {
                    out.0.push((s.clone(), chain.to_vec()));
                }
                return;
            }
            (Some("Cond"), _) => ("cond", [("then", Some(false)), ("else", Some(true))]),
            (Some("Binary"), Some("&&")) => ("left", [("left", None), ("right", Some(false))]),
            (Some("Binary"), Some("||")) => ("left", [("left", None), ("right", Some(true))]),
            _ => return o.values().for_each(|x| walk(x, expr, chain, n, out)),
        };
        let id = format!("arm:{expr}#{n}");
        *n += 1;
        let Some(cond) = o.get(test) else { return };
        out.1.push((id.clone(), cond));
        if kind == Some("Cond") {
            walk(cond, expr, chain, n, out);
        }
        for (side, negated) in arms {
            let Some(x) = o.get(side) else { continue };
            match negated {
                None => walk(x, expr, chain, n, out),
                Some(false) => walk(x, expr, &with(id.clone()), n, out),
                Some(true) => walk(x, expr, &with(format!("!{id}")), n, out),
            }
        }
    }
    let mut out = (Vec::new(), Vec::new());
    walk(ast, expr, &[], &mut 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_template_arm_chooses_its_literal_and_hands_over_its_condition() {
        // `a ? (b && 'k1') : (c || 'k2') ?? 'k3'` as the template parser writes it, under a `Source` wrapper.
        let read = |n: &str| json!({"k": "Read", "name": n, "receiver": {"k": "Implicit"}});
        let lit = |v: &str| json!({"k": "Literal", "v": v});
        let ast = json!({"k": "Source", "ast": {"k": "Binary", "op": "??", "left": {"k": "Cond", "cond": read("a"),
            "then": {"k": "Binary", "op": "&&", "left": read("b"), "right": lit("k1")},
            "else": {"k": "Binary", "op": "||", "left": read("c"), "right": lit("k2")}}, "right": lit("k3")}});
        let (lits, arms) = template_arms("x:9", &ast);
        assert_eq!(lits, [
            ("k1".to_string(), vec!["arm:x:9#0".to_string(), "arm:x:9#1".to_string()]),
            ("k2".to_string(), vec!["!arm:x:9#0".to_string(), "!arm:x:9#2".to_string()]),
            ("k3".to_string(), vec![]),
        ]);
        let conds: Vec<(&str, &Value)> = arms.iter().map(|(id, t)| (id.as_str(), *t)).collect();
        assert_eq!(conds, [("arm:x:9#0", &read("a")), ("arm:x:9#1", &read("b")), ("arm:x:9#2", &read("c"))]);
    }
}
