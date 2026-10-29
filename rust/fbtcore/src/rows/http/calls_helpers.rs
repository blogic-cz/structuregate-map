//! A CALL THAT HANDS ITS FIRST ARGUMENT BACK as the url it extends (`calls.rs`, rows 80, 81).
//!
//! `createQueryUrl(url, params)` returns `` `${url}${query}` ``: its result IS the url its caller handed in, with an unknown
//! tail the url never carries (`finished` cuts a query), so a wrapper's url reads through it and a caller's url folds to its
//! first argument. `this.getBase(productTypeID)` does not: the parameter is an id the helper turns into a url, and reading
//! the call as the parameter made the url `*` and lost the literal suffix written after it - such a BUILDER is folded as what it
//! returns instead (`built`, rows 88). WHICH it is comes from the
//! callee's OWN `returns`, found by the call's `$target` (file and name) - never from the name: EVERY return of the one
//! declaration there must hand the first parameter back, in one of these forms (rows 81 added the last two):
//!   * the parameter itself, or a template that opens with it and has no text of its own;
//!   * a `+` concatenation (`$concat`) whose first operand is the parameter and whose tail is no path text of its
//!     own - an unknown piece (`url + stringify(p)`), or text that starts a query (`?`, `&`, `#`): `url + '/orders'` changes the url;
//!   * the `toString()` or `href` of a LOCAL of the callee that is `new URL(<the parameter>, ..)` (`const parsedUrl = new URL(url);
//!     return parsedUrl.toString();`) - the receiver found by the checker's `$owner` (`URL`), the local by the callee's own `locals`.
//!
//! A call with no `$target` at all (a value the checker did not resolve) keeps the reading it always had - the parameter
//! as the base - where the CALLER's fold takes only a call it resolved. A `$target` that names no row of the tree
//! (a library function) or several rows (two classes of one file share a method name) is no such helper.

use super::*;

