//! THE POSTGRESQL HALF OF THE SQL MAP: a `.sql` file the SQL config names `postgres` (`dialects`, `mapper/deep/sqlconfig.rs`)
//! is parsed here by sqlparser-rs (Apache-2.0, pure rust, compiled into this library) instead of by ScriptDom, and
//! answered in the rows ScriptDom's are - statements, their trees, objects, columns, refs, errors - which the C# half
//! writes as it writes its own (`SqlDeep.Postgres`). ONE PRODUCER of `lang = 'sql'` rows: two writing separately would
//! each delete the other's. The fingerprint, the column refs and the shape views are the shared rust pass's.
//!
//! ONE BAD STATEMENT COSTS ONE DIAGNOSTIC: sqlparser stops at its first error, so the tokens are split at every
//! top-level `;` and each statement is parsed on its own, keeping its tokens' own lines. psql's meta-commands (`\i`,
//! `\set`) are blanked first, as SQLCMD's are for T-SQL, so lines still point into the file.
//!
//! NO DEPTH CAP: sqlparser's recursion limit is lifted, and the parse runs on a thread with a stack sized for it.

mod exprs;
mod rows;
mod tree;

use crate::{in_string, out_string};
use serde_json::{json, Value};
use sqlparser::ast::Statement;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Location, Token, TokenWithSpan, Tokenizer};
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// The stack the parse runs on: sqlparser descends by recursion, and its limit is lifted.
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

/// `{statements, nodes, objects, columns, refs, diagnostics}` - a statement's `parent`/`object`, a node's, column's and
/// ref's `statement`/`object` are indexes into those lists (-1 for none).
pub fn parse(text: String) -> Value {
    let failed = |why: &str| json!({ "diagnostics": [{ "line": 1, "name": "syntax", "message": why }] });
    let worker = std::thread::Builder::new().stack_size(STACK).spawn(move || catch_unwind(AssertUnwindSafe(|| rows_of(&text))));
    match worker.map(|w| w.join()) {
        Ok(Ok(Ok(rows))) => rows,
        Ok(Ok(Err(_))) | Ok(Err(_)) => failed("the PostgreSQL parser panicked"),
        Err(why) => failed(&format!("the PostgreSQL parser could not start - {why}")),
    }
}

/// The text the parser read, its tokens, and where each line starts - for a statement's own text.
pub struct Text<'a> {
    text: &'a str,
    tokens: &'a [TokenWithSpan],
    lines: Vec<usize>,
}

impl Text<'_> {
    fn offset(&self, at: Location) -> usize {
        let Some(&start) = self.lines.get((at.line as usize).saturating_sub(1)) else { return self.text.len() };
        self.text[start..].char_indices().nth((at.column as usize).saturating_sub(1)).map_or(self.text.len(), |(i, _)| start + i)
    }

    /// The source from the first of these tokens to the end of the last.
    fn range(&self, tokens: &[usize]) -> String {
        match (tokens.first(), tokens.last()) {
            (Some(&a), Some(&b)) => {
                let (from, to) = (self.offset(self.tokens[a].span.start), self.offset(self.tokens[b].span.end));
                self.text.get(from..to.max(from)).unwrap_or("").to_string()
            }
            _ => String::new(),
        }
    }
}

