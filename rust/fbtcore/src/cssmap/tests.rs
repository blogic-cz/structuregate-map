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
