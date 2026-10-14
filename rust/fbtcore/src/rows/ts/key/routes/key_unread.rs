//! A COMPONENT FIELD NOTHING READS IS NO SITE OF THE KEY ITS INITIALIZER HOLDS (#54, divergence #44).
//!
//! A bare literal inside a component's class is that component's `literal` site (#31): the file says the key, and
//! nothing proves it is not used as one. When the literal is the initializer of a FIELD that nothing can read, the
//! file does prove it: the value never leaves the field. Such a site was an ungated way that emptied the key's
//! `always_*` - `labelCode = 'k'` declared beside a template binding only `otherCode`.
//!
//! "Nothing can read it" is proven by NAME, which only over-counts reads, never under-counts them. A field is
//! unread when no `expressions` row (a template's or the class's own) spells its name as an identifier, no file
//! reads a property of that name (`files.prop_reads`, `src/TsRows/TsDecls/TsPropReads.mjs`: `x.name`, a
//! destructuring, a shorthand), no string literal equals it (`x['name']`, a host binding), the member carries no
//! decorator (`@Input`, `@HostBinding` read it for the framework), and its file reads no instance of its own by a
//! name no source spells (`files.this_indexed`: `this[k]`, `...this`, `Object.values(this)`).
//! Not read, so never dropped: a write in a method (`this.f = 'k'`); only an initializer is the field's whole value.

use super::store::{Row, Store};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Every identifier a text spells, of ANY length - `sourcescan::words` keeps three characters or more, and a field
/// named `id` read in a template would then read as unread.
fn identifiers(line: &str, out: &mut HashSet<String>) {
    let bytes = line.as_bytes();
    let start = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b == b'$';
    let cont = |b: u8| start(b) || b.is_ascii_digit();
    let mut i = 0usize;
    while i < bytes.len() {
        if !cont(bytes[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < bytes.len() && cont(bytes[j]) {
            j += 1;
        }
        if start(bytes[i]) {
            out.insert(line[i..j].to_string());
        }
        i = j;
    }
}

/// What the expressions read: a member the checker resolved a read to (`target.row`, one declaration), and every
/// name read without one - a receiver of unknown type, a pipe, a read of several declarations.
#[derive(Default)]
struct Reads {
    rows: HashSet<String>,
    names: HashSet<String>,
}

impl Reads {
    fn walk(&mut self, v: &Value) {
        match v {
            Value::Object(map) => {
                // A TEMPLATE THE COMPILER COULD NOT PARSE keeps its text alone: every identifier in it is a read.
                if map.get("k").and_then(Value::as_str) == Some("Source") && !matches!(map.get("ast"), Some(Value::Object(_))) {
                    identifiers(map.get("src").and_then(Value::as_str).unwrap_or_default(), &mut self.names);
                }
                if let Some(name) = map.get("name").and_then(Value::as_str) {
                    let target = map.get("target").and_then(Value::as_object);
                    let row = target.filter(|t| t.get("also").is_none()).and_then(|t| t.get("row")).and_then(Value::as_str);
                    match row {
                        Some(row) => {
                            self.rows.insert(row.to_string());
                        }
                        None => {
                            self.names.insert(name.to_string());
                        }
                    }
                }
                for (k, child) in map {
                    if k != "target" {
                        self.walk(child);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|i| self.walk(i)),
            _ => {}
        }
    }
}

/// EVERY WAY A MEMBER CAN BE READ, by what proves it: a template read resolved to it, a name read unresolved, a
/// string equal to its name, each file's property reads by name and count, and the files that hand `this` away.
pub struct MemberReads {
    template: Reads,
    strings: HashSet<String>,
    by_file: HashMap<String, HashMap<String, i64>>,
    indexed: HashSet<String>,
}

impl MemberReads {
    pub fn new(store: &Store<'_>) -> MemberReads {
        let mut template = Reads::default();
        let mut by_file: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut indexed: HashSet<String> = HashSet::new();
        for f in store.table("files").iter() {
            let Some(id) = text(f, "id") else { continue };
            if let Some(Value::Object(counts)) = f.get("prop_reads") {
                by_file.insert(id.clone(), counts.iter().map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(i64::MAX))).collect());
            }
            if f.get("this_indexed").and_then(Value::as_i64) == Some(1) {
                indexed.insert(id);
            }
        }
        for e in store.table("expressions").iter() {
            // A TYPESCRIPT body's expression is that file's own source, every property read of it in `prop_reads`.
            if text(e, "lang").as_deref() == Some("ts") {
                continue;
            }
            match e.get("ast") {
                Some(ast @ Value::Object(_)) => template.walk(ast),
                _ => identifiers(&text(e, "source").unwrap_or_default(), &mut template.names),
            }
        }
        let strings = store.table("string_literals").iter().filter_map(|s| text(s, "value")).collect();
        MemberReads { template, strings, by_file, indexed }
    }

    /// How a member may be reached OTHER than by its own file's property reads: `None` when something else can
    /// (an expression, a string, another file's read, a decorator, `this` handed away); otherwise how many times its
    /// own file reads a property of its name.
    pub fn own_reads(&self, m: &Row) -> Option<i64> {
        let (id, name, file) = (text(m, "id")?, text(m, "name")?, text(m, "file")?);
        let decorated = matches!(m.get("decorators"), Some(Value::Array(a)) if !a.is_empty());
        if decorated || self.indexed.contains(&file) || self.template.rows.contains(&id)
            || self.template.names.contains(&name) || self.strings.contains(&name) {
            return None;
        }
        if self.by_file.iter().any(|(f, counts)| *f != file && counts.contains_key(&name)) {
            return None;
        }
        Some(self.by_file.get(&file).and_then(|c| c.get(&name)).copied().unwrap_or(0))
    }
}

/// The (class id, field name) pairs whose initializer nothing reads.
pub struct Unread {
    fields: HashSet<(String, String)>,
}

impl Unread {
    pub fn new(store: &Store<'_>, reads: &MemberReads) -> Unread {
        let mut fields = HashSet::new();
        for m in store.table("members").iter() {
            let (Some(class), Some(name)) = (text(m, "class"), text(m, "name")) else { continue };
            if text(m, "kind").as_deref() == Some("property") && reads.own_reads(m) == Some(0) {
                fields.insert((class, name));
            }
        }
        Unread { fields }
    }

    /// Whether the literal `s` is the initializer of an unread field of `class`.
    pub fn initializes(&self, s: &Row, class: &str) -> bool {
        if text(s, "context").as_deref() != Some("PropertyDeclaration") {
            return false;
        }
        text(s, "property").is_some_and(|p| self.fields.contains(&(class.to_string(), p)))
    }
}

#[cfg(test)]
mod tests {
    use super::identifiers;
    use std::collections::HashSet;

    #[test]
    fn every_identifier_of_any_length_is_read() {
        let mut out = HashSet::new();
        identifiers("a.id?.x2 + f(1abc, $v) && b!.c", &mut out);
        for name in ["a", "id", "x2", "f", "$v", "b", "c"] {
            assert!(out.contains(name), "{name}");
        }
        // a run that starts with a digit is a number, never an identifier
        assert!(!out.contains("abc") && !out.contains("1abc"));
    }
}