impl World {
    /// Each TypeScript file's paths - relative to the root, as a ref names it (rows 132), and absolute, as a map written before
    /// did - then every callable by (path, name) and every callable's returns.
    pub(super) fn load_helpers(&mut self, conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<()> {
        if !has_columns(conn, "files", &["id", "path", "abs"]) {
            return Ok(());
        }
        let mut spellings: HashMap<String, Vec<String>> = HashMap::new();
        let mut stmt = conn.prepare("SELECT id, path, abs FROM files")?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?)))? {
            let (id, path, abs) = row?;
            if typescript.contains_key(&id) {
                spellings.insert(id, vec![path.replace('\\', "/"), abs.replace('\\', "/")]);
            }
        }
        for table in ["functions", "members"] {
            if !has_columns(conn, table, &["id", "file", "name"]) {
                continue;
            }
            let mut stmt = conn.prepare(&format!("SELECT id, file, name FROM \"{table}\""))?;
            for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?)))? {
                let (id, file, name) = row?;
                if let Some(paths) = spellings.get(&file) {
                    for path in paths {
                        self.callees.entry((path.clone(), name.clone())).or_default().push(id.clone());
                    }
                    self.callable_file.insert(id, file);
                }
            }
        }
        self.load_constructed(conn, typescript)?;
        if has_columns(conn, "returns", read_of("returns")) {
            let mut value = Column::new(conn, "returns", "value")?;
            for (member, _, _, _, owner, held, _) in read(conn, "returns", "member, '', '', ''", &["value"], "")? {
                self.returns.entry(member).or_default().push(blank(null_literal(value.value(&owner, &held)?)));
            }
        }
        Ok(())
    }

    /// A MEMBER WITH NO INITIALIZER THAT ITS CONSTRUCTOR SETS (rows 88): `featuresUrl: string;` then `this.featuresUrl = `${this.api}/csv`` in the
    /// constructor is that value, as an initializer would be - when that is the ONLY write of the member anywhere (`=`, through `this`): a member
    /// written again, by a compound operator or from another method, holds no one value.
    fn load_constructed(&mut self, conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<()> {
        if !has_columns(conn, "assignments", read_of("assignments")) || !has_columns(conn, "members", &["id", "file", "class", "name"]) {
            return Ok(());
        }
        let mut named: HashMap<String, (String, String)> = HashMap::new();
        let mut constructors: HashSet<String> = HashSet::new();
        let mut stmt = conn.prepare("SELECT id, file, class, name FROM members")?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?)))? {
            let (id, file, class, name) = row?;
            if !typescript.contains_key(&file) || class.is_empty() {
                continue;
            }
            if name == "constructor" {
                constructors.insert(id.clone());
            }
            named.insert(id, (class, name));
        }
        let mut value = Column::new(conn, "assignments", "value")?;
        let mut written: HashMap<String, Vec<(String, Value)>> = HashMap::new();
        for (target, member, scope, operator, owner, held, _) in read(conn, "assignments", "target_id, member, scope, operator", &["value"], "")? {
            if named.contains_key(&target) {
                let from = if scope == "this" && operator == "=" { member } else { String::new() };
                written.entry(target).or_default().push((from, blank(null_literal(value.value(&owner, &held)?))));
            }
        }
        for (target, writes) in written {
            let [(member, value)] = writes.as_slice() else { continue };
            let key = named[&target].clone();
            if constructors.contains(member) && !is_empty(value) && !self.members.contains_key(&key) {
                self.members.insert(key, value.clone());
            }
        }
        Ok(())
    }

    /// Does this `$call` value return its first argument (with an unknown tail)? `strict`: a call with no resolvable `$target` is not
    /// one (a caller's fold); otherwise (a wrapper's base) it is, as it was before the callee was read.
    pub(super) fn forwards_first(&self, call: &serde_json::Map<String, Value>, strict: bool) -> bool {
        let Some(target) = call.get("$target").and_then(Value::as_object) else { return !strict };
        let (Some(file), Some(name)) = (target.get("file").and_then(Value::as_str), target.get("name").and_then(Value::as_str)) else {
            return !strict;
        };
        let Some([id]) = self.callees.get(&(file.replace('\\', "/"), name.to_string())).map(Vec::as_slice) else { return false };
        let Some(first) = self.params.get(id).and_then(|p| p.iter().find(|n| *n != "this")) else { return false };
        self.returns.get(id).is_some_and(|rs| !rs.is_empty() && rs.iter().all(|r| self.hands_back(id, r, first)))
    }

    /// A CALL OF A HELPER THAT BUILDS THE URL (rows 88): `this.ordersUrl('list')` over `ordersUrl = (relative) => `${base}orders/${relative}``
    /// is what the helper returns, folded in the helper - its `this`, locals and consts - with each parameter the caller's argument folded
    /// where the call is. The one declaration the call's `$target` names (file and name, as `forwards_first`); several returns that fold
    /// apart are an unknown piece; a helper on the way to itself (`seen`) is a cycle and `*`. None when the call names no such helper.
    pub(super) fn built(&self, call: &serde_json::Map<String, Value>, at: &At, seen: &mut HashSet<String>) -> Option<String> {
        call.get("$call").filter(|c| c.is_string())?;
        let target = call.get("$target")?.as_object()?;
        let (file, name) = (target.get("file")?.as_str()?, target.get("name")?.as_str()?);
        let [id] = self.callees.get(&(file.replace('\\', "/"), name.to_string()))?.as_slice() else { return None };
        let returns = self.returns.get(id).filter(|r| !r.is_empty())?;
        let key = format!("helper\0{id}");
        if !seen.insert(key.clone()) {
            return Some("*".into());
        }
        let args = call.get("$args").and_then(Value::as_array);
        let names: Vec<&String> = self.params.get(id).into_iter().flatten().filter(|n| *n != "this").collect();
        let bound: HashMap<String, String> = names
            .iter()
            .enumerate()
            .map(|(i, n)| ((*n).clone(), args.and_then(|a| a.get(i)).map_or_else(|| "*".to_string(), |a| self.fold(a, at, seen))))
            .collect();
        let inner = At {
            file: self.callable_file.get(id).map_or(at.file, String::as_str),
            member: id,
            class: self.class_of.get(id).map(String::as_str),
            line: i64::MAX,
            bound: Some(&bound),
        };
        let urls: HashSet<String> = returns.iter().map(|r| self.fold(r, &inner, seen)).collect();
        seen.remove(&key);
        let one = urls.len() == 1;
        Some(urls.into_iter().next().filter(|_| one).unwrap_or_else(|| "*".into()))
    }

    /// Does this return of the callable `id` hand its parameter `first` back (see the module doc)?
    fn hands_back(&self, id: &str, value: &Value, first: &str) -> bool {
        if begins_with(value, first) || self.url_of(id, value, first) {
            return true;
        }
        let mut operands = vec![];
        flatten(value, &mut operands);
        operands.len() > 1 && (begins_with(operands[0], first) || self.url_of(id, operands[0], first)) && query_tail(operands[1..].iter().copied())
    }

    /// `parsedUrl.toString()` / `parsedUrl.href` over a local the callable holds only as `new URL(first, ..)`.
    fn url_of(&self, id: &str, value: &Value, first: &str) -> bool {
        let Value::Object(o) = value else { return false };
        let local = match o.get("$call").and_then(Value::as_str) {
            Some(call) if o.get("$owner").and_then(Value::as_str) == Some("URL") => call.strip_suffix(".toString"),
            Some(_) => None,
            None => o.get("$expr").and_then(Value::as_str).and_then(|e| e.strip_suffix(".href")),
        };
        let Some(name) = local.filter(|n| is_name(n)) else { return false };
        let held: Vec<&Value> = self.locals.get(id).into_iter().flatten().filter(|(n, _, _)| n == name).map(|(_, _, v)| v).collect();
        let made_of_first = |v: &Value| {
            v.get("$new").and_then(Value::as_str) == Some("URL")
                && v.get("$args").and_then(Value::as_array).and_then(|a| a.first()).is_some_and(|a| begins_with(a, first))
        };
        !held.is_empty() && held.into_iter().all(made_of_first)
    }
}

