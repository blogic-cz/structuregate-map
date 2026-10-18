//! A RULE THAT HIDES ITS ELEMENT WITH NO SINGLE DECLARATION SAYING SO (#89) - decided over the WHOLE rule.
//!
//! `display: none` hides an element alone, so `sheet.rs` decides it per declaration. Two idioms hide it only
//! together with a partner, and so are decided over every declaration of one rule (one block, under one
//! selector and one set of at-rules):
//!
//! - THE ZERO-HEIGHT COLLAPSE: `height: 0` or `max-height: 0` (a literal zero, bare or a length - never a
//!   percentage, which computes to `auto` inside a parent of no fixed height) WITH `overflow: hidden|clip` (or
//!   `overflow-y`, or a two-value `overflow` whose block axis is one of them). Without the overflow the content
//!   spills out and stays visible. Refused when the same rule says the box still shows something: a
//!   `min-height` other than zero or `auto` (it wins over both), a vertical `padding` that is not zero (the
//!   content shows through the padding box), or a `display` of `inline` (no height applies) or `contents` (no
//!   box at all).
//! - FULLY TRANSPARENT AND INERT: `opacity: 0` (or `0%`) WITH `pointer-events: none`. Transparent alone is
//!   still clicked. Keyboard focus and a screen reader still reach it - this is "out of view", as the issue asks.
//!
//! `hides` lands on the ZERO declaration (`height`/`max-height`/`opacity`) - the one that takes the element out of
//! view, so `class_hides` names it as it names `display: none` - never on the partner, so one rule gives one
//! `class_hides` row per binding (two when it zeroes both `height` and `max-height`).
//!
//! ONLY WHAT IS WRITTEN: a `$variable`, a Less `@variable`, a function (`calc(0px)`) or an interpolated property
//! name is no literal, so a zero or a partner written so does not count, and a disqualifier written so DOES
//! refuse. Inside a rule the CASCADE is kept: a later declaration of a property replaces an earlier one unless the
//! earlier is `!important` and the later is not. A rule nested as `&` plus more of the SAME element (`&.closed`,
//! `&:not(.open)`, `&[hidden]` - never `&--x`, `& .x` or `&::before`) inherits the partners and the disqualifiers
//! of the rule around it, since the element it matches matches that rule too; the nested rule's own declarations
//! win unless the outer one is `!important` and its own is not, and a vertical padding refuses from either.
//! Nothing else is joined: two separate rules (`.panel` and `.panel.closed` written apart), a rule inside a
//! `@media` of a rule, a partner from a `@mixin`, `@extend` or another sheet. The rows keep `media`/`context` as
//! for any declaration.

use raffia::ast::{
    ComplexSelectorChild, ComponentValue, Declaration, DimensionKind, InterpolableIdent, SelectorList, SimpleSelector,
    Statement,
};

/// What one property family says, as far as hiding goes.
#[derive(Clone, Copy, Debug)]
struct Seen {
    /// The value is the one the idiom needs (a zero, `hidden`, `none`) - or, for a disqualifier, harmless.
    ok: bool,
    important: bool,
    /// The statement index of the deciding declaration when it is THIS rule's own.
    own: Option<usize>,
}

const HEIGHT: usize = 0;
const MAX_HEIGHT: usize = 1;
const OVERFLOW_Y: usize = 2;
const MIN_HEIGHT: usize = 3;
const PADDING: usize = 4;
const DISPLAY: usize = 5;
const OPACITY: usize = 6;
const POINTER: usize = 7;
const FAMILIES: usize = 8;

/// Every family's deciding declaration, for one rule and whatever it inherits.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    seen: [Option<Seen>; FAMILIES],
}

fn zero(value: &ComponentValue) -> bool {
    match value {
        ComponentValue::Number(n) => n.value == 0.0,
        ComponentValue::Dimension(d) => matches!(d.kind, DimensionKind::Length) && d.value.value == 0.0,
        _ => false,
    }
}

fn word(value: &ComponentValue) -> Option<String> {
    match value {
        ComponentValue::InterpolableIdent(InterpolableIdent::Literal(id)) => Some(id.name.to_ascii_lowercase()),
        _ => None,
    }
}

fn clipped(value: &ComponentValue) -> bool {
    matches!(word(value).as_deref(), Some("hidden" | "clip"))
}

