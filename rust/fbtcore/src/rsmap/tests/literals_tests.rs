use super::*;

fn literals(text: &str) -> Vec<Literal> {
    let file = syn::parse_file(text).unwrap();
    let found = read(&file, &LineStarts::new(text), false);
    super::super::forget_spans();
    found
}

fn row(l: &Literal) -> String {
    format!("{}:{}:{}:{}:{}", l.line, l.value, l.uses, l.callee, l.target)
}

#[test]
fn every_literal_says_what_it_is_doing() {
    let text = "/// doc text\nconst LIMIT: usize = 450;\nfn go(line: &str, n: i32) -> i32 {\n    \
                if n > 3 { return line.split(':').nth(2).map_or(-1, |s| s.len() as i32 * 60); }\n    \
                let parts: Vec<&str> = line.split(\",\").collect();\n    let x = parts[1];\n    \
                match n { 404 => 0, _ => n }\n}\n";
    let rows: Vec<String> = literals(text).iter().map(row).collect();
    assert_eq!(rows, [
        "2:450:declared::",
        "4:3:compare::n",
        "4:::argument:split:line",
        "4:2:argument:nth:line.split(':')",
        "4:-1:argument:map_or:line.split(':').nth(2)",
        "4:60:arith::",
        "5:,:argument:split:line",
        "6:1:index::parts",
        "7:404:compare::n",
        "7:0:other::",
    ]);
}

#[test]
fn a_compare_literal_names_the_other_operand_or_the_scrutinee() {
    let text = "fn f(depth: u32, k: &str) {\n    if depth > 60 {}\n    if \"a\" == k.trim() {}\n    \
                match depth { 1..=9 => {}, 404 => {}, _ => {} }\n    if let 7 = depth {}\n}\n";
    let rows: Vec<String> = literals(text).iter().map(row).collect();
    assert_eq!(rows, ["2:60:compare::depth", "3:a:compare::k.trim()", "4:1:compare::depth", "4:9:compare::depth",
                      "4:404:compare::depth", "5:7:compare::depth"]);
}

#[test]
fn a_macro_s_format_string_and_arguments_are_read_and_test_code_is_marked() {
    let found = literals("fn a() { println!(\"{} of {}\", 5, \"x\"); }\n#[test]\nfn t() { assert_eq!(f(), 7); }\n");
    let rows: Vec<String> = found.iter().map(row).collect();
    assert_eq!(rows, ["1:{} of {}:format::", "1:5:argument:println!:", "1:x:argument:println!:", "3:7:argument:assert_eq!:"]);
    assert_eq!((found[0].func.as_str(), found[0].test, found[3].test), ("a", false, true));
    assert_eq!(found[1].number, Some(5.0));
}

fn pieces(text: &str) -> Vec<String> {
    let mac: syn::Macro = syn::parse_str(text).unwrap();
    let out = body_pieces(&mac)
        .iter()
        .map(|p| match p {
            Piece::Expr(e) => format!("e:{}", quote::ToTokens::to_token_stream(e)),
            Piece::Lit(l) => format!("l:{}", quote::ToTokens::to_token_stream(l)),
            Piece::Pat(p) => format!("p:{}", quote::ToTokens::to_token_stream(p)),
        })
        .collect();
    super::super::forget_spans();
    out
}

#[test]
fn a_json_body_is_read_by_its_object_and_array_grammar() {
    assert_eq!(pieces("json!({\"a\": x.y(), \"b\": [1, \"t\"], \"c\": {\"d\": null}, \"e\": a::b})"),
        ["l:\"a\"", "e:x . y ()", "l:\"b\"", "e:1", "e:\"t\"", "l:\"c\"", "l:\"d\"", "e:null", "l:\"e\"", "e:a :: b"]);
    assert_eq!(pieces("json!([1, {\"k\": 2}])"), ["e:1", "l:\"k\"", "e:2"]);
    // A value that is no expression falls back to its literal tokens.
    assert_eq!(pieces("json!({\"k\": $x + 3})"), ["l:\"k\"", "l:3"]);
}

