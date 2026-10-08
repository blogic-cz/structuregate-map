//! THE LENSES OVER THE SQLITE MAP.
//!
//! `--sql` is here and answers anything, but the questions people actually arrive with are the same
//! handful every time - what is in this database, what does this table record, what is this thing called
//! that, what does the map say about this file, what does the file actually SAY - and writing the join
//! for each of them from memory is how a session spends its first ten minutes. Each lens is one of those
//! questions with the joins already made.
//!
//! THE TEXT IS THE OUTPUT, so it is built here rather than handed back as rows. A person reads it, and
//! the column widths, the rule under the header and the "(no rows)" are the contract,
//! held character for character against the implementation this replaced.
//!
//! CELLS ARE CUT, NEVER WRAPPED: a wrapped cell makes a column of one row look like a column of four,
//! and a count read off that is wrong.

mod bound;
mod cat;
mod dead;
mod decorators;
mod defaults;
mod lenses;
mod magic;
mod unused;

use anyhow::Result;
use rusqlite::types::Value;
use rusqlite::Connection;
use serde::Deserialize;
use std::fmt::Write as _;
use std::path::Path;
use lenses::*;

/// Every column that carries a NAME, which is what `--find` looks through. A name lives in a different
/// column in each table, so the list is stated rather than guessed.
const NAME_COLUMNS: &[(&str, &str)] = &[
    ("functions", "qualname"),
    ("functions", "name"),
    ("classes", "qualname"),
    ("classes", "name"),
    ("calls", "callee"),
    ("imports", "module"),
    ("imports", "name"),
    ("imports", "alias"),
    ("consts", "name"),
    ("decorators", "name"),
    ("decorators", "target"),
    ("assignments", "target"),
    ("exports", "name"),
    ("raises", "name"),
    ("globals", "name"),
    ("parameters", "name"),
    ("parameters", "qualname"),
    ("files", "path"),
    ("files", "module"),
    // THE OTHER HALVES' NAMES. The list above was the python half's, and on an Angular map `--find` of a
    // method answered with the CALLS to it and never its declaration - `members` was simply not asked.
    ("refs", "name"),
    ("types", "name"),
    ("members", "name"),
    ("components", "name"),
    ("components", "selector"),
    ("directives", "name"),
    ("directives", "selector"),
    ("pipes", "name"),
    ("pipes", "pipe_name"),
    ("injectables", "name"),
    ("ng_modules", "name"),
    ("routes", "path"),
    ("routes", "full_path"),
    ("routes", "const_name"),
    ("locals", "name"),
    ("enums", "name"),
    ("interfaces", "name"),
    ("type_aliases", "name"),
    ("type_members", "name"),
    ("type_members", "path"),
    ("io", "binding_name"),
    ("state_actions", "name"),
    ("state_selectors", "name"),
    ("selector_index", "name"),
    ("sql_objects", "name"),
    ("sql_columns", "name"),
];

/// The tables that carry a resolved `reads`, and the column of each worth printing beside it.
const READ_SOURCES: &[(&str, &str)] = &[
    ("expressions", "source"),
    ("assignments", "target"),
    ("consts", "name"),
    ("parameters", "name"),
    ("returns", "source"),
    ("branches", "test"),
    ("raises", "name"),
    ("withs", "source"),
    ("deletes", "target"),
    ("handlers", "types"),
];

/// The columns `--grep` looks through, in the order it looks.
const GREP_COLUMNS: &[&str] = &["source", "value", "test", "callee", "target", "name", "text"];

/// One question, as the caller parsed it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Ask {
    pub lens: String,
    /// The lens's own argument: a name, a fragment, a table, a query.
    pub arg: String,
    /// `--table`, which narrows `--grep`.
    pub table: String,
    /// `--lines a-b`, which narrows `--cat`.
    pub lines: String,
    pub limit: usize,
    pub width: usize,
}

/// A cell as python's `str()` would print it, which is what the two ends are compared on.
fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Integer(i) => i.to_string(),
        // `str(1.0)` is "1.0" in python and "1" in rust. The map stores no floats today; this keeps
        // the two ends agreeing if it ever does.
        Value::Real(f) => {
            if f.fract() == 0.0 && f.abs() < 1e16 {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        Value::Text(t) => t.clone(),
        Value::Blob(b) => format!("{b:?}"),
    }
}

