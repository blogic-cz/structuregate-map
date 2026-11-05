//! THE NAMES A PATTERN BINDS - a `let`, a closure or fn parameter, a `match` arm, a `for`, an `if let`. A name bound
//! here is a LOCAL, never an item, so a later bare `n` that reads it is no use of another file's `pub fn n`: on this
//! repo such locals (`n`, `name`, `value`, `file`) carried about half of all rust name-join edges and most cycles.
//!
//! A CAPITALISED bare identifier in a pattern is a const or a unit variant being MATCHED (`MAX =>`, `None`), which
//! rustc resolves to the item when one is in scope - not a binding - unless `ref`, `mut` or `@` make it one.
use syn::visit::{self, Visit};

struct Binder(Vec<String>);

impl<'ast> Visit<'ast> for Binder {
    fn visit_pat_ident(&mut self, i: &'ast syn::PatIdent) {
        let name = i.ident.to_string();
        let name = name.trim_start_matches("r#");
        let capital = name.chars().next().is_some_and(|c| c.is_uppercase());
        if i.by_ref.is_some() || i.mutability.is_some() || i.subpat.is_some() || !capital {
            self.0.push(name.to_string());
        }
        visit::visit_pat_ident(self, i);
    }
}

/// Every name `pat` binds, in order.
pub(crate) fn bindings(pat: &syn::Pat) -> Vec<String> {
    let mut binder = Binder(vec![]);
    binder.visit_pat(pat);
    binder.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(pat: &str) -> Vec<String> {
        let expr = syn::parse_str::<syn::Expr>(&format!("|{pat}| ()")).expect("parses");
        let syn::Expr::Closure(closure) = expr else { panic!("a closure") };
        closure.inputs.iter().flat_map(bindings).collect()
    }

    #[test]
    fn a_pattern_binds_its_lowercase_names_and_matches_its_capitalised_ones() {
        assert_eq!(names("n"), ["n"]);
        assert_eq!(names("Some(x)"), ["x"]);
        assert_eq!(names("(a, Kind::B, MAX)"), ["a"]);
        assert_eq!(names("ref mut Big"), ["Big"]);
        assert_eq!(names("Point { x, y: py }"), ["x", "py"]);
    }
}
