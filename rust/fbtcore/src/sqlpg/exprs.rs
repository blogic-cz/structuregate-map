//! ONE PART OF A POSTGRESQL TREE MADE INTO ITS NODE, and what hangs off it - in the kinds the T-SQL half writes
//! (`column`, `literal`, `null`, `param`, `op`, `func`, `in`, `exists`, `case`, `when`, `subquery`, `table`, `join`,
//! `query`, `item`, `sort`, `cte`, `over`, `type`, ...). An operator is named by its symbol, a function by its name
//! lowercased; parentheses never reach the tree. A part this file does not name is transparent.

use super::stmts;
use super::tree::{made, nodes, strings, text, typed, Ctx, Item, Made, Role};
use serde_json::Value;

pub fn expand<'v>(ctx: &Ctx, item: Item<'v>) -> (Option<Made>, Vec<Item<'v>>) {
    match item {
        Item::Leaf(kind, value, at) => (made(kind, value, at), Vec::new()),
        Item::Wrap(kind, children) => (made(kind, "", None), children),
        Item::Valued(kind, value, children) => (made(kind, value, None), children),
        Item::Select(body) => {
            let mut children: Vec<Item> = body["withClause"]["ctes"].as_array().into_iter().flatten().map(Item::Node).collect();
            children.push(Item::As("SelectBody", body));
            (None, children)
        }
        Item::Res(body, role) => res(ctx, body, role),
        Item::Fields(body) => (None, fields(body)),
        Item::As(kind, body) => node(ctx, kind, body),
        Item::Node(v) => match typed(v) {
            Some((kind, body)) => node(ctx, kind, body),
            None => (None, Vec::new()),
        },
    }
}

/// Every node an unmapped part holds, in field order: a field of one node type (no wrapper) is read as its fields, a
/// RangeVar - the one such field a statement's touches hang on - by its `relname`.
fn fields(body: &Value) -> Vec<Item<'_>> {
    let mut out = Vec::new();
    for value in body.as_object().into_iter().flat_map(|m| m.values()) {
        match value {
            Value::Array(items) => out.extend(items.iter().filter(|v| typed(v).is_some()).map(Item::Node)),
            Value::Object(_) if typed(value).is_some() => out.push(Item::Node(value)),
            Value::Object(map) if map.contains_key("relname") => out.push(Item::As("RangeVar", value)),
            Value::Object(_) => out.push(Item::Fields(value)),
            _ => {}
        }
    }
    out
}

fn res<'v>(ctx: &Ctx, body: &'v Value, role: Role) -> (Option<Made>, Vec<Item<'v>>) {
    let name = text(body, "name");
    let mut indirect: Vec<String> = vec![name.to_string()];
    indirect.extend(strings(&body["indirection"]));
    let at = ctx.at(body);
    match role {
        Role::Column => (made("column", ctx.dotted_at(at, &indirect), at), Vec::new()),
        Role::Set => {
            let mut children = vec![Item::Leaf("column", ctx.dotted_at(at, &indirect), at)];
            children.extend(body.get("val").map(Item::Node));
            (made("set", "", at), children)
        }
        Role::Item => {
            // The expression, then its alias: where each starts in the source, as ScriptDom orders them.
            let mut children: Vec<Item> = body.get("val").map(Item::Node).into_iter().collect();
            if !name.is_empty() {
                let spelled = at.and_then(|a| ctx.source.last_word(a, name)).unwrap_or_else(|| ctx.spell(name));
                children.push(Item::Leaf("alias", spelled, None));
            }
            (made("item", "", at), children)
        }
    }
}

