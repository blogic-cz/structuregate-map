//! A TYPESCRIPT TREE'S READS, TIED TO THE ROWS THEY DECLARE — so a class's own condition is
//! read by the walks written for template gates.
//!
//! A template read carries `target.row`, the declaration already joined. A TypeScript read
//! carries `{name, file, line}`, the declaration's own anchor, with the file ABSOLUTE while
//! `files.path` is relative to the frontend. Every reader of a restriction asks for `row`,
//! so without this step a TypeScript `if` resolved nothing at all.
//!
//! THE ANCHOR IS (FILE, LINE), NEVER THE NAME — the rule `gate_features::declaration_index`
//! states for members, over every table a read can land on. An anchor two rows share
//! resolves to NOTHING rather than to whichever came first — with ONE exception, which is not
//! two declarations at all: `const isX = (id) => …` is a `consts` row AND the `functions` row
//! of its arrow, on the same line under the same name. Refusing it left every predicate
//! written as an arrow const unread; the function is the one a call runs.
//!
//! Two marks are added beside the row, because the TypeScript tree has no other way to say
//! them:
//!   * `target.enum` — the read IS a member of that enum (`TierIDs.Small`). A template
//!     reaches a constant through an alias member; a class names the enum itself.
//!   * `lambda: true` — a bare name one of the enclosing member's own callbacks binds. The
//!     tree keeps no parameter list, so `ids.some((x) => x === y)` cannot otherwise say
//!     which side is the element.
//!
//! A `const` LOCAL OF THE ENCLOSING MEMBER is resolved too, when the member is named: a list
//! of enum members becomes the local's row (`const picked = [TierIDs.A, …]; picked.some(…)`), and
//! an arrow is INLINED by its tree (`const hasTier = (e) => e === t.TierID;
//! picked.some(hasTier)`), so the list readers see the shape they read. A `let` is never
//! resolved: it may hold something else by the time the condition runs.

use super::astreads::is_read;
use super::gate_values::{enum_of_type_ref, EnumIndex};
use super::store::Store;
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Map, Value};

/// The tables a TypeScript read can land on, by their own (file, line).
const DECLARING: &[&str] = &["members", "type_members", "functions", "consts", "enums"];

fn tail(s: &str) -> String {
    let normalised = s.replace('\\', "/");
    let parts: Vec<&str> = normalised.split('/').collect();
    parts[parts.len().saturating_sub(2)..].join("/")
}

fn cell(v: Option<&Value>) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

#[derive(Debug, Default)]
pub struct TsRows {
    by_tail: IndexMap<String, Vec<(String, String)>>,
    decl_at: IndexMap<String, Option<String>>,
    /// `member + " " + name` -> a `const` local: its row, and the tree of an arrow it holds.
    locals: IndexMap<String, (String, Option<Value>)>,
    /// `file + " " + name` -> the class members of that name in that file.
    members_named: IndexMap<String, Vec<String>>,
}

/// A bare name that resolved to nothing: a local, a parameter, a callback's element.
pub(crate) fn bare(o: &Map<String, Value>) -> Option<&str> {
    let implicit = o.get("receiver").and_then(|r| r.get("k")).and_then(|k| k.as_str()) == Some("Implicit");
    let read = o.get("k").and_then(|k| k.as_str()) == Some("Read");
    (read && implicit && !o.contains_key("target")).then(|| o.get("name").and_then(|n| n.as_str()))?
}

