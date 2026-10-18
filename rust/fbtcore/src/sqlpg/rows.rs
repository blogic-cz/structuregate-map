//! WHAT A POSTGRESQL STATEMENT DECLARES AND TOUCHES, in the rows the T-SQL half writes: `sql_objects` for a CREATE,
//! `sql_columns` for a table's columns, and `sql_refs` - read off the statement's tree (`tree.rs`), so a ref is what a
//! `table` node says by where it sits: the TARGET of an INSERT/UPDATE/DELETE/MERGE/TRUNCATE is that action (of a COPY
//! ... FROM `insert`, of an ALTER TABLE `alter`, of a CREATE POLICY `policy`), a FOREIGN KEY's table `references`, a
//! PARTITION OF / INHERITS parent `inherits`, an ATTACH/DETACH PARTITION's table `attach`/`detach`, a CALL's procedure
//! `exec`, any other table `select`; a CTE's name is no table. A column's type and DEFAULT are the source's own text.

use super::scan::Source;
use super::tree::{strings, text, typed, Ctx, PgNode};
use serde_json::{json, Value};

/// The object a CREATE declares: `(kind, schema, name, parent)`.
pub fn declared(ctx: &Ctx, kind: &str, body: &Value) -> Option<(&'static str, String, String, String)> {
    let spell = |s: &str| ctx.spell(s);
    let rel = |rv: &Value| {
        let ([_, schema, name], _) = ctx.rel_parts(rv);
        (schema, name)
    };
    let path = |parts: Vec<String>| {
        let schema = if parts.len() >= 2 { spell(&parts[parts.len() - 2]) } else { String::new() };
        (schema, parts.last().map_or(String::new(), |n| spell(n)))
    };
    let (kind, (schema, name), parent) = match kind {
        "CreateStmt" => ("table", rel(&body["relation"]), String::new()),
        "CreateTableAsStmt" if text(body, "objtype") == "OBJECT_MATVIEW" => ("view", rel(&body["into"]["rel"]), String::new()),
        "CreateTableAsStmt" => ("table", rel(&body["into"]["rel"]), String::new()),
        "ViewStmt" => ("view", rel(&body["view"]), String::new()),
        "CreateFunctionStmt" if body["is_procedure"].as_bool() == Some(true) => ("procedure", path(strings(&body["funcname"])), String::new()),
        "CreateFunctionStmt" => ("function", path(strings(&body["funcname"])), String::new()),
        "IndexStmt" => ("index", (String::new(), spell(body["idxname"].as_str()?)), ctx.relation(&body["relation"])),
        "CreateTrigStmt" => ("trigger", (String::new(), spell(text(body, "trigname"))), ctx.relation(&body["relation"])),
        "CreateSeqStmt" => ("sequence", rel(&body["sequence"]), String::new()),
        "CompositeTypeStmt" => ("type", rel(&body["typevar"]), String::new()),
        "CreateEnumStmt" | "CreateRangeStmt" => ("type", path(strings(&body["typeName"])), String::new()),
        "CreateDomainStmt" => ("type", path(strings(&body["domainname"])), String::new()),
        "CreatePolicyStmt" => ("policy", (String::new(), spell(text(body, "policy_name"))), ctx.relation(&body["table"])),
        _ => return None,
    };
    Some((kind, schema, name, parent))
}

/// A CREATE TABLE's columns, `object` the index of its object row.
pub fn columns(ctx: &Ctx, body: &Value, object: usize) -> Vec<Value> {
    let elements: Vec<(&str, &Value)> = body["tableElts"].as_array().into_iter().flatten().filter_map(typed).collect();
    let keyed: Vec<String> = elements
        .iter()
        .filter(|(t, c)| *t == "Constraint" && text(c, "contype") == "CONSTR_PRIMARY")
        .flat_map(|(_, c)| strings(&c["keys"]))
        .map(|k| k.to_lowercase())
        .collect();
    let mut rows = Vec::new();
    for (position, (_, column)) in elements.iter().filter(|(t, _)| *t == "ColumnDef").enumerate() {
        let name = text(column, "colname");
        let at = ctx.at(column);
        let (type_text, default_text) = at.map_or((String::new(), String::new()), |a| spelled(ctx.source, a));
        let mut nullable = true;
        let mut key = keyed.contains(&name.to_lowercase());
        let lower = type_text.to_lowercase();
        let mut identity = matches!(lower.as_str(), "serial" | "bigserial" | "smallserial" | "serial4" | "serial8" | "serial2");
        for (_, constraint) in column["constraints"].as_array().into_iter().flatten().filter_map(typed) {
            match text(constraint, "contype") {
                "CONSTR_NOTNULL" => nullable = false,
                "CONSTR_PRIMARY" => key = true,
                "CONSTR_IDENTITY" => identity = true,
                _ => {}
            }
        }
        if key {
            nullable = false;
        }
        rows.push(json!({ "object": object, "position": position, "name": ctx.dotted_at(at, &[name.to_string()]), "type": type_text,
            "nullable": nullable, "identity": identity, "key": key, "default": default_text, "line": at.map_or(0, |a| ctx.source.line(a)) }));
    }
    rows
}