fn node<'v>(ctx: &Ctx, kind: &str, body: &'v Value) -> (Option<Made>, Vec<Item<'v>>) {
    let at = || ctx.at(body);
    let one = |field: &str| body.get(field).map(Item::Node).into_iter().collect::<Vec<_>>();
    match kind {
        "ColumnRef" => {
            let parts = strings(&body["fields"]);
            let first = parts.first().map(|p| p.to_lowercase()).unwrap_or_default();
            if ctx.vars.contains(&first) {
                (made("param", parts.join("."), at()), Vec::new())
            } else {
                (made("column", ctx.dotted_at(at(), &parts), at()), Vec::new())
            }
        }
        "A_Const" => {
            let literal = |v: &Value| match v {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            };
            if body["isnull"].as_bool() == Some(true) {
                return (made("null", "", at()), Vec::new());
            }
            let value = match () {
                _ if body.get("ival").is_some() => body["ival"].get("ival").map_or("0".into(), literal),
                _ if body.get("boolval").is_some() => body["boolval"].get("boolval").map_or("false".into(), literal),
                _ => ["fval", "sval", "bsval"].iter().find_map(|f| body.get(*f).map(|v| v.get(*f).map_or(String::new(), literal))).unwrap_or_default(),
            };
            (made("literal", value, at()), Vec::new())
        }
        "ParamRef" => (made("param", format!("${}", body["number"].as_i64().unwrap_or(0)), at()), Vec::new()),
        "A_Expr" => a_expr(ctx, body),
        "BoolExpr" => bool_expr(ctx, body),
        "NullTest" => (made("op", if text(body, "nulltesttype") == "IS_NOT_NULL" { "is not null" } else { "is null" }, at()), one("arg")),
        "BooleanTest" => {
            let test = text(body, "booltesttype").trim_start_matches("IS_").replace('_', " ").to_lowercase();
            (made("op", format!("is {test}"), at()), one("arg"))
        }
        "SubLink" => sublink(ctx, body),
        "FuncCall" => func(ctx, body),
        "CoalesceExpr" => (made("func", "coalesce", at()), nodes(body, "args")),
        "MinMaxExpr" => (made("func", if text(body, "op") == "IS_LEAST" { "least" } else { "greatest" }, at()), nodes(body, "args")),
        "SQLValueFunction" => (made("func", text(body, "op").trim_start_matches("SVFOP_").to_lowercase(), at()), Vec::new()),
        "GroupingFunc" => (made("func", "grouping", at()), nodes(body, "args")),
        "TypeCast" => {
            let mut children = one("arg");
            children.extend(body.get("typeName").map(|t| Item::As("TypeName", t)));
            (made("func", "cast", at()), children)
        }
        "TypeName" => (made("type", type_name(ctx, body), at()), nodes(body, "typmods")),
        "CaseExpr" => {
            let mut children = one("arg");
            children.extend(nodes(body, "args"));
            children.extend(one("defresult"));
            (made("case", "", at()), children)
        }
        "CaseWhen" => {
            let mut children = one("expr");
            children.extend(one("result"));
            (made("when", "", at()), children)
        }
        "SortBy" => {
            let dir = match text(body, "sortby_dir") { "SORTBY_DESC" => "desc", "SORTBY_ASC" => "asc", _ => "" };
            (made("sort", dir, at()), one("node"))
        }
        "WindowDef" => {
            let mut children: Vec<Item> = body["partitionClause"].as_array().into_iter().flatten().map(|p| Item::Wrap("partition", vec![Item::Node(p)])).collect();
            let sorts = nodes(body, "orderClause");
            if !sorts.is_empty() {
                children.push(Item::Wrap("orderby", sorts));
            }
            (made("over", "", at()), children)
        }
        "RangeVar" => {
            let ([catalog, schema, name], alias) = ctx.rel_parts(body);
            let mut children = Vec::new();
            for (kind, part) in [("database", catalog), ("schema", schema)] {
                if !part.is_empty() {
                    children.push(Item::Leaf(kind, part, at()));
                }
            }
            if !alias.is_empty() {
                children.push(Item::Leaf("alias", alias, None));
            }
            (made("table", name, at()), children)
        }
        "RangeSubselect" => {
            let mut children = one("subquery");
            children.extend(alias(ctx, body));
            (made("subquery", "", None), children)
        }
        "RangeFunction" => {
            // `FROM fn(..) AS f`: the function, its alias under it.
            let call = body["functions"].as_array().and_then(|f| f.first()).and_then(|l| l["List"]["items"].as_array()).and_then(|i| i.first());
            match call.and_then(typed) {
                Some(("FuncCall", f)) => {
                    let (node, mut children) = func(ctx, f);
                    children.extend(alias(ctx, body));
                    (node, children)
                }
                _ => (None, Vec::new()),
            }
        }
        "JoinExpr" => join(body),
        "CommonTableExpr" => (made("cte", ctx.dotted_at(at(), &[text(body, "ctename").to_string()]), at()), one("ctequery")),
        "SelectStmt" => (None, vec![Item::Select(body)]),
        "SelectBody" => stmts::select_body(body),
        "ResTarget" => res(ctx, body, Role::Item),
        "MultiAssignRef" => (None, if body["colno"].as_i64() == Some(1) { one("source") } else { Vec::new() }),
        "MergeWhenClause" => stmts::merge_when(body),
        "ColumnDef" | "ColumnDefItem" => stmts::column_def(ctx, body),
        "InsertStmt" | "UpdateStmt" | "DeleteStmt" | "MergeStmt" => (None, stmts::roots(ctx, kind, body)),
        "String" | "Integer" | "Float" | "Boolean" | "BitString" | "A_Star" | "Alias" | "RoleSpec" | "DefElem" => (None, Vec::new()),
        _ => (None, fields(body)),
    }
}

