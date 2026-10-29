//! THE KEYS A CLASS INSTANCE CARRIES, read through a receiver typed by its class, a BASE class or an INTERFACE.
//!
//! `this.btn = new Btn(this.t); this.c = this.btn.text;` holds what `Btn`'s constructor wrote to `text`, and
//! `this.s: IStrat = new StratA(..); this.c = this.s.tip()` holds what `tip` returns in ANY implementer a `new` builds -
//! the receiver's declared type says which members can answer, never which one does.
//!
//! THE EXTRACTOR NAMES THE DECLARING TYPE (`$owner` on a call, `$read` on a property read, the checker's own answer), and
//! this module joins it to the `classes` rows by (file, name) - no name alone. A receiver typed C stands for C itself and
//! for every class that `extends` or `implements` C, transitively, WHEN A `new` BUILDS IT SOMEWHERE IN THE TREE: a subclass
//! nothing builds is a way no run takes. A subclass without the member inherits the nearest base's. NO CONDITION IS
//! CARRIED from the callee - its branches test its own parameters, and the instance is chosen by the run, not by a gate.

use super::key_branches::{member_ways, Branches};
use super::key_fields::{id_of, text};
use super::key_follow::Returned;
use crate::rows::IndexMap;
use super::store::{Row, Store};
use serde_json::Value;
use std::cell::OnceCell;
use crate::rows::{FastMap as HashMap, FastSet as HashSet};
use std::rc::Rc;

/// (file row id, type name) - how a class or interface is told from another of the same name.
type TypeKey = (String, String);

/// class id -> the names of its own members; member id -> its initializer; (class id, field) -> what its constructors write.
#[derive(Default)]
pub(super) struct Inst {
    pub(super) names: HashMap<String, Vec<String>>,
    pub(super) initial: HashMap<String, Value>,
    pub(super) written: HashMap<(String, String), Vec<Value>>,
}

pub(super) struct Classes {
    by_tail: HashMap<String, Vec<(String, String)>>,
    by_path: HashMap<String, String>,
    by_key: HashMap<TypeKey, String>,
    key_of: HashMap<String, TypeKey>,
    /// class id -> the type it extends.
    parent: HashMap<String, TypeKey>,
    /// type -> the classes that extend or implement it directly.
    subs: HashMap<TypeKey, Vec<String>>,
    /// (class id, member name) -> member id.
    member: HashMap<(String, String), String>,
    constructors: HashSet<String>,
    /// `functions` row id -> the member or function it sits in: a callback written inside a constructor is part of it.
    fn_parent: HashMap<String, String>,
    /// member id (a class's, or an interface's `type_members` row) -> the type declaring it and its name.
    declared_in: HashMap<String, (TypeKey, String)>,
    /// What an instance's properties are made of (`key_instances`), read when the first element asks.
    inst: OnceCell<Inst>,
    /// The tables a `new` can sit in, read once when the first receiver asks which classes are built.
    tables: Vec<Rc<Vec<Row>>>,
    built: OnceCell<HashSet<String>>,
    /// class id -> the classes whose code runs a `new` of it; None when one such place sits in no class this can name.
    builders: OnceCell<HashMap<String, Option<Vec<String>>>>,
    /// `members` row id -> the class it belongs to: whose code a call row (which names its member, not its class) runs in.
    member_class: HashMap<String, String>,
    /// The members a subclass declares again, read when the first call asks.
    overridden: OnceCell<HashSet<String>>,
}

fn tail(s: &str) -> String {
    let normalised = s.replace('\\', "/");
    let parts: Vec<&str> = normalised.split('/').collect();
    parts[parts.len().saturating_sub(2)..].join("/")
}

