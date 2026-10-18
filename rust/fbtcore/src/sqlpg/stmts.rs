//! WHAT A POSTGRESQL STATEMENT'S ROOT HOLDS, and the statement's kind in the words the T-SQL half uses for the same
//! statement. A statement type not named here is its root alone: an unmapped GRANT or VACUUM names tables it neither
//! reads nor writes, and a `table` node under it would be read as a touch.

use super::tree::{made, nodes, strings, text, typed, Ctx, Item, Made, Role};
use serde_json::Value;

/// A statement's kind: the T-SQL half's word where there is one, else its type in words (`VacuumStmt` is `vacuum`).
pub fn kind_of(kind: &str, body: &Value) -> String {
    let fixed = match kind {
        "SelectStmt" => "select",
        "InsertStmt" => "insert",
        "UpdateStmt" => "update",
        "DeleteStmt" => "delete",
        "MergeStmt" => "merge",
        "CreateStmt" => "create table",
        "CreateTableAsStmt" if text(body, "objtype") == "OBJECT_MATVIEW" => "create materialized view",
        "CreateTableAsStmt" => "create table",
        "ViewStmt" => "create view",
        "IndexStmt" => "create index",
        "CreateFunctionStmt" if body["is_procedure"].as_bool() == Some(true) => "create procedure",
        "CreateFunctionStmt" => "create function",
        "CreateTrigStmt" => "create trigger",
        "CreateSeqStmt" => "create sequence",
        "CompositeTypeStmt" | "CreateEnumStmt" | "CreateRangeStmt" => "create type",
        "CreateDomainStmt" => "create domain",
        "CreateSchemaStmt" => "create schema",
        "CreateExtensionStmt" => "create extension",
        "CreatePolicyStmt" => "create policy",
        "AlterTableStmt" => "alter table",
        "DropStmt" => "drop",
        "TruncateStmt" => "truncate",
        "VariableSetStmt" => "set",
        "TransactionStmt" => match text(body, "kind") {
            "TRANS_STMT_COMMIT" => "commit",
            "TRANS_STMT_ROLLBACK" => "rollback",
            "TRANS_STMT_BEGIN" | "TRANS_STMT_START" => "begin transaction",
            _ => "transaction",
        },
        "GrantStmt" if body["is_grant"].as_bool() == Some(true) => "grant",
        "GrantStmt" => "revoke",
        "CommentStmt" => "comment",
        "CopyStmt" => "copy",
        "CallStmt" => "execute",
        "ExplainStmt" => "explain",
        "DeclareCursorStmt" => "declare",
        "DoStmt" => "do",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    let name = kind.strip_suffix("Stmt").unwrap_or(kind);
    let mut words = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() && !words.is_empty() {
            words.push(' ');
        }
        words.push(c.to_ascii_lowercase());
    }
    words
}

/// The CTEs of a statement's WITH: they answer the root, as ScriptDom's do.
fn ctes(body: &Value) -> Vec<Item<'_>> {
    body["withClause"]["ctes"].as_array().into_iter().flatten().map(Item::Node).collect()
}

fn output(body: &Value) -> Option<Item<'_>> {
    let items: Vec<Item> = body["returningList"].as_array().into_iter().flatten().filter_map(|r| typed(r).map(|(_, b)| Item::Res(b, Role::Item))).collect();
    (!items.is_empty()).then(|| Item::Wrap("output", items))
}

fn wrap<'v>(kind: &'static str, body: &'v Value, field: &str) -> Option<Item<'v>> {
    body.get(field).map(|v| Item::Wrap(kind, vec![Item::Node(v)]))
}

fn listed<'v>(kind: &'static str, body: &'v Value, field: &str) -> Option<Item<'v>> {
    let items = nodes(body, field);
    (!items.is_empty()).then(|| Item::Wrap(kind, items))
}

fn targets<'v>(body: &'v Value, field: &str, role: Role) -> Vec<Item<'v>> {
    body[field].as_array().into_iter().flatten().filter_map(|r| typed(r).map(|(_, b)| Item::Res(b, role))).collect()
}

fn target(rv: &Value) -> Item<'_> {
    Item::Wrap("target", vec![Item::As("RangeVar", rv)])
}

