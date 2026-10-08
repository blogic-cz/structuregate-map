//! WHAT A RESOLVED SELECTOR STYLES: its SUBJECT, the last compound - the element the declarations land on.
//!
//! The resolved selector is a string this pass spliced together, so it is PARSED again rather than taken apart
//! by hand: `.a .b--hidden` hides `.b--hidden`, not `.a`, and only the parser knows where the last compound
//! starts once an attribute selector holds a space or a `:not(...)` holds a class.

use super::sheet::squeeze;
use raffia::ast::{ComplexSelectorChild, InterpolableIdent, SelectorList, SimpleSelector};
use raffia::{Parser, Syntax};

/// The subject of one resolved selector.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Subject {
    /// The last compound as written - `.legend-item--hidden:hover`.
    pub text: String,
    /// The classes the compound itself requires - never one inside `:not(...)` or `:host(...)`, which do not
    /// put the class on the element.
    pub classes: Vec<String>,
    /// False when the compound styles a PSEUDO-ELEMENT (`::before`, and the legacy one-colon spellings): hiding
    /// `.x::after` leaves the element itself in view.
    pub on_element: bool,
}

/// The pseudo-elements CSS 2 wrote with one colon, which a parser reads as pseudo-classes.
const LEGACY_PSEUDO_ELEMENTS: &[&str] = &["before", "after", "first-line", "first-letter"];

/// The subject of `resolved`, or none when it does not parse as ONE selector - an interpolation that only a
/// compile could finish, say. None is the honest answer: a guessed class list would join the wrong gates.
pub fn subject(resolved: &str, syntax: Syntax) -> Option<Subject> {
    // THE INDENTED SYNTAX HAS NO SELECTOR GRAMMAR OF ITS OWN to speak of: a selector is read as SCSS reads it.
    let syntax = if matches!(syntax, Syntax::Sass) { Syntax::Scss } else { syntax };
    let mut parser = Parser::new(resolved, syntax);
    let list = parser.parse::<SelectorList>().ok()?;
    let [selector] = list.selectors.as_slice() else { return None };
    if selector.span.end < resolved.trim_end().len() {
        return None;
    }
    let compound = selector.children.iter().rev().find_map(|child| match child {
        ComplexSelectorChild::CompoundSelector(c) => Some(c),
        ComplexSelectorChild::Combinator(_) => None,
    })?;
    let mut out = Subject {
        text: squeeze(resolved.get(compound.span.start..compound.span.end).unwrap_or("")),
        classes: Vec::new(),
        on_element: true,
    };
    for simple in &compound.children {
        match simple {
            SimpleSelector::Class(class) => {
                if let InterpolableIdent::Literal(id) = &class.name {
                    out.classes.push(id.name.to_string());
                }
            }
            SimpleSelector::PseudoElement(_) => out.on_element = false,
            SimpleSelector::PseudoClass(pseudo) => {
                if let InterpolableIdent::Literal(id) = &pseudo.name
                    && LEGACY_PSEUDO_ELEMENTS.contains(&id.name.to_ascii_lowercase().as_str())
                {
                    out.on_element = false;
                }
            }
            _ => {}
        }
    }
    Some(out)
}