#[test]
fn vec_repeat_and_matches_bodies_are_read_and_any_other_by_its_literal_tokens() {
    assert_eq!(pieces("vec![0u8; n]"), ["e:0u8", "e:n"]);
    assert_eq!(pieces("matches!(x, Some(1) | None if y > 2)"), ["e:x", "p:Some (1) | None", "e:y > 2"]);
    assert_eq!(pieces("other! { a => \"s\", [4] }"), ["l:\"s\"", "l:4"]);
    assert_eq!(pieces("f!(a, b)"), ["e:a", "e:b"]);
}

#[test]
fn a_format_string_is_at_its_std_position() {
    let rows: Vec<String> = literals("fn a() { writeln!(out, \"w {}\", 1); assert_eq!(a, 2, \"m\"); vec![\"v\"]; }\n")
        .iter().map(row).collect();
    assert_eq!(rows, ["1:w {}:format::", "1:1:argument:writeln!:", "1:2:argument:assert_eq!:", "1:m:format::", "1:v:argument:vec!:"]);
}

#[test]
fn a_byte_string_is_a_string_literal_and_raw_bytes_are_escaped() {
    let found = literals("fn f(h: &mut H) { h.update(b\"link:\"); h.update(b\"\\xFF\\x00\"); let q = br#\"{\"a\":1}\"#; let s = b' '; }\n");
    let rows: Vec<String> = found.iter().map(row).collect();
    assert_eq!(rows, ["1:link::argument:update:h", "1:\\xff\\x00:argument:update:h", "1:{\"a\":1}:assign::"]);
}

#[test]
fn a_json_value_with_a_turbofish_comma_stays_one_expression() {
    assert_eq!(pieces("json!({\"k\": r.get::<_, String>(0)?, \"j\": 5})"), ["l:\"k\"", "e:r . get :: < _ , String > (0) ?", "l:\"j\"", "e:5"]);
    assert_eq!(pieces("json!({\"a\": [f::<A, B>(1), 2]})"), ["l:\"a\"", "e:f :: < A , B > (1)", "e:2"]);
    // A run that never parses is kept as its parts.
    assert_eq!(pieces("json!({\"k\": $x, \"j\": 5})"), ["l:\"k\"", "l:\"j\"", "e:5"]);
}

#[test]
fn literals_under_a_cfg_test_impl_trait_or_statement_are_test_code() {
    let found = literals("impl T { fn a(&self) { go(\"p\"); } }\n#[cfg(test)]\nimpl T { fn t(&self) { go(\"x\"); } }\n#[cfg(test)]\ntrait F { fn d(&self) { go(\"y\"); } }\nfn b() { #[cfg(test)] { go(\"z\"); } }\n");
    let tests: Vec<(String, bool)> = found.iter().map(|l| (l.value.clone(), l.test)).collect();
    assert_eq!(tests, [("p".to_string(), false), ("x".to_string(), true), ("y".to_string(), true), ("z".to_string(), true)]);
}

#[test]
fn literals_under_a_cfg_test_trait_item_arm_or_field_are_test_code() {
    let found = literals("pub trait Tr {\n#[cfg(test)] const Q: u32 = 55;\n#[cfg(test)] fn t(&self) { go(\"tfn\"); }\nfn p(&self) { go(\"keep\"); }\n}\nfn g(v: u32) -> u32 { match v {\n#[cfg(test)] 77 => 1,\n_ => 0 } }\npub struct F { #[cfg(test)] pub f: [u8; 88], pub g: [u8; 9] }\n");
    let tests: Vec<(String, bool)> = found.iter().map(|l| (l.value.clone(), l.test)).collect();
    let want = [("55", true), ("tfn", true), ("keep", false), ("77", true), ("1", true), ("0", false), ("88", true), ("9", false)];
    assert_eq!(tests, want.map(|(v, t)| (v.to_string(), t)));
}