/// psql's meta-commands blanked to their length, so every line and column still points into the file.
fn prepared(text: &str) -> String {
    text.split('\n')
        .map(|line| if line.trim_start().starts_with('\\') { " ".repeat(line.trim_end_matches('\r').chars().count()) + if line.ends_with('\r') { "\r" } else { "" } } else { line.to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rows_of(source: &str) -> Value {
    let text = prepared(source);
    let dialect = PostgreSqlDialect {};
    let mut out = json!({ "statements": [], "nodes": [], "objects": [], "columns": [], "refs": [], "diagnostics": [] });
    let tokens = match Tokenizer::new(&dialect, &text).tokenize_with_location() {
        Ok(tokens) => tokens,
        Err(e) => {
            push(&mut out, "diagnostics", json!({ "line": e.location.line.max(1), "name": "syntax", "message": e.message }));
            return out;
        }
    };
    let mut lines: Vec<usize> = vec![0];
    lines.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    let view = Text { text: &text, tokens: &tokens, lines };
    // EACH TOP-LEVEL `;` ENDS A STATEMENT; a dollar-quoted body is one token, so its own `;` are not seen.
    let mut open_parens = 0i64;
    let mut start = 0;
    let mut chunks = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        match token.token {
            Token::LParen => open_parens += 1,
            Token::RParen => open_parens -= 1,
            Token::SemiColon if open_parens <= 0 => {
                chunks.push((start, i));
                start = i + 1;
                open_parens = 0;
            }
            _ => {}
        }
    }
    chunks.push((start, tokens.len()));
    let mut ordinal = 0;
    for (from, to) in chunks {
        let real: Vec<usize> = (from..to).filter(|&i| !matches!(tokens[i].token, Token::Whitespace(_) | Token::EOF)).collect();
        let (Some(&first), Some(&last)) = (real.first(), real.last()) else { continue };
        let line = tokens[first].span.start.line as i64;
        let parsed = Parser::new(&dialect).with_recursion_limit(usize::MAX).with_tokens_with_locations(tokens[from..to].to_vec()).parse_statements();
        match parsed {
            Err(e) => push(&mut out, "diagnostics", json!({ "line": line, "name": "syntax", "message": e.to_string() })),
            Ok(statements) => {
                for statement in &statements {
                    let source = view.range(&[first, last]);
                    let end = tokens[last].span.end.line as i64;
                    let col = tokens[first].span.start.column as i64;
                    record(&mut out, statement, &view, (line, end, col), source, &mut ordinal);
                }
            }
        }
    }
    out
}

/// One statement's rows: itself, its tree, what it declares and touches - and a view's query as a statement inside it,
/// as ScriptDom has one.
fn record(out: &mut Value, statement: &Statement, text: &Text, (line, end, col): (i64, i64, i64), source: String, ordinal: &mut i64) {
    let object = rows::declared(statement).map(|(kind, schema, name, parent)| {
        let at = out["objects"].as_array().map_or(0, Vec::len);
        push(out, "objects", json!({ "kind": kind, "schema": schema, "name": name, "parent": parent, "line": line, "end_line": end, "source": source }));
        at
    });
    let kind = kind_of(statement);
    let index = out["statements"].as_array().map_or(0, Vec::len);
    push(out, "statements", json!({ "parent": -1, "object": object.map_or(-1, |o| o as i64), "batch": 0, "ordinal": *ordinal,
        "line": line, "end_line": end, "col": col, "kind": kind, "source": source }));
    *ordinal += 1;
    let nodes = tree::of(statement, kind, line);
    write_tree(out, index, object, &nodes);
    if let Some(o) = object {
        for column in rows::columns(statement, o, text) {
            push(out, "columns", column);
        }
    }
    if let Statement::CreateView(view) = statement {
        let inner = out["statements"].as_array().map_or(0, Vec::len);
        push(out, "statements", json!({ "parent": index, "object": object.map_or(-1, |o| o as i64), "batch": 0, "ordinal": *ordinal,
            "line": line, "end_line": end, "col": col, "kind": "select", "source": "" }));
        *ordinal += 1;
        write_tree(out, inner, object, &tree::of_query(&view.query, line));
    }
}

fn write_tree(out: &mut Value, statement: usize, object: Option<usize>, nodes: &[tree::PgNode]) {
    for (n, node) in nodes.iter().enumerate() {
        push(out, "nodes", json!({ "statement": statement, "n": n, "parent": node.parent, "ord": node.ord, "kind": node.kind,
            "value": node.value, "line": node.line }));
    }
    for (action, schema, name, column, line) in rows::refs(nodes) {
        push(out, "refs", json!({ "statement": statement, "object": object.map_or(-1, |o| o as i64), "action": action,
            "schema": schema, "name": name, "column": column, "line": line }));
    }
}

fn push(out: &mut Value, list: &str, row: Value) {
    if let Some(rows) = out[list].as_array_mut() {
        rows.push(row);
    }
}

/// A statement's kind in the words the T-SQL half uses for the same statement.
fn kind_of(statement: &Statement) -> &'static str {
    match statement {
        Statement::Query(_) => "select",
        Statement::Insert(_) => "insert",
        Statement::Update(_) => "update",
        Statement::Delete(_) => "delete",
        Statement::Merge(_) => "merge",
        Statement::CreateTable(_) => "create table",
        Statement::CreateView(_) => "create view",
        Statement::CreateIndex(_) => "create index",
        Statement::CreateFunction(_) => "create function",
        Statement::CreateTrigger(_) => "create trigger",
        Statement::CreateSequence { .. } => "create sequence",
        Statement::CreateType { .. } => "create type",
        Statement::CreateSchema { .. } => "create schema",
        Statement::CreateExtension(_) => "create extension",
        Statement::AlterTable(_) => "alter table",
        Statement::Drop { .. } => "drop",
        Statement::Truncate(_) => "truncate",
        Statement::Set(_) => "set",
        Statement::Declare { .. } => "declare",
        Statement::StartTransaction { .. } => "begin transaction",
        Statement::Commit { .. } => "commit",
        Statement::Rollback { .. } => "rollback",
        Statement::Grant(_) => "grant",
        Statement::Revoke(_) => "revoke",
        Statement::Comment { .. } => "comment",
        Statement::Copy { .. } => "copy",
        Statement::Call(_) | Statement::Execute { .. } => "execute",
        Statement::Explain { .. } => "explain",
        Statement::If(_) => "if",
        _ => "other",
    }
}