/// What a statement's root holds.
pub fn roots<'v>(ctx: &Ctx, kind: &str, body: &'v Value) -> Vec<Item<'v>> {
    let name = |at: &Value, n: String| Item::Leaf("name", n, ctx.at(at));
    let mut items: Vec<Item> = Vec::new();
    match kind {
        "SelectStmt" => items.push(Item::Select(body)),
        "InsertStmt" => {
            items.extend(ctes(body));
            items.push(target(&body["relation"]));
            items.extend(targets(body, "cols", Role::Column));
            items.extend(body.get("selectStmt").map(Item::Node));
            if let Some(conflict) = body.get("onConflictClause") {
                items.extend(targets(conflict, "targetList", Role::Set));
                items.extend(wrap("where", conflict, "whereClause"));
            }
            items.extend(output(body));
        }
        "UpdateStmt" => {
            items.extend(ctes(body));
            items.push(target(&body["relation"]));
            items.extend(targets(body, "targetList", Role::Set));
            items.extend(listed("from", body, "fromClause"));
            items.extend(wrap("where", body, "whereClause"));
            items.extend(output(body));
        }
        "DeleteStmt" => {
            items.extend(ctes(body));
            items.push(target(&body["relation"]));
            items.extend(listed("from", body, "usingClause"));
            items.extend(wrap("where", body, "whereClause"));
            items.extend(output(body));
        }
        "MergeStmt" => {
            items.extend(ctes(body));
            items.push(target(&body["relation"]));
            items.extend(wrap("using", body, "sourceRelation"));
            items.extend(wrap("on", body, "joinCondition"));
            items.extend(nodes(body, "mergeWhenClauses"));
            items.extend(output(body));
        }
        "CreateStmt" => {
            items.push(name(&body["relation"], ctx.relation(&body["relation"])));
            for element in body["tableElts"].as_array().into_iter().flatten() {
                match typed(element) {
                    Some(("ColumnDef", c)) => items.push(Item::As("ColumnDefItem", c)),
                    Some(("Constraint", c)) => items.extend(constraint(c)),
                    _ => {}
                }
            }
            items.extend(listed("inherits", body, "inhRelations"));
        }
        "CreateTableAsStmt" => {
            let rel = &body["into"]["rel"];
            items.push(name(rel, ctx.relation(rel)));
            items.extend(body.get("query").map(Item::Node));
        }
        "ViewStmt" => items.push(name(&body["view"], ctx.relation(&body["view"]))),
        "IndexStmt" => {
            if let Some(index) = body["idxname"].as_str() {
                items.push(Item::Leaf("name", ctx.spell(index), None));
            }
            items.push(Item::As("RangeVar", &body["relation"]));
        }
        "CreateFunctionStmt" => items.push(Item::Leaf("name", ctx.dotted_at(None, &strings(&body["funcname"])), None)),
        "CreateTrigStmt" => {
            items.push(Item::Leaf("name", ctx.spell(text(body, "trigname")), None));
            items.push(Item::As("RangeVar", &body["relation"]));
        }
        "CreateSeqStmt" => items.push(name(&body["sequence"], ctx.relation(&body["sequence"]))),
        "CompositeTypeStmt" => items.push(name(&body["typevar"], ctx.relation(&body["typevar"]))),
        "CreateEnumStmt" | "CreateRangeStmt" => items.push(Item::Leaf("name", ctx.dotted_at(None, &strings(&body["typeName"])), None)),
        "CreateDomainStmt" => items.push(Item::Leaf("name", ctx.dotted_at(None, &strings(&body["domainname"])), None)),
        "CreatePolicyStmt" => {
            items.push(Item::Leaf("name", ctx.spell(text(body, "policy_name")), None));
            items.push(target(&body["table"]));
            items.extend(body.get("qual").map(Item::Node));
            items.extend(body.get("with_check").map(Item::Node));
        }
        "AlterTableStmt" => {
            items.push(target(&body["relation"]));
            for cmd in body["cmds"].as_array().into_iter().flatten().filter_map(|c| typed(c).map(|(_, b)| b)) {
                match (text(cmd, "subtype"), typed(&cmd["def"])) {
                    ("AT_AttachPartition", Some((_, part))) => items.push(Item::Wrap("attach", vec![Item::As("RangeVar", &part["name"])])),
                    ("AT_DetachPartition", Some((_, part))) => items.push(Item::Wrap("detach", vec![Item::As("RangeVar", &part["name"])])),
                    (_, Some(("Constraint", c))) => items.extend(constraint(c)),
                    (_, Some(("ColumnDef", c))) => items.push(Item::As("ColumnDefItem", c)),
                    _ => {}
                }
            }
        }
        "DropStmt" => {
            for object in body["objects"].as_array().into_iter().flatten() {
                let parts = match typed(object) {
                    Some(("List", l)) => strings(&l["items"]),
                    Some(("String", s)) => vec![text(s, "sval").to_string()],
                    Some(("TypeName", t)) => strings(&t["names"]),
                    Some(("ObjectWithArgs", o)) => strings(&o["objname"]),
                    _ => Vec::new(),
                };
                if !parts.is_empty() {
                    items.push(Item::Leaf("name", ctx.dotted_at(None, &parts), None));
                }
            }
        }
        "TruncateStmt" => items.extend(body["relations"].as_array().into_iter().flatten().map(|r| Item::Wrap("target", vec![Item::Node(r)]))),
        // COPY ... FROM writes the table, and its column list is what it writes - an INSERT's; COPY ... TO reads it.
        "CopyStmt" => {
            if let Some(rel) = body.get("relation") {
                if body["is_from"].as_bool() == Some(true) {
                    items.push(target(rel));
                    items.extend(strings(&body["attlist"]).into_iter().map(|c| Item::Leaf("column", ctx.spell(&c), None)));
                } else {
                    items.push(Item::As("RangeVar", rel));
                }
            }
            items.extend(body.get("query").map(Item::Node));
        }
        "CallStmt" => {
            let call = &body["funccall"];
            items.push(Item::Leaf("proc", ctx.dotted_at(None, &strings(&call["funcname"])), ctx.at(call)));
            items.extend(nodes(call, "args"));
        }
        _ => {}
    }
    items
}

