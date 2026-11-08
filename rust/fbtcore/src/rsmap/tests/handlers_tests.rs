use super::*;

fn handlers(text: &str) -> Vec<Handler> {
    let file = syn::parse_file(text).unwrap();
    let found = read(&file, false);
    super::super::forget_spans();
    found
}

fn shapes(found: &[Handler]) -> Vec<&str> {
    found.iter().map(|h| h.shape).collect()
}

#[test]
fn a_match_arm_on_err_is_a_handler_with_what_it_binds_reads_and_raises() {
    let found = handlers("fn go() -> Result<(), E> {\n    match load() {\n        Ok(v) => use_it(v),\n        \
                          Err(e) => { log(&e); return Err(Wrap(e)); }\n    }\n}\n");
    assert_eq!(shapes(&found), ["match"]);
    let h = &found[0];
    assert_eq!((h.line, h.try_line, h.end_line, h.name.as_str(), h.bare), (4, 2, 4, "e", true));
    assert_eq!((h.name_read, h.raises, h.reraises, h.passes, h.func.as_str()), (true, 1, false, false, "go"));
    assert_eq!(h.calls, ["log", "Wrap"]);
}

#[test]
fn a_swallowed_error_passes_and_a_returned_one_is_the_same_error_going_on() {
    let found = handlers("fn a() { match f() { Err(Kind::Gone) => {}, Err(e) => return Err(e), _ => () } }\n");
    assert_eq!((found[0].types.clone(), found[0].bare, found[0].passes), (vec!["Kind::Gone".to_string()], false, true));
    assert_eq!((found[1].reraises, found[1].raises), (true, 0));
}

#[test]
fn every_other_shape_is_a_row_and_test_code_says_so() {
    let text = "impl Store { fn open(&self) -> Result<(), E> {\n    let Ok(f) = File::open(p) else { return Ok(()) };\n    \
                if let Err(e) = f.sync() { warn(e); }\n    let n = read(f)?;\n    let _ = flush();\n    \
                let v = n.parse::<u8>().unwrap_or_default();\n    let w = n.parse::<u8>().map_err(|e| Bad(e))?;\n    \
                let x = cfg().ok();\n    let r = std::panic::catch_unwind(|| run());\n    Ok(())\n} }\n\
                #[cfg(test)]\nmod tests { #[test] fn t() { go().unwrap(); go().expect(\"x\"); } }\n";
    let found = handlers(text);
    assert_eq!(shapes(&found), ["let_else", "if_let", "question", "discard", "fallback", "question", "map_err", "ok", "catch_unwind", "unwrap", "expect"]);
    assert_eq!((found[0].cls.as_str(), found[0].func.as_str(), found[0].test), ("Store", "open", false));
    assert_eq!((found[1].name.as_str(), found[1].name_read), ("e", true));
    assert!(found[2].reraises && found[3].passes && found[7].passes);
    assert_eq!(found[6].name, "e");
    assert!(found[9].panics && found[9].test && found[10].test);
}

#[test]
fn a_let_chain_an_nested_ok_err_arm_and_a_discarded_if_are_rows() {
    let text = "fn h(y: Option<u8>, z: R, empty: bool) -> Result<(), String> {\n    if let Some(v) = y && let Err(e) = check(v) {\n        drop(e);\n    }\n    \
                match z {\n        Ok(Err(e)) => return Err(e),\n        _ => {}\n    }\n    let _ = if empty { a() } else { b() };\n    Ok(())\n}\n";
    let found = handlers(text);
    assert_eq!(shapes(&found), ["if_let", "match", "discard"]);
}

#[test]
fn a_handler_inside_a_macro_argument_is_a_row() {
    let found = handlers("fn f() { let s = format!(\"{}\", x?); assert!(y.ok().is_some()); }\n");
    assert_eq!(shapes(&found), ["question", "ok"]);
}

#[test]
fn a_cfg_test_impl_is_test_code_and_a_plain_impl_is_not() {
    let found = handlers("pub struct T;\nimpl T { fn a(&self) { let _ = go(); } }\n#[cfg(test)]\nimpl T { fn b(&self) { let _ = go(); x.unwrap(); } }\n");
    let tests: Vec<(usize, bool)> = found.iter().map(|h| (h.line, h.test)).collect();
    assert_eq!(tests, [(2, false), (4, true), (4, true)]);
}
