//! THE STATEMENTS INSIDE A FUNCTION OR A DO BLOCK, each a CHILD `sql_statements` row of the CREATE FUNCTION or DO it is
//! in, with its own tree, refs and column refs - as ScriptDom has a procedure's statements under the procedure.
//!
//! A PL/pgSQL body is compiled by PL/pgSQL's own grammar (`pg_query_parse_plpgsql`): every SQL statement it runs is
//! parsed again by PostgreSQL's parser, and so is an expression or assignment that reads a table (`PLpgSQL_expr` in
//! its own parse mode). Its line is the body's first line plus the statement's `lineno` less one. A name the body
//! declares (a parameter, a variable, a record) is a `param`, never a column - PL/pgSQL resolves it before any column.
//! A `LANGUAGE sql` body is SQL text parsed as a file is; a `BEGIN ATOMIC` body is in the statement's own tree.

use super::ffi;
use super::scan::Source;
use super::stmts::{kind_of, roots};
use super::tree::{build, build_first, strings, text, typed, Ctx, Item, PgNode};
use serde_json::Value;
use std::collections::HashSet;

/// One statement of a body: kind, lines, column, its source and its tree.
pub struct Child {
    pub kind: String,
    pub line: i64,
    pub end_line: i64,
    pub col: i64,
    pub source: String,
    pub nodes: Vec<PgNode>,
}