impl TsRows {
    pub fn new(store: &Store<'_>) -> TsRows {
        let mut out = TsRows::default();
        for f in store.table("files").iter() {
            let path = cell(f.get("path")).unwrap_or_default();
            let Some(id) = cell(f.get("id")) else { continue };
            let slashed = format!("/{}", path.replace('\\', "/"));
            out.by_tail.entry(tail(&path)).or_default().push((id, slashed));
        }
        let mut at: IndexMap<String, Vec<(&str, Option<String>, Option<String>)>> = IndexMap::new();
        for m in store.table("members").iter() {
            if let (Some(file), Some(name), Some(id)) = (cell(m.get("file")), cell(m.get("name")), cell(m.get("id"))) {
                out.members_named.entry(format!("{file} {name}")).or_default().push(id);
            }
        }
        for table in DECLARING {
            for r in store.table(table).iter() {
                let (Some(file), Some(line)) = (cell(r.get("file")), cell(r.get("line"))) else { continue };
                at.entry(format!("{file} {line}")).or_default().push((table, cell(r.get("id")), cell(r.get("name"))));
            }
        }
        for (k, rows) in at {
            let id = match rows.as_slice() {
                [(_, id, _)] => id.clone(),
                [(a, x, xn), (b, y, yn)] if xn.is_some() && xn == yn => match (*a, *b) {
                    ("consts", "functions") => y.clone(),
                    ("functions", "consts") => x.clone(),
                    _ => None,
                },
                _ => None,
            };
            out.decl_at.insert(k, id);
        }
        let mut wanted: IndexMap<String, String> = IndexMap::new();
        for l in store.table("locals").iter() {
            let (Some(id), Some(m), Some(n)) = (cell(l.get("id")), cell(l.get("member")), cell(l.get("name"))) else { continue };
            if cell(l.get("declared")).as_deref() != Some("const") {
                continue;
            }
            let key = format!("{m} {n}");
            // A NAME DECLARED TWICE IN ONE MEMBER (two blocks) is two locals, and neither is it.
            if out.locals.contains_key(&key) {
                out.locals.insert(key, (String::new(), None));
                continue;
            }
            let value = l.get("value");
            let arrow = value.and_then(|v| v.get("$fn")).and(value.and_then(|v| v.get("$expr_id"))).and_then(|x| x.as_str());
            if let Some(x) = arrow {
                wanted.insert(x.to_string(), key.clone());
            }
            out.locals.insert(key, (id, None));
        }
        for e in store.table("expressions").iter() {
            let Some(key) = cell(e.get("id")).and_then(|x| wanted.get(&x)) else { continue };
            if let (Some(slot), Some(ast)) = (out.locals.get_mut(key), e.get("ast")) {
                slot.1 = Some(super::astreads::unwrap(ast).clone());
            }
        }
        out
    }

