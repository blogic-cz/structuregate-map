//! WHAT A POSTGRESQL STATEMENT DECLARES AND TOUCHES, in the rows the T-SQL half writes: `sql_objects` for a CREATE,
//! `sql_columns` for a table's columns, and `sql_refs` - read off the statement's tree (`tree.rs`), so a ref is
//! what a `table` node says: the TARGET of an INSERT/UPDATE/DELETE/MERGE/TRUNCATE is that action, any other a
//! `select`, and a CTE's name is no table. A column's type and DEFAULT are the source's own text.

use super::tree::{dotted, parts, PgNode};
use super::Text;
use serde_json::{json, Value};
use sqlparser::ast::{ColumnOption, Expr, GeneratedAs, ObjectName, Statement, TableConstraint};
use sqlparser::tokenizer::Token;

/// The object a CREATE declares: `(kind, schema, name, parent)`.
pub fn declared(statement: &Statement) -> Option<(&'static str, String, String, String)> {
    let split = |name: &ObjectName| {
        let p = parts(name);
        let schema = if p.len() >= 2 { p[p.len() - 2].value.clone() } else { String::new() };
        (schema, p.last().map_or(String::new(), |i| i.value.clone()))
    };
    let (kind, name, parent) = match statement {
        Statement::CreateTable(t) => ("table", &t.name, String::new()),
        Statement::CreateView(v) => ("view", &v.name, String::new()),
        Statement::CreateFunction(f) => ("function", &f.name, String::new()),
        Statement::CreateIndex(i) => ("index", i.name.as_ref()?, dotted(&i.table_name)),
        Statement::CreateTrigger(t) => ("trigger", &t.name, dotted(&t.table_name)),
        Statement::CreateSequence { name, .. } => ("sequence", name, String::new()),
        Statement::CreateType { name, .. } => ("type", name, String::new()),
        _ => return None,
    };
    let (schema, base) = split(name);
    Some((kind, schema, base, parent))
}

/// A CREATE TABLE's columns, `object` the index of its object row.
pub fn columns(statement: &Statement, object: usize, text: &Text) -> Vec<Value> {
    let Statement::CreateTable(table) = statement else { return Vec::new() };
    let mut keyed: Vec<String> = Vec::new();
    for constraint in &table.constraints {
        if let TableConstraint::PrimaryKey(pk) = constraint {
            for column in &pk.columns {
                if let Expr::Identifier(i) = &column.column.expr {
                    keyed.push(i.value.to_lowercase());
                }
            }
        }
    }
    let mut rows = Vec::new();
    for (position, column) in table.columns.iter().enumerate() {
        let (type_text, default_text) = text.column(&column.name);
        let mut nullable = true;
        let mut key = keyed.contains(&column.name.value.to_lowercase());
        let lower = type_text.to_lowercase();
        let mut identity = matches!(lower.as_str(), "serial" | "bigserial" | "smallserial" | "serial4" | "serial8" | "serial2");
        for option in &column.options {
            match &option.option {
                ColumnOption::NotNull => nullable = false,
                ColumnOption::PrimaryKey(_) => key = true,
                ColumnOption::Identity(_) => identity = true,
                ColumnOption::Generated { generated_as: GeneratedAs::Always | GeneratedAs::ByDefault, generation_expr: None, .. } => identity = true,
                _ => {}
            }
        }
        if key {
            nullable = false;
        }
        let line = if column.name.span.start.line > 0 { column.name.span.start.line as i64 } else { 0 };
        rows.push(json!({ "object": object, "position": position, "name": column.name.value, "type": type_text,
            "nullable": nullable, "identity": identity, "key": key, "default": default_text, "line": line }));
    }
    rows
}

/// What a statement touches, read off its tree: `(action, schema, name, column, line)`.
pub fn refs(nodes: &[PgNode]) -> Vec<(String, String, String, String, i64)> {
    let root = nodes.first().map_or("", |n| n.kind.as_str());
    let children = |i: usize| nodes.iter().enumerate().filter(move |(_, n)| n.parent == Some(i)).map(|(c, _)| c);
    let child = |i: usize, kind: &str| children(i).find(|&c| nodes[c].kind == kind).map(|c| nodes[c].value.clone());
    let ctes: Vec<String> = nodes.iter().filter(|n| n.kind == "cte").map(|n| n.value.to_lowercase()).collect();
    let written = ["insert", "update", "delete", "merge", "truncate"].contains(&root);
    let mut target: Option<(String, String)> = None;
    let mut out = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if node.kind != "table" {
            continue;
        }
        let schema = child(i, "schema").unwrap_or_default();
        if schema.is_empty() && ctes.contains(&node.value.to_lowercase()) {
            continue;
        }
        let is_target = written && node.parent.is_some_and(|p| nodes[p].kind == "target");
        if is_target && target.is_none() {
            target = Some((schema.clone(), node.value.clone()));
        }
        out.push((if is_target { root.to_string() } else { "select".into() }, schema, node.value.clone(), String::new(), node.line));
    }
    // THE COLUMNS AN INSERT LISTS AND AN UPDATE SETS are the target's.
    if let Some((schema, name)) = target {
        for (i, node) in nodes.iter().enumerate() {
            let listed = root == "insert" && node.kind == "column" && node.parent == Some(0);
            let set = root == "update" && node.kind == "column" && node.parent.is_some_and(|p| nodes[p].kind == "set" && nodes[p].parent == Some(0)) && node.ord == 0;
            if listed || set {
                let column = node.value.rsplit('.').next().unwrap_or("").to_string();
                out.push((root.to_string(), schema.clone(), name.clone(), column, nodes[i].line));
            }
        }
    }
    out
}

impl Text<'_> {
    /// A column definition's type and DEFAULT as the source spells them: the tokens after its name up to the first
    /// constraint keyword, and after DEFAULT up to the next one - at the definition's own parenthesis depth.
    fn column(&self, name: &sqlparser::ast::Ident) -> (String, String) {
        let Some(start) = self.tokens.iter().position(|t| t.span.start == name.span.start && name.span.start.line > 0) else {
            return (String::new(), String::new());
        };
        const STOP: [&str; 11] = ["NOT", "NULL", "DEFAULT", "PRIMARY", "UNIQUE", "REFERENCES", "CHECK", "CONSTRAINT", "GENERATED", "COLLATE", "IDENTITY"];
        let mut open_parens = 0i64;
        let mut runs: Vec<(String, Vec<usize>)> = vec![("type".into(), Vec::new())];
        for (i, token) in self.tokens.iter().enumerate().skip(start + 1) {
            match &token.token {
                Token::LParen => open_parens += 1,
                Token::RParen if open_parens == 0 => break,
                Token::RParen => open_parens -= 1,
                Token::Comma if open_parens == 0 => break,
                Token::Word(w) if open_parens == 0 && STOP.contains(&w.value.to_uppercase().as_str()) && w.quote_style.is_none() => {
                    runs.push((w.value.to_uppercase(), Vec::new()));
                    continue;
                }
                Token::Whitespace(_) => continue,
                _ => {}
            }
            if let Some(run) = runs.last_mut() {
                run.1.push(i);
            }
        }
        let spelled = |kind: &str| runs.iter().find(|r| r.0 == kind).map(|r| self.range(&r.1)).unwrap_or_default();
        (spelled("type"), spelled("DEFAULT"))
    }
}