/// A python `%r` of a string: single quotes, which is what the messages print.
fn quoted(text: &str) -> String {
    format!("'{text}'")
}

/// A cell as shown: whole when it fits or `width` is 0, and otherwise cut WITH A MARKER that says how
/// much is missing (ASCII: a console that cannot print `…` would mangle it). A cell cut silently read as the whole value - a 400-character call looked like
/// the call - and a reader acted on the half it was shown.
pub(crate) fn clip(text: &str, width: usize) -> String {
    let total = text.chars().count();
    if width == 0 || total <= width {
        return text.to_string();
    }
    let shown: String = text.chars().take(width).collect();
    format!("{shown}...[+{} chars]", total - width)
}

/// A result as a table.
fn show(out: &mut String, rows: &[Vec<Value>], columns: &[String], limit: usize, width: usize) {
    if rows.is_empty() {
        let _ = writeln!(out, "(no rows)");
        return;
    }
    let cut: Vec<Vec<String>> = rows
        .iter()
        .take(limit)
        .map(|row| {
            row.iter()
                .map(|c| {
                    clip(&cell(c).replace('\n', " "), width)
                })
                .collect()
        })
        .collect();
    let widths: Vec<usize> = (0..columns.len())
        .map(|i| {
            let header = columns[i].chars().count();
            let widest = cut.iter().map(|r| r[i].chars().count()).max().unwrap_or(0);
            header.max(widest)
        })
        .collect();

    let pad = |text: &str, to: usize| {
        let have = text.chars().count();
        format!("{text}{}", " ".repeat(to.saturating_sub(have)))
    };
    let head: Vec<String> =
        columns.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
    let _ = writeln!(out, "{}", head.join("  "));
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    let _ = writeln!(out, "{}", rule.join("  "));
    for row in &cut {
        let cells: Vec<String> =
            row.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
        let _ = writeln!(out, "{}", cells.join("  "));
    }
    if rows.len() > limit {
        let _ = writeln!(out, "... {} more row(s); raise --limit", rows.len() - limit);
    }
}

/// One query, as columns and rows of untyped values.
fn query(db: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<(Vec<String>, Vec<Vec<Value>>)> {
    let mut stmt = db.prepare(sql)?;
    let columns: Vec<String> = stmt.column_names().iter().map(|c| (*c).to_string()).collect();
    let width = columns.len();
    let mut rows = stmt.query(params)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut one = Vec::with_capacity(width);
        for i in 0..width {
            one.push(row.get::<_, Value>(i)?);
        }
        out.push(one);
    }
    Ok((columns, out))
}

fn named(columns: &[&str]) -> Vec<String> {
    columns.iter().map(|c| (*c).to_string()).collect()
}

/// The tables anyone queries. The FTS5 shadow tables are storage; the virtual table itself stays.
fn tables_of(db: &Connection) -> Vec<String> {
    let Ok(mut stmt) =
        db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
    else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.filter_map(|n| n.ok())
        .filter(|n| !n.starts_with('_') && !n.starts_with("file_text_"))
        .collect()
}

fn columns_of(db: &Connection, table: &str) -> Vec<String> {
    let Ok(mut stmt) = db.prepare(&format!("PRAGMA table_info(\"{}\")", escape(table))) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) else {
        return Vec::new();
    };
    rows.filter_map(|c| c.ok()).collect()
}

fn escape(name: &str) -> String {
    name.replace('"', "\"\"")
}

fn count_of(db: &Connection, table: &str) -> Result<i64> {
    Ok(db.query_row(&format!("SELECT count(*) FROM \"{}\"", escape(table)), [], |r| r.get(0))?)
}

/// What a lens printed, and whether it could not finish. The second is the process's exit code:
/// python's `main` returned 2 when the driver raised, and a build that reads a lens has to hear it.
pub struct Answer {
    pub text: String,
    pub failed: bool,
}