impl Classes {
    pub(super) fn new(store: &Store<'_>) -> Classes {
        let (mut by_tail, mut by_path): (HashMap<String, Vec<(String, String)>>, HashMap<String, String>) = Default::default();
        for f in store.table("files").iter() {
            let (Some(id), Some(path)) = (id_of(f, "id"), text(f, "path")) else { continue };
            by_tail.entry(tail(&path)).or_default().push((id.clone(), format!("/{}", path.replace('\\', "/"))));
            by_path.insert(path.replace('\\', "/"), id);
        }
        let mut out = Classes {
            by_tail, by_path, by_key: HashMap::default(), key_of: HashMap::default(), parent: HashMap::default(), subs: HashMap::default(), member: HashMap::default(),
            constructors: HashSet::default(), fn_parent: HashMap::default(), declared_in: HashMap::default(), inst: OnceCell::new(), tables: Vec::new(), built: OnceCell::new(),
            builders: OnceCell::new(), member_class: HashMap::default(),
            overridden: OnceCell::new(),
        };
        let classes = store.table("classes");
        for c in classes.iter() {
            if let (Some(id), Some(file), Some(name)) = (id_of(c, "id"), id_of(c, "file"), text(c, "name")) {
                out.key_of.insert(id.clone(), (file.clone(), name.clone()));
                out.by_key.insert((file, name), id);
            }
        }
        for c in classes.iter() {
            let Some(id) = id_of(c, "id") else { continue };
            let heritage = c.get("extends").into_iter().chain(c.get("implements").and_then(Value::as_array).into_iter().flatten());
            for (i, h) in heritage.enumerate() {
                let key = match (h.get("file").and_then(Value::as_str), h.get("name").and_then(Value::as_str)) {
                    (Some(file), Some(name)) => out.by_path.get(&file.replace('\\', "/")).map(|f| (f.clone(), name.to_string())),
                    _ => None,
                };
                let Some(key) = key else { continue };
                if i == 0 && c.get("extends").is_some_and(|e| e.is_object()) {
                    out.parent.insert(id.clone(), key.clone());
                }
                out.subs.entry(key).or_default().push(id.clone());
            }
        }
        for m in store.table("members").iter() {
            let (Some(id), Some(class), Some(name)) = (id_of(m, "id"), id_of(m, "class"), text(m, "name")) else { continue };
            out.member_class.insert(id.clone(), class.clone());
            match text(m, "kind").as_deref() {
                Some("constructor") => {
                    out.constructors.insert(id);
                }
                Some("setter") => {}
                _ => {
                    if let Some(key) = out.key_of.get(&class) {
                        out.declared_in.insert(id.clone(), (key.clone(), name.clone()));
                    }
                    out.member.entry((class, name)).or_insert(id);
                }
            }
        }
        let interfaces: HashMap<String, TypeKey> = store.table("interfaces").iter().filter_map(|i| Some((id_of(i, "id")?, (id_of(i, "file")?, text(i, "name")?)))).collect();
        for m in store.table("type_members").iter() {
            if let (Some(id), Some(key), Some(name)) = (id_of(m, "id"), id_of(m, "owner").and_then(|o| interfaces.get(&o)), text(m, "name")) {
                out.declared_in.insert(id, (key.clone(), name));
            }
        }
        out.fn_parent = store.table("functions").iter().filter_map(|f| Some((id_of(f, "id")?, id_of(f, "parent")?))).collect();
        out.tables = ["assignments", "locals", "returns", "calls", "members"].iter().map(|t| store.table(t)).collect();
        out
    }

    fn file_of(&self, abs: &str) -> Option<&str> {
        let abs = crate::rows::ts::feature_writes::ref_file(abs);
        self.by_tail.get(&tail(&abs))?.iter().find(|(_, p)| abs.ends_with(p.as_str())).map(|(id, _)| id.as_str())
    }

