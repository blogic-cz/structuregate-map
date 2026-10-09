//! AN OBSERVABLE OF A SELECTOR FACTORY HANDED ONE ENUM MEMBER — `*ngIf="alpha$ | async"` over
//! `alpha$ = this.store.select(hasItemOfTone(Tone.Alpha))` — as the set its projector tests.
//!
//! The factory is found by DECLARATION: the call's evaluated `$target` names a `const` (or a key of
//! a `const` object) whose `$fn` arrow is a `functions` row of one enum-typed parameter. Its one
//! return is `createSelector(..., (items) => items.some((i) => i.F === p))`, and only that projector
//! proves anything: true means an item whose `F` is the member is in the store. So the row is a SET
//! row, `in` the member, dimension `<factory>.<F>`.
//!
//! TRUE ONLY. `async` is null until the first emit, so `!(alpha$ | async)` holds before anything was
//! selected and proves nothing. A property written again after its initializer, a factory of any
//! other shape, and a projector testing anything else stay unread.

use super::gate_predicates::{kind, local, method_call, only_return, param_enum};
use super::gate_values::EnumIndex;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

fn text<'a>(n: &'a Value, field: &str) -> Option<&'a str> {
    n.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// An evaluated `$call` aimed at a declaration under `@ngrx/store` named `name`.
fn ngrx(v: &Value, name: &str) -> bool {
    let target = v.get("$target");
    target.and_then(|t| text(t, "name")) == Some(name)
        && target.and_then(|t| text(t, "file")).is_some_and(|f| f.replace('\\', "/").contains("/@ngrx/store/"))
}

/// What the store's rows say about the factories, read once.
struct Decls<'s> {
    /// File row id -> its path, slash-led so an absolute `$target.file` can end with it.
    paths: IndexMap<String, String>,
    /// (file id, name) -> the `$expr_id` of every `$fn` a const declares under that name.
    arrows: IndexMap<(String, String), Vec<String>>,
    /// Expression id -> (file, line, col).
    at: IndexMap<String, (String, String, String)>,
    /// (file, line, col) -> the top-level `functions` row there.
    fns: IndexMap<(String, String, String), &'s Row>,
    /// (file id, name) -> every top-level `functions` row of that name: a `const` arrow's row sits at
    /// its NAME, not at the arrow its `$expr_id` points to.
    named: IndexMap<(String, String), Vec<&'s Row>>,
    /// Function id -> the parameter names every function nested in it binds.
    inner: IndexMap<String, IndexSet<String>>,
    asts: IndexMap<String, &'s Row>,
}

