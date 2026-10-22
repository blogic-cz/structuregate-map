//! A CALL THAT HANDS ITS FIRST ARGUMENT BACK as the url it extends (`calls.rs`, rows 80).
//!
//! `createQueryUrl(url, params)` returns `` `${url}${query}` ``: its result IS the url its caller handed in, with an unknown
//! tail the url never carries (`finished` cuts a query), so a wrapper's url reads through it and a caller's url folds to its
//! first argument. `this.getBase(productTypeID)` does not: the parameter is an id the helper turns into a url, and reading
//! the call as the parameter made the url `*` and lost the literal suffix written after it. WHICH it is comes from the
//! callee's OWN `returns`, found by the call's `$target` (file and name) - never from the name: every return of the one
//! declaration there must be its first parameter, or a template that opens with it and has no other text.
//!
//! A call with no `$target` at all (a value the checker did not resolve) keeps the reading it always had - the parameter
//! as the base - where the CALLER's fold takes only a call it resolved. A `$target` that names no row of the tree
//! (a library function) or several rows (two classes of one file share a method name) is no such helper.

use super::*;

impl World {
    /// The abs path of each TypeScript file, then every callable by (path, name) and every callable's returns.
    pub(super) fn load_helpers(&mut self, conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<()> {
        if !has_columns(conn, "files", &["id", "abs"]) {
            return Ok(());
        }
        let mut abs: HashMap<String, String> = HashMap::new();
        let mut stmt = conn.prepare("SELECT id, abs FROM files")?;
        for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))? {
            let (id, path) = row?;
            if typescript.contains_key(&id) {
                abs.insert(id, path.replace('\\', "/"));
            }
        }
        for table in ["functions", "members"] {
            if !has_columns(conn, table, &["id", "file", "name"]) {
                continue;
            }
            let mut stmt = conn.prepare(&format!("SELECT id, file, name FROM \"{table}\""))?;
            for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?)))? {
                let (id, file, name) = row?;
                if let Some(path) = abs.get(&file) {
                    self.callees.entry((path.clone(), name)).or_default().push(id);
                }
            }
        }
        if has_columns(conn, "returns", read_of("returns")) {
            let mut value = Column::new(conn, "returns", "value")?;
            for (member, _, _, _, owner, held, _) in read(conn, "returns", "member, '', '', ''", &["value"], "")? {
                self.returns.entry(member).or_default().push(blank(null_literal(value.value(&owner, &held)?)));
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
        self.returns.get(id).is_some_and(|rs| !rs.is_empty() && rs.iter().all(|r| begins_with(r, first)))
    }
}

/// `first`, or `` `${first}${..}` `` with no text of its own beyond the holes.
fn begins_with(value: &Value, first: &str) -> bool {
    let is_first = |v: &Value| v.get("$expr").and_then(Value::as_str) == Some(first);
    let Value::Object(o) = value else { return false };
    match (o.get("$template"), o.get("$hole_values")) {
        (Some(Value::Array(parts)), Some(Value::Array(holes))) => {
            parts.iter().all(|p| p.as_str() == Some("")) && holes.first().is_some_and(is_first)
        }
        _ => is_first(value),
    }
}
