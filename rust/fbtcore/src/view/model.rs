//! THE GRAPH A MAP HOLDS, read out of its database: every file and function, and every edge between them that a
//! half RESOLVED - never one guessed from a name. What it reads is what each half already joined:
//!
//!   * a call's CALLER is the innermost function whose lines contain it - exact in every half, overloads included;
//!   * its CALLEE is the function its `symbol` names (C#, bound by Roslyn) or the one at `target_path` named
//!     `target_name` (python and plain TypeScript, bound by the half); a callee in no mapped file is not an edge;
//!   * an import's `from_path` (python, plain TypeScript), a C# type reference's `symbol` matched to a class, and
//!     an Angular `render_graph` row are FILE edges.
//!
//! THE FILE-LEVEL MAP (`buildmap.json`), when one is given, adds what the database does not hold: an import edge
//! in every language its halves read - rust's `mod`, a PowerShell dot-source, a TypeScript import - for a pair the
//! database has none for.
//!
//! A COLUMN OR TABLE A MAP DOES NOT HAVE IS SKIPPED, never an error: a python-only map has no `symbol`, a C# one no
//! `target_path`, and both are maps worth drawing.

use crate::rows::schema;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

/// More function edges than this and the page would be hundreds of megabytes; the heaviest are kept and it says so.
const MAX_FUNCTION_EDGES: usize = 300_000;

struct Function {
    file: usize,
    line: i64,
    end: i64,
}

/// THE FOLDER THE GRAPH HANGS FROM. A map of several `--root`s keeps the first in `root` and every one, with the prefix
/// its paths carry, in `roots:<lang>`; named after the first, the other trees read as if they sat inside it. So
/// with more than one root it is the deepest folder holding them all.
fn shown_root(db: &Connection) -> String {
    let first: String = db.query_row("SELECT value FROM _meta WHERE key = 'root'", [], |r| r.get(0)).unwrap_or_default();
    let listed: Vec<String> = db.prepare("SELECT value FROM _meta WHERE key LIKE 'roots:%'")
        .and_then(|mut rows| rows.query_map([], |r| r.get::<_, String>(0)).map(|found| found.flatten().collect()))
        .unwrap_or_default();
    let mut dirs: Vec<String> = listed.iter()
        .filter_map(|text| serde_json::from_str::<Vec<(String, String)>>(text).ok())
        .flatten()
        .map(|(_, dir)| dir.replace('\\', "/").trim_end_matches('/').to_string())
        .collect();
    dirs.sort();
    dirs.dedup();
    if dirs.len() < 2 {
        return first;
    }
    let parts: Vec<Vec<&str>> = dirs.iter().map(|d| d.split('/').collect()).collect();
    let shared = (0..parts[0].len()).take_while(|&i| parts.iter().all(|p| p.get(i) == parts[0].get(i))).count();
    let common = parts[0][..shared].join("/");
    if common.is_empty() { first } else { common }
}