/// A column definition's type and DEFAULT as the source spells them: the tokens after its name up to the first
/// constraint keyword, and after DEFAULT up to the next one - at the definition's own parenthesis depth.
fn spelled(source: &Source, at: usize) -> (String, String) {
    const STOP: [&str; 11] = ["NOT", "NULL", "DEFAULT", "PRIMARY", "UNIQUE", "REFERENCES", "CHECK", "CONSTRAINT", "GENERATED", "COLLATE", "IDENTITY"];
    let start = source.token_at(at);
    let mut open_parens = 0i64;
    let mut runs: Vec<(String, Vec<usize>)> = vec![("type".into(), Vec::new())];
    for (i, token) in source.tokens.iter().enumerate().skip(start + 1) {
        let word = source.word(token);
        if Source::comment(token) {
            continue;
        }
        match word {
            "(" => open_parens += 1,
            ")" if open_parens == 0 => break,
            ")" => open_parens -= 1,
            "," | ";" if open_parens == 0 => break,
            _ if open_parens == 0 && token.keyword && STOP.contains(&word.to_ascii_uppercase().as_str()) => {
                runs.push((word.to_ascii_uppercase(), Vec::new()));
                continue;
            }
            _ => {}
        }
        if let Some(run) = runs.last_mut() {
            run.1.push(i);
        }
    }
    let text_of = |kind: &str| {
        runs.iter()
            .find(|r| r.0 == kind)
            .and_then(|r| Some((*r.1.first()?, *r.1.last()?)))
            .map(|(a, b)| source.range(source.tokens[a].start, source.tokens[b].end))
            .unwrap_or_default()
    };
    (text_of("type"), text_of("DEFAULT"))
}

/// What a statement touches, read off its tree: `(action, database, schema, name, column, line)`.
pub fn refs(nodes: &[PgNode]) -> Vec<(String, String, String, String, String, i64)> {
    let root = nodes.first().map_or("", |n| n.kind.as_str());
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        if let Some(p) = n.parent {
            children[p].push(i);
        }
    }
    let child = |i: usize, kind: &str| children[i].iter().find(|&&c| nodes[c].kind == kind).map(|&c| nodes[c].value.clone());
    let ctes: Vec<String> = nodes.iter().filter(|n| n.kind == "cte").map(|n| n.value.to_lowercase()).collect();
    let written = ["insert", "update", "delete", "merge", "truncate"].contains(&root);
    let mut target: Option<(String, String, String)> = None;
    let mut out = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if node.kind == "proc" {
            let (schema, name) = node.value.rsplit_once('.').map_or((String::new(), node.value.clone()), |(s, n)| (s.to_string(), n.to_string()));
            out.push(("exec".to_string(), String::new(), schema, name, String::new(), node.line));
            continue;
        }
        if node.kind != "table" {
            continue;
        }
        let schema = child(i, "schema").unwrap_or_default();
        let database = child(i, "database").unwrap_or_default();
        if schema.is_empty() && ctes.contains(&node.value.to_lowercase()) {
            continue;
        }
        let wrapper = node.parent.map_or("", |p| nodes[p].kind.as_str());
        let action = match (wrapper, root) {
            ("target", r) if written => r,
            ("target", "copy") => "insert",
            ("target", "alter table") => "alter",
            ("target", "create policy") => "policy",
            ("references" | "inherits" | "attach" | "detach", _) => wrapper,
            _ => "select",
        };
        if wrapper == "target" && (written || root == "copy") && target.is_none() {
            target = Some((database.clone(), schema.clone(), node.value.clone()));
        }
        out.push((action.to_string(), database, schema, node.value.clone(), String::new(), node.line));
    }
    // THE COLUMNS AN INSERT OR A COPY ... FROM LISTS AND AN UPDATE SETS are the target's.
    if let Some((database, schema, name)) = target {
        let action = if root == "copy" { "insert" } else { root };
        for node in nodes {
            let listed = matches!(root, "insert" | "copy") && node.kind == "column" && node.parent == Some(0);
            let set = root == "update" && node.kind == "column" && node.parent.is_some_and(|p| nodes[p].kind == "set" && nodes[p].parent == Some(0)) && node.ord == 0;
            if listed || set {
                let column = node.value.rsplit('.').next().unwrap_or("").to_string();
                out.push((action.to_string(), database.clone(), schema.clone(), name.clone(), column, node.line));
            }
        }
    }
    out
}
