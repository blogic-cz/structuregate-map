//! The source scanners, pinned on a corpus of SYNTHETIC source lines.
//!
//! A REGRESSION PIN, no longer a differential. This used to hold the scanners against the
//! python pass they replaced, on lines sampled out of real code with python's answers
//! recorded once. That fixture was not one to publish, so it was replaced
//! by ~480 hand-written TypeScript/HTML lines - template strings, comments, dotted and
//! braced names, quotes and escapes, accented text, and the JavaScript whitespace a trim
//! must know (no-break space, BOM, line separator) - each in a few indentations, with the
//! answers captured ONCE from these same scanners while they still agreed with that
//! differential. A change that moves any answer fails here; a deliberate one re-records
//! the fixture.
//!
//! THE FIXTURE IS COMMITTED: it runs with no workspace, no python, and nothing else.

use serde_json::Value;

/// `fbtcore` builds as a staticlib/cdylib, so an integration test cannot link the crate
/// as a library. The scanners are included directly instead — the module has no state and
/// no dependency beyond `jsstr`, which comes with it.
#[path = "../src/rows/ts/jsstr.rs"]
mod jsstr;
#[path = "../src/rows/ts/sourcescan.rs"]
mod sourcescan;

fn cases() -> Vec<Value> {
    let text = include_str!("fixtures/sourcescan.json");
    serde_json::from_str(text).expect("the fixture is valid json")
}

fn strings(row: &Value, field: &str) -> Vec<String> {
    row[field]
        .as_array()
        .expect("a list field")
        .iter()
        .map(|v| v.as_str().expect("a string").to_string())
        .collect()
}

#[test]
fn every_scanner_answers_what_the_fixture_recorded() {
    let cases = cases();
    assert!(cases.len() > 400, "the fixture is too small to prove anything");

    let mut checked = 0usize;
    for row in &cases {
        let line = row["line"].as_str().expect("a line");

        assert_eq!(
            sourcescan::words(line),
            strings(row, "words"),
            "words disagreed on {line:?}"
        );
        assert_eq!(
            sourcescan::dotted_names(line),
            strings(row, "dotted"),
            "dotted_names disagreed on {line:?}"
        );
        assert_eq!(
            sourcescan::braced_names(line),
            strings(row, "braced"),
            "braced_names disagreed on {line:?}"
        );
        assert_eq!(
            jsstr::trim_start(line),
            row["trim_start"].as_str().expect("trim_start"),
            "trim_start disagreed on {line:?}"
        );
        assert_eq!(
            jsstr::trim(line),
            row["trim"].as_str().expect("trim"),
            "trim disagreed on {line:?}"
        );
        checked += 1;
    }
    eprintln!("{checked} source line(s) agreed with the recorded answers");
}

#[test]
fn the_fixture_covers_the_cases_that_matter() {
    // A corpus that happened to contain no brace marker would pass the test above while
    // proving nothing about the one scanner a translation key depends on.
    let cases = cases();
    let braced = cases.iter().filter(|c| !c["braced"].as_array().unwrap().is_empty()).count();
    let dotted = cases.iter().filter(|c| !c["dotted"].as_array().unwrap().is_empty()).count();
    let non_ascii = cases
        .iter()
        .filter(|c| !c["line"].as_str().unwrap().is_ascii())
        .count();
    let trimmed = cases
        .iter()
        .filter(|c| c["line"].as_str().unwrap() != c["trim"].as_str().unwrap())
        .count();

    assert!(braced > 10, "no line spells a brace marker");
    assert!(dotted > 200, "too few dotted names to be meaningful: {dotted}");
    assert!(non_ascii > 100, "no accented source, so the ascii sets are untested: {non_ascii}");
    assert!(trimmed > 200, "nothing needed trimming: {trimmed}");
    eprintln!("braced={braced} dotted={dotted} non_ascii={non_ascii} trimmed={trimmed}");
}