/// The families a declaration speaks for and whether it says what hiding needs, or nothing when it is none of them.
fn family(decl: &Declaration) -> Option<(usize, bool)> {
    let InterpolableIdent::Literal(name) = &decl.name else { return None };
    let values: Vec<&ComponentValue> =
        decl.value.iter().filter(|v| !matches!(v, ComponentValue::ImportantAnnotation(_))).collect();
    let one = |test: &dyn Fn(&ComponentValue) -> bool| matches!(values.as_slice(), [v] if test(v));
    let property = name.name.to_ascii_lowercase();
    Some(match property.as_str() {
        "height" => (HEIGHT, one(&zero)),
        "max-height" => (MAX_HEIGHT, one(&zero)),
        // `overflow: <x> <y>` - the block axis is the second value, and the only one when there is one.
        "overflow" => (OVERFLOW_Y, matches!(values.as_slice(), [v] | [_, v] if clipped(v))),
        "overflow-y" => (OVERFLOW_Y, one(&clipped)),
        "min-height" => (MIN_HEIGHT, one(&|v| zero(v) || word(v).as_deref() == Some("auto"))),
        // `padding: <top> [<right> [<bottom> [<left>]]]` - the vertical sides are the first and the third (or the first).
        "padding" => (PADDING, match values.as_slice() {
            [v] | [v, _] => zero(v),
            [t, _, b] | [t, _, b, _] => zero(t) && zero(b),
            _ => false,
        }),
        "padding-top" | "padding-bottom" | "padding-block-start" | "padding-block-end" => (PADDING, one(&zero)),
        "padding-block" => (PADDING, !values.is_empty() && values.len() <= 2 && values.iter().all(|v| zero(v))),
        "display" => (DISPLAY, one(&|v| word(v).is_some_and(|w| w != "inline" && w != "contents"))),
        "opacity" => (OPACITY, one(&|v| zero(v) || matches!(v, ComponentValue::Percentage(p) if p.value.value == 0.0))),
        "pointer-events" => (POINTER, one(&|v| word(v).as_deref() == Some("none"))),
        _ => return None,
    })
}

/// The later of two declarations of one family, by the cascade inside a rule - or across a nesting, where the
/// inner rule is the later one. A vertical padding refuses from wherever it is written.
fn later(earlier: Option<Seen>, next: Seen, index: usize) -> Seen {
    match earlier {
        Some(e) if index == PADDING => Seen { ok: e.ok && next.ok, ..next },
        Some(e) if e.important && !next.important => e,
        _ => next,
    }
}

impl Facts {
    /// One rule's statements over what the rule around it holds: the facts its `&`-nested rules inherit, and the
    /// statement index of every declaration that HIDES because of the rule as a whole.
    pub fn of(statements: &[Statement], outer: &Facts) -> (Facts, Vec<usize>) {
        let mut own: [Option<Seen>; FAMILIES] = [None; FAMILIES];
        for (at, statement) in statements.iter().enumerate() {
            let Statement::Declaration(decl) = statement else { continue };
            let Some((index, ok)) = family(decl) else { continue };
            let important = decl.important.is_some()
                || decl.value.iter().any(|v| matches!(v, ComponentValue::ImportantAnnotation(_)));
            own[index] = Some(later(own[index], Seen { ok, important, own: Some(at) }, index));
        }
        let mut merged = Facts::default();
        for index in 0..FAMILIES {
            let outer_seen = outer.seen[index].map(|s| Seen { own: None, ..s });
            merged.seen[index] = match own[index] {
                Some(mine) => Some(later(outer_seen, mine, index)),
                None => outer_seen,
            };
        }
        let ok = |i: usize| merged.seen[i].is_some_and(|s| s.ok);
        let harmless = |i: usize| merged.seen[i].is_none_or(|s| s.ok);
        let decider = |i: usize| merged.seen[i].filter(|s| s.ok).and_then(|s| s.own);
        let mut hides = Vec::new();
        if ok(OVERFLOW_Y) && harmless(MIN_HEIGHT) && harmless(PADDING) && harmless(DISPLAY) {
            hides.extend([HEIGHT, MAX_HEIGHT].into_iter().filter_map(decider));
        }
        if ok(POINTER) {
            hides.extend(decider(OPACITY));
        }
        hides.sort_unstable();
        for seen in merged.seen.iter_mut().flatten() {
            seen.own = None;
        }
        (merged, hides)
    }
}

/// Does every selector of a nested rule match the SAME element as the rule around it - `&` alone at its start, then
/// only classes, ids, attributes and pseudo-classes?
pub fn same_element(list: &SelectorList) -> bool {
    list.selectors.iter().all(|selector| match selector.children.as_slice() {
        [ComplexSelectorChild::CompoundSelector(compound)] => match compound.children.split_first() {
            Some((SimpleSelector::Nesting(nesting), rest)) => {
                nesting.suffix.is_none()
                    && rest.iter().all(|s| {
                        matches!(
                            s,
                            SimpleSelector::Class(_) | SimpleSelector::Id(_) | SimpleSelector::Attribute(_)
                                | SimpleSelector::PseudoClass(_)
                        )
                    })
            }
            _ => false,
        },
        _ => false,
    })
}
