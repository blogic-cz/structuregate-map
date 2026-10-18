//! THE POSTGRESQL HALF OF THE SQL MAP: a `.sql` file the SQL config names `postgres` (`dialects`, `mapper/deep/sqlconfig.rs`)
//! is parsed here by POSTGRESQL'S OWN PARSER - libpg_query (`vendor/libpg_query/`, PostgreSQL 17's grammar and scanner,
//! compiled into this library by `build.rs`), as every half parses with its language's own - and answered in the rows
//! ScriptDom's are: statements, their trees, objects, columns, refs, errors, which the C# half writes as it writes its
//! own (`SqlDeep.Postgres`). ONE PRODUCER of `lang = 'sql'` rows: two writing separately would each delete the other's.
//! The fingerprint, the column refs and the shape views are the shared rust pass's.
//!
//! ONE BAD STATEMENT COSTS ONE DIAGNOSTIC, at the parser's own position: the file is parsed whole, and where it is
//! refused, the statement holding that position (between top-level `;` tokens of PostgreSQL's scanner) is the
//! diagnostic and the text on each side of it is parsed again. psql's meta-commands (`\i`, `\set`) are blanked first,
//! as SQLCMD's are for T-SQL, so lines still point into the file.
//!
//! NO DEPTH CAP: the grammar's stack is lifted (`YYMAXDEPTH`, `build.rs`) - the only bound left is PostgreSQL's own
//! 1 GB allocation limit - the JSON is read without serde_json's recursion limit, and the parse runs on a thread with
//! a stack sized for the library's recursive printer.

mod bodies;
mod exprs;
mod ffi;
mod rows;
mod scan;
mod stmts;
mod tree;

use crate::{in_string, out_string};
use scan::Source;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Mutex};
use tree::{typed, Ctx, Item, PgNode};

/// The stack the parse runs on: libpg_query prints its tree by recursion, one frame per level of the statement.
const STACK: usize = 512 << 20;

/// The C door: one file's text, its rows as JSON (see `parse`). Freed with `fbt_string_free`.
///
/// # Safety
/// `text` must be NUL-terminated UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fbt_sql_parse(text: *const c_char) -> *mut c_char {
    let text = unsafe { in_string(text) }.unwrap_or_default();
    out_string(parse(text).to_string())
}

type Job = (String, mpsc::Sender<Value>);

/// THE ONE THREAD THAT PARSES, for the life of the process. libpg_query keeps a memory context per thread and registers
/// a thread-exit key the first time a thread calls it - a thread per file would spend a key per file, and the process
/// has a fixed number (1024 on Linux). Calls are answered one at a time, in the order they come.
static WORKER: Mutex<Option<mpsc::Sender<Job>>> = Mutex::new(None);

fn failed(why: &str) -> Value {
    json!({ "diagnostics": [{ "line": 1, "name": "syntax", "message": why }] })
}

/// `{statements, nodes, objects, columns, refs, diagnostics}` - a statement's `parent`/`object`, a node's, column's and
/// ref's `statement`/`object` are indexes into those lists (-1 for none).
pub fn parse(text: String) -> Value {
    let mut worker = WORKER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if worker.is_none() {
        let (send, jobs) = mpsc::channel::<Job>();
        let started = std::thread::Builder::new().name("postgres-parse".into()).stack_size(STACK).spawn(move || {
            for (text, reply) in jobs {
                let rows = catch_unwind(AssertUnwindSafe(|| rows_of(&text))).unwrap_or_else(|_| failed("the PostgreSQL parser panicked"));
                let _ = reply.send(rows);
            }
        });
        match started {
            Ok(_) => *worker = Some(send),
            Err(why) => return failed(&format!("the PostgreSQL parser could not start - {why}")),
        }
    }
    let (reply, answer) = mpsc::channel();
    let sent = worker.as_ref().map(|w| w.send((text, reply)));
    match (sent, answer.recv()) {
        (Some(Ok(())), Ok(rows)) => rows,
        _ => {
            *worker = None;
            failed("the PostgreSQL parser stopped")
        }
    }
}

