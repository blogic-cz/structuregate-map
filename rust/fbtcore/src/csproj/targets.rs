//! THE `<Reference>` ITEMS A PACKAGE'S OWN BUILD FILE ADDS - read as XML, never evaluated by MSBuild. A package
//! can put an assembly on the compiler's command line from `build/<tfm>/*.targets` instead of listing it as a
//! `compile` asset: MSTest.TestFramework 3.7 does that for the assembly where `TestContext` lives, and on a
//! restored, never-built tree `TestContext.CancellationTokenSource` read as a member that does not exist.
//!
//! A SUBSET, NOT AN EVALUATOR. Properties come from the file's own `PropertyGroup`s and
//! `$(MSBuildThisFileDirectory)`; a condition is honoured when it is a plain `'a' == 'b'` / `'a' != 'b'` /
//! `Exists('path')` joined by `and` / `or`. Anything else makes the condition FALSE and the item is skipped:
//! a reference this pass cannot justify is not guessed onto the command line.

use super::disk::{self, combine};
use std::collections::HashMap;
use std::path::Path;

pub fn references(file: &str) -> Vec<String> {
    let Some(text) = disk::text(file) else { return Vec::new() };
    let Some(document) = disk::parse(&text) else { return Vec::new() };
    let folder = disk::parent(file).unwrap_or_default();
    let mut properties = Properties::default();
    properties.set("MSBuildThisFileDirectory", format!("{folder}{}", std::path::MAIN_SEPARATOR));
    properties.set("MSBuildThisFile", disk::file_name(file));
    let mut found = Vec::new();
    for element in document.root_element().children().filter(|n| n.is_element()) {
        if !holds(element, &properties) {
            continue;
        }
        match element.tag_name().name() {
            "PropertyGroup" => {
                for property in element.children().filter(|n| n.is_element()) {
                    if holds(property, &properties) {
                        let value = expand(disk::value(property).trim(), &properties);
                        properties.set(property.tag_name().name(), value);
                    }
                }
            }
            "ItemGroup" => {
                for item in element.children().filter(|n| n.is_element() && n.tag_name().name() == "Reference") {
                    if !holds(item, &properties) {
                        continue;
                    }
                    let Some(hint) = disk::child(item, "HintPath").map(|h| disk::value(h).trim().to_string()).filter(|h| !h.is_empty()) else {
                        continue;
                    };
                    let path = expand(&hint, &properties);
                    if path.contains("$(") {
                        continue;
                    }
                    let full = disk::full(&if disk::rooted(&path) { path.clone() } else { combine(&[&folder, &path]) });
                    if disk::is_file(&full) {
                        found.push(full);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// The file's properties, their names case-insensitive as MSBuild's are.
#[derive(Default)]
struct Properties(HashMap<String, String>);

impl Properties {
    fn set(&mut self, name: &str, value: String) {
        self.0.insert(name.to_ascii_lowercase(), value);
    }
    fn get(&self, name: &str) -> &str {
        self.0.get(&name.to_ascii_lowercase()).map_or("", String::as_str)
    }
}

/// Whether an element's `Condition` holds; no condition holds.
fn holds(element: roxmltree::Node, properties: &Properties) -> bool {
    match element.attribute("Condition") {
        None => true,
        Some(condition) if condition.trim().is_empty() => true,
        Some(condition) => evaluate(condition, properties),
    }
}

fn evaluate(condition: &str, properties: &Properties) -> bool {
    let text = condition.trim();
    let ors = split(text, " or ");
    if ors.len() > 1 {
        return ors.iter().any(|part| evaluate(part, properties));
    }
    let ands = split(text, " and ");
    if ands.len() > 1 {
        return ands.iter().all(|part| evaluate(part, properties));
    }
    if text.starts_with('(') && text.ends_with(')') && text.len() >= 2 {
        return evaluate(&text[1..text.len() - 1], properties);
    }
    if text.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("Exists(")) && text.ends_with(')') {
        let path = expand(unquote(text[7..text.len() - 1].trim()), properties);
        return !path.contains("$(") && Path::new(&path).exists();
    }
    for (op, equal) in [("!=", false), ("==", true)] {
        let Some(at) = text.find(op) else { continue };
        let left = expand(unquote(text[..at].trim()), properties);
        let right = expand(unquote(text[at + 2..].trim()), properties);
        // A property function or an unknown construct left unexpanded is a condition this pass cannot judge.
        if [&left, &right].iter().any(|side| side.contains("$(") || side.contains("@(")) {
            return false;
        }
        return left.to_lowercase() == right.to_lowercase() && equal || left.to_lowercase() != right.to_lowercase() && !equal;
    }
    false
}

/// `text` cut at every `separator` outside quotes, case-insensitively.
fn split<'a>(text: &'a str, separator: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let (mut start, mut quoted, mut i) = (0, false, 0);
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            quoted = !quoted;
        }
        if !quoted && text.get(i..i + separator.len()).is_some_and(|s| s.eq_ignore_ascii_case(separator)) {
            parts.push(&text[start..i]);
            start = i + separator.len();
            i = start;
            continue;
        }
        i += 1;
    }
    parts.push(&text[start..]);
    parts
}

fn unquote(value: &str) -> &str {
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') { &value[1..value.len() - 1] } else { value }
}

/// `$(Name)` replaced by what the file defined, "" for a plain name it did not; a property FUNCTION
/// (`$([...])`) is left as written, which every caller treats as "cannot say".
fn expand(value: &str, properties: &Properties) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(at) = rest.find("$(") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        match after.find(')') {
            Some(end) if end > 0 && after[..end].chars().all(|c| c.is_alphanumeric() || c == '_') => {
                out.push_str(properties.get(&after[..end]));
                rest = &after[end + 1..];
            }
            _ => {
                out.push_str("$(");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_condition_it_cannot_judge_is_false() {
        let mut properties = Properties::default();
        properties.set("Tfm", "net8.0".into());
        assert!(evaluate("'$(Tfm)' == 'NET8.0'", &properties));
        assert!(evaluate("'$(Tfm)' != 'net9.0' and '$(Tfm)' == 'net8.0'", &properties));
        assert!(!evaluate("'$([MSBuild]::Foo())' == 'x'", &properties));
        assert_eq!(expand("a$(Tfm)b$(Missing)c", &properties), "anet8.0bc");
    }
}