fn is_first(v: &Value, first: &str) -> bool {
    v.get("$expr").and_then(Value::as_str) == Some(first)
}

/// `first`, or `` `${first}${..}` `` with no text of its own beyond the holes (text after it is a path, or a query a template cannot be told from: `${url}?${q}` stays no helper).
fn begins_with(value: &Value, first: &str) -> bool {
    let Value::Object(o) = value else { return false };
    match (o.get("$template"), o.get("$hole_values")) {
        (Some(Value::Array(parts)), Some(Value::Array(holes))) => {
            parts.iter().all(|p| p.as_str() == Some("")) && holes.first().is_some_and(|h| is_first(h, first))
        }
        _ => is_first(value, first),
    }
}

/// The operands of a `+` chain in source order, a nested chain flattened into its place (`(a + b) + c`).
fn flatten<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value.get("$concat") {
        Some(Value::Array(operands)) => operands.iter().for_each(|o| flatten(o, out)),
        _ => out.push(value),
    }
}

/// What follows the url adds no PATH to it: nothing, an unknown piece, or text that starts a query. The first literal text decides - what
/// comes after a query is the query's - and a template's own text before its first hole is that literal. `rest` is a template's text
/// pieces after the url's hole, or a chain's operands after its first.
fn query_tail<'a>(rest: impl IntoIterator<Item = &'a Value>) -> bool {
    let starts_query = |s: &str| s.starts_with(['?', '&', '#']);
    for piece in rest {
        match piece {
            Value::String(s) if s.is_empty() => {}
            Value::String(s) => return starts_query(s),
            Value::Object(o) => {
                let text = o.get("$template").and_then(Value::as_array).and_then(|p| p.first()).and_then(Value::as_str);
                return text.filter(|t| !t.is_empty()).is_none_or(|t| starts_query(t));
            }
            _ => return true,
        }
    }
    true
}
