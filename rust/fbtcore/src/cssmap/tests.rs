//! The stylesheet reader against the shapes it has to read: SCSS nesting, `@media`, a mixin, a parse error.

use super::select::subject;
use super::sheet::{read, Decl};
use raffia::Syntax;

fn rows(text: &str, syntax: Syntax) -> Vec<Decl> {
    let sheet = read(text, syntax);
    assert!(sheet.error.is_none(), "{:?}", sheet.error);
    sheet.decls
}

fn line_of(d: &Decl) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        d.line, d.selector, d.resolved, d.media.as_deref().unwrap_or("-"), d.context.as_deref().unwrap_or("-"),
        d.property, d.value, u8::from(d.important), u8::from(d.hides)
    )
}

#[test]
fn a_nested_selector_is_resolved_against_its_parent_list() {
    let text = "// a comment only SCSS allows\n$gap: 4px;\n.legend-item, .chip {\n  margin: $gap;\n  \
                &--hidden { display: none !important; }\n  .label { color: red; }\n}\n";
    let got: Vec<String> = rows(text, Syntax::Scss).iter().map(line_of).collect();
    assert_eq!(
        got,
        vec![
            "4|.legend-item|.legend-item|-|-|margin|$gap|0|0",
            "4|.chip|.chip|-|-|margin|$gap|0|0",
            "5|&--hidden|.legend-item--hidden|-|-|display|none|1|1",
            "5|&--hidden|.chip--hidden|-|-|display|none|1|1",
            "6|.label|.legend-item .label|-|-|color|red|0|0",
            "6|.label|.chip .label|-|-|color|red|0|0",
        ]
    );
}

#[test]
fn a_media_query_and_an_include_wrap_what_they_hold() {
    let text = "@media (max-width: 768px) {\n  .hide-mobile { display: none; }\n}\n\
                .card { @media print { visibility: hidden; } }\n\
                @include down(sm) { .x { display: none; } }\n";
    let got: Vec<String> = rows(text, Syntax::Scss).iter().map(line_of).collect();
    assert_eq!(
        got,
        vec![
            "2|.hide-mobile|.hide-mobile|(max-width: 768px)|-|display|none|0|1",
            "4|.card|.card|print|-|visibility|hidden|0|1",
            "5|.x|.x|-|@include down(sm)|display|none|0|1",
        ]
    );
}

#[test]
fn a_mixin_body_and_a_placeholder_are_not_live() {
    let text = "@mixin hidden { .m { display: none; } }\n%ph { display: none; }\n.real { display: none; }\n";
    let live: Vec<(String, bool)> = rows(text, Syntax::Scss).iter().map(|d| (d.resolved.clone(), d.live)).collect();
    assert_eq!(live, vec![(".m".into(), false), ("%ph".into(), false), (".real".into(), true)]);
}

#[test]
fn a_value_the_sheet_does_not_spell_out_is_not_a_hide() {
    let text = ".a { display: $none; }\n.b { display: flex; }\n.c { visibility: collapse; }\n";
    let hides: Vec<bool> = rows(text, Syntax::Scss).iter().map(|d| d.hides).collect();
    assert_eq!(hides, vec![false, false, true]);
}

#[test]
fn plain_css_less_and_the_indented_syntax_are_read_too() {
    assert_eq!(rows(".a > .b { display: none }", Syntax::Css)[0].resolved, ".a > .b");
    assert_eq!(rows(".a { &-b { display: none; } }", Syntax::Less)[0].resolved, ".a-b");
    assert_eq!(rows(".a\n  &-b\n    display: none\n", Syntax::Sass)[0].resolved, ".a-b");
}

#[test]
fn a_sheet_that_does_not_parse_says_where_and_has_no_rows() {
    let sheet = read(".ok { display: none }\n.bad { color: red; ]\n", Syntax::Css);
    let (message, line) = sheet.error.expect("an error");
    assert!(!message.is_empty());
    assert_eq!(line, 2);
    assert!(sheet.decls.is_empty());
}

#[test]
fn the_subject_is_the_last_compound_and_a_pseudo_element_is_not_the_element() {
    let s = subject(".a .b--hidden.c:hover", Syntax::Scss).expect("parses");
    assert_eq!((s.text.as_str(), s.classes.clone(), s.on_element), (".b--hidden.c:hover", vec!["b--hidden".to_string(), "c".to_string()], true));
    assert!(!subject(".x::after", Syntax::Css).expect("parses").on_element);
    assert!(!subject(".x:before", Syntax::Css).expect("parses").on_element);
    assert_eq!(subject(".y:not(.x)", Syntax::Css).expect("parses").classes, vec!["y".to_string()]);
}