/// psql's meta-commands blanked to their length, so every line and column still points into the file.
fn prepared(text: &str) -> String {
    text.split('\n')
        .map(|line| if line.trim_start().starts_with('\\') { " ".repeat(line.trim_end_matches('\r').len()) + if line.ends_with('\r') { "\r" } else { "" } } else { line.to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What one pass over the text found, in source order.
enum Found {
    /// A parsed statement: where the text it was parsed from starts (its locations' base), its range, the tree.
    Statement(usize, usize, usize, Value),
    /// A statement the parser refused: where, and why.
    Refused(usize, String),
}

fn rows_of(source: &str) -> Value {
    let text = prepared(source);
    let mut out = json!({ "statements": [], "nodes": [], "objects": [], "columns": [], "refs": [], "diagnostics": [] });
    // A TEXT THE SCANNER STOPS IN (a quote never closed runs to the end of the file): what comes before the statement
    // it stops in is read as ever, and that statement is the one diagnostic.
    let (src, stopped) = match Source::new(&text, 1) {
        Ok(src) => (src, None),
        Err(refused) => {
            let at = refused.at.unwrap_or(0).min(text.len());
            match Source::new(&text[..at], 1) {
                Ok(src) => (src, Some((at, refused.message))),
                Err(_) => {
                    let line = 1 + text[..at].matches('\n').count() as i64;
                    push(&mut out, "diagnostics", json!({ "line": line, "name": "syntax", "message": refused.message }));
                    return out;
                }
            }
        }
    };
    let end = match stopped {
        Some((at, _)) => around(&src, 0, at, at).0,
        None => text.len(),
    };
    let mut ordinal = 0;
    let refused = |out: &mut Value, at: usize, message: String| {
        let place = format!("{message} (line {}, column {})", src.line(at), src.column(at));
        push(out, "diagnostics", json!({ "line": src.line(at), "name": "syntax", "message": place }));
    };
    for found in statements(&src, end) {
        match found {
            Found::Refused(at, message) => refused(&mut out, at, message),
            Found::Statement(base, from, to, raw) => {
                let Some((kind, body)) = typed(&raw) else { continue };
                let Some((first, last)) = src.extent(from, to) else { continue };
                record(&mut out, &src, base, kind, body, (first, last), &mut ordinal);
            }
        }
    }
    if let Some((at, message)) = stopped {
        refused(&mut out, at, message);
    }
    out
}

/// Every statement of the text, and every refused one. The whole text is parsed at once; where the parser stops, the
/// statement around its position is refused and the text before and after it is parsed again - an explicit stack of
/// ranges, leftmost on top, so what is found stays in source order.
fn statements(src: &Source, end: usize) -> Vec<Found> {
    enum Work {
        Range(usize, usize),
        Refused(usize, String),
    }
    let mut found = Vec::new();
    let mut work = vec![Work::Range(0, end)];
    while let Some(next) = work.pop() {
        let (from, to) = match next {
            Work::Refused(at, message) => {
                found.push(Found::Refused(at, message));
                continue;
            }
            Work::Range(from, to) => (from, to),
        };
        if src.extent(from, to).is_none() {
            continue;
        }
        match ffi::parse(&src.text[from..to], ffi::MODE_STATEMENT) {
            Ok(tree) => {
                for raw in tree["stmts"].as_array().into_iter().flatten() {
                    let at = from + raw["stmt_location"].as_u64().unwrap_or(0) as usize;
                    let len = raw["stmt_len"].as_u64().filter(|&l| l > 0).map_or(to.saturating_sub(at), |l| l as usize);
                    found.push(Found::Statement(from, at, at + len, raw["stmt"].clone()));
                }
            }
            Err(refused) => {
                let at = from + refused.at.unwrap_or(0).min(to - from);
                let (start, end, resume) = around(src, from, to, at);
                if resume < to {
                    work.push(Work::Range(resume, to));
                }
                let first = src.extent(start, end.max(at)).map_or(at, |e| e.0);
                work.push(Work::Refused(if refused.at.is_some() { at } else { first }, refused.message));
                if start > from {
                    work.push(Work::Range(from, start));
                }
            }
        }
    }
    found
}

/// The statement holding byte `at` of `[from, to)`: from just after the last top-level `;` before it to the next one
/// at or after it - and where the text after it resumes.
fn around(src: &Source, from: usize, to: usize, at: usize) -> (usize, usize, usize) {
    let mut open_parens = 0i64;
    let mut start = from;
    for token in src.tokens[src.token_at(from)..].iter().take_while(|t| t.end <= to) {
        match src.word(token) {
            "(" => open_parens += 1,
            ")" => open_parens -= 1,
            ";" if open_parens <= 0 && token.start < at => {
                start = token.end;
                open_parens = 0;
            }
            ";" if open_parens <= 0 => return (start, token.start, token.end),
            _ => {}
        }
    }
    (start, to, to)
}

/// One statement's rows: itself, its tree, what it declares and touches - and the statements inside it (a view's
/// query, a function's or a DO block's body) as statements with it as their `parent`.
fn record(out: &mut Value, src: &Source, base: usize, kind: &str, body: &Value, (first, last): (usize, usize), ordinal: &mut i64) {
    let none = HashSet::new();
    let ctx = Ctx { source: src, base, scope: (first, last), vars: &none };
    let line = src.line(first);
    let end = src.line(last.saturating_sub(1).max(first));
    let source = src.range(first, last);
    let object = rows::declared(&ctx, kind, body).map(|(kind, schema, name, parent)| {
        let at = out["objects"].as_array().map_or(0, Vec::len);
        push(out, "objects", json!({ "kind": kind, "schema": schema, "name": name, "parent": parent, "line": line, "end_line": end, "source": source }));
        at
    });
    let object_index = object.map_or(-1, |o| o as i64);
    let words = stmts::kind_of(kind, body);
    let index = statement(out, -1, object_index, ordinal, (line, end, src.column(first)), &words, &source);
    write_tree(out, index, object_index, &tree::build(&ctx, &words, Some(first), stmts::roots(&ctx, kind, body)));
    if let (Some(o), "CreateStmt") = (object, kind) {
        for column in rows::columns(&ctx, body, o) {
            push(out, "columns", column);
        }
    }
    if kind == "ViewStmt"
        && let Some(query) = body.get("query")
    {
        let inner = statement(out, index as i64, object_index, ordinal, (line, end, src.column(first)), "select", "");
        write_tree(out, inner, object_index, &tree::build(&ctx, "select", None, vec![Item::Node(query)]));
    }
    if matches!(kind, "CreateFunctionStmt" | "DoStmt") {
        let (children, errors) = bodies::of(&ctx, kind, body, &source);
        for child in children {
            let inner = statement(out, index as i64, object_index, ordinal, (child.line, child.end_line, child.col), &child.kind, &child.source);
            write_tree(out, inner, object_index, &child.nodes);
        }
        for (line, message) in errors {
            push(out, "diagnostics", json!({ "line": line, "name": "syntax", "message": message }));
        }
    }
}

fn statement(out: &mut Value, parent: i64, object: i64, ordinal: &mut i64, (line, end, col): (i64, i64, i64), kind: &str, source: &str) -> usize {
    let index = out["statements"].as_array().map_or(0, Vec::len);
    push(out, "statements", json!({ "parent": parent, "object": object, "batch": 0, "ordinal": *ordinal,
        "line": line, "end_line": end, "col": col, "kind": kind, "source": source }));
    *ordinal += 1;
    index
}

fn write_tree(out: &mut Value, statement: usize, object: i64, nodes: &[PgNode]) {
    for (n, node) in nodes.iter().enumerate() {
        push(out, "nodes", json!({ "statement": statement, "n": n, "parent": node.parent, "ord": node.ord, "kind": node.kind,
            "value": node.value, "line": node.line }));
    }
    for (action, database, schema, name, column, line) in rows::refs(nodes) {
        push(out, "refs", json!({ "statement": statement, "object": object, "action": action,
            "written_database": database, "schema": schema, "name": name, "column": column, "line": line }));
    }
}

fn push(out: &mut Value, list: &str, row: Value) {
    if let Some(rows) = out[list].as_array_mut() {
        rows.push(row);
    }
}