/// A RangeVar's, a subquery's or a function's alias, spelled. It has no location of its own: made last, it sorts after
/// what it names, where the source has it.
fn alias<'v>(ctx: &Ctx, body: &Value) -> Vec<Item<'v>> {
    body["alias"]["aliasname"].as_str().map(|a| Item::Leaf("alias", ctx.spell(a), None)).into_iter().collect()
}

fn a_expr<'v>(ctx: &Ctx, body: &'v Value) -> (Option<Made>, Vec<Item<'v>>) {
    let op = strings(&body["name"]).pop().unwrap_or_default();
    let at = ctx.at(body);
    let mut sides: Vec<Item> = body.get("lexpr").map(Item::Node).into_iter().collect();
    let listed = |sides: &mut Vec<Item<'v>>| match typed(&body["rexpr"]) {
        Some(("List", list)) => sides.extend(nodes(list, "items")),
        _ => sides.extend(body.get("rexpr").map(Item::Node)),
    };
    let named = match text(body, "kind") {
        "AEXPR_IN" => {
            listed(&mut sides);
            return (made("in", if op == "<>" { "not" } else { "" }, at), sides);
        }
        "AEXPR_BETWEEN" | "AEXPR_BETWEEN_SYM" => "between".to_string(),
        "AEXPR_NOT_BETWEEN" | "AEXPR_NOT_BETWEEN_SYM" => "not between".to_string(),
        "AEXPR_NULLIF" => {
            listed(&mut sides);
            return (made("func", "nullif", at), sides);
        }
        "AEXPR_OP_ANY" => format!("{op} any"),
        "AEXPR_OP_ALL" => format!("{op} all"),
        "AEXPR_DISTINCT" => "is distinct from".to_string(),
        "AEXPR_NOT_DISTINCT" => "is not distinct from".to_string(),
        "AEXPR_LIKE" => if op.starts_with('!') { "not like" } else { "like" }.to_string(),
        "AEXPR_ILIKE" => if op.starts_with('!') { "not ilike" } else { "ilike" }.to_string(),
        "AEXPR_SIMILAR" => if op.starts_with('!') { "not similar to" } else { "similar to" }.to_string(),
        _ => op.to_lowercase(),
    };
    listed(&mut sides);
    (made("op", named, at), sides)
}

/// AND and OR as T-SQL nests them - each a pair, left-deep - so `a AND b AND c` shapes alike in both dialects. NOT IN
/// a subquery is `in` `not`, as T-SQL has it.
fn bool_expr<'v>(ctx: &Ctx, body: &'v Value) -> (Option<Made>, Vec<Item<'v>>) {
    let args: Vec<&Value> = body["args"].as_array().into_iter().flatten().collect();
    let at = ctx.at(body);
    match text(body, "boolop") {
        "NOT_EXPR" => {
            if let [only] = args.as_slice()
                && let Some(("SubLink", link)) = typed(only)
                && text(link, "subLinkType") == "ANY_SUBLINK"
                && link.get("operName").is_none()
            {
                let (_, children) = sublink(ctx, link);
                return (made("in", "not", ctx.at(link)), children);
            }
            (made("op", "not", at), args.into_iter().map(Item::Node).collect())
        }
        op => {
            let name = if op == "OR_EXPR" { "or" } else { "and" };
            let mut items = args.into_iter().map(Item::Node);
            let mut left = match items.next() {
                Some(first) => first,
                None => return (made("op", name, at), Vec::new()),
            };
            let rest: Vec<Item> = items.collect();
            let Some((last, middle)) = split_last(rest) else { return (None, vec![left]) };
            for right in middle {
                left = Item::Valued("op", name.to_string(), vec![left, right]);
            }
            (made("op", name, at), vec![left, last])
        }
    }
}

