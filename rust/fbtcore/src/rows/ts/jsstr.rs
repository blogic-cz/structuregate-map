//! JavaScript's idea of whitespace, and the collation the closure SORTS by.
//!
//! Both were measured rather than assumed in the python port, and both are the kind of
//! difference that is found late: they change the ORDER of rows, not their content, and
//! a gate comparing two columns in order then reports thousands of changed rows with no
//! changed fact.

use std::sync::LazyLock;

/// WHAT JAVASCRIPT CALLS WHITESPACE, spelled out by code point, because no runtime's
/// built-in trim agrees with it in both directions — and no two of them disagree the
/// same way:
///
/// | code point     | javascript | python `strip` | rust `trim` |
/// |----------------|------------|----------------|-------------|
/// | U+FEFF (BOM)   | strips     | leaves         | leaves      |
/// | U+001C..U+001F | leaves     | strips         | leaves      |
/// | U+0085 (NEL)   | leaves     | strips         | strips      |
///
/// The python port documented its own two disagreements, which are not rust's: rust
/// over-strips the byte order mark's opposite (NEL) and under-strips the mark itself.
/// The set is spelled out here for the same reason it was spelled out there.
///
/// A file that opens with a byte order mark and then `// Hi...` made the two ends
/// disagree about whether that line is a comment and the
/// tally came out one short.
///
/// This is the whole WhiteSpace + LineTerminator production of the language the passes
/// were ported from.
const JS_SPACE: &[char] = &[
    '\u{0009}', '\u{000A}', '\u{000B}', '\u{000C}', '\u{000D}', '\u{0020}', '\u{00A0}', '\u{1680}',
    '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}', '\u{2007}',
    '\u{2008}', '\u{2009}', '\u{200A}', '\u{2028}', '\u{2029}', '\u{202F}', '\u{205F}', '\u{3000}',
    '\u{FEFF}',
];

fn is_js_space(c: char) -> bool {
    JS_SPACE.contains(&c)
}

/// `String.prototype.trimStart`, exactly.
pub fn trim_start(text: &str) -> &str {
    text.trim_start_matches(is_js_space)
}

/// `String.prototype.trim`, exactly — the same set from both ends.
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

// ---------------------------------------------------------------------------------
// COLLATION — `String.prototype.localeCompare`, for the strings the closure sorts by.
//
// TWO OF THE FOLDS SORT WITH IT (`fold_values` and `unreadable_values` order their rows
// by `enum_name + dimension`), and those rows land in `key_reach` columns the 53-table
// gate compares IN ORDER. Get the comparator wrong and every row of both columns reads
// as changed while carrying no changed fact.
//
// IT IS NOT CODE-POINT ORDER, and that was measured. On the distinct
// `enum_name + dimension` strings a large workspace produces, the two orders
// diverge near the top: `Shapedetail...` sorts BEFORE
// `ShapeTypedetail...`, because collation compares base letters first
// and ignores case until the tertiary level, while `T` (0x54) precedes `d` (0x64) by
// code point.
//
// NEITHER IS IT THE OPERATING SYSTEM'S. A locale-driven comparator would make the map
// depend on the machine that ran it, which is exactly the property a map shipped into
// every consumer must not have.
//
// So the primary order is BAKED, read out of the runtime once for printable ASCII.
// BEYOND PRINTABLE ASCII IT IS AN APPROXIMATION and says so: an unknown character sorts
// after every known one, by code point.
// ---------------------------------------------------------------------------------

/// Printable ASCII in collation order, read out of the runtime. Case-folded, so `a`
/// covers `A`.
const ORDER: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789abcdefghijklmnopqrstuvwxyz";

static PRIMARY: LazyLock<std::collections::HashMap<char, usize>> = LazyLock::new(|| {
    ORDER.chars().enumerate().map(|(i, c)| (c, i)).collect()
});

/// An unknown character sorts after every known one; ties among unknowns go by code point.
fn unknown_weight() -> usize {
    ORDER.chars().count()
}

/// The key a sort needs to order like `localeCompare` does.
///
/// Two levels, which is what the divergence above is about: every character's PRIMARY
/// weight first, so case is invisible, then the case pattern as the tie-break with
/// lowercase before uppercase.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortKey {
    primary: Vec<(usize, u32)>,
    tertiary: Vec<u8>,
}

