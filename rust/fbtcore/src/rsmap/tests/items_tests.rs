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
    for name in ["Rows", "vec"] {
        assert!(found.uses.contains(name), "{name} is a use");
    }
    // AN IN-TREE `use` IS A QUALIFIED PATH, walked through the module tree, never the names `store` and `Store`.
    assert!(!found.uses.contains("store") && !found.uses.contains("Store"), "{:?}", found.uses);
    assert_eq!(found.qualified.len(), 1);
    assert_eq!(found.qualified[0].segs, ["store", "Store"]);
    assert!(!found.uses.contains("new"), "a later segment of an unimported type's path is its member");
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
fn a_visibility_restriction_is_no_path() {
    let found = items("pub(crate) fn a() {}\npub(super) struct B;\npub(in crate::m) fn c() {}\n");
    assert!(found.qualified.is_empty(), "{:?}", found.qualified);
}

#[test]
fn an_inline_module_is_recorded_with_the_chain_around_it_and_every_own_name_is_kept() {
    let found = items("mod outer { mod inner { fn deep() {} } }\nfn top() {}\n");
    assert_eq!(found.inline_mods, [(vec![], "outer".to_string()), (vec!["outer".to_string()], "inner".to_string())]);
    assert!(found.own.contains("top") && found.own.contains("deep") && found.own.contains("inner"));
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
    assert!(!found.uses.contains("new"), "a later segment of an unimported type's path is its member");
    assert!(!found.uses.contains("walk"));
}

#[test]
fn a_fns_local_item_is_its_own_and_draws_no_use() {
    let uses = items("pub fn k() -> u8 { fn helper() -> u8 { 1 } helper() }\n").uses;
    assert!(!uses.contains("helper"), "{uses:?}");
}

fn cfg_test(attr: &str) -> bool {
    let f: syn::ItemFn = syn::parse_str(&format!("{attr}\nfn f() {{}}\n")).unwrap();
    super::is_cfg_test(&f.attrs)
}

#[test]
fn a_cfg_requires_test_by_its_predicate() {
    assert!(cfg_test("#[cfg(test)]"));
    assert!(cfg_test("#[cfg(all(test, windows))]"));
    assert!(!cfg_test("#[cfg(any(test, feature = \"x\"))]"));
    assert!(cfg_test("#[cfg(any(test, all(test, unix)))]"));
    assert!(!cfg_test("#[cfg(not(test))]"));
    assert!(!cfg_test("#[cfg(windows)]"));
}

#[test]
fn macro_arguments_are_code_and_a_macro_rules_template_names_nothing() {
    let found = items("fn f(v: &[u8]) { println!(\"{}\", v.iter().map(|alpha| alpha + 1).count()); assert_eq!(S { beta: 1 }.beta, 1); let _ = matches!(v.len(), Some(gamma) if gamma > 1); let _ = vec![delta(); 2]; }\n");
    for name in ["alpha", "beta", "gamma"] {
        assert!(!found.uses.contains(name), "{name}: {:?}", found.uses);
    }
    assert!(found.uses.contains("delta"), "a call inside an argument is a use");
    let found = items("macro_rules! m { ($tmpl:expr) => { $tmpl + kept() }; }\n");
    for name in ["tmpl", "expr"] {
        assert!(!found.uses.contains(name), "{name}: {:?}", found.uses);
    }
    assert!(found.uses.contains("kept"));
}

#[test]
fn a_template_s_let_binding_and_field_label_are_not_uses() {
    let found = items("macro_rules! m { ($b:block) => {{ let value = $b; R { files: value, kept: other() } }}; }\n");
    assert!(!found.uses.contains("value"), "a let binding is the body's local: {:?}", found.uses);
    assert!(!found.uses.contains("files") && !found.uses.contains("kept"), "a field label is no use: {:?}", found.uses);
    assert!(found.uses.contains("R") && found.uses.contains("other"), "{:?}", found.uses);
}