#[test]
fn a_hiding_rule_meets_an_ng_class_and_its_own_sheet_through_their_nodes() {
    // `components.style_urls` is a list and `expressions.ast` an object: both NODES, the way the store writes them.
    use serde_json::json;
    let db = rusqlite::Connection::open_in_memory().unwrap();
    let put = |table: &str, rows: Vec<serde_json::Value>| crate::rows::store::insert_rows(&db, table, &rows).unwrap();
    put("files", vec![
        json!({"id": "f:1", "path": "app/card.component.ts", "half": "typescript"}),
        json!({"id": "f:2", "path": "app/card.component.scss", "half": "typescript"}),
    ]);
    put("components", vec![json!({"id": "cmp:1", "file": "f:1", "half": "typescript", "style_urls": ["./card.component.scss"]})]);
    put("expressions", vec![json!({"id": "e:1", "template": "t:1", "node": "n:1", "component": "cmp:1", "half": "typescript",
        "name": "ngClass", "source": "{gone: off}",
        "ast": {"k": "Source", "ast": {"k": "Map", "keys": [{"key": "gone"}], "values": [{"k": "Read", "name": "off"}]}}})]);
    let rule = json!({"id": "css:1", "file": "f:2", "hides": 1, "live": 1, "on_element": 1, "classes": ["gone"],
                      "resolved": ".gone", "property": "display", "value": "none"});
    let joined = super::hides::join(&db, "typescript", &[rule]);
    assert_eq!(joined.len(), 1);
    assert_eq!((joined[0]["via"].clone(), joined[0]["scope"].clone()), (json!("ngClass"), json!("own")));
}

/// `resolved|property` of every declaration that HIDES.
fn hiding(text: &str) -> Vec<String> {
    rows(text, Syntax::Scss).iter().filter(|d| d.hides).map(|d| format!("{}|{}", d.resolved, d.property)).collect()
}

#[test]
fn a_rule_hides_when_its_zero_has_its_partner() {
    let text = ".c { height: 0; overflow: hidden; }\n.m { max-height: 0px; overflow: auto clip; }\n\
                .y { height: 0; overflow-y: hidden; }\n.f { opacity: 0; pointer-events: none; }\n\
                .p { opacity: 0%; pointer-events: none; padding: 4px; }\n\
                .v { padding: 0 8px; height: 0; overflow: hidden; display: block; min-height: auto; }\n";
    assert_eq!(hiding(text), vec![".c|height", ".m|max-height", ".y|height", ".f|opacity", ".p|opacity", ".v|height"]);
}

#[test]
fn a_zero_without_its_partner_or_with_a_box_left_does_not_hide() {
    let text = ".a { height: 0; }\n.b { opacity: 0; }\n.c { overflow: hidden; pointer-events: none; }\n\
                .d { height: $h; overflow: hidden; }\n.e { height: 0%; overflow: hidden; }\n\
                .f { height: 0; overflow: hidden; padding: 4px 0; }\n.g { height: 0; overflow: hidden; min-height: 2px; }\n\
                .h { height: 0; overflow: hidden; display: inline; }\n.i { height: 0; overflow: $o; }\n\
                .j { height: 0; overflow: hidden; overflow: visible; }\n.k { height: calc(0px); overflow: hidden; }\n\
                .l { opacity: 0.01; pointer-events: none; }\n.m { height: 0; overflow: hidden visible; }\n";
    assert_eq!(hiding(text), Vec::<String>::new());
}

#[test]
fn the_cascade_inside_a_rule_and_important_decide_the_partner() {
    let text = ".a { overflow: hidden !important; height: 0; overflow: visible; }\n\
                .b { height: 0 !important; height: 10px; overflow: hidden; }\n\
                .c { overflow: visible; height: 0; overflow: hidden; }\n";
    assert_eq!(hiding(text), vec![".a|height", ".b|height", ".c|height"]);
}

#[test]
fn a_nested_rule_of_the_same_element_inherits_the_partner_and_nothing_else_does() {
    let text = ".panel { overflow: hidden; padding: 0;\n  &.closed { max-height: 0; }\n  &--gone { height: 0; }\n  \
                .x { height: 0; }\n  &::before { height: 0; }\n  @media print { height: 0; }\n  \
                &.open { height: 0; overflow: visible; }\n}\n\
                .card { overflow: hidden !important; &.shut { height: 0; overflow: visible; } }\n\
                .pad { overflow: hidden; padding: 6px; &.shut { height: 0; padding-top: 0; } }\n\
                .fade { pointer-events: none; &:not(.on) { opacity: 0; } }\n";
    assert_eq!(hiding(text), vec![".panel.closed|max-height", ".card.shut|height", ".fade:not(.on)|opacity"]);
}

#[test]
fn a_custom_property_marked_important_is_important_and_its_value_is_the_value_alone() {
    let text = ".a { --gap: 20px !important; --tone: red; color: blue !important; --pad:4px ! important; }\n";
    let got: Vec<String> = rows(text, Syntax::Css).iter().map(|d| format!("{}|{}|{}", d.property, d.value, u8::from(d.important))).collect();
    assert_eq!(got, vec!["--gap|20px|1", "--tone|red|0", "color|blue|1", "--pad|4px|1"]);
}

#[test]
fn an_ng_class_array_and_string_read_every_arm() {
    use serde_json::json;
    let lit = |v: &str| json!({"k": "Literal", "v": v});
    let cond = |a: serde_json::Value, b: serde_json::Value| json!({"k": "Cond", "cond": {"k": "Read", "name": "c"}, "then": a, "else": b});
    let read = |ast: serde_json::Value| {
        let mut out = Vec::new();
        super::hides::ng_classes(&json!({"k": "Source", "ast": ast}), &mut out);
        out
    };
    let array = json!({"k": "Array", "items": [cond(lit("btn"), lit("x")), cond(lit("hide-mobile"), lit(""))]});
    assert_eq!(read(array), vec!["btn", "x", "hide-mobile"]);
    assert_eq!(read(cond(lit("a b"), lit("c"))), vec!["a", "b", "c"]);
    assert_eq!(read(lit("p q")), vec!["p", "q"]);
}
