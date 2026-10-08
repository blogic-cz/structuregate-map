//! The indexes the directive restrictions are read through: row helpers, members, classes and the comparisons a class makes.
//! A child of `gate_directives` by `#[path]`: its items are `pub(super)` for that file alone.

use super::*;

pub(super) const EQ: &[&str] = &["===", "=="];
pub(super) const NE: &[&str] = &["!==", "!="];

pub(super) fn slash(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.replace('\\', "/"),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string().replace('\\', "/"),
    }
}

/// The same normalisation for a path already out of its row.
pub(super) fn slash_str(path: Option<&str>) -> String {
    path.unwrap_or("").replace('\\', "/")
}

pub(super) fn id_of(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

pub(super) fn str_of(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

pub(super) fn int_of(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// Truthy the way the language this was ported from reads it: a present, non-empty,
/// non-zero, non-false value.
pub(super) fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

/// A candidate declaration: where it lives, and its row id.
pub(super) struct Decl {
    pub(super) path: String,
    pub(super) id: String,
}

/// A TS AST target names `{name, file, line}` and carries no row id, unlike a template one.
pub(super) struct MemberIdx {
    pub(super) by_line_name: IndexMap<String, Vec<Decl>>,
    pub(super) name_of: IndexMap<String, String>,
}

pub(super) fn member_index(store: &Store<'_>) -> MemberIdx {
    let mut file_path: IndexMap<String, Option<String>> = IndexMap::new();
    for f in store.table("files").iter() {
        if let Some(id) = id_of(f, "id") {
            file_path.insert(id, str_of(f.get("path")));
        }
    }
    let mut idx =
        MemberIdx { by_line_name: IndexMap::new(), name_of: IndexMap::new() };
    for m in store.table("members").iter() {
        let line = m.get("line").map(|v| v.to_string()).unwrap_or_else(|| "null".into());
        let name = str_of(m.get("name")).unwrap_or_default();
        let key = format!("{line}|{name}");
        let path = id_of(m, "file")
            .and_then(|f| file_path.get(&f).cloned())
            .flatten();
        if let Some(id) = id_of(m, "id") {
            idx.by_line_name
                .entry(key)
                .or_default()
                .push(Decl { path: slash_str(path.as_deref()), id: id.clone() });
            if let Some(n) = str_of(m.get("name")) {
                idx.name_of.insert(id, n);
            }
        }
    }
    idx
}

/// `type_ref` and AST targets name the file ABSOLUTELY while `files.path` is FE-relative, so
/// the match is BY SUFFIX.
///
/// THE GUARD COMES FIRST, exactly as the source has it: a target with a `row` but no `file`
/// returns nothing, because the name and file test runs before the row shortcut.
pub(super) fn member_at(idx: &MemberIdx, target: Option<&Value>) -> Option<String> {
    let t = target?.as_object()?;
    if str_of(t.get("name")).is_none() || str_of(t.get("file")).is_none() {
        return None;
    }
    if let Some(row) = str_of(t.get("row")) {
        return Some(row);
    }
    let file_name = slash(t.get("file"));
    let line = t.get("line").map(|v| v.to_string()).unwrap_or_else(|| "null".into());
    let name = str_of(t.get("name")).unwrap_or_default();
    for c in idx.by_line_name.get(&format!("{line}|{name}"))? {
        if file_name == c.path || file_name.ends_with(&format!("/{}", c.path)) {
            return Some(c.id.clone());
        }
    }
    None
}

/// Every `Binary` at any depth, because a comparison may sit inside an `if` condition or a
/// callback.
pub(super) fn binaries<'a>(node: &'a Value, out: &mut Vec<&'a serde_json::Map<String, Value>>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n {
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(map) => {
                if map.get("k").and_then(|k| k.as_str()) == Some("Binary") {
                    out.push(map);
                }
                stack.extend(map.values());
            }
            _ => {}
        }
    }
}

/// One comparison of a field against another member of the same enum.
pub(super) struct Comparison {
    pub(super) op: String,
    pub(super) row: String,
    pub(super) dim: String,
}