fn split_last<T>(mut items: Vec<T>) -> Option<(T, Vec<T>)> {
    let last = items.pop()?;
    Some((last, items))
}

fn sublink<'v>(ctx: &Ctx, body: &'v Value) -> (Option<Made>, Vec<Item<'v>>) {
    let at = ctx.at(body);
    let query = || Item::Wrap("subquery", body.get("subselect").map(Item::Node).into_iter().collect());
    let test: Vec<Item> = body.get("testexpr").map(Item::Node).into_iter().collect();
    let op = strings(&body["operName"]).pop();
    let with = |mut t: Vec<Item<'v>>, q: Item<'v>| {
        t.push(q);
        t
    };
    match text(body, "subLinkType") {
        "EXISTS_SUBLINK" => (made("exists", "", at), vec![query()]),
        "ANY_SUBLINK" => match op {
            None => (made("in", "", at), with(test, query())),
            Some(op) => (made("op", format!("{op} any"), at), with(test, query())),
        },
        "ALL_SUBLINK" => (made("op", format!("{} all", op.unwrap_or_default()), at), with(test, query())),
        "EXPR_SUBLINK" => (made("subquery", "", at), body.get("subselect").map(Item::Node).into_iter().collect()),
        "ARRAY_SUBLINK" => (made("func", "array", at), vec![query()]),
        _ => (None, with(test, query())),
    }
}

/// A function by its name, lowercased; `pg_catalog.` only where the source spells it - the parser names a few
/// built-ins (`extract`, `substring`) by their catalog path.
pub fn func<'v>(ctx: &Ctx, body: &'v Value) -> (Option<Made>, Vec<Item<'v>>) {
    let mut parts = strings(&body["funcname"]);
    let at = ctx.at(body);
    let spelled_catalog = at.is_some_and(|a| {
        let i = ctx.source.token_at(a);
        ctx.source.tokens.get(i).is_some_and(|t| ctx.source.word(t).eq_ignore_ascii_case("pg_catalog"))
    });
    if parts.len() > 1 && parts[0] == "pg_catalog" && !spelled_catalog {
        parts.remove(0);
    }
    let mut children = nodes(body, "args");
    if body["agg_star"].as_bool() == Some(true) {
        children.push(Item::Leaf("column", "*".into(), None));
    }
    let sorts = nodes(body, "agg_order");
    if !sorts.is_empty() {
        children.push(Item::Wrap("orderby", sorts));
    }
    children.extend(body.get("agg_filter").map(|f| Item::Wrap("where", vec![Item::Node(f)])));
    children.extend(body.get("over").map(|w| Item::As("WindowDef", w)));
    (made("func", parts.join(".").to_lowercase(), at), children)
}

/// A type by its name lowercased - as the source spells it where the parser named a built-in by its catalog name
/// (`int` is `pg_catalog.int4`).
fn type_name(ctx: &Ctx, body: &Value) -> String {
    let parts = strings(&body["names"]);
    if parts.first().is_some_and(|p| p == "pg_catalog")
        && let Some(at) = ctx.at(body)
        && let Some(token) = ctx.source.tokens.get(ctx.source.token_at(at)).filter(|t| t.start == at)
        && !ctx.source.word(token).eq_ignore_ascii_case("pg_catalog")
    {
        return ctx.source.word(token).to_lowercase();
    }
    parts.last().cloned().unwrap_or_default().to_lowercase()
}

/// A join: its two sides in order, then `on`. A JOIN with no condition is `cross`, and `cross apply` when its right
/// side is LATERAL - T-SQL's word for the same thing.
fn join(body: &Value) -> (Option<Made>, Vec<Item<'_>>) {
    let lateral = typed(&body["rarg"]).is_some_and(|(_, r)| r["lateral"].as_bool() == Some(true));
    let bare = body.get("quals").is_none() && body.get("usingClause").is_none() && body["isNatural"].as_bool() != Some(true);
    let kind = match text(body, "jointype") {
        "JOIN_LEFT" => "left",
        "JOIN_RIGHT" => "right",
        "JOIN_FULL" => "full",
        _ if bare && lateral => "cross apply",
        _ if bare => "cross",
        _ => "inner",
    };
    let mut children: Vec<Item> = ["larg", "rarg"].iter().filter_map(|f| body.get(*f).map(Item::Node)).collect();
    children.extend(body.get("quals").map(|q| Item::Wrap("on", vec![Item::Node(q)])));
    (made("join", kind, None), children)
}
