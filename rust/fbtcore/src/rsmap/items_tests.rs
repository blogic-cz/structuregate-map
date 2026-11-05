use super::*;

fn items(text: &str) -> Items {
    let file = syn::parse_file(text).expect("parses");
    read(&file, &LineStarts::new(text))
}

#[test]
fn only_what_another_file_can_name_is_declared() {
    let found = items("pub struct A;\nstruct Hidden;\npub(crate) fn b() {}\nmacro_rules! m { () => {} }\n");
    assert_eq!(found.declares.into_iter().collect::<Vec<_>>(), ["A", "b", "m"]);
}

#[test]
fn a_test_module_and_an_item_inside_a_body_declare_nothing() {
    let found = items("#[cfg(test)]\nmod tests { pub struct T; }\npub fn f() { pub struct Local; }\n");
    assert_eq!(found.declares.into_iter().collect::<Vec<_>>(), ["f"]);
}

#[test]
fn a_path_a_macro_argument_and_a_use_are_uses_and_a_member_is_not() {
    let found = items("use crate::store::Store;\nfn f() { let x = vec![Rows::new()]; x.walk(); }\n");
    for name in ["store", "Store", "Rows", "new", "vec"] {
        assert!(found.uses.contains(name), "{name} is a use");
    }
    assert!(!found.uses.contains("walk"), "a method after a dot is a member");
    assert!(!found.uses.contains("crate"), "a path keyword is not a name");
}

#[test]
fn a_mod_declaration_is_recorded_with_its_path_attribute_and_its_inline_parents() {
    let found = items("mod a;\n#[path = \"x/y.rs\"]\nmod b;\nmod outer { mod c; }\n");
    let shape: Vec<_> = found.mods.iter().map(|m| (m.name.as_str(), m.path_attr.clone(), m.inline.clone())).collect();
    assert_eq!(shape[0], ("a", None, vec![]));
    assert_eq!(shape[1], ("b", Some("x/y.rs".to_string()), vec![]));
    assert_eq!(shape[2], ("c", None, vec!["outer".to_string()]));
}

#[test]
fn two_bodies_that_differ_only_in_local_names_and_comments_fingerprint_alike() {
    let one = items("fn a(p: &str) { let x = p.len(); let y = x + 1; println!(\"{}\", y); }\n");
    let two = items("fn b(q: &str) {\n // why\n let m = q.len(); let n = m + 1; println!(\"{}\", n); }\n");
    assert_eq!(one.bodies[0].digest, two.bodies[0].digest);
    assert_eq!(one.bodies[0].name, "a");
}

#[test]
fn a_body_under_the_statement_threshold_is_not_recorded() {
    assert!(items("fn a() { let x = 1; x; }\n").bodies.is_empty());
}

#[test]
fn the_summary_is_the_first_line_of_the_crate_doc() {
    assert_eq!(items("//! First line.\n//! Second.\nfn a() {}\n").summary, "First line.");
}

#[test]
fn main_at_the_top_is_an_entry_and_main_inside_a_module_is_not() {
    assert!(items("fn main() {}\n").has_main);
    assert!(!items("mod m { fn main() {} }\n").has_main);
}

#[test]
fn a_cfg_test_item_declares_nothing() {
    let found = items("#[cfg(test)]\npub fn fixture() {}\npub fn real() {}\n");
    assert_eq!(found.declares.into_iter().collect::<Vec<_>>(), ["real"]);
}

#[test]
fn a_name_a_pattern_binds_is_no_use_and_a_call_is() {
    let found = items(
        "fn f(n: u8) { let x = n; g(x); [1].iter().map(|y| y + 1); match x { MAX => (), z => h(z) } }\n",
    );
    for name in ["g", "h"] {
        assert!(found.uses.contains(name), "{name} is a use");
    }
    for name in ["n", "x", "y", "z"] {
        assert!(!found.uses.contains(name), "{name} is a local");
    }
}

#[test]
fn a_name_the_file_defines_itself_is_no_use_of_another_files_item() {
    let found = items("fn walk() {}\npub fn go() { walk(); }\n");
    assert!(!found.uses.contains("walk"));
}
