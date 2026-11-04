//! A URL BUILT AS AN ARRAY: `const parts = ['get-application']; parts.push(`?id=${id}`); get(parts.join(''))`.
//!
//! The evaluator hands `parts.join('')` over as a `$call` of `Array.join` whose `$receiver` is already the local's initializer, its
//! elements. What the callable PUSHES onto that local between its declaration and the call follows them, in source order, each argument
//! folded where it is pushed; a push from another callable (a `forEach` callback) is a piece nobody can spell, `*`. A conditional push is
//! read as taken - the pushes a url is built with are its query, which `finished` cuts.

use super::*;

impl World {
    /// Every `<name>.push(..)` of the tree: (file, name) -> (line, callable, arguments), in source order.
    pub(super) fn load_pushes(&mut self, conn: &Connection, typescript: &HashMap<String, String>) -> rusqlite::Result<()> {
        if !has_columns(conn, "calls", &["file", "member", "line", "col", "callee", "method", "args"]) {
            return Ok(());
        }
        let mut args = Column::new(conn, "calls", "args")?;
        for (file, member, line, callee, owner, held, _) in read(conn, "calls", "file, member, line, callee", &["args"], "WHERE t.method = 'push' ORDER BY t.line, t.col")? {
            let Some(name) = callee.strip_suffix(".push").filter(|n| is_name(n)) else { continue };
            if !typescript.contains_key(&file) {
                continue;
            }
            let Value::Array(pushed) = args.value(&owner, &held)? else { continue };
            self.pushes.entry((file, name.to_string())).or_default().push((line.parse().unwrap_or(0), member, pushed));
        }
        Ok(())
    }

    /// `parts.join(sep)` over a local array: its elements and its pushes folded and joined by `sep` (`,` when none is given).
    /// None when the value is no such join.
    pub(super) fn joined(&self, call: &serde_json::Map<String, Value>, at: &At, seen: &mut HashSet<String>) -> Option<String> {
        if call.get("$method").and_then(Value::as_str) != Some("join") || call.get("$owner").and_then(Value::as_str) != Some("Array") {
            return None;
        }
        let name = call.get("$call")?.as_str()?.strip_suffix(".join").filter(|n| is_name(n))?;
        let Some(Value::Array(elements)) = call.get("$receiver") else { return None };
        let (_, declared, _) = self.local(at, name)?;
        let separator = call.get("$args").and_then(Value::as_array).and_then(|a| a.first()).map_or_else(|| ",".to_string(), |s| self.fold(s, at, seen));
        let mut pieces: Vec<String> = elements.iter().map(|e| self.fold(e, at, seen)).collect();
        let pushes = self.pushes.get(&(at.file.to_string(), name.to_string())).into_iter().flatten();
        for (line, member, pushed) in pushes.filter(|(line, _, _)| *line >= *declared && *line <= at.line) {
            if member != at.member {
                pieces.push("*".into());
                continue;
            }
            let there = At { line: *line, ..at.clone() };
            pieces.extend(pushed.iter().map(|p| self.fold(p, &there, seen)));
        }
        Some(pieces.join(&separator))
    }
}