impl<'s> Decls<'s> {
    fn new(files: &'s [Row], consts: &'s [Row], exprs: &'s [Row], functions: &'s [Row]) -> Decls<'s> {
        let mut d = Decls { paths: IndexMap::new(), arrows: IndexMap::new(), at: IndexMap::new(),
                            fns: IndexMap::new(), named: IndexMap::new(), inner: IndexMap::new(), asts: IndexMap::new() };
        for f in files {
            if let (Some(id), Some(path)) = (cell(f, "id"), cell(f, "path")) {
                d.paths.insert(id, format!("/{}", path.replace('\\', "/")));
            }
        }
        for k in consts {
            let (Some(file), Some(name), Some(value)) = (cell(k, "file"), cell(k, "name"), k.get("value")) else { continue };
            let mut add = |key: &str, v: &Value| {
                if let (Some(_), Some(x)) = (v.get("$fn"), v.get("$expr_id").and_then(|x| x.as_str())) {
                    d.arrows.entry((file.clone(), key.to_string())).or_default().push(x.to_string());
                }
            };
            add(&name, value);
            for (key, v) in value.as_object().into_iter().flatten().filter(|(k, _)| !k.starts_with('$')) {
                add(key, v);
            }
        }
        for e in exprs {
            if let (Some(id), Some(file), Some(line), Some(col)) = (cell(e, "id"), cell(e, "file"), cell(e, "line"), cell(e, "col")) {
                d.at.insert(id.clone(), (file, line, col));
                d.asts.insert(id, e);
            }
        }
        let parent_of: IndexMap<String, String> = functions.iter()
            .filter_map(|f| Some((cell(f, "id")?, cell(f, "parent")?))).collect();
        for f in functions {
            let (Some(id), Some(file), Some(line), Some(col)) = (cell(f, "id"), cell(f, "file"), cell(f, "line"), cell(f, "col")) else { continue };
            if !parent_of.contains_key(&id) {
                if let Some(name) = cell(f, "name") {
                    d.named.entry((file.clone(), name)).or_default().push(f);
                }
                d.fns.insert((file, line, col), f);
                continue;
            }
            let names: Vec<String> = f.get("params").and_then(|p| p.as_array()).into_iter().flatten()
                .filter_map(|p| text(p, "name").map(str::to_string)).collect();
            let mut up = parent_of.get(&id);
            while let Some(p) = up {
                d.inner.entry(p.clone()).or_default().extend(names.iter().cloned());
                up = parent_of.get(p);
            }
        }
        d
    }

    /// The factory a `$call`'s target names: its one function row, through the const that holds it.
    fn factory(&self, target: &Value) -> Option<&'s Row> {
        let (name, abs) = (text(target, "name")?, text(target, "file")?.replace('\\', "/"));
        let files: Vec<&String> = self.paths.iter().filter(|(_, p)| abs.ends_with(p.as_str())).map(|(id, _)| id).collect();
        if let [file] = files.as_slice()
            && let Some([one]) = self.named.get(&((*file).clone(), name.to_string())).map(|v| v.as_slice())
        {
            return Some(one);
        }
        let mut found = files.iter().filter_map(|id| self.arrows.get(&((*id).clone(), name.to_string()))).flatten();
        let (Some(x), None) = (found.next(), found.next()) else { return None };
        self.fns.get(self.at.get(x)?).copied()
    }
}

/// `items.some((i) => i.F === p)` over the projector's own `items`, as `F`.
fn tested_field(n: &Value, param: &str, inner: &IndexSet<String>) -> Option<String> {
    let (receiver, callback) = method_call(n, "some")?;
    let test = only_return(callback)?;
    let bound = |v: &Value| local(v).is_some_and(|name| name != param && inner.contains(name));
    if !bound(receiver) {
        return None;
    }
    if kind(test) != Some("Binary") || !matches!(text(test, "op"), Some("===") | Some("==")) {
        return None;
    }
    let (left, right) = (test.get("left")?, test.get("right")?);
    let field = if local(right) == Some(param) { left } else if local(left) == Some(param) { right } else { return None };
    (kind(field) == Some("Read") && bound(field.get("receiver")?)).then(|| text(field, "name").map(str::to_string))?
}

/// Property row id -> the SET row its observable proves when true.
pub struct Selectors {
    by_prop: IndexMap<String, Value>,
}

impl Selectors {
    pub fn new(store: &Store<'_>, idx: &EnumIndex) -> Selectors {
        let (files, consts, exprs, functions, returns) = (store.table("files"), store.table("consts"),
            store.table("expressions"), store.table("functions"), store.table("returns"));
        let decls = Decls::new(&files, &consts, &exprs, &functions);
        let written: IndexSet<String> = store.table("assignments").iter().filter_map(|a| cell(a, "target_id")).collect();
        let mut returned: IndexMap<String, Vec<String>> = IndexMap::new();
        for r in returns.iter() {
            if let (Some(m), Some(x)) = (cell(r, "member"), cell(r, "expression")) {
                returned.entry(m).or_default().push(x);
            }
        }
        let mut by_prop = IndexMap::new();
        for m in store.table("members").iter() {
            let (Some(id), Some(value)) = (cell(m, "id"), m.get("value")) else { continue };
            if cell(m, "kind").as_deref() != Some("property") || written.contains(&id) || !ngrx(value, "select") {
                continue;
            }
            let Some([made]) = value.get("$args").and_then(|a| a.as_array()).map(|a| a.as_slice()) else { continue };
            let Some([arg]) = made.get("$args").and_then(|a| a.as_array()).map(|a| a.as_slice()) else { continue };
            let Some(row) = made.get("$target").and_then(|t| decls.factory(t))
                .and_then(|f| Self::proves(f, &decls, &returned, idx)) else { continue };
            let member = text(arg, "$enum").and_then(|e| e.rsplit('.').next());
            let (Some(member), Some(name)) = (member, made.get("$target").and_then(|t| text(t, "name"))) else { continue };
            if idx.by_id.get(&row.0).is_some_and(|e| e.domain.contains(member)) {
                by_prop.insert(id, json!({"enum": row.0, "dim": format!("{name}.{}", row.1), "row": null,
                                          "op": "in", "values": [member], "set": true}));
            }
        }
        Selectors { by_prop }
    }