/// Every comparison of `field` against ANOTHER member of the same enum, inside the class
/// that DECLARES the field.
///
/// Deduplicated by (operator, other side): the same condition is serialized into several
/// `expressions` rows as its parent conjunctions are recorded, and counting those as several
/// comparisons would make an unambiguous class look ambiguous.
pub(super) fn comparisons_in(
    expr_by_file: &IndexMap<String, Vec<&Row>>,
    mem: &MemberEnums,
    idx: &MemberIdx,
    cls: &Class,
    field: &str,
    enum_id: &str,
) -> Vec<Comparison> {
    let mut out: Vec<Comparison> = Vec::new();
    let mut seen: IndexSet<String> = IndexSet::new();
    let Some(file) = cls.file.as_deref() else { return out };
    let Some(rows) = expr_by_file.get(file) else { return out };
    for r in rows {
        let (Some(line), Some(from), Some(to)) = (int_of(r.get("line")), cls.line, cls.end_line)
        else {
            continue;
        };
        if line < from || line > to {
            continue;
        }
        let Some(ast) = r.get("ast").filter(|a| truthy(Some(a))) else { continue };
        let unwrapped = unwrap(ast);
        let mut found = Vec::new();
        binaries(unwrapped, &mut found);
        for b in found {
            let op = str_of(b.get("op")).unwrap_or_default();
            if !EQ.contains(&op.as_str()) && !NE.contains(&op.as_str()) {
                continue;
            }
            let mut sides: [Option<String>; 2] = [None, None];
            for (i, side) in ["left", "right"].iter().enumerate() {
                if let Some(s) = b.get(*side).and_then(|s| s.as_object())
                    && matches!(s.get("k").and_then(|k| k.as_str()), Some("Read") | Some("SafeRead"))
                {
                    sides[i] = member_at(idx, s.get("target"));
                }
            }
            let at = match sides.iter().position(|s| s.as_deref() == Some(field)) {
                Some(i) => i,
                None => continue,
            };
            let Some(other) = sides[1 - at].clone() else { continue };
            if other == field || mem.typed.get(&other).map(String::as_str) != Some(enum_id) {
                continue;
            }
            let key = format!("{op}|{other}");
            if !seen.insert(key) {
                continue;
            }
            let dim = idx.name_of.get(&other).cloned().unwrap_or_else(|| other.clone());
            out.push(Comparison { op, row: other, dim });
        }
    }
    // BY COLLATION, the comparator the rest of the closure sorts with.
    out.sort_by(|a, b| {
        sort_key(&format!("{}{}", a.dim, a.op)).cmp(&sort_key(&format!("{}{}", b.dim, b.op)))
    });
    out
}

#[derive(Clone)]
pub(super) struct Class {
    pub(super) id: String,
    pub(super) file: Option<String>,
    pub(super) line: Option<i64>,
    pub(super) end_line: Option<i64>,
    /// `extends`, which carries the base's file so a shared name cannot merge two.
    pub(super) ext: Option<Value>,
}

pub(super) struct ClassIdx {
    pub(super) by_id: IndexMap<String, Class>,
    pub(super) by_name: IndexMap<String, Vec<Decl>>,
}

/// The class hierarchy, by DECLARATION.
pub(super) fn class_index(store: &Store<'_>) -> ClassIdx {
    let mut file_path: IndexMap<String, Option<String>> = IndexMap::new();
    for f in store.table("files").iter() {
        if let Some(id) = id_of(f, "id") {
            file_path.insert(id, str_of(f.get("path")));
        }
    }
    let mut idx = ClassIdx { by_id: IndexMap::new(), by_name: IndexMap::new() };
    for c in store.table("classes").iter() {
        let Some(id) = id_of(c, "id") else { continue };
        let ext = c.get("extends").filter(|v| v.is_object()).cloned();
        idx.by_id.insert(
            id.clone(),
            Class {
                id: id.clone(),
                file: id_of(c, "file"),
                line: int_of(c.get("line")),
                end_line: int_of(c.get("end_line")),
                ext,
            },
        );
        let path = id_of(c, "file").and_then(|f| file_path.get(&f).cloned()).flatten();
        let name = str_of(c.get("name")).unwrap_or_default();
        idx.by_name
            .entry(name)
            .or_default()
            .push(Decl { path: slash_str(path.as_deref()), id });
    }
    idx
}

/// A class and every ancestor, CYCLE-SAFE — `extends` is data, and data can name a loop.
pub(super) fn chain_of(idx: &ClassIdx, class_id: &str) -> Vec<Class> {
    let mut out = Vec::new();
    let mut seen: IndexSet<String> = IndexSet::new();
    let mut cur = idx.by_id.get(class_id).cloned();
    while let Some(c) = cur {
        if !seen.insert(c.id.clone()) {
            break;
        }
        let ext = c.ext.clone();
        out.push(c);
        let Some(ext) = ext.as_ref().and_then(|e| e.as_object()) else { break };
        let Some(name) = str_of(ext.get("name")) else { break };
        let file_name = slash(ext.get("file"));
        cur = None;
        for cand in idx.by_name.get(&name).map(Vec::as_slice).unwrap_or(&[]) {
            if file_name.is_empty()
                || file_name == cand.path
                || file_name.ends_with(&format!("/{}", cand.path))
            {
                cur = idx.by_id.get(&cand.id).cloned();
                break;
            }
        }
    }
    out
}