pub fn read(db: &Connection, graph: Option<&Value>) -> anyhow::Result<Value> {
    let has = |table: &str, column: &str| schema::existing_columns(db, table).map(|c| c.iter().any(|n| n == column)).unwrap_or(false);
    let pick = |table: &str, column: &str| if has(table, column) { column.to_string() } else { "''".to_string() };

    // FILES, by their id in the database and by path.
    let mut files: Vec<Value> = Vec::new();
    let mut file_of: HashMap<String, usize> = HashMap::new();
    let mut path_of: HashMap<String, usize> = HashMap::new();
    let sql = format!("SELECT id, path, lang, {}, {}, {} FROM files ORDER BY path", pick("files", "lines"), pick("files", "errors"), pick("files", "entry"));
    let mut rows = db.prepare(&sql)?;
    let mut query = rows.query([])?;
    while let Some(row) = query.next()? {
        let (id, path): (String, String) = (row.get(0)?, row.get(1)?);
        let number = |at: usize| row.get::<_, rusqlite::types::Value>(at).ok().map_or(0, |v| match v {
            rusqlite::types::Value::Integer(n) => n,
            rusqlite::types::Value::Real(r) => r as i64,
            rusqlite::types::Value::Text(t) => t.parse().unwrap_or(0),
            _ => 0,
        });
        file_of.insert(id, files.len());
        path_of.insert(path.clone(), files.len());
        files.push(json!([path, row.get::<_, String>(2).unwrap_or_default(), number(3), number(4), number(5)]));
    }

    // FUNCTIONS, by file in line order, by symbol and by (file, name).
    let mut functions: Vec<Function> = Vec::new();
    let mut listed: Vec<Value> = Vec::new();
    let mut by_symbol: HashMap<String, usize> = HashMap::new();
    let mut by_name: HashMap<(usize, String), usize> = HashMap::new();
    let mut in_file: HashMap<usize, Vec<usize>> = HashMap::new();
    if has("functions", "file") {
        let sql = format!("SELECT file, name, {}, line, {}, {} FROM functions", pick("functions", "qualname"), pick("functions", "end_line"), pick("functions", "symbol"));
        let mut rows = db.prepare(&sql)?;
        let mut query = rows.query([])?;
        while let Some(row) = query.next()? {
            let Some(&file) = file_of.get(&row.get::<_, String>(0)?) else { continue };
            let name: String = row.get(1).unwrap_or_default();
            let qualname: String = row.get(2).unwrap_or_default();
            let line: i64 = row.get(3).unwrap_or(0);
            let end: i64 = row.get::<_, i64>(4).unwrap_or(line).max(line);
            let symbol: String = row.get(5).unwrap_or_default();
            let at = functions.len();
            if !symbol.is_empty() {
                by_symbol.entry(symbol).or_insert(at);
            }
            by_name.entry((file, qualname.clone())).or_insert(at);
            by_name.entry((file, name.clone())).or_insert(at);
            in_file.entry(file).or_default().push(at);
            listed.push(json!([file, if qualname.is_empty() { name } else { qualname }, line, end]));
            functions.push(Function { file, line, end });
        }
    }
    // THE INNERMOST FUNCTION AROUND A LINE: the one with the smallest span that contains it.
    let caller = |file: usize, line: i64| -> Option<usize> {
        in_file.get(&file)?.iter().copied()
            .filter(|&f| functions[f].line <= line && line <= functions[f].end)
            .min_by_key(|&f| functions[f].end - functions[f].line)
    };

    let mut file_edges: BTreeMap<(usize, usize), [i64; 4]> = BTreeMap::new();
    let mut function_edges: HashMap<(usize, usize), i64> = HashMap::new();
    let mut external = 0i64;
    let mut bump = |a: usize, b: usize, kind: usize| {
        if a != b {
            file_edges.entry((a, b)).or_insert([0; 4])[kind] += 1;
        }
    };

    if has("calls", "file") {
        let sql = format!("SELECT file, line, {}, {}, {} FROM calls", pick("calls", "symbol"), pick("calls", "target_path"), pick("calls", "target_name"));
        let mut rows = db.prepare(&sql)?;
        let mut query = rows.query([])?;
        while let Some(row) = query.next()? {
            let Some(&file) = file_of.get(&row.get::<_, String>(0)?) else { continue };
            let line: i64 = row.get(1).unwrap_or(0);
            let (symbol, path, name): (String, String, String) = (row.get(2).unwrap_or_default(), row.get(3).unwrap_or_default(), row.get(4).unwrap_or_default());
            let callee = by_symbol.get(&symbol).copied().or_else(|| {
                let target = *path_of.get(&path)?;
                by_name.get(&(target, name.clone())).copied()
                    .or_else(|| by_name.get(&(target, name.rsplit('.').next().unwrap_or("").to_string())).copied())
            });
            let target_file = callee.map(|f| functions[f].file).or_else(|| path_of.get(&path).copied());
            let Some(target_file) = target_file else {
                external += 1;
                continue;
            };
            bump(file, target_file, 0);
            if let (Some(from), Some(to)) = (caller(file, line), callee)
                && from != to
            {
                *function_edges.entry((from, to)).or_insert(0) += 1;
            }
        }
    }
    if has("imports", "from_path") {
        let mut rows = db.prepare("SELECT file, from_path FROM imports WHERE from_path <> ''")?;
        let mut query = rows.query([])?;
        while let Some(row) = query.next()? {
            if let (Some(&a), Some(&b)) = (file_of.get(&row.get::<_, String>(0)?), path_of.get(&row.get::<_, String>(1)?)) {
                bump(a, b, 1);
            }
        }
    }
    // A C# TYPE REFERENCE is a dependency no call shows: a field of that type, a parameter, a `typeof`.
    if has("refs", "symbol") && has("classes", "symbol") {
        let mut classes: HashMap<String, usize> = HashMap::new();
        let mut rows = db.prepare("SELECT file, symbol FROM classes WHERE symbol <> ''")?;
        let mut query = rows.query([])?;
        while let Some(row) = query.next()? {
            if let Some(&file) = file_of.get(&row.get::<_, String>(0)?) {
                classes.entry(row.get(1)?).or_insert(file);
            }
        }
        let mut rows = db.prepare("SELECT file, symbol FROM refs WHERE symbol <> ''")?;
        let mut query = rows.query([])?;
        while let Some(row) = query.next()? {
            let Some(&file) = file_of.get(&row.get::<_, String>(0)?) else { continue };
            let symbol: String = row.get(1)?;
            // A member's symbol is its type's plus a name: the type is what the file depends on.
            let target = classes.get(&symbol).or_else(|| symbol.rsplit_once('.').and_then(|(owner, _)| classes.get(owner)));
            if let Some(&target) = target {
                bump(file, target, 2);
            }
        }
    }
    if has("render_graph", "from_class") && has("render_graph", "to_class") {
        let owner = if has("classes", "owner_file") { "owner_file" } else { "file" };
        let sql = format!("SELECT a.{owner}, b.{owner} FROM render_graph g JOIN classes a ON a.id = g.from_class JOIN classes b ON b.id = g.to_class");
        if let Ok(mut rows) = db.prepare(&sql) {
            let mut query = rows.query([])?;
            while let Some(row) = query.next()? {
                if let (Some(&a), Some(&b)) = (file_of.get(&row.get::<_, String>(0)?), file_of.get(&row.get::<_, String>(1)?)) {
                    bump(a, b, 3);
                }
            }
        }
    }

    if let Some(graph) = graph {
        for section in ["imports", "soft_imports"] {
            for (from, targets) in graph[section].as_object().into_iter().flatten() {
                let Some(&a) = path_of.get(from) else { continue };
                for to in targets.as_array().into_iter().flatten().filter_map(Value::as_str) {
                    if let Some(&b) = path_of.get(to)
                        && a != b
                        && file_edges.get(&(a, b)).is_none_or(|k| k[1] == 0)
                    {
                        file_edges.entry((a, b)).or_insert([0; 4])[1] += 1;
                    }
                }
            }
        }
    }
    let mut function_edges: Vec<((usize, usize), i64)> = function_edges.into_iter().collect();
    let truncated = function_edges.len() > MAX_FUNCTION_EDGES;
    function_edges.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    function_edges.truncate(MAX_FUNCTION_EDGES);
    function_edges.sort();
    let root = shown_root(db);
    Ok(json!({
        "root": root,
        "files": files,
        "functions": listed,
        "fileEdges": file_edges.iter().map(|((a, b), k)| json!([a, b, k[0], k[1], k[2], k[3]])).collect::<Vec<_>>(),
        "functionEdges": function_edges.iter().map(|((a, b), n)| json!([a, b, n])).collect::<Vec<_>>(),
        "external": external,
        "truncated": truncated,
    }))
}