/// A body's statements, and what could not be parsed in it as `(line, message)`. `ctx` is the file's, `text` the
/// statement's own source (what PL/pgSQL is handed).
pub fn of(ctx: &Ctx, kind: &str, body: &Value, text_of_statement: &str) -> (Vec<Child>, Vec<(i64, String)>) {
    let mut children = Vec::new();
    let mut errors = Vec::new();
    let options = if kind == "DoStmt" { &body["args"] } else { &body["options"] };
    let mut language = if kind == "DoStmt" { "plpgsql".to_string() } else { String::new() };
    let mut source: Option<(&str, Option<usize>)> = None;
    for (_, option) in options.as_array().into_iter().flatten().filter_map(typed) {
        match text(option, "defname") {
            "language" => language = typed(&option["arg"]).map(|(_, s)| text(s, "sval").to_lowercase()).unwrap_or_default(),
            "as" => {
                let held = match typed(&option["arg"]) {
                    Some(("String", s)) => s["sval"].as_str(),
                    Some(("List", l)) => l["items"].as_array().and_then(|i| i.first()).and_then(typed).and_then(|(_, s)| s["sval"].as_str()),
                    _ => None,
                };
                let at = option["arg_location"].as_i64().filter(|&n| n >= 0).map(|n| ctx.base + n as usize);
                source = held.map(|h| (h, at));
            }
            _ => {}
        }
    }
    let mut vars: HashSet<String> = body["parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(typed)
        .filter_map(|(_, p)| p["name"].as_str().map(str::to_lowercase))
        .collect();
    if let Some((held, at)) = source {
        let start = at.map_or(0, |a| a + opening(ctx.source, a));
        let line = ctx.source.line(start);
        match language.as_str() {
            "plpgsql" => match ffi::parse_plpgsql(text_of_statement) {
                Ok(functions) => {
                    if let Some(name) = strings(&body["funcname"]).last() {
                        vars.insert(name.to_lowercase());
                    }
                    for function in functions.as_array().into_iter().flatten() {
                        plpgsql(ctx.source, function, line, &mut vars.clone(), &mut children, &mut errors);
                    }
                }
                Err(refused) => errors.push((line + near_line(&refused.context).unwrap_or(1) - 1, refused.message)),
            },
            "sql" => sql_text(held, (line, ctx.source.column(start)), &vars, &mut children, &mut errors),
            _ => {}
        }
    }
    // BEGIN ATOMIC ... END: the statements are in the parse tree already, their locations the file's.
    fn items(list: &Value) -> Vec<&Value> {
        list["items"].as_array().into_iter().flatten().collect()
    }
    let mut atomic: Vec<&Value> = Vec::new();
    match typed(&body["sql_body"]) {
        Some(("List", list)) => {
            for item in items(list) {
                match typed(item) {
                    Some(("List", inner)) => atomic.extend(items(inner)),
                    Some(_) => atomic.push(item),
                    None => {}
                }
            }
        }
        Some(_) => atomic.push(&body["sql_body"]),
        None => {}
    }
    for statement in atomic {
        if let Some((kind, b)) = typed(statement) {
            let inner = Ctx { source: ctx.source, base: ctx.base, scope: ctx.scope, vars: &vars };
            let (nodes, first) = build_first(&inner, &kind_of(kind, b), roots(&inner, kind, b));
            // Its extent: from the token after the `;` (or the ATOMIC) before its first node, to the `;` after it.
            let first = ctx.source.atomic_start(ctx.scope.0, first.unwrap_or(ctx.scope.0));
            let end = ctx.source.statement_end(first, ctx.scope.1);
            children.push(Child {
                kind: kind_of(kind, b),
                line: ctx.source.line(first),
                end_line: ctx.source.line(end.saturating_sub(1).max(first)),
                col: ctx.source.column(first),
                source: ctx.source.range(first, end),
                nodes,
            });
        }
    }
    (children, errors)
}

/// The width of a body's opening quote: `$tag$`, `E'` or `'`.
fn opening(source: &Source, at: usize) -> usize {
    let word = source.tokens.get(source.token_at(at)).filter(|t| t.start == at).map_or("", |t| source.word(t));
    if word.starts_with('$') {
        word[1..].find('$').map_or(1, |end| end + 2)
    } else if word.starts_with("E'") || word.starts_with("e'") {
        2
    } else {
        1
    }
}

/// `... near line 7` in PL/pgSQL's error context - the only place its compile error says where.
fn near_line(message: &str) -> Option<i64> {
    let after = &message[message.find("line ")? + 5..];
    after.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// A `LANGUAGE sql` body: SQL text, each statement a child. `at` is the file line and column the body starts at.
fn sql_text(held: &str, at: (i64, i64), vars: &HashSet<String>, children: &mut Vec<Child>, errors: &mut Vec<(i64, String)>) {
    let Ok(source) = Source::within(held, at.0, at.1) else { return };
    match ffi::parse(held, ffi::MODE_STATEMENT) {
        Err(refused) => errors.push((refused.at.map_or(at.0, |a| source.line(a)), refused.message)),
        Ok(tree) => {
            for raw in tree["stmts"].as_array().into_iter().flatten() {
                let from = raw["stmt_location"].as_u64().unwrap_or(0) as usize;
                let len = raw["stmt_len"].as_u64().filter(|&l| l > 0).map_or(held.len().saturating_sub(from), |l| l as usize);
                let (Some((kind, body)), Some((first, last))) = (typed(&raw["stmt"]), source.extent(from, from + len)) else { continue };
                let ctx = Ctx { source: &source, base: 0, scope: (first, last), vars };
                let nodes = build(&ctx, &kind_of(kind, body), Some(first), roots(&ctx, kind, body));
                children.push(Child {
                    kind: kind_of(kind, body),
                    line: source.line(first),
                    end_line: source.line(last.saturating_sub(1).max(first)),
                    col: source.column(first),
                    source: source.range(first, last),
                    nodes,
                });
            }
        }
    }
}

/// The word PL/pgSQL's statement type is for a child that is an expression, not a statement.
fn context_word(stmt: &str) -> String {
    match stmt {
        "PLpgSQL_stmt_assign" => "set".into(),
        "PLpgSQL_stmt_dynexecute" | "PLpgSQL_stmt_dynfors" => "execute".into(),
        "PLpgSQL_stmt_fori" | "PLpgSQL_stmt_fors" | "PLpgSQL_stmt_forc" | "PLpgSQL_stmt_foreach_a" => "for".into(),
        "PLpgSQL_var" => "declare".into(),
        "PLpgSQL_if_elsif" => "if".into(),
        other => other.trim_start_matches("PLpgSQL_stmt_").replace('_', " "),
    }
}

/// Every `PLpgSQL_expr` of one compiled function, found with an explicit stack: a statement (parse mode 0) is a child
/// always, an expression or assignment only when it reads a table.
fn plpgsql(file: &Source, function: &Value, body_line: i64, vars: &mut HashSet<String>, children: &mut Vec<Child>, errors: &mut Vec<(i64, String)>) {
    let mut stack: Vec<(&Value, i64, &str)> = vec![(function, 1, "")];
    let mut found: Vec<(&str, i64, i32, &str)> = Vec::new();
    while let Some((value, lineno, context)) = stack.pop() {
        match value {
            Value::Array(items) => stack.extend(items.iter().rev().map(|v| (v, lineno, context))),
            Value::Object(map) => {
                if let Some(expr) = map.get("PLpgSQL_expr") {
                    found.push((text(expr, "query"), lineno, expr["parseMode"].as_i64().unwrap_or(0) as i32, context));
                    continue;
                }
                for (key, inner) in map.iter().rev() {
                    let lineno = inner.get("lineno").and_then(Value::as_i64).unwrap_or(lineno);
                    if key.starts_with("PLpgSQL_") {
                        if let Some(name) = inner["refname"].as_str().filter(|n| !n.starts_with('(')) {
                            vars.insert(name.to_lowercase());
                        }
                        stack.push((inner, lineno, key.as_str()));
                    } else {
                        stack.push((inner, lineno, context));
                    }
                }
            }
            _ => {}
        }
    }
    // In source order: a FOR's query is held after its body in the compiled function.
    found.sort_by_key(|f| f.1);
    for (query, lineno, mode, context) in found {
        let line = body_line + lineno - 1;
        let Ok(source) = Source::new(query, line) else { continue };
        let tree = match ffi::parse(query, mode) {
            Ok(tree) => tree,
            Err(refused) => {
                errors.push((refused.at.map_or(line, |a| source.line(a)), refused.message));
                continue;
            }
        };
        let Some((kind, body)) = tree["stmts"].as_array().and_then(|s| s.first()).and_then(|s| typed(&s["stmt"])) else { continue };
        let ctx = Ctx { source: &source, base: 0, scope: (0, query.len()), vars: &*vars };
        let (word, items) = match (mode, kind) {
            (ffi::MODE_STATEMENT, _) => (kind_of(kind, body), roots(&ctx, kind, body)),
            (_, "PLAssignStmt") => {
                let mut items = vec![Item::Leaf("param", text(body, "name").to_string(), ctx.at(body))];
                items.extend(body.get("val").map(Item::Select));
                (context_word(context), items)
            }
            _ => (context_word(context), roots(&ctx, kind, body)),
        };
        let first = source.extent(0, query.len()).map_or(0, |e| e.0);
        let nodes = build(&ctx, &word, Some(first), items);
        if mode != ffi::MODE_STATEMENT && !nodes.iter().any(|n| n.kind == "table") {
            continue;
        }
        let col = file.find_on_line(line, query.trim_start().split_whitespace().next().unwrap_or("")).unwrap_or(1);
        children.push(Child { kind: word, line, end_line: line + query.trim_end().matches('\n').count() as i64, col, source: query.trim().to_string(), nodes });
    }
}
