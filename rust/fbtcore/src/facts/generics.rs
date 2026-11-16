//! A GENERIC BASE'S TYPE PARAMETERS, AS THE CONTROLLER SERVING ITS ACTION BINDS THEM. An action declared on
//! `JobBase<TA, TB>` returns `TB` there; served by `WidgetsController : JobBase<ItemDto, List<ItemDto>>`, it returns
//! `List<ItemDto>`. The walk goes from the serving class up its bases (`classes.bases`, resolved symbols) to the
//! declaring one, binding each base's parameters to the arguments the class below it gives - through any number of
//! generic classes between them (a visited set ends bases that name each other; no depth cap).

use crate::rows::dups::{has_columns, owner_sql, scalar_sql, text, Column};
use crate::rows::http::generic_key;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

/// Every C# class: its symbol and bases by (file, name), where each generic key is declared, and a file's id by path.
#[derive(Default)]
pub struct Hierarchy {
    classes: HashMap<(String, String), (String, Vec<String>)>,
    declared: HashMap<String, (String, String)>,
    files: HashMap<String, String>,
}

impl Hierarchy {
    pub fn read(conn: &Connection) -> rusqlite::Result<Hierarchy> {
        let mut out = Hierarchy::default();
        if !has_columns(conn, "classes", &["file", "name", "symbol", "bases"]) {
            return Ok(out);
        }
        if has_columns(conn, "files", &["id", "path"]) {
            let mut stmt = conn.prepare("SELECT id, path FROM files")?;
            for row in stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?)))? {
                let (id, path) = row?;
                out.files.insert(path, id);
            }
        }
        let mut list = Column::new(conn, "classes", "bases")?;
        let sql = format!("SELECT k.file, k.name, k.symbol, {}, {} FROM classes k", owner_sql(conn, "classes", "k"), scalar_sql(conn, "classes", "k", "bases"));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, r.get::<_, SqlValue>(4)?)))?;
        for row in rows {
            let (file, name, symbol, owner, cell) = row?;
            let bases = list.strings(&owner, &cell)?;
            if !symbol.is_empty() {
                out.declared.entry(generic_key(&symbol)).or_insert((file.clone(), name.clone()));
            }
            out.classes.insert((file, name), (symbol, bases));
        }
        Ok(out)
    }

    /// The type parameters of the class `declaring` (a file id and a name) as the class `server` (a file PATH, as an
    /// endpoint names it, and a name) binds them; empty when it is the same class or no base leads there.
    pub fn bindings(&self, server: (&str, &str), declaring: (&str, &str)) -> HashMap<String, String> {
        let Some(file) = self.files.get(server.0) else { return HashMap::new() };
        let start = (file.clone(), server.1.to_string());
        let goal = (declaring.0.to_string(), declaring.1.to_string());
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut todo: Vec<((String, String), HashMap<String, String>)> = vec![(start, HashMap::new())];
        while let Some((at, bound)) = todo.pop() {
            if at == goal {
                return bound;
            }
            if !seen.insert(at.clone()) {
                continue;
            }
            let Some((_, bases)) = self.classes.get(&at) else { continue };
            for base in bases {
                let Some(next) = self.declared.get(&generic_key(base)) else { continue };
                let Some((symbol, _)) = self.classes.get(next) else { continue };
                // THE BASE'S PARAMETERS take the arguments this class gives it, in this class's own binding.
                let given: Vec<String> = arguments(base).iter().map(|a| substitute(a, &bound)).collect();
                let names = arguments(symbol);
                let next_bound: HashMap<String, String> = names.into_iter().zip(given).collect();
                todo.push((next.clone(), next_bound));
            }
        }
        HashMap::new()
    }
}

/// The top-level type arguments a symbol is written with: `A.B<C, D<E, F>>` gives `C` and `D<E, F>`.
fn arguments(symbol: &str) -> Vec<String> {
    let Some(open) = symbol.find('<') else { return vec![] };
    let (mut out, mut nested, mut start) = (Vec::new(), 0usize, open + 1);
    for (i, c) in symbol.char_indices().skip_while(|(i, _)| *i <= open) {
        match c {
            '<' => nested += 1,
            '>' if nested == 0 => {
                out.push(symbol[start..i].trim().to_string());
                break;
            }
            '>' => nested -= 1,
            ',' if nested == 0 => {
                out.push(symbol[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out
}

/// A type with each type parameter the binding names replaced by its argument - whole names only.
pub fn substitute(typed: &str, bound: &HashMap<String, String>) -> String {
    if bound.is_empty() {
        return typed.to_string();
    }
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(bound.get(word.as_str()).unwrap_or(word));
        word.clear();
    };
    for c in typed.chars() {
        if c.is_alphanumeric() || c == '_' || c == '.' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_parameter_takes_the_argument_the_serving_class_gives_through_every_class_between() {
        let class = |symbol: &str, bases: &[&str]| (symbol.to_string(), bases.iter().map(|b| b.to_string()).collect::<Vec<_>>());
        let key = |name: &str| ("f:1".to_string(), name.to_string());
        let mut h = Hierarchy::default();
        h.files.insert("App/Jobs.cs".into(), "f:1".into());
        h.classes.insert(key("WidgetsController"), class("Demo.WidgetsController", &["Demo.Mid<Demo.ItemDto>"]));
        h.classes.insert(key("Mid"), class("Demo.Mid<TM>", &["Demo.JobBase<TM, System.Collections.Generic.List<TM>>"]));
        h.classes.insert(key("JobBase"), class("Demo.JobBase<TA, TB>", &[]));
        h.classes.insert(key("Loop"), class("Demo.Loop", &["Demo.Loop"]));
        for (k, (symbol, _)) in &h.classes {
            h.declared.insert(generic_key(symbol), k.clone());
        }
        let bound = h.bindings(("App/Jobs.cs", "WidgetsController"), ("f:1", "JobBase"));
        assert_eq!(bound.get("TA").map(String::as_str), Some("Demo.ItemDto"));
        assert_eq!(bound.get("TB").map(String::as_str), Some("System.Collections.Generic.List<Demo.ItemDto>"));
        assert_eq!(substitute("System.Threading.Tasks.Task<TB>", &bound), "System.Threading.Tasks.Task<System.Collections.Generic.List<Demo.ItemDto>>");
        assert_eq!(substitute("Demo.TAX<TA>", &bound), "Demo.TAX<Demo.ItemDto>");
        assert_eq!(substitute("Other.TA<TA_1>", &bound), "Other.TA<TA_1>");
        assert!(h.bindings(("App/Jobs.cs", "Loop"), ("f:1", "JobBase")).is_empty());
        assert!(h.bindings(("App/Other.cs", "WidgetsController"), ("f:1", "JobBase")).is_empty());
        assert_eq!(arguments("A.B<C, D<E, F>>"), ["C", "D<E, F>"]);
    }
}