    /// The classes a `new` builds, by (file, name) of the `$target` the extractor resolved.
    fn built(&self) -> &HashSet<String> {
        self.built.get_or_init(|| {
            fn walk(v: &Value, me: &Classes, out: &mut HashSet<String>) {
                match v {
                    Value::Object(o) => {
                        if let (Some(_), Some(t)) = (o.get("$new"), o.get("$target")) {
                            let key = t.get("file").and_then(Value::as_str).and_then(|f| me.file_of(f)).zip(t.get("name").and_then(Value::as_str));
                            if let Some(id) = key.and_then(|(f, n)| me.by_key.get(&(f.to_string(), n.to_string()))) {
                                out.insert(id.clone());
                            }
                        }
                        o.values().for_each(|x| walk(x, me, out));
                    }
                    Value::Array(xs) => xs.iter().for_each(|x| walk(x, me, out)),
                    _ => {}
                }
            }
            let mut out = HashSet::default();
            for row in self.tables.iter().flat_map(|t| t.iter()) {
                row.get("value").into_iter().chain(row.get("args")).for_each(|v| walk(v, self, &mut out));
            }
            out
        })
    }

    /// WHO BUILDS EACH CLASS: the class whose code holds each `new` of it - a row's own `class`, or its member's for a call.
    /// One `new` in no class (a module-level function, a row that names neither) makes the answer None: unknown, not empty.
    pub(super) fn builders(&self, class: &str) -> Option<&Vec<String>> {
        let all = self.builders.get_or_init(|| {
            fn walk(v: &Value, me: &Classes, at: Option<&String>, out: &mut HashMap<String, Option<Vec<String>>>) {
                match v {
                    Value::Object(o) => {
                        if let (Some(_), Some(t)) = (o.get("$new"), o.get("$target")) {
                            let key = t.get("file").and_then(Value::as_str).and_then(|f| me.file_of(f)).zip(t.get("name").and_then(Value::as_str));
                            if let Some(built) = key.and_then(|(f, n)| me.by_key.get(&(f.to_string(), n.to_string()))) {
                                let slot = out.entry(built.clone()).or_insert_with(|| Some(Vec::new()));
                                match (slot.as_mut(), at) {
                                    (Some(list), Some(class)) if !list.contains(class) => list.push(class.clone()),
                                    (Some(_), Some(_)) => {}
                                    _ => *slot = None,
                                }
                            }
                        }
                        o.values().for_each(|x| walk(x, me, at, out));
                    }
                    Value::Array(xs) => xs.iter().for_each(|x| walk(x, me, at, out)),
                    _ => {}
                }
            }
            let mut out = HashMap::default();
            for row in self.tables.iter().flat_map(|t| t.iter()) {
                let at = id_of(row, "class").or_else(|| id_of(row, "member").and_then(|m| self.member_class.get(&m).cloned()));
                row.get("value").into_iter().chain(row.get("args")).for_each(|v| walk(v, self, at.as_ref(), &mut out));
            }
            out
        });
        all.get(class).and_then(Option::as_ref)
    }

    /// The class `a` extends, transitively, is `base`.
    pub(super) fn extends(&self, class: &str, base: &str) -> bool {
        let mut at = class.to_string();
        for _ in 0..32 {
            match self.parent.get(&at).and_then(|k| self.by_key.get(k)) {
                Some(up) if up == base => return true,
                Some(up) => at = up.clone(),
                None => return false,
            }
        }
        false
    }

    pub(super) fn inst(&self) -> &Inst {
        self.inst.get_or_init(|| {
            let mut out = Inst::default();
            for m in self.tables[4].iter() {
                let (Some(id), Some(class), Some(name)) = (id_of(m, "id"), id_of(m, "class"), text(m, "name")) else { continue };
                if matches!(text(m, "kind").as_deref(), Some("constructor" | "setter")) {
                    continue;
                }
                out.names.entry(class).or_default().push(name);
                if let Some(v) = m.get("value").filter(|v| !v.is_null()) {
                    out.initial.insert(id, v.clone());
                }
            }
            for a in self.tables[0].iter() {
                let (Some(member), Some(class), Some(name), Some(value)) = (id_of(a, "member"), id_of(a, "class"), text(a, "target"), a.get("value")) else { continue };
                if self.is_constructor(&member) && text(a, "scope").as_deref() == Some("this") && text(a, "operator").as_deref() == Some("=") {
                    out.written.entry((class, name)).or_default().push(value.clone());
                }
            }
            out
        })
    }