pub fn sort_key(text: &str) -> SortKey {
    let mut primary = Vec::with_capacity(text.len());
    let mut tertiary = Vec::with_capacity(text.len());
    for c in text.chars() {
        // A character whose lowercase form is more than one character has no single
        // weight to look up. Python's version would raise here; every string these folds
        // order is an enum name or a dotted property path, so it is unreachable — and
        // treating it as unknown is the answer that does not crash a build over it.
        let mut lower = c.to_lowercase();
        let lowered = match (lower.next(), lower.next()) {
            (Some(one), None) => one,
            (Some(first), Some(_)) => {
                primary.push((unknown_weight(), first as u32));
                tertiary.push(u8::from(c.is_uppercase()));
                continue;
            }
            _ => c,
        };
        match PRIMARY.get(&lowered) {
            Some(weight) => primary.push((*weight, 0)),
            None => primary.push((unknown_weight(), lowered as u32)),
        }
        tertiary.push(u8::from(c.is_uppercase()));
    }
    SortKey { primary, tertiary }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_order_mark_is_leading_whitespace_to_javascript() {
        // The measured case: a file opening with a BOM and then `// Hi...`. A trim that
        // leaves the mark in place does not see a comment, and the tally comes out one
        // short.
        assert_eq!(trim_start("\u{FEFF}// Hi"), "// Hi");
        // Rust's own trim does not strip it, which is why the set is spelled out.
        assert_eq!("\u{FEFF}// Hi".trim_start(), "\u{FEFF}// Hi");
    }

    #[test]
    fn the_separators_javascript_leaves_alone_are_left_alone() {
        // None of these is whitespace to `String.prototype.trimStart`, and each is
        // whitespace to at least one of the runtimes this has been ported through.
        for c in ['\u{001C}', '\u{001D}', '\u{001E}', '\u{001F}', '\u{0085}'] {
            let text = format!("{c}x");
            assert_eq!(trim_start(&text), text, "{c:?} must survive a javascript trim");
        }
        // Rust's own trim disagrees on exactly ONE of them, which is why the set is
        // written out rather than delegated to `char::is_whitespace`.
        assert_eq!("\u{0085}x".trim_start(), "x", "rust strips NEL");
        assert_eq!("\u{001C}x".trim_start(), "\u{001C}x", "rust leaves the separators");
    }

    #[test]
    fn trim_takes_the_same_set_from_both_ends() {
        assert_eq!(trim("\u{FEFF} x \u{2028}"), "x");
    }

    #[test]
    fn collation_is_not_code_point_order() {
        // THE MEASURED DIVERGENCE, near the top of a large workspace's strings.
        let a = "Shapedetail";
        let b = "ShapeTypedetail";
        assert!(sort_key(a) < sort_key(b), "collation compares base letters first");
        // By code point the order is the other way round: `T` (0x54) precedes `d` (0x64).
        assert!(b < a, "which is exactly what a naive sort would have done");
    }

    #[test]
    fn case_is_invisible_until_the_tie_break() {
        // Same letters, so the primary weights match and only the case pattern decides —
        // lowercase before uppercase.
        assert!(sort_key("abc") < sort_key("Abc"));
        assert!(sort_key("aBc") < sort_key("ABc"));
        // And a different base letter still wins over any case difference.
        assert!(sort_key("Abd") > sort_key("abc"));
    }

    #[test]
    fn punctuation_orders_by_the_baked_table_not_by_ascii() {
        // `_` sorts before `-`, which is the first place the operating system's own
        // collation disagreed with the runtime.
        assert!(sort_key("_") < sort_key("-"));
        assert!(sort_key("-") < sort_key("."));
        assert!(sort_key("$") < sort_key("0"));
        assert!(sort_key("9") < sort_key("a"));
    }

    #[test]
    fn an_unknown_character_sorts_after_every_known_one() {
        assert!(sort_key("z") < sort_key("\u{00E9}"), "e-acute is outside the baked table");
        // Ties among unknowns go by code point, so the order is at least reproducible.
        assert!(sort_key("\u{00E9}") < sort_key("\u{00FF}"));
    }

    #[test]
    fn sorting_a_list_orders_it_the_way_the_folds_expect() {
        let mut names = vec!["BetaType", "alpha", "Alpha", "beta", "_leading"];
        names.sort_by_key(|n| sort_key(n));
        assert_eq!(names, vec!["_leading", "alpha", "Alpha", "beta", "BetaType"]);
    }
}
