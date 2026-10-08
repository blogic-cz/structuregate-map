//! WHICH CAPABILITY A GATE NECESSARILY REQUIRES — every template gate resolved to the
//! FlagCodes that must be granted for it to render.
//!
//! A template gate reaches a feature through a PROPERTY (`ngIf="isExportButtonVisible"`)
//! in the common case — a gate expression rarely calls the check directly — but a
//! direct call is resolved too, by the same declaration identity, so the day one appears
//! it is not silently ungated.
//!
//! POSITIVE OCCURRENCES ONLY. "Renders when the feature is OFF" is not a grantable gate,
//! so the polarity is read and then dropped.
//!
//! NONE OF THE THREE PASSES MAY ATTRIBUTE A PROPERTY THE TEMPLATE WRITES. All three reason
//! over "every write to this property" out of `assignments`, and a two-way binding's write
//! is not there — Angular desugars `[(value)]="prop"` into an OUTPUT binding whose handler
//! is `prop =$event`, which lands in `bindings` alone.

use super::feature_writes::{
    callback_checks, callback_features, property_features, slash, ternary_features, Check, Codes,
    Features, Owner,
};
use super::featurewalk::{codes_in, necessary, OrderedSet};
use super::store::{Row, Store};
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::HashSet;

/// A write that names a PROPERTY. `local` and `local-indexed` write a VARIABLE, and
/// `indexed`'s target is a whole receiver PATH, never a bare property name. All three were
/// keyed as writes to `(enclosing class, that text)`: noise for the path shapes, a real
/// defect for `local`, where `const enabled = await isOn(X)` attributed X to
/// any PROPERTY of the same name.
const PROPERTY_WRITES: &[&str] = &["this", "property", "this-indexed"];

/// The gates that render ONCE PER ITEM, where an empty collection renders nothing and `[]`
/// is not truth.
const ITERATION_GATES: &[&str] = &["ngForOf"];