    /// The class a `new`'s `$target` names.
    pub(super) fn class_built(&self, new: &Value) -> Option<&str> {
        let t = new.get("$target")?;
        let file = self.file_of(t.get("file")?.as_str()?)?;
        self.by_key.get(&(file.to_string(), t.get("name")?.as_str()?.to_string())).map(String::as_str)
    }

    /// `class` and every base it extends, nearest first.
    pub(super) fn chain(&self, class: &str) -> Vec<String> {
        let mut out = vec![class.to_string()];
        for _ in 0..32 {
            match self.parent.get(out.last().expect("never empty")).and_then(|k| self.by_key.get(k)) {
                Some(up) if !out.contains(up) => out.push(up.clone()),
                _ => break,
            }
        }
        out
    }

    /// The member of that name `class` declares or inherits.
    pub(super) fn member_of(&self, class: &str, name: &str) -> Option<&str> {
        self.chain(class).iter().find_map(|c| self.member.get(&(c.clone(), name.to_string()))).map(String::as_str)
    }

    /// A SUBCLASS DECLARES A MEMBER OF THE SAME NAME (#48): a call of `member` may run that body instead, so nothing its
    /// own body proves holds for every call of it.
    pub(super) fn overridden(&self, member: &str) -> bool {
        self.overridden.get_or_init(|| {
            let bases = |class: &String, name: &String| self.chain(class).into_iter().skip(1).filter_map(|b| self.member.get(&(b, name.clone())).cloned()).collect::<Vec<_>>();
            self.member.keys().flat_map(|(class, name)| bases(class, name)).collect()
        }).contains(member)
    }

    /// `member` is a constructor, or a callback written inside one (`Promise.resolve(x).then(() => { this.a = .. })`):
    /// what the callback writes to `this` is the instance's too, whenever it runs.
    pub(super) fn is_constructor(&self, member: &str) -> bool {
        let mut at = member;
        for _ in 0..32 {
            if self.constructors.contains(at) {
                return true;
            }
            match self.fn_parent.get(at) {
                Some(up) => at = up,
                None => return false,
            }
        }
        false
    }

    /// (class, member id) for every class a receiver typed `owner` (in `file`) can hold that has `name`: the owner when it
    /// is a class, and each built subtype - the member its own class declares, or the nearest base's.
    fn answering(&self, owner: &str, file: &str, name: &str) -> Vec<(String, String)> {
        let Some(file) = self.file_of(file) else { return Vec::new() };
        self.answering_key((file.to_string(), owner.to_string()), name)
    }

    /// The members that can answer a TEMPLATE'S call of `row` - a class member or an interface's: those of the declaring
    /// type's built subtypes. Nothing when the type has none (the row answers itself) so a plain class reads as before.
    pub(super) fn answers_of(&self, row: &str) -> Vec<String> {
        self.answering_row(row).into_iter().map(|(_, member)| member).collect()
    }

    /// (class, member id) for every class that can answer a read of `row` - a member or an interface's - when its
    /// declaring type has subtypes; nothing otherwise.
    pub(super) fn answering_row(&self, row: &str) -> Vec<(String, String)> {
        let Some((key, name)) = self.declared_in.get(row) else { return Vec::new() };
        if !self.subs.contains_key(key) {
            return Vec::new();
        }
        self.answering_key(key.clone(), name)
    }