pub fn render(db_path: &Path, ask: &Ask) -> Result<Answer> {
    if !db_path.is_file() {
        return Ok(Answer {
            text: format!("no database at {} - build it with --map --map-sqlite\n", db_path.display()),
            failed: true,
        });
    }
    let db = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    // `--limit 0` IS ALL ROWS, as other row limits spell it. It used to mean 50, so an audit that asked for
    // every row to count them got 50 and a "raise --limit" line it had no reason to read. The caller sends
    // 50 when no --limit was given.
    let limit = if ask.limit == 0 { usize::MAX } else { ask.limit };
    // `--width 0` IS EVERY CELL WHOLE, the same way. The caller sends 70 when no --width was given; a cut
    // cell carries its marker either way - see `clip`.
    let width = ask.width;
    let mut out = String::new();
    // A LENS THAT ASKED AND WAS REFUSED - `--cat` over a fragment that names several files, or past a file's
    // end - prints why and fails like one that could not ask, so a script reading it does not take the wrong file.
    let mut refused = false;

    // A LENS THAT CANNOT ASK ITS QUESTION SAYS SO AND THE PROCESS FAILS. The python wrapped every
    // one of these in `except sqlite3.OperationalError`, printed `query failed: ...` and returned 2 -
    // and whatever the lens had already printed stays, because it was true. A map without the table
    // a lens joins on is the ordinary case here: these lenses are shared by the python map and the
    // Angular one, and `decorators` exists in only one of them.
    let outcome = match ask.lens.as_str() {
        "tables" => lens_tables(&db, &mut out, limit, width),
        "schema" => lens_schema(&db, &mut out, &ask.arg, limit, width),
        "find" => lens_find(&db, &mut out, &ask.arg, limit, width),
        "id" => lens_id(&db, &mut out, &ask.arg, width),
        "file" => lens_file(&db, &mut out, &ask.arg, limit, width),
        "cat" => cat::lens(&db, &mut out, &ask.arg, &ask.lines).map(|shown| refused = !shown),
        "text" => lens_text(&db, &mut out, &ask.arg, limit, width),
        "grep" => lens_grep(&db, &mut out, &ask.arg, &ask.table, limit, width),
        "reads" => lens_reads(&db, &mut out, &ask.arg, limit, width),
        "key" => lens_key(&db, &mut out, &ask.arg, limit, width),
        "decorators" => decorators::lens(&db, &mut out, &ask.arg, limit, width),
        "dead" => dead::lens(&db, &mut out, &ask.arg, limit, width),
        "defaults" => defaults::lens(&db, &mut out, &ask.arg, limit, width),
        "unused-imports" => unused::lens(&db, &mut out, &ask.arg, limit, width),
        "magic" => magic::lens(&db, &mut out, &ask.arg, limit, width),
        "sql" => query(&db, &ask.arg, &[]).map(|(columns, rows)| {
            show(&mut out, &rows, &columns, limit, width);
        }),
        _ => {
            lenses(&mut out);
            Ok(())
        }
    };
    let failed = match outcome {
        Ok(()) => refused,
        Err(e) => {
            let _ = writeln!(out, "query failed: {}", sqlite_message(&e));
            true
        }
    };
    Ok(Answer { text: out, failed })
}

/// What SQLite said, without rusqlite's own wrapping - the python printed the driver's message.
fn sqlite_message(error: &anyhow::Error) -> String {
    for cause in error.chain() {
        match cause.downcast_ref::<rusqlite::Error>() {
            Some(rusqlite::Error::SqliteFailure(_, Some(message))) => return message.clone(),
            // A statement that would not PREPARE. rusqlite adds the sql and an offset to this one;
            // the driver the python used says only what was wrong, and that is what is compared.
            Some(rusqlite::Error::SqlInputError { msg, .. }) => return msg.clone(),
            _ => {}
        }
    }
    error.to_string()
}

fn lenses(out: &mut String) {
    let list = serde_json::json!({"lenses": [
        "--tables", "--schema T", "--find NAME", "--id ROW", "--file FRAGMENT",
        "--cat FRAGMENT [--lines a-b]", "--text FTS", "--grep TEXT [--table T]",
        "--reads NAME", "--key SETTINGS_KEY", "--decorators [FRAGMENT]", "--dead [FRAGMENT]", "--defaults [FRAGMENT]", "--unused-imports [FRAGMENT]",
        "--magic [FRAGMENT]", "--sql QUERY"]});
    // python's `json.dumps(..., indent=1)`: one space a level, and a newline after it.
    let mut buffer = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    let _ = serde::Serialize::serialize(&list, &mut ser);
    let _ = writeln!(out, "{}", String::from_utf8_lossy(&buffer));
}