    /// The row a `{name, file, line}` target declares, or nothing.
    /// The row a target names - ANY of its declarations, tried in path and line order. A union's property
    /// has one per member type, and which one the checker lists first follows the order it was asked in: a
    /// full and a partial run of the same tree put `Unit.model.ts` and `UnitDto.model.ts` first in turn,
    /// and a gate resolved through the first alone had a row in one map and none in the other.
    fn row_of(&self, target: &Map<String, Value>) -> Option<String> {
        let place = |t: &Map<String, Value>| match (t.get("file"), t.get("line")) {
            (Some(Value::String(file)), Some(Value::Number(line))) => Some((file.replace('\\', "/"), line.to_string())),
            _ => None,
        };
        let mut places: Vec<(String, String)> = place(target).into_iter().collect();
        for also in target.get("also").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object) {
            places.extend(place(also));
        }
        places.sort();
        places.iter().find_map(|(abs, line)| {
            let (id, _) = self.by_tail.get(&tail(abs))?.iter().find(|(_, p)| abs.ends_with(p.as_str()))?;
            self.decl_at.get(&format!("{id} {line}")).cloned().flatten()
        })
    }

    /// The one declaration at (`file` row id, `line`): the anchor a stored tree carries.
    pub fn declared_at(&self, file: &str, line: &str) -> Option<String> {
        self.decl_at.get(&format!("{file} {line}")).cloned().flatten()
    }

    /// The ONE member called `name` in the file a `{name, file}` target names - the anchor a call's
    /// evaluated `$target` carries, which has no line. Two of that name in the file are nothing.
    pub fn member_named(&self, target: &Value) -> Option<String> {
        let (name, abs) = (target.get("name")?.as_str()?, target.get("file")?.as_str()?.replace('\\', "/"));
        let (id, _) = self.by_tail.get(&tail(&abs))?.iter().find(|(_, p)| abs.ends_with(p.as_str()))?;
        match self.members_named.get(&format!("{id} {name}"))?.as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        }
    }

    /// A copy of `ast` with every TypeScript target resolved to its row. `bound` is the names
    /// the enclosing member's callbacks bind.
    pub fn resolve(&self, ast: &Value, bound: &IndexSet<String>, idx: &EnumIndex) -> Value {
        self.resolve_in(ast, bound, idx, None)
    }

    /// `resolve`, with the `const` locals of `member` in scope.
    pub fn resolve_in(&self, ast: &Value, bound: &IndexSet<String>, idx: &EnumIndex, member: Option<&str>) -> Value {
        let mut out = ast.clone();
        self.walk(&mut out, bound, idx, member, 0);
        out
    }

    fn walk(&self, n: &mut Value, bound: &IndexSet<String>, idx: &EnumIndex, member: Option<&str>, depth: usize) {
        // A CONST LOCAL, before anything reads the node as the bare name it is.
        let local = match (member, n.as_object().and_then(bare)) {
            (Some(m), Some(name)) if !bound.contains(name) => self.locals.get(&format!("{m} {name}")).filter(|(row, _)| !row.is_empty()),
            _ => None,
        };
        if let Some((row, arrow)) = local {
            match arrow {
                Some(tree) if depth < 4 => {
                    *n = tree.clone();
                    self.walk(n, bound, idx, member, depth + 1);
                }
                Some(_) => {}
                None => {
                    if let Some(o) = n.as_object_mut() {
                        o.insert("target".into(), json!({"row": row}));
                    }
                }
            }
            return;
        }
        let object = match n {
            Value::Array(items) => {
                items.iter_mut().for_each(|i| self.walk(i, bound, idx, member, depth));
                return;
            }
            Value::Object(o) => o,
            _ => return,
        };
        // CHILDREN FIRST: a member read learns its enum from its RECEIVER's row.
        for v in object.values_mut() {
            self.walk(v, bound, idx, member, depth);
        }
        if bare(object).is_some_and(|name| bound.contains(name)) {
            object.insert("lambda".into(), Value::Bool(true));
        }
        let row = match object.get("target") {
            Some(Value::Object(t)) if !t.contains_key("row") => self.row_of(t),
            Some(Value::Object(_)) => None,
            _ => return,
        };
        let en = if is_read(n) { self.enum_of(n, idx) } else { None };
        let Some(Value::Object(target)) = n.get_mut("target") else { return };
        if let Some(row) = row {
            target.insert("row".into(), Value::String(row));
        }
        if let Some(en) = en {
            target.insert("enum".into(), Value::String(en));
        }
    }

    /// The enum a member read's RECEIVER declares: by its row, or — a receiver the checker
    /// gave no anchor — by its name AND the file the member is declared in, never the name
    /// alone (many enum names in a large frontend are declared twice).
    fn enum_of(&self, n: &Value, idx: &EnumIndex) -> Option<String> {
        let receiver = n.get("receiver")?;
        if let Some(Value::String(row)) = receiver.get("target").and_then(|t| t.get("row")) {
            return idx.by_id.contains_key(row).then(|| row.clone());
        }
        let name = bare(receiver.as_object()?)?;
        let file = n.get("target")?.get("file")?;
        enum_of_type_ref(idx, name, Some(&json!({"name": name, "file": file})))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::gate_values::{enum_index, EnumIndex};

    fn store_of(tables: Value) -> Store<'static> {
        Store::from_payload(tables.as_object().cloned().expect("tables"), "typescript")
    }

    fn tables() -> Value {
        json!({
            "files": [{"id": "f:1", "path": "app/tiers.ts"}, {"id": "f:2", "path": "app/cmp.ts"}],
            "enums": [{"id": "e:1", "name": "TierIDs", "file": "f:1", "line": 3,
                       "members": [{"name": "Small"}, {"name": "Big"}]}],
            "members": [{"id": "m:1", "file": "f:2", "line": 10, "name": "tierID"}],
        })
    }

    fn member(name: &str, receiver: Value, line: i64, file: &str) -> Value {
        json!({"k": "Read", "name": name, "receiver": receiver,
               "target": {"name": name, "file": file, "line": line}})
    }

    fn idx(store: &Store<'_>) -> EnumIndex {
        enum_index(store)
    }

    #[test]
    fn a_member_read_is_tied_to_its_row_by_file_and_line() {
        let store = store_of(tables());
        let ast = member("tierID", json!({"k": "This"}), 10, "C:/fe/app/cmp.ts");
        let out = TsRows::new(&store).resolve(&ast, &IndexSet::new(), &idx(&store));
        assert_eq!(out["target"]["row"], json!("m:1"));
    }

    #[test]
    fn a_union_property_resolves_to_the_same_row_whichever_declaration_the_checker_put_first() {
        let store = store_of(tables());
        // Only the declaration in cmp.ts has a row; the checker may list either one first.
        let elsewhere = json!({"file": "C:/fe/app/other.ts", "line": 4});
        let here = json!({"file": "C:/fe/app/cmp.ts", "line": 10});
        for (first, also) in [(&elsewhere, &here), (&here, &elsewhere)] {
            let mut target = first.clone();
            target["name"] = json!("tierID");
            target["also"] = json!([also]);
            let ast = json!({"k": "Read", "name": "tierID", "receiver": {"k": "This"}, "target": target});
            let out = TsRows::new(&store).resolve(&ast, &IndexSet::new(), &idx(&store));
            assert_eq!(out["target"]["row"], json!("m:1"), "first {first}");
        }
    }

    #[test]
    fn an_enum_member_read_names_its_enum_through_the_receivers_row() {
        let store = store_of(tables());
        let receiver = member("TierIDs", json!({"k": "Implicit"}), 3, "C:/fe/app/tiers.ts");
        let ast = member("Small", receiver, 4, "C:/fe/app/tiers.ts");
        let out = TsRows::new(&store).resolve(&ast, &IndexSet::new(), &idx(&store));
        assert_eq!(out["target"]["enum"], json!("e:1"));
    }

    #[test]
    fn a_receiver_with_no_anchor_names_its_enum_by_name_and_file_together() {
        let store = store_of(tables());
        let ast = member("Small", json!({"k": "Read", "name": "TierIDs", "receiver": {"k": "Implicit"}}),
                         4, "C:/fe/app/tiers.ts");
        let out = TsRows::new(&store).resolve(&ast, &IndexSet::new(), &idx(&store));
        assert_eq!(out["target"]["enum"], json!("e:1"));
        // The same name declared in ANOTHER file is not that enum.
        let elsewhere = member("Small", json!({"k": "Read", "name": "TierIDs", "receiver": {"k": "Implicit"}}),
                               4, "C:/fe/app/cmp.ts");
        let out = TsRows::new(&store).resolve(&elsewhere, &IndexSet::new(), &idx(&store));
        assert!(out["target"].get("enum").is_none());
    }

    #[test]
    fn an_anchor_two_rows_share_resolves_to_nothing() {
        let mut t = tables();
        t["consts"] = json!([{"id": "k:1", "file": "f:2", "line": 10}]);
        let store = store_of(t);
        let ast = member("tierID", json!({"k": "This"}), 10, "C:/fe/app/cmp.ts");
        let out = TsRows::new(&store).resolve(&ast, &IndexSet::new(), &idx(&store));
        assert!(out["target"].get("row").is_none());
    }

    #[test]
    fn an_arrow_const_resolves_to_the_function_it_holds() {
        let mut t = tables();
        t["consts"] = json!([{"id": "k:1", "file": "f:2", "line": 20, "name": "isBig"}]);
        t["functions"] = json!([{"id": "fn:1", "file": "f:2", "line": 20, "name": "isBig"}]);
        let store = store_of(t);
        let ast = member("isBig", json!({"k": "Implicit"}), 20, "C:/fe/app/cmp.ts");
        let out = TsRows::new(&store).resolve(&ast, &IndexSet::new(), &idx(&store));
        assert_eq!(out["target"]["row"], json!("fn:1"));
        // Two DIFFERENT names on one line are still two declarations, and still nothing.
        let mut t = tables();
        t["consts"] = json!([{"id": "k:1", "file": "f:2", "line": 20, "name": "a"}]);
        t["functions"] = json!([{"id": "fn:1", "file": "f:2", "line": 20, "name": "b"}]);
        let store = store_of(t);
        let out = TsRows::new(&store).resolve(&ast, &IndexSet::new(), &idx(&store));
        assert!(out["target"].get("row").is_none());
    }

    #[test]
    fn a_const_local_list_takes_its_row_and_a_const_local_arrow_is_inlined() {
        let mut t = tables();
        t["locals"] = json!([
            {"id": "lo:1", "member": "m:9", "name": "picked", "declared": "const", "value": [{"$enum": "TierIDs.Small"}]},
            {"id": "lo:2", "member": "m:9", "name": "has", "declared": "const", "value": {"$fn": "(e) => e", "$expr_id": "x:7"}},
            {"id": "lo:3", "member": "m:9", "name": "later", "declared": "let", "value": [{"$enum": "TierIDs.Small"}]},
        ]);
        t["expressions"] = json!([{"id": "x:7", "ast": {"k": "Fn", "returns": [{"k": "Read", "name": "e", "receiver": {"k": "Implicit"}}]}}]);
        let store = store_of(t);
        let rows = TsRows::new(&store);
        let local = |n: &str| json!({"k": "Read", "name": n, "receiver": {"k": "Implicit"}});
        let out = rows.resolve_in(&local("picked"), &IndexSet::new(), &idx(&store), Some("m:9"));
        assert_eq!(out["target"]["row"], json!("lo:1"));
        let out = rows.resolve_in(&local("has"), &IndexSet::new(), &idx(&store), Some("m:9"));
        assert_eq!(out["k"], json!("Fn"), "the arrow's own tree, in place");
        let out = rows.resolve_in(&local("later"), &IndexSet::new(), &idx(&store), Some("m:9"));
        assert!(out.get("target").is_none(), "a `let` may hold something else by then");
        let out = rows.resolve_in(&local("picked"), &IndexSet::new(), &idx(&store), Some("m:other"));
        assert!(out.get("target").is_none(), "another member's local is not in scope");
    }

    #[test]
    fn only_a_name_a_callback_binds_is_marked_as_one() {
        let store = store_of(tables());
        let ast = json!({"k": "Binary", "op": "===",
                         "left": {"k": "Read", "name": "x", "receiver": {"k": "Implicit"}},
                         "right": {"k": "Read", "name": "y", "receiver": {"k": "Implicit"}}});
        let bound: IndexSet<String> = ["x".to_string()].into_iter().collect();
        let out = TsRows::new(&store).resolve(&ast, &bound, &idx(&store));
        assert_eq!(out["left"]["lambda"], json!(true));
        assert!(out["right"].get("lambda").is_none());
    }
}