/// A table constraint: a FOREIGN KEY's referenced table under `references`; a CHECK's or DEFAULT's expression as it is.
fn constraint(c: &Value) -> Vec<Item<'_>> {
    match text(c, "contype") {
        "CONSTR_FOREIGN" => c.get("pktable").map(|t| Item::Wrap("references", vec![Item::As("RangeVar", t)])).into_iter().collect(),
        _ => ["raw_expr", "where_clause"].iter().filter_map(|f| c.get(*f).map(Item::Node)).collect(),
    }
}

/// A column definition: its name, its type, and what its constraints hold.
pub fn column_def<'v>(ctx: &Ctx, body: &'v Value) -> (Option<Made>, Vec<Item<'v>>) {
    let mut children: Vec<Item> = body.get("typeName").map(|t| Item::As("TypeName", t)).into_iter().collect();
    for c in body["constraints"].as_array().into_iter().flatten().filter_map(|c| typed(c).map(|(_, b)| b)) {
        children.extend(constraint(c));
    }
    (made("columndef", ctx.dotted_at(ctx.at(body), &[text(body, "colname").to_string()]), ctx.at(body)), children)
}

/// A SELECT's body: a set operation, a VALUES list or a query - then its ORDER BY, LIMIT and OFFSET on that node, as
/// the T-SQL tree has them.
pub fn select_body(body: &Value) -> (Option<Made>, Vec<Item<'_>>) {
    let (node, mut children) = match text(body, "op") {
        "SETOP_UNION" | "SETOP_EXCEPT" | "SETOP_INTERSECT" => {
            let kind = text(body, "op").trim_start_matches("SETOP_").to_lowercase();
            let all = if body["all"].as_bool() == Some(true) { "all" } else { "" };
            (made(&kind, all, None), ["larg", "rarg"].iter().filter_map(|f| body.get(*f).map(Item::Select)).collect())
        }
        _ if body.get("valuesLists").is_some() => {
            let rows = body["valuesLists"].as_array().into_iter().flatten().filter_map(|r| typed(r).map(|(_, l)| Item::Wrap("row", nodes(l, "items")))).collect();
            (made("values", "", None), rows)
        }
        _ => {
            let mut items: Vec<Item> = Vec::new();
            if body.get("distinctClause").is_some() {
                items.push(Item::Leaf("distinct", String::new(), None));
            }
            items.extend(targets(body, "targetList", Role::Item));
            if let Some(into) = body.get("intoClause") {
                items.push(Item::Wrap("into", vec![Item::As("RangeVar", &into["rel"])]));
            }
            items.extend(listed("from", body, "fromClause"));
            items.extend(wrap("where", body, "whereClause"));
            items.extend(listed("groupby", body, "groupClause"));
            items.extend(wrap("having", body, "havingClause"));
            (made("query", "", None), items)
        }
    };
    children.extend(listed("orderby", body, "sortClause"));
    children.extend(wrap("limit", body, "limitCount"));
    children.extend(wrap("offset", body, "limitOffset"));
    (node, children)
}

/// A MERGE's WHEN: `action` (`matched`, `not matched`, `not matched by source`), its condition, then what it does.
pub fn merge_when(body: &Value) -> (Option<Made>, Vec<Item<'_>>) {
    let kind = match text(body, "matchKind") {
        "MERGE_WHEN_MATCHED" => "matched",
        "MERGE_WHEN_NOT_MATCHED_BY_SOURCE" => "not matched by source",
        _ => "not matched",
    };
    let mut children: Vec<Item> = wrap("where", body, "condition").into_iter().collect();
    children.push(match text(body, "commandType") {
        "CMD_INSERT" => {
            let mut insert = targets(body, "targetList", Role::Column);
            let values = nodes(body, "values");
            if !values.is_empty() {
                insert.push(Item::Wrap("values", vec![Item::Wrap("row", values)]));
            }
            Item::Wrap("insert", insert)
        }
        "CMD_UPDATE" => Item::Wrap("update", targets(body, "targetList", Role::Set)),
        "CMD_DELETE" => Item::Wrap("delete", Vec::new()),
        _ => Item::Wrap("nothing", Vec::new()),
    });
    (made("action", kind, None), children)
}