    fn answering_key(&self, key: TypeKey, name: &str) -> Vec<(String, String)> {
        let mut classes: Vec<String> = self.by_key.get(&key).cloned().into_iter().collect();
        let (mut todo, mut seen) = (vec![key], HashSet::default());
        while let Some(at) = todo.pop() {
            for sub in self.subs.get(&at).into_iter().flatten() {
                if !seen.insert(sub.clone()) {
                    continue;
                }
                if self.built().contains(sub) {
                    classes.push(sub.clone());
                }
                if let Some(k) = self.key_of.get(sub) {
                    todo.push(k.clone());
                }
            }
        }
        let mut out: Vec<(String, String)> = Vec::new();
        for class in classes {
            let mut at = class.clone();
            for _ in 0..32 {
                if let Some(id) = self.member.get(&(at.clone(), name.to_string())) {
                    if !out.iter().any(|(c, m)| *c == class && m == id) {
                        out.push((class.clone(), id.clone()));
                    }
                    break;
                }
                match self.parent.get(&at).and_then(|k| self.by_key.get(k)) {
                    Some(up) => at = up.clone(),
                    None => break,
                }
            }
        }
        out
    }

    /// What a value reads through a receiver: a call (`$owner` beside its `$target`) or a property (`$read`).
    pub(super) fn through(&self, value: &Value) -> Vec<(String, String)> {
        let call = value.get("$owner").and_then(Value::as_str).zip(value.get("$target"));
        let read = value.get("$read").and_then(|r| r.get("owner")).and_then(Value::as_str).zip(value.get("$read"));
        let Some((owner, t)) = call.or(read) else { return Vec::new() };
        match (t.get("file").and_then(Value::as_str), t.get("name").and_then(Value::as_str)) {
            (Some(file), Some(name)) => self.answering(owner, file, name),
            _ => Vec::new(),
        }
    }
}

type Ways = IndexMap<String, Vec<Vec<String>>>;

/// What the fields' own keys left for a read through a receiver: their ways, those of a field only a subclass's constructor
/// wrote from outside, what each such constructor wrote, and the translate calls written DIRECTLY to a member (the
/// fields' own pass leaves those to the `ts_call` route): member id -> (writing class, keys).
struct Held<'a> {
    out: &'a IndexMap<String, Ways>,
    only: &'a IndexMap<String, Ways>,
    by_writer: &'a IndexMap<(String, String), Ways>,
    direct: HashMap<String, Vec<(String, Vec<String>)>>,
}

/// The keys a value reads off an instance: a property read through a receiver (`$read`) is every key the member holds in a
/// class that can answer - its own writes, what that class's constructor wrote to a base's field and the translate call a
/// class or a base of it wrote to it - through a ternary's arms and either side of `??`/`||`, as a call's are followed.
fn read_keys(value: &Value, reader: &Returned<'_, '_>, held: &Held<'_>) -> Vec<String> {
    let mut keys = Vec::new();
    if value.get("$read").is_some() {
        for (class, member) in reader.classes.through(value) {
            keys.extend(answer_keys(&class, &member, reader, held));
        }
    }
    let arms = [value.get("$then"), value.get("$else")].into_iter().flatten();
    let sides = value.get("$operands").and_then(Value::as_array).into_iter().flatten();
    for inner in arms.chain(sides).chain(value.get("$await")) {
        keys.extend(read_keys(inner, reader, held));
    }
    keys
}

/// The keys `member` holds when `class` answers: its own writes, what that class's constructor wrote to a base's field, and
/// the translate call the class or a base of it wrote to it.
fn answer_keys(class: &str, member: &str, reader: &Returned<'_, '_>, held: &Held<'_>) -> Vec<String> {
    let own = held.out.get(member).or_else(|| held.only.get(member));
    let written = held.by_writer.get(&(class.to_string(), member.to_string()));
    let mut keys: Vec<String> = own.into_iter().chain(written).flat_map(|w| w.keys().cloned()).collect();
    for (writer, ks) in held.direct.get(member).into_iter().flatten() {
        if writer == class || reader.classes.extends(class, writer) {
            keys.extend(ks.iter().cloned());
        }
    }
    keys
}

