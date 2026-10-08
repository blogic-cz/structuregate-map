//! The row helpers and the config members the config restrictions are read through.
//! A child of `gate_config` by `#[path]`: its items are `pub(super)` for that file alone.

use super::*;

pub(super) const EQ: &[&str] = &["===", "=="];

/// `every` IS NOT A MEMBERSHIP TEST. `xs.every((m) => m === x)` renders when EVERY listed
/// member equals one value, which permits one member and not the list — publishing the
/// list as `in` would overstate it. It is not read as a restriction: a
/// rule that would be wrong the first time a directive writes it is not worth the row it would add.
pub(super) const MEMBERSHIP: &[&str] = &["some", "includes"];
pub(super) const HAS: &[&str] = &["has", "includes"];

pub(super) fn slash(s: &str) -> String {
    s.replace('\\', "/")
}

pub(super) fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

pub(super) fn id_of(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

pub(super) fn number(row: &Row, field: &str) -> Option<i64> {
    match row.get(field) {
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

pub(super) fn name_of(n: &Value) -> Option<&str> {
    n.get("name").and_then(|v| v.as_str())
}

/// A `type_ref` -> the enum it names, unwrapping an ARRAY: `VendorIDs[]` carries its
/// `element`.
pub(super) fn enum_of_type(idx: &EnumIndex, type_ref: Option<&Value>) -> Option<String> {
    let reference = type_ref?.as_object()?;
    let target = reference
        .get("unwrapped")
        .filter(|v| !v.is_null())
        .or_else(|| reference.get("element").filter(|v| !v.is_null()))
        .and_then(|v| v.as_object())
        .unwrap_or(reference);

    let name = match target.get("name") {
        Some(Value::String(n)) if !n.is_empty() => n.clone(),
        _ => return None,
    };
    let file = match target.get("file") {
        Some(Value::String(f)) if !f.is_empty() => slash(f),
        _ => return None,
    };
    for (path, id) in idx.by_decl.get(&name)? {
        if file.ends_with(&format!("/{path}")) {
            return Some(id.clone());
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigMember {
    pub name: String,
    pub row: String,
    pub enum_id: String,
}

/// Class id -> its config members, for every ENUM-TYPED member of the interfaces that
/// class takes as an input, whether the input is a property or a setter parameter.
///
/// A NAME DECLARED TWICE WITH TWO ENUMS IS NOT A DIMENSION. It resolves to nothing rather
/// than to whichever interface was read first.
pub fn config_members(
    store: &Store<'_>,
    idx: &EnumIndex,
) -> IndexMap<String, IndexMap<String, Option<ConfigMember>>> {
    let mut file_path: IndexMap<String, Option<String>> = IndexMap::new();
    for f in store.table("files").iter() {
        if let Some(id) = id_of(f, "id") {
            file_path.insert(id, text(f, "path"));
        }
    }

    let mut ifaces: IndexMap<String, Vec<(String, String)>> = IndexMap::new();
    for i in store.table("interfaces").iter() {
        let Some(name) = text(i, "name") else { continue };
        let Some(id) = id_of(i, "id") else { continue };
        let path = id_of(i, "file")
            .and_then(|f| file_path.get(&f).cloned().flatten())
            .map(|p| slash(&p))
            .unwrap_or_default();
        ifaces.entry(name).or_default().push((id, path));
    }

    let mut tms: IndexMap<String, Vec<ConfigMember>> = IndexMap::new();
    for tm in store.table("type_members").iter() {
        let Some(en) = enum_of_type(idx, tm.get("type_ref")) else { continue };
        let (Some(owner), Some(name), Some(row)) =
            (id_of(tm, "owner"), text(tm, "name"), id_of(tm, "id"))
        else {
            continue;
        };
        tms.entry(owner).or_default().push(ConfigMember { name, row, enum_id: en });
    }

    let mut out: IndexMap<String, IndexMap<String, Option<ConfigMember>>> = IndexMap::new();
    let mut add = |cls: String, name: &str, file: Option<String>| {
        let Some(candidates) = ifaces.get(name) else { return };
        for (id, path) in candidates {
            if let Some(file) = &file
                && !slash(file).ends_with(&format!("/{path}"))
            {
                continue;
            }
            let Some(members) = tms.get(id) else { continue };
            let bucket = out.entry(cls.clone()).or_default();
            for m in members {
                let seen = bucket.get(&m.name).cloned().flatten();
                let value = match seen {
                    Some(held) if held.enum_id != m.enum_id => None,
                    _ => Some(m.clone()),
                };
                bucket.insert(m.name.clone(), value);
            }
        }
    };

    let members = store.table("members");
    for m in members.iter() {
        let Some(cls) = id_of(m, "class") else { continue };
        if let Some(reference) = m.get("type_ref").and_then(|r| r.as_object())
            && let Some(Value::String(name)) = reference.get("name")
        {
            let file = reference.get("file").and_then(|f| f.as_str()).map(|s| s.to_string());
            add(cls, name, file);
        }
    }
    for m in members.iter() {
        if text(m, "kind").as_deref() != Some("setter") {
            continue;
        }
        let Some(cls) = id_of(m, "class") else { continue };
        let Some(Value::Array(params)) = m.get("params") else { continue };
        for p in params {
            let Some(param) = p.as_object() else { continue };
            let Some(Value::String(declared)) = param.get("type") else { continue };
            let file = param
                .get("type_ref")
                .and_then(|r| r.as_object())
                .and_then(|r| r.get("file"))
                .and_then(|f| f.as_str())
                .map(|s| s.to_string());
            add(cls.clone(), declared, file);
        }
    }
    out
}

/// Every object literal a gate's expression may bind, or nothing when ONE of them is not
/// one: a plain literal is one branch, `cond ? {…} : {…}` is the branches of both arms.
///
/// A CONDITIONAL WAS READ AS NO LITERAL AT ALL, so every typed member of the interface was
/// listed as `unknown` - a dimension the source never writes among them. An arm
/// that is not a literal (`cond ? {…} : other`) still makes the whole binding unread: what
/// that arm holds is not in the source.
pub(super) fn literal_branches(ast: &Value) -> Option<Vec<IndexMap<String, Value>>> {
    let n = unwrap(ast);
    let object = n.as_object()?;
    match object.get("k").and_then(|k| k.as_str()) {
        Some("Cond") => {
            let mut out = literal_branches(object.get("then")?)?;
            out.extend(literal_branches(object.get("else")?)?);
            Some(out)
        }
        // A compiler that keeps the parentheses (`cond ? ({…}) : …`) names them this way.
        Some("ParenthesizedExpression") => literal_branches(object.get("expression")?),
        _ => literal_entries(Some(n)).map(|entries| vec![entries]),
    }
}

/// Every enum member ONE config entry lists, sorted, or nothing when one item does not
/// resolve - a list missing one member is not what the source says. An empty list is an
/// answer: `[]`.
fn listed_of(node: &Value, en: &str, mem: &MemberEnums, idx: &EnumIndex) -> Option<Vec<String>> {
    let items: Vec<&Value> = match node.as_object() {
        Some(o) if o.get("k").and_then(|k| k.as_str()) == Some("Array") => match o.get("items") {
            Some(Value::Array(v)) => v.iter().collect(),
            _ => Vec::new(),
        },
        _ => vec![node],
    };
    let mut out: IndexSet<String> = IndexSet::new();
    for item in items {
        let (found, value) = constant_of(item, mem, idx)?;
        if found != en {
            return None;
        }
        out.insert(value);
    }
    let mut values: Vec<String> = out.into_iter().collect();
    values.sort();
    Some(values)
}

/// What the bound object's branches write for one member.
pub(super) struct Supplied {
    /// Every branch writes the member.
    every: bool,
    /// What each writing branch lists, or nothing when one of them does not resolve.
    lists: Option<Vec<Vec<String>>>,
}

impl Supplied {
    /// The members the gate PERMITS when the member is a restriction: the union of every
    /// branch's list, since either arm may be the one bound.
    ///
    /// A branch that omits the member, and an EMPTY list, give nothing: the restriction is
    /// read off `some(...)`, which an empty or absent list fails - yet what the directive
    /// does then (render for everyone, or for no one) is its own choice, and `in []` would
    /// be a guess that hides the subtree.
    pub(super) fn permitted(&self) -> Option<Vec<String>> {
        let lists = self.lists.as_ref()?;
        if !self.every || lists.iter().any(|l| l.is_empty()) {
            return None;
        }
        self.listed()
    }

    /// Every member any branch lists, sorted - published whatever the `op`.
    pub(super) fn listed(&self) -> Option<Vec<String>> {
        let mut all: IndexSet<String> = IndexSet::new();
        for l in self.lists.as_ref()? {
            all.extend(l.iter().cloned());
        }
        let mut out: Vec<String> = all.into_iter().collect();
        out.sort();
        Some(out)
    }
}

/// What `branches` write for `name`, or nothing when no branch writes it at all.
pub(super) fn supplied(
    branches: &[IndexMap<String, Value>],
    name: &str,
    en: &str,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> Option<Supplied> {
    let present: Vec<&Value> = branches.iter().filter_map(|b| b.get(name)).collect();
    if present.is_empty() {
        return None;
    }
    let lists = present.iter().map(|v| listed_of(v, en, mem, idx)).collect();
    Some(Supplied { every: present.len() == branches.len(), lists })
}
