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
//! A plain `=` to `this.<name>` is a write, not a read (#68): `this.f = 'k'` in a method counts against nothing - nor
//! does one in ANOTHER file (a sibling class of one shape writing its own `this.f`); `x.f = 'k'` on any other receiver
//! still reads, and its literal stays a site. The literal a `this.f` write holds is no site either - nor is a
//! `return` of an unread GETTER.
//! A literal is dropped only when the statement it sits in (by file, class, value and line) is such a write or
//! return, whole or as an arm of a ternary of strings that is its whole value; one inside a call or a condition
//! stays a site.

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
    /// How many plain `=` writes each file makes to a property of each name: counted in `prop_reads`, read by none.
    writes: HashMap<(String, String), i64>,
}

/// A plain `=` to a property of `this` (`this.f = ..`): a class's write of its own member, though `prop_reads` counts
/// its `this.f` (#68). A `+=` reads; `this.f[k] = ..` (`this-indexed`) reads `f`. A write through ANY OTHER receiver
/// (`notice.f = 'k'` on an instance a factory made) is no such write: it hands the value to another object, which the
/// name proof cannot follow, so its `x.f` stays a read and the literal it writes stays a site.
fn plain_write(a: &Row) -> bool {
    text(a, "operator").as_deref() == Some("=") && text(a, "scope").as_deref() == Some("this")
}

impl MemberReads {
    pub fn new(store: &Store<'_>) -> MemberReads {
        let mut writes: HashMap<(String, String), i64> = HashMap::new();
        for a in store.table("assignments").iter() {
            if let (true, Some(file), Some(target)) = (plain_write(a), text(a, "file"), text(a, "target")) {
                *writes.entry((file, target)).or_default() += 1;
            }
        }
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
            // PEEKED: every template expression's tree is walked once here, and kept it was held to the closure's end.
            e.peek("ast", |ast| match ast {
                Some(ast @ Value::Object(_)) => template.walk(ast),
                _ => identifiers(&text(e, "source").unwrap_or_default(), &mut template.names),
            })
        }
        let strings = store.table("string_literals").iter().filter_map(|s| text(s, "value")).collect();
        MemberReads { template, strings, by_file, indexed, writes }
    }

    /// How many plain `=` writes `file` makes to a property named `name`.
    pub fn written(&self, file: &str, name: &str) -> i64 {
        self.writes.get(&(file.to_string(), name.to_string())).copied().unwrap_or(0)
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
        // ANOTHER FILE reads it when it spells the name more often than it WRITES it: a sibling class of the same
        // shape assigning its own `this.f` is no read of this one's `f` (#68).
        if self.by_file.iter().any(|(f, counts)| *f != file && counts.get(&name).is_some_and(|&n| n > self.written(f, &name))) {
            return None;
        }
        Some(self.by_file.get(&file).and_then(|c| c.get(&name)).copied().unwrap_or(0))
    }
}

fn number(row: &Row, field: &str) -> Option<i64> {
    match row.get(field) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// A statement that may hold a literal: a plain `=` write to a member, or a getter's `return` (#68). Matched to a
/// literal by file, class, value and line span; `unread` when the member it feeds is. `arms` is set when the value is
/// a TERNARY of strings (`c ? 'a' : 'b'`, nested too): `values` are then its arms, each a literal of context
/// `ConditionalExpression`; otherwise `values` is the one string the statement holds whole.
struct Holder {
    file: String,
    class: String,
    values: Vec<String>,
    arms: bool,
    lines: (i64, i64),
    unread: bool,
}

/// The strings a ternary's arms hold, down nested ternaries; `None` when an arm is anything but a string or a ternary.
fn arm_strings(v: &Value, out: &mut Vec<String>) -> Option<()> {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Object(o) if o.contains_key("$then") && o.contains_key("$else") => {
            arm_strings(o.get("$then")?, out)?;
            arm_strings(o.get("$else")?, out)?;
        }
        _ => return None,
    }
    Some(())
}

/// The (class id, member name) pairs nothing reads - a field or a getter - and the writes and returns that feed them.
pub struct Unread {
    fields: HashSet<(String, String)>,
    assigned: Vec<Holder>,
    returned: Vec<Holder>,
}

impl Unread {
    pub fn new(store: &Store<'_>, reads: &MemberReads) -> Unread {
        // A PLAIN `=` TO A PROPERTY IS A WRITE, NOT A READ (#68): each one in the member's own file is taken off its
        // count (another file's, `MemberReads::own_reads`).
        let assignments = store.table("assignments");
        let mut fields = HashSet::new();
        let mut getters: HashSet<String> = HashSet::new();
        let mut field_ids: HashSet<String> = HashSet::new();
        for m in store.table("members").iter() {
            let (Some(id), Some(class), Some(name), Some(file)) = (text(m, "id"), text(m, "class"), text(m, "name"), text(m, "file")) else { continue };
            let kind = text(m, "kind");
            if !matches!(kind.as_deref(), Some("property") | Some("getter")) {
                continue;
            }
            let written = reads.written(&file, &name);
            if reads.own_reads(m).is_some_and(|own| own <= written) {
                if kind.as_deref() == Some("getter") {
                    getters.insert(id);
                } else {
                    field_ids.insert(id);
                    fields.insert((class, name));
                }
            }
        }
        let holder = |r: &Row, unread: bool| -> Option<Holder> {
            let line = number(r, "line")?;
            let (values, arms) = match r.get("value")? {
                Value::String(s) if !s.is_empty() => (vec![s.clone()], false),
                v @ Value::Object(_) => {
                    let mut out = Vec::new();
                    arm_strings(v, &mut out)?;
                    (out, true)
                }
                _ => return None,
            };
            Some(Holder { file: text(r, "file")?, class: text(r, "class")?, values, arms,
                lines: (line, number(r, "end_line").unwrap_or(line)), unread })
        };
        let assigned = assignments.iter()
            .filter(|a| plain_write(a))
            .filter_map(|a| holder(a, text(a, "target_id").is_some_and(|t| field_ids.contains(&t))))
            .collect();
        let returned = store.table("returns").iter()
            .filter_map(|r| holder(r, text(r, "member").is_some_and(|m| getters.contains(&m))))
            .collect();
        Unread { fields, assigned, returned }
    }

    /// Whether the literal `s`, inside `class`, only feeds members nothing reads: the initializer of an unread field,
    /// or the whole value of a `=` write to one or of an unread getter's `return` - every such statement it may sit
    /// in feeding an unread member, so a literal two statements on one line could hold is kept unless both do. An
    /// ARM of a ternary that is such a statement's whole value is held the same way.
    pub fn holds(&self, s: &Row, class: &str) -> bool {
        let (holders, arms): (Vec<&Holder>, bool) = match text(s, "context").as_deref() {
            Some("PropertyDeclaration") => {
                return text(s, "property").is_some_and(|p| self.fields.contains(&(class.to_string(), p)));
            }
            Some("BinaryExpression") => (self.assigned.iter().collect(), false),
            Some("ReturnStatement") => (self.returned.iter().collect(), false),
            Some("ConditionalExpression") => (self.assigned.iter().chain(self.returned.iter()).collect(), true),
            _ => return false,
        };
        let (Some(file), Some(value), Some(line)) = (text(s, "file"), text(s, "value"), number(s, "line")) else { return false };
        let mut at = holders.into_iter()
            .filter(|h| h.arms == arms && h.file == file && h.class == class && h.values.contains(&value)
                && h.lines.0 <= line && line <= h.lines.1)
            .peekable();
        at.peek().is_some() && at.all(|h| h.unread)
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