/// A FIELD, OR A GETTER, HOLDING A PROPERTY READ OFF AN INSTANCE (`this.c = this.btn.text`) holds the keys that property
/// holds, no condition carried but the write's own. Run once the fields' own keys are known, over the members `out` kept.
pub(super) fn add_reads(
    store: &Store<'_>,
    reader: &Returned<'_, '_>,
    branches: &Branches,
    out: &mut IndexMap<String, Ways>,
    only: &IndexMap<String, Ways>,
    by_writer: &IndexMap<(String, String), Ways>,
    foreign: &[String],
) {
    let members = store.table("members");
    let assignments = store.table("assignments");
    let class_of: HashMap<String, String> = members.iter().filter_map(|m| Some((id_of(m, "id")?, text(m, "class")?))).collect();
    let getters: HashSet<String> = members.iter().filter(|m| text(m, "kind").as_deref() == Some("getter")).filter_map(|m| id_of(m, "id")).collect();
    let mut direct: HashMap<String, Vec<(String, Vec<String>)>> = HashMap::default();
    let writes = members.iter().filter_map(|m| Some((id_of(m, "id")?, text(m, "class")?, m.get("value")?)));
    let writes = writes.chain(assignments.iter().filter_map(|a| Some((id_of(a, "target_id")?, text(a, "class")?, a.get("value")?))));
    for (member, writer, value) in writes {
        let Some(args) = super::key_follow::translated(value, reader.i18n) else { continue };
        let mut spelled = Vec::new();
        args.iter().for_each(|arg| super::key_follow::arg_keys(arg, &[], reader.keys, &mut spelled));
        if !spelled.is_empty() {
            direct.entry(member).or_default().push((writer, spelled.into_iter().map(|(k, _)| k).collect()));
        }
    }
    let held = Held { out, only, by_writer, direct };
    let mut found: Vec<(String, String, Vec<String>)> = Vec::new();
    for m in members.iter() {
        if let (Some(id), Some(value)) = (id_of(m, "id"), m.get("value")) {
            found.extend(read_keys(value, reader, &held).into_iter().map(|k| (id.clone(), k, Vec::new())));
        }
    }
    for a in assignments.iter() {
        let (Some(target), Some(value)) = (id_of(a, "target_id").filter(|t| class_of.contains_key(t) && !foreign.contains(t)), a.get("value")) else { continue };
        if text(a, "class").as_ref() == class_of.get(&target) {
            found.extend(read_keys(value, reader, &held).into_iter().map(|k| (target.clone(), k, branches.chain_of(a))));
        }
    }
    for r in store.table("returns").iter() {
        let (Some(member), Some(value)) = (id_of(r, "member").filter(|m| getters.contains(m)), r.get("value")) else { continue };
        found.extend(read_keys(value, reader, &held).into_iter().map(|k| (member.clone(), k, branches.chain_of(r))));
    }
    // A TEMPLATE READING A PROPERTY OF A BASE OR AN INTERFACE (`{{ b.text }}` over `b: Base`) renders what every built
    // subtype holds in it, as a template's call of such a method runs every built subtype's (`answers_of`). Added once the
    // reads above are done, so a receiver typed by one subclass still reads that subclass alone.
    // A SUBTYPE'S KEY RENDERS ONLY WHERE ITS INSTANCES CAN BE: on a path through a component that builds it (`via:`,
    // `keyreach::ways_of`). Every built subtype's key at every read of the base lost real restrictions - a button built in
    // one page read as rendered by each table any page shows. A builder that is no component says nothing of the path.
    let component_of: HashMap<String, String> = store.table("components").iter().filter_map(|c| Some((id_of(c, "class")?, id_of(c, "id")?))).collect();
    let via = |class: &str, declaring: bool| -> Vec<String> {
        if declaring {
            return Vec::new();
        }
        let comps: Option<Vec<&String>> = reader.classes.builders(class).map(|b| b.iter().map(|c| component_of.get(c)).collect()).flatten();
        comps.filter(|c| !c.is_empty()).map(|c| vec![format!("via:{}", c.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("|"))]).unwrap_or_default()
    };
    let type_members = store.table("type_members");
    let rows = members.iter().filter_map(|m| id_of(m, "id")).chain(type_members.iter().filter_map(|m| id_of(m, "id")));
    for row in rows {
        let declaring = class_of.get(&row);
        for (class, member) in reader.classes.answering_row(&row) {
            let chain = via(&class, declaring == Some(&class));
            found.extend(answer_keys(&class, &member, reader, &held).into_iter().map(|k| (row.clone(), k, chain.clone())));
        }
    }
    for (member, key, chain) in found {
        member_ways(out.entry(member).or_default(), &key, chain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn store(extra: Value) -> Classes {
        let mut tables = json!({
            "files": [{"id": "f:1", "path": "src/a.ts"}],
            "classes": [
                {"id": "c:1", "file": "f:1", "name": "Base", "abstract": 1},
                {"id": "c:2", "file": "f:1", "name": "Built", "extends": {"name": "Base", "file": "src/a.ts"}},
                {"id": "c:3", "file": "f:1", "name": "Spare", "extends": {"name": "Base", "file": "src/a.ts"}},
                {"id": "c:4", "file": "f:1", "name": "Inherits", "extends": {"name": "Built", "file": "src/a.ts"}},
            ],
            "members": [
                {"id": "m:1", "class": "c:1", "name": "tip"},
                {"id": "m:2", "class": "c:2", "name": "tip"},
                {"id": "m:3", "class": "c:3", "name": "tip"},
            ],
            "assignments": [{"value": {"$new": "Built", "$target": {"name": "Built", "file": "/w/src/a.ts"}}},
                            {"value": {"$new": "Inherits", "$target": {"name": "Inherits", "file": "/w/src/a.ts"}}}],
        });
        tables.as_object_mut().expect("tables").extend(extra.as_object().cloned().unwrap_or_default());
        let map: Map<String, Value> = tables.as_object().cloned().expect("tables");
        Classes::new(&Store::from_payload(map, "typescript"))
    }

    #[test]
    fn a_receiver_typed_by_a_base_is_every_subtype_a_new_builds_and_never_one_nothing_builds() {
        let classes = store(json!({}));
        let mut got = classes.answering("Base", "/w/src/a.ts", "tip");
        got.sort();
        // Base itself, Built, and Inherits (which has no `tip` of its own, so Built's) - never Spare
        assert_eq!(got, vec![("c:1".into(), "m:1".into()), ("c:2".into(), "m:2".into()), ("c:4".into(), "m:2".into())]);
    }

    #[test]
    fn a_member_a_subclass_declares_again_is_overridden_and_the_last_one_is_not() {
        let classes = store(json!({}));
        // Base's `tip` is declared again by Built and Spare; Built's by nobody (Inherits has none of its own)
        assert!(classes.overridden("m:1"));
        assert!(!classes.overridden("m:2"));
        assert!(!classes.overridden("m:3"));
    }

    #[test]
    fn a_class_of_another_file_is_not_the_one_named() {
        let classes = store(json!({"files": [{"id": "f:1", "path": "src/a.ts"}, {"id": "f:2", "path": "src/b.ts"}]}));
        assert!(classes.answering("Base", "/w/src/b.ts", "tip").is_empty());
    }

    #[test]
    fn a_template_call_of_a_bases_member_answers_with_the_built_subtypes_members_and_a_plain_class_with_none() {
        let classes = store(json!({}));
        let mut got = classes.answers_of("m:1");
        got.sort();
        assert_eq!(got, vec!["m:1".to_string(), "m:2".to_string(), "m:2".to_string()]);
        assert!(classes.answers_of("m:3").is_empty(), "Spare has no subtype of its own: the row answers itself");
    }
}