fn text(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn id_of(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// THE OWNER OF A WRITE IS THE CLASS THAT DECLARES THE PROPERTY, never the class the
/// statement sits in.
///
/// For most `scope='property'` writes the two differ: they resolve to a member of a
/// DIFFERENT class, so a write through a component reference was filed under the WRITER
/// while the gate reading it resolves to the OWNER. Both halves of that are wrong in the
/// same direction.
///
/// A `property` WRITE WITH NO RESOLVED TARGET IS UNATTRIBUTABLE, AND STAYS THAT WAY — IT IS
/// NOT CONTAGIOUS. Poisoning the NAME instead was MEASURED WRONG: a blindly written
/// property is also one of the gate-input names, so a by-name poison dropped gates
/// every other pass proves.
pub fn write_owners(store: &Store<'_>) -> impl Fn(&Row) -> Option<String> + use<> {
    let mut member_class: IndexMap<String, Option<String>> = IndexMap::new();
    for r in store.table("members").iter() {
        if let Some(id) = id_of(r, "id") {
            member_class.insert(id, text(r, "class"));
        }
    }
    move |a: &Row| {
        let scope = text(a, "scope")?;
        if id_of(a, "target").is_none() || !PROPERTY_WRITES.contains(&scope.as_str()) {
            return None;
        }
        // `target_id` is the authoritative link and is present on most rows, so
        // it is read FIRST and the enclosing class is only the fallback.
        if let Some(target_id) = id_of(a, "target_id")
            && let Some(Some(got)) = member_class.get(&target_id)
        {
            return Some(got.clone());
        }
        if scope == "property" {
            return None;
        }
        text(a, "class")
    }
}

/// `Class.method` / `Interface.method` -> the declaration rows.
///
/// BOTH halves are checked, so a spec naming either wrongly resolves to nothing rather
/// than to a look-alike. Interfaces are searched too because part of an
/// app may call the feature service through an interface.
pub fn resolve_checks(store: &Store<'_>, specs: &[String]) -> Vec<Check> {
    let mut file_path: IndexMap<String, Option<String>> = IndexMap::new();
    for f in store.table("files").iter() {
        if let Some(id) = id_of(f, "id") {
            file_path.insert(id, text(f, "path"));
        }
    }
    let mut class_name: IndexMap<String, Option<String>> = IndexMap::new();
    for c in store.table("classes").iter() {
        if let Some(id) = id_of(c, "id") {
            class_name.insert(id, text(c, "name"));
        }
    }
    let mut interface_name: IndexMap<String, Option<String>> = IndexMap::new();
    for i in store.table("interfaces").iter() {
        if let Some(id) = id_of(i, "id") {
            interface_name.insert(id, text(i, "name"));
        }
    }

    let path_of = |file: Option<String>| -> String {
        let value = file
            .and_then(|f| file_path.get(&f).cloned().flatten())
            .map(Value::String);
        slash(value.as_ref())
    };

    let mut found = Vec::new();
    for spec in specs {
        let Some(dot) = spec.rfind('.') else { continue };
        let owner = &spec[..dot];
        let method = &spec[dot + 1..];

        for m in store.table("members").iter() {
            if text(m, "name").as_deref() != Some(method) {
                continue;
            }
            let declared = id_of(m, "class").and_then(|c| class_name.get(&c).cloned().flatten());
            if declared.as_deref() != Some(owner) {
                continue;
            }
            if let Some(member) = id_of(m, "id") {
                found.push(Check { member, path: path_of(id_of(m, "file")) });
            }
        }
        for tm in store.table("type_members").iter() {
            if text(tm, "name").as_deref() != Some(method) {
                continue;
            }
            let declared =
                id_of(tm, "owner").and_then(|o| interface_name.get(&o).cloned().flatten());
            if declared.as_deref() != Some(owner) {
                continue;
            }
            if let Some(member) = id_of(tm, "id") {
                found.push(Check { member, path: path_of(id_of(tm, "file")) });
            }
        }
    }
    found
}

/// `(class, property)` pairs the TEMPLATE writes, through a two-way binding.
///
/// `[(value)]="prop"` is desugared into an input binding plus an OUTPUT one whose handler
/// is `prop =$event`, and only the output records the write. It lands in `bindings`, never
/// in `assignments`.
///
/// Split on `=` rather than matched: `!==` and `>=` yield three parts and are rejected by
/// the count, and a write to a nested path does not write the property itself.
pub fn template_written(store: &Store<'_>) -> HashSet<String> {
    let mut comp_class: IndexMap<String, Option<String>> = IndexMap::new();
    for r in store.table("components").iter() {
        if let Some(id) = id_of(r, "id") {
            comp_class.insert(id, text(r, "class"));
        }
    }

    let mut out = HashSet::new();
    for b in store.table("bindings").iter() {
        if text(b, "kind").as_deref() != Some("output") {
            continue;
        }
        let Some(source) = text(b, "source") else { continue };
        let parts: Vec<&str> = source.split('=').collect();
        if parts.len() != 2 || parts[1].trim() != "$event" {
            continue;
        }
        let prop = parts[0].trim();
        if prop.is_empty() || prop.contains('.') || prop.contains('(') || prop.contains(' ') {
            continue;
        }
        if let Some(Some(cls)) = id_of(b, "component").map(|c| comp_class.get(&c).cloned().flatten())
        {
            out.insert(format!("{cls} {prop}"));
        }
    }
    out
}

/// A RESOLVED `target` -> the member row it names, for BOTH shapes the map writes one in.
///
/// A template gate's AST carries `target.row` — the member id, already joined. The
/// TypeScript side does not: a ternary condition's target is `{name, file, line}`, the
/// DECLARATION's own anchor, with the file absolute while `files.path` is workspace-relative.
/// Reading only `row` finds nothing there, and the whole ternary pass reported zero
/// properties for exactly that reason before this existed.
///
/// THE ANCHOR IS (FILE, LINE), NOT THE NAME. Two files can share a basename, so a
/// basename match would conflate them and a name match would conflate every `isEnabled` in
/// the workspace. An anchor two members share resolves to NOTHING rather than to whichever came
/// first.
pub fn declaration_index(store: &Store<'_>) -> impl Fn(Option<&Value>) -> Option<String> + use<> {
    fn tail(s: &str) -> String {
        let normalised = s.replace('\\', "/");
        let parts: Vec<&str> = normalised.split('/').collect();
        parts[parts.len().saturating_sub(2)..].join("/")
    }

    let mut by_tail: IndexMap<String, Vec<(String, String)>> = IndexMap::new();
    for f in store.table("files").iter() {
        let path = text(f, "path").unwrap_or_default();
        let Some(id) = id_of(f, "id") else { continue };
        by_tail
            .entry(tail(&path))
            .or_default()
            .push((id, format!("/{}", path.replace('\\', "/"))));
    }

    let mut decl_at: IndexMap<String, Option<String>> = IndexMap::new();
    for m in store.table("members").iter() {
        let Some(file) = id_of(m, "file") else { continue };
        let k = format!("{file} {}", id_of(m, "line").unwrap_or_else(|| "None".into()));
        if decl_at.contains_key(&k) {
            decl_at.insert(k, None);
        } else {
            decl_at.insert(k, id_of(m, "id"));
        }
    }

    move |t: Option<&Value>| -> Option<String> {
        let target = t?.as_object()?;
        if let Some(row) = target.get("row")
            && !row.is_null()
            && let Value::String(row) = row
            && !row.is_empty()
        {
            return Some(row.clone());
        }
        let Some(Value::String(file_name)) = target.get("file") else { return None };
        // `typeof t.line !== 'number'` — a bool is not a number there and must not pass
        // here either.
        let line = match target.get("line") {
            Some(Value::Number(n)) => n.clone(),
            _ => return None,
        };
        let abs_path = file_name.replace('\\', "/");
        for (id, candidate) in by_tail.get(&tail(file_name))? {
            if abs_path.ends_with(candidate) {
                let k = format!("{id} {line}");
                return decl_at.get(&k).cloned().flatten();
            }
        }
        None
    }
}

#[derive(Debug, Default)]
pub struct GateFeatures {
    /// Gate id -> the component it sits in and the codes it requires.
    pub map: IndexMap<String, (Option<String>, Vec<String>)>,
    pub checks: Vec<Check>,
    pub props: usize,
    pub direct: usize,
    pub callbacks: usize,
    pub ternaries: usize,
    /// The assignment rows the ternary pass proved, for the literal-gate pass.
    pub ternary_writes: IndexMap<String, Row>,
    /// Condition id -> the codes it necessarily requires, for the conditions handed in.
    pub conds: IndexMap<String, Vec<String>>,
}

/// A CONDITION THAT IS NOT A TEMPLATE GATE, by id: its tree, and whether it holds NEGATED
/// (an `else` branch). Read by the same resolver as every gate, so a TypeScript `if` and an
/// `*ngIf` over the same property cannot answer differently.
pub type Conds = IndexMap<String, (Value, bool)>;

/// Gate id -> the FlagCodes it necessarily requires.
///
/// `declared` is the feature API this map was extracted with. It is handed in rather than
/// read back out of the database, because the half that wrote the rows is the half that
/// read the config, and a second reader of the same declaration is a second thing to
/// drift.
pub fn gate_features(
    store: &Store<'_>,
    checks_declared: &[String],
    enum_declared: &str,
    note: &mut dyn FnMut(&str),
) -> GateFeatures {
    gate_features_with(store, checks_declared, enum_declared, &Conds::new(), note)
}

/// `gate_features`, plus the codes each of `conds` necessarily requires.
pub fn gate_features_with(
    store: &Store<'_>,
    checks_declared: &[String],
    enum_declared: &str,
    conds: &Conds,
    note: &mut dyn FnMut(&str),
) -> GateFeatures {
    if checks_declared.is_empty() || enum_declared.is_empty() {
        // NOTHING DECLARED IS AN ANSWER, and it is said out loud rather than returned
        // silently: no rows would otherwise read as "no gate in this workspace requires a
        // capability", which is a claim about the application instead of its configuration.
        note(
            "no feature API declared - gate_features stays empty. Declare \"featureChecks\" + \
             \"featureEnum\" in structuregate.ts.json and map again",
        );
        return GateFeatures::default();
    }

    let root = enum_declared;
    let checks = resolve_checks(store, checks_declared);
    let check_members: HashSet<String> = checks.iter().map(|c| c.member.clone()).collect();
    let written = template_written(store);
    let owner = write_owners(store);
    let owner_ref: &Owner<'_> = &owner;

    let mut props = property_features(store, &checks, root, &written, owner_ref);
    let direct = props.len();

    let callbacks = callback_checks(store, &check_members, root);
    for (k, codes) in callback_features(store, &callbacks, &written, owner_ref) {
        let bucket = props.entry(k).or_default();
        for c in codes.keys() {
            bucket.insert(c.clone(), true);
        }
    }

    let mut member_class: IndexMap<String, Option<String>> = IndexMap::new();
    for r in store.table("members").iter() {
        if let Some(id) = id_of(r, "id") {
            member_class.insert(id, text(r, "class"));
        }
    }
    let member_at = declaration_index(store);

    // A GUARDED TERNARY CAN GATE A PROPERTY THAT GATES ANOTHER, so this runs to a FIXED
    // POINT rather than once. `props` only ever grows, over a finite (property, code)
    // domain, which is what terminates it.
    loop {
        let feats = {
            let mut resolve =
                make_resolve(&props, None, &member_class, &member_at, &check_members, root);
            ternary_features(store, &mut resolve, false, &written, owner_ref).0
        };
        let mut grew = false;
        for (k, codes) in feats {
            let bucket = props.entry(k).or_default();
            for c in codes.keys() {
                if bucket.insert(c.clone(), true).is_none() {
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }

    let (iter_props, ternary_writes) = {
        let mut resolve =
            make_resolve(&props, None, &member_class, &member_at, &check_members, root);
        ternary_features(store, &mut resolve, true, &written, owner_ref)
    };

    let expressions = store.table("expressions");
    let mut expr_by_id: IndexMap<String, &Row> = IndexMap::new();
    for e in expressions.iter() {
        if let Some(id) = id_of(e, "id") {
            expr_by_id.insert(id, e);
        }
    }

    let mut out: IndexMap<String, (Option<String>, Vec<String>)> = IndexMap::new();
    for g in store.table("gates").iter() {
        let Some(e) = id_of(g, "expression").and_then(|x| expr_by_id.get(&x)) else { continue };
        let Some(ast) = e.get("ast").filter(|a| !a.is_null()) else { continue };

        let iterated = text(g, "name")
            .map(|n| ITERATION_GATES.contains(&n.as_str()))
            .unwrap_or(false);
        let extra = if iterated { Some(&iter_props) } else { None };

        let mut got = OrderedSet::new();
        {
            let mut resolve =
                make_resolve(&props, extra, &member_class, &member_at, &check_members, root);
            necessary(ast, false, &mut resolve, &mut got);
        }
        if !got.is_empty()
            && let Some(id) = id_of(g, "id")
        {
            let mut features = got.strings();
            features.sort();
            out.insert(id, (text(g, "component"), features));
        }
    }

    let mut cond_out: IndexMap<String, Vec<String>> = IndexMap::new();
    for (id, (ast, negated)) in conds {
        let mut got = OrderedSet::new();
        {
            let mut resolve =
                make_resolve(&props, None, &member_class, &member_at, &check_members, root);
            necessary(ast, *negated, &mut resolve, &mut got);
        }
        if !got.is_empty() {
            let mut features = got.strings();
            features.sort();
            cond_out.insert(id.clone(), features);
        }
    }

    GateFeatures {
        map: out,
        checks,
        props: props.len(),
        direct,
        callbacks: callbacks.len(),
        ternaries: iter_props.len(),
        ternary_writes,
        conds: cond_out,
    }
}

/// The polarity walk's leaf resolver.
///
/// A node that is not a Read or a Call carries no property to resolve — a `===` comparison
/// is a leaf here too, and means nothing to a feature. `extra` is the iteration-only half
/// of the ternary pass, handed in only for the gates that may read it.
fn make_resolve<'a>(
    props: &'a Features,
    extra: Option<&'a Features>,
    member_class: &'a IndexMap<String, Option<String>>,
    member_at: &'a impl Fn(Option<&Value>) -> Option<String>,
    check_members: &'a HashSet<String>,
    root: &'a str,
) -> impl FnMut(&Value, bool) -> Vec<Value> + 'a {
    move |n: &Value, negated: bool| -> Vec<Value> {
        let mut got: Codes = IndexMap::new();
        let Some(object) = n.as_object() else { return Vec::new() };
        let kind = object.get("k").and_then(|k| k.as_str()).unwrap_or("");
        if negated || !matches!(kind, "Read" | "Call" | "SafeRead") {
            return Vec::new();
        }

        let row = member_at(object.get("target"));
        if let Some(row) = &row
            && check_members.contains(row)
        {
            for (c, _) in codes_in(n, root) {
                got.insert(c, true);
            }
        }
        let Some(Some(cls)) = row.and_then(|r| member_class.get(&r).cloned()) else {
            return got.into_keys().map(Value::String).collect();
        };

        let name = object.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let k = format!("{cls} {name}");
        if let Some(codes) = props.get(&k) {
            for c in codes.keys() {
                got.insert(c.clone(), true);
            }
        }
        if let Some(codes) = extra.and_then(|e| e.get(&k)) {
            for c in codes.keys() {
                got.insert(c.clone(), true);
            }
        }
        got.into_keys().map(Value::String).collect()
    }
}

#[cfg(test)]
#[path = "gate_features_tests.rs"]
mod tests;