    /// (enum, tested field) when the factory's one return is a `createSelector` whose projector tests
    /// its items' field against the factory's one enum-typed parameter.
    fn proves(f: &Row, decls: &Decls<'_>, returned: &IndexMap<String, Vec<String>>, idx: &EnumIndex) -> Option<(String, String)> {
        let id = cell(f, "id")?;
        let Some([p]) = f.get("params").and_then(|p| p.as_array()).map(|p| p.as_slice()) else { return None };
        let (enum_id, false) = param_enum(p, idx)? else { return None };
        let param = text(p, "name")?;
        let inner = decls.inner.get(&id)?;
        let Some([x]) = returned.get(&id).map(|r| r.as_slice()) else { return None };
        let call = super::astreads::unwrap(decls.asts.get(x)?.get("ast")?);
        let callee = call.get("receiver")?;
        if kind(call) != Some("Call") || text(callee, "name") != Some("createSelector") || inner.contains(param) {
            return None;
        }
        let projector = call.get("args")?.as_array()?.last()?;
        Some((enum_id, tested_field(only_return(projector)?, param, inner)?))
    }

    /// The row an `X | async` proves, for a property read here; nothing when negated.
    pub fn restriction(&self, n: &Value, negated: bool) -> Option<Value> {
        if negated || kind(n) != Some("Pipe") || text(n, "name") != Some("async") {
            return None;
        }
        let exp = n.get("exp")?;
        let own = matches!(exp.get("receiver").and_then(|r| text(r, "k")), Some("This" | "Implicit"));
        let row = exp.get("target").and_then(|t| text(t, "row")).filter(|_| own && kind(exp) == Some("Read"))?;
        self.by_prop.get(row).cloned()
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::rows::ts::gate_values::EnumInfo;
    use serde_json::Map;

    const NGRX: &str = "/ws/node_modules/@ngrx/store/src/store.d.ts";

    fn idx() -> EnumIndex {
        let mut idx = EnumIndex::default();
        let domain = ["Alpha", "Beta", "Gamma"].iter().map(|s| s.to_string()).collect();
        idx.by_id.insert("e:1".to_string(), EnumInfo { name: Some("Tone".to_string()), domain });
        idx.by_decl.insert("Tone".to_string(), vec![("app/tone.ts".to_string(), "e:1".to_string())]);
        idx
    }

    fn bare(name: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": {"k": "Implicit"}})
    }

    /// `(tone: Tone) => createSelector(selectAll, (items) => items.some((i) => <test>))`, the const
    /// holding it, and a property selecting it for `Tone.<member>`.
    fn tables(test: Value, member: &str) -> Value {
        let some = json!({"k": "Call", "receiver": {"k": "Read", "name": "some", "receiver": bare("items")},
                          "args": [{"k": "Fn", "returns": [test]}]});
        let made = json!({"k": "Call", "receiver": {"k": "Read", "name": "createSelector", "receiver": {"k": "Implicit"}},
                          "args": [bare("selectAll"), {"k": "Fn", "returns": [some]}]});
        let tone = json!({"name": "tone", "type": "Tone", "type_ref": {"name": "Tone", "file": "/ws/app/tone.ts"}});
        json!({
            "files": [{"id": "f:1", "path": "app/sel.ts"}],
            "consts": [{"id": "k:1", "file": "f:1", "name": "hasTone", "value": {"$fn": "(tone) => ...", "$expr_id": "x:fn"}}],
            "expressions": [{"id": "x:fn", "file": "f:1", "line": 4, "col": 22, "ast": {"k": "Fn", "returns": [made]}},
                            {"id": "x:ret", "file": "f:1", "line": 4, "col": 40, "ast": made}],
            "functions": [{"id": "fn:1", "file": "f:1", "line": 4, "col": 22, "name": "hasTone", "params": [tone]},
                          {"id": "fn:2", "file": "f:1", "line": 4, "col": 60, "parent": "fn:1", "params": [{"name": "items"}]},
                          {"id": "fn:3", "file": "f:1", "line": 4, "col": 80, "parent": "fn:2", "params": [{"name": "i"}]}],
            "returns": [{"member": "fn:1", "expression": "x:ret"}],
            "members": [{"id": "m:1", "kind": "property", "name": "alpha$", "value": {"$call": "this.store.select",
                "$target": {"name": "select", "file": NGRX},
                "$args": [{"$call": "hasTone", "$target": {"name": "hasTone", "file": "/ws/app/sel.ts"},
                           "$args": [{"$enum": format!("Tone.{member}"), "value": member}]}]}}],
            "assignments": []
        })
    }

    fn same_tone() -> Value {
        json!({"k": "Binary", "op": "===", "left": {"k": "Read", "name": "tone", "receiver": bare("i")}, "right": bare("tone")})
    }

    fn selectors(t: Value) -> Selectors {
        let map: Map<String, Value> = t.as_object().cloned().expect("tables");
        Selectors::new(&Store::from_payload(map, "typescript"), &idx())
    }

    fn piped() -> Value {
        json!({"k": "Pipe", "name": "async", "exp": {"k": "Read", "name": "alpha$", "receiver": {"k": "Implicit"},
                                                     "target": {"row": "m:1"}}, "args": []})
    }

    #[test]
    fn an_async_selector_for_one_member_is_a_set_row_on_the_tested_field_when_true() {
        let s = selectors(tables(same_tone(), "Alpha"));
        let row = s.restriction(&piped(), false).expect("read");
        assert_eq!(row, json!({"enum": "e:1", "dim": "hasTone.tone", "row": null, "op": "in", "values": ["Alpha"], "set": true}));
        assert_eq!(s.restriction(&piped(), true), None, "null before the first emit proves nothing");
    }

    #[test]
    fn a_projector_testing_anything_else_or_a_property_written_again_is_unread() {
        let other = json!({"k": "Read", "name": "on", "receiver": bare("i")});
        assert_eq!(selectors(tables(other, "Alpha")).restriction(&piped(), false), None);
        let mut t = tables(same_tone(), "Alpha");
        t["assignments"] = json!([{"target_id": "m:1", "operator": "="}]);
        assert_eq!(selectors(t).restriction(&piped(), false), None);
        let mut t = tables(same_tone(), "Alpha");
        t["members"][0]["value"]["$target"]["file"] = json!("/ws/app/my-store.ts");
        assert_eq!(selectors(t).restriction(&piped(), false), None, "only the ngrx store's select");
    }

    #[test]
    fn a_factory_held_as_a_key_of_a_const_object_resolves_like_a_const() {
        let mut t = tables(same_tone(), "Beta");
        t["consts"] = json!([{"id": "k:1", "file": "f:1", "name": "toneSelectors",
                              "value": {"hasTone": {"$fn": "(tone) => ...", "$expr_id": "x:fn"}}}]);
        t["functions"][0]["name"] = Value::Null;
        let row = selectors(t).restriction(&piped(), false).expect("read");
        assert_eq!(row["values"], json!(["Beta"]));
    }
}
