//! THE TEXT OF A C# HTTP CALL'S URL, for `cscalls.rs`: a `string.Format`'s holes, a template's alternatives as patterns, a
//! prefix joined to a caller's url, and the key a wrapper is found by (a qualname and its parameter count).

use serde_json::Value;

/// A url a `string.Format` fills: each `{n}`, `{n,w}` or `{n:f}` is a hole `{}`, `{{` and `}}` the brace itself.
pub(super) fn formatted(url: &str) -> String {
    let mut out = String::new();
    let mut rest = url;
    while let Some(c) = rest.chars().next() {
        let item = rest.strip_prefix('{').and_then(|r| r.find('}').map(|end| &r[..end]));
        let digits = item.map(|i| i.len() - i.trim_start_matches(|d: char| d.is_ascii_digit()).len()).unwrap_or(0);
        if rest.starts_with("{{") || rest.starts_with("}}") {
            out.push(c);
            rest = &rest[2..];
        } else if let (Some(item), true) = (item, digits > 0) {
            out.push_str("{}");
            rest = &rest[item.len() + 2..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// An interpolation's alternatives as patterns, each path once: each piece's text, a hole `{}`. An alternative of holes
/// alone (a ternary's arm only the run time knows) says nothing while another one says a path, and is left out; when
/// none says one (`{}` and `{}/{}`, a joiner's arms over two run-time values), they are one unknown url, the shortest.
pub(super) fn folded(alternatives: &[Value]) -> Vec<String> {
    let all = spelled(alternatives);
    let said: Vec<String> = all.iter().filter(|p| p.replace("{}", "").chars().any(char::is_alphanumeric)).cloned().collect();
    if said.is_empty() { all.into_iter().min_by_key(String::len).into_iter().collect() } else { said }
}

pub(super) fn spelled(alternatives: &[Value]) -> Vec<String> {
    let spelled: std::collections::BTreeSet<String> = alternatives
        .iter()
        .map(|alt| {
            alt.as_array()
                .map(|pieces| pieces.iter().map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| "{}".into())).collect())
                .unwrap_or_default()
        })
        .map(|s: String| s.split('?').next().unwrap_or("").to_string())
        .collect();
    spelled.into_iter().collect()
}

/// A method as wrappers are keyed: its qualname, how many type parameters and how many parameters it declares
/// (`Demo.Api.Post/1/2`) - overloads differing in either are two keys.
pub(super) fn overload(qualname: &str, generics: usize, parameters: usize) -> String {
    format!("{qualname}/{generics}/{parameters}")
}

/// How many type arguments a symbol gives its method - the `<..>` it ends in: `A.B<T>.C<D, E<F>>` gives two,
/// `A.B<T>.C` none.
pub(super) fn generics(symbol: &str) -> usize {
    if !symbol.ends_with('>') {
        return 0;
    }
    let (mut nested, mut count) = (0usize, 1usize);
    for c in symbol.chars().rev() {
        match c {
            '>' => nested += 1,
            '<' if nested == 1 => return count,
            '<' => nested -= 1,
            ',' if nested == 1 => count += 1,
            _ => {}
        }
    }
    0
}

/// How many parameters a call's `signature` lists: `A.B<T, U>(T, (int, string), D<E, F>)` lists three.
pub(super) fn arity(signature: &str) -> usize {
    let (mut nested, mut open, mut count, mut any) = (0usize, false, 0usize, false);
    for c in signature.chars() {
        match c {
            '(' if !open && nested == 0 => open = true,
            '(' | '<' | '[' => nested += 1,
            ')' if open && nested == 0 => break,
            ')' | '>' | ']' => nested = nested.saturating_sub(1),
            ',' if open && nested == 0 => count += 1,
            c if open && !c.is_whitespace() => any = true,
            _ => {}
        }
    }
    if any { count + 1 } else { 0 }
}

/// A wrapper's prefix and its caller's url: one `/` where the prefix ends in one and the url starts with one (a joiner that
/// trims both edges and puts a `/` between).
pub(super) fn joined(prefix: &str, url: &str) -> String {
    match (prefix.ends_with('/'), url.strip_prefix('/')) {
        (true, Some(rest)) => format!("{prefix}{rest}"),
        _ => format!("{prefix}{url}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_counts_its_parameters_at_the_top_level_only() {
        assert_eq!(arity("A.B<T, U>(T, (int, string), D<E, F>, int[,])"), 4);
        assert_eq!(arity("A.B()"), 0);
        assert_eq!(arity("A.B<T>(string)"), 1);
        assert_eq!(overload("Demo.Api.Post", 1, 2), "Demo.Api.Post/1/2");
        assert_eq!(generics("A.B<T>.C<D, E<F, G>>"), 2);
        assert_eq!(generics("A.B<T>.C"), 0);
        assert_eq!(generics("A.C<System.String>"), 1);
        let alternatives = serde_json::json!([[{"symbol": "a"}, "/", {"symbol": "b"}], [{"symbol": "b"}]]);
        assert_eq!(folded(alternatives.as_array().unwrap()), ["{}"]);
        let said = serde_json::json!([[{"symbol": "a"}], ["/x/", {"symbol": "b"}]]);
        assert_eq!(folded(said.as_array().unwrap()), ["/x/{}"]);
    }
}
