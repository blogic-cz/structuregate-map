//! A POSTGRESQL STATEMENT AS THE NORMALIZED TREE `sql_nodes` holds - the kinds the T-SQL half writes
//! (`src/Map/Sql/SqlTree.cs`), so one query, one fingerprint and one column pass read both dialects. sqlparser's
//! AST is mapped by hand: a variant mapped here is a node, a wrapper no AST type is (`on`, `target`, `using`,
//! `partition`) is made from where a part sits, and what is not mapped is left out with its children.
//!
//! WITH AN EXPLICIT STACK, never by recursion: a statement's tree is as deep as the statement.

use sqlparser::ast::{
    Assignment, AssignmentTarget, Cte, Distinct, Expr, FromTable, Function, FunctionArg, FunctionArgExpr, FunctionArguments,
    GroupByExpr, Ident, JoinConstraint, JoinOperator, LimitClause, MergeAction, MergeClause, MergeClauseKind, MergeInsertKind,
    MergeUpdateKind, ObjectName, ObjectNamePart, OrderByKind, OrderByExpr, OrderBySort, Query, Select, SelectItem, SelectItemQualifiedWildcardKind, SetExpr,
    SetOperator, SetQuantifier, Statement, TableFactor, TableObject, TableWithJoins, TopQuantity, UpdateTableFromKind, Value,
    WindowType,
};

#[derive(Clone, Debug)]
pub struct PgNode {
    pub parent: Option<usize>,
    pub ord: usize,
    pub kind: String,
    pub value: String,
    pub line: i64,
}

/// A part of the statement still to be turned into nodes, under `parent`.
pub(super) enum Item<'a> {
    Query(&'a Query),
    Body(&'a SetExpr, Option<&'a Query>),
    Expr(&'a Expr),
    Joined(&'a TableWithJoins, usize),
    Factor(&'a TableFactor),
    Select(&'a SelectItem),
    Sort(&'a OrderByExpr),
    Set(&'a Assignment),
    Cte(&'a Cte),
    Merge(&'a MergeClause),
    Function(&'a Function),
    Leaf(&'static str, String, i64),
    Wrap(&'static str, Vec<Item<'a>>),
}

pub fn parts(name: &ObjectName) -> Vec<&Ident> {
    name.0.iter().filter_map(|p| match p { ObjectNamePart::Identifier(i) => Some(i), _ => None }).collect()
}

pub fn dotted(name: &ObjectName) -> String {
    parts(name).iter().map(|i| i.value.as_str()).collect::<Vec<_>>().join(".")
}

fn line_of(ident: &Ident, fallback: i64) -> i64 {
    if ident.span.start.line > 0 { ident.span.start.line as i64 } else { fallback }
}

/// The tree of a statement: its root `kind`, then what the statement holds.
pub fn of(statement: &Statement, kind: &str, line: i64) -> Vec<PgNode> {
    built(roots(statement, line), kind, line)
}

/// A view's query as a statement of its own, as ScriptDom has the SELECT inside a CREATE VIEW.
pub fn of_query(query: &Query, line: i64) -> Vec<PgNode> {
    built(vec![Item::Query(query)], "select", line)
}

fn built(roots: Vec<Item<'_>>, kind: &str, line: i64) -> Vec<PgNode> {
    let mut nodes = vec![PgNode { parent: None, ord: 0, kind: kind.into(), value: String::new(), line }];
    let mut count = vec![0usize];
    let mut stack: Vec<(Item, usize, i64)> = Vec::new();
    for item in roots.into_iter().rev() {
        stack.push((item, 0, line));
    }
    while let Some((item, parent, line)) = stack.pop() {
        let mut add = |kind: &str, value: String, line: i64, under: usize| -> usize {
            nodes.push(PgNode { parent: Some(under), ord: count[under], kind: kind.into(), value, line });
            count[under] += 1;
            count.push(0);
            nodes.len() - 1
        };
        let (at, children) = expand(item, parent, line, &mut add);
        let line = nodes.get(at).map_or(line, |n| n.line);
        for child in children.into_iter().rev() {
            stack.push((child, at, line));
        }
    }
    nodes
}

/// What a statement's root holds.
fn roots(statement: &Statement, line: i64) -> Vec<Item<'_>> {
    let name = |kind: &'static str, n: &ObjectName| Item::Leaf(kind, dotted(n), parts(n).first().map_or(line, |i| line_of(i, line)));
    match statement {
        Statement::Query(q) => vec![Item::Query(q)],
        Statement::Insert(insert) => {
            let mut items = Vec::new();
            if let TableObject::TableName(table) = &insert.table {
                items.push(Item::Wrap("target", vec![table_leaf(table, line)]));
            }
            items.extend(insert.columns.iter().map(|c| name("column", c)));
            if let Some(source) = &insert.source {
                items.push(Item::Query(source));
            }
            if let Some(returning) = &insert.returning {
                items.push(Item::Wrap("output", returning.iter().map(Item::Select).collect()));
            }
            items
        }
        Statement::Update(update) => {
            let mut items = vec![Item::Wrap("target", vec![Item::Joined(&update.table, update.table.joins.len())])];
            items.extend(update.assignments.iter().map(Item::Set));
            if let Some(from) = &update.from {
                let tables = match from { UpdateTableFromKind::BeforeSet(t) | UpdateTableFromKind::AfterSet(t) => t };
                items.push(Item::Wrap("from", tables.iter().map(|t| Item::Joined(t, t.joins.len())).collect()));
            }
            if let Some(selection) = &update.selection {
                items.push(Item::Wrap("where", vec![Item::Expr(selection)]));
            }
            if let Some(returning) = &update.returning {
                items.push(Item::Wrap("output", returning.iter().map(Item::Select).collect()));
            }
            items
        }
        Statement::Delete(delete) => {
            let tables = match &delete.from { FromTable::WithFromKeyword(t) | FromTable::WithoutKeyword(t) => t };
            let mut items: Vec<Item> = tables.iter().map(|t| Item::Wrap("target", vec![Item::Joined(t, t.joins.len())])).collect();
            if let Some(using) = &delete.using {
                items.push(Item::Wrap("from", using.iter().map(|t| Item::Joined(t, t.joins.len())).collect()));
            }
            if let Some(selection) = &delete.selection {
                items.push(Item::Wrap("where", vec![Item::Expr(selection)]));
            }
            if let Some(returning) = &delete.returning {
                items.push(Item::Wrap("output", returning.iter().map(Item::Select).collect()));
            }
            items
        }
        Statement::Merge(merge) => {
            let mut items = vec![
                Item::Wrap("target", vec![Item::Factor(&merge.table)]),
                Item::Wrap("using", vec![Item::Factor(&merge.source)]),
                Item::Wrap("on", vec![Item::Expr(&merge.on)]),
            ];
            items.extend(merge.clauses.iter().map(Item::Merge));
            items
        }
        Statement::CreateTable(table) => {
            let mut items = vec![name("name", &table.name)];
            items.extend(table.columns.iter().map(|c| Item::Leaf("columndef", c.name.value.clone(), line_of(&c.name, line))));
            if let Some(query) = &table.query {
                items.push(Item::Query(query));
            }
            items
        }
        Statement::CreateView(view) => vec![name("name", &view.name)],
        Statement::CreateIndex(index) => index.name.iter().map(|n| name("name", n)).chain([table_leaf(&index.table_name, line)]).collect(),
        Statement::CreateFunction(function) => vec![name("name", &function.name)],
        Statement::CreateTrigger(trigger) => vec![name("name", &trigger.name), table_leaf(&trigger.table_name, line)],
        Statement::CreateSequence { name: n, .. } | Statement::CreateType { name: n, .. } => vec![name("name", n)],
        Statement::Drop { names, .. } => names.iter().map(|n| name("name", n)).collect(),
        Statement::Truncate(truncate) => truncate.table_names.iter().map(|t| Item::Wrap("target", vec![table_leaf(&t.name, line)])).collect(),
        _ => Vec::new(),
    }
}

/// A table named by an ObjectName alone - an INSERT's, a TRUNCATE's, an index's.
fn table_leaf(name: &ObjectName, line: i64) -> Item<'_> {
    let parts = parts(name);
    let line = parts.first().map_or(line, |i| line_of(i, line));
    let mut items = Vec::new();
    if parts.len() >= 3 {
        items.push(Item::Leaf("database", parts[parts.len() - 3].value.clone(), line));
    }
    if parts.len() >= 2 {
        items.push(Item::Leaf("schema", parts[parts.len() - 2].value.clone(), line));
    }
    let base = parts.last().map_or(String::new(), |i| i.value.clone());
    Item::Wrap("table", std::iter::once(Item::Leaf("=", base, line)).chain(items).collect())
}

/// One item made into its node (or none - a transparent one answers its parent), and what hangs off it.
fn expand<'a>(item: Item<'a>, parent: usize, line: i64, add: &mut dyn FnMut(&str, String, i64, usize) -> usize) -> (usize, Vec<Item<'a>>) {
    match item {
        Item::Leaf(kind, value, line) => (add(kind, value, line, parent), Vec::new()),
        // `table` from a bare name: its first child is its own value.
        Item::Wrap("table", mut items) if matches!(items.first(), Some(Item::Leaf("=", ..))) => {
            let Item::Leaf(_, value, line) = items.remove(0) else { unreachable!() };
            (add("table", value, line, parent), items)
        }
        Item::Wrap(kind, items) => (add(kind, String::new(), line, parent), items),
        Item::Query(q) => {
            let mut items: Vec<Item> = q.with.iter().flat_map(|w| w.cte_tables.iter().map(Item::Cte)).collect();
            items.push(Item::Body(&q.body, Some(q)));
            (parent, items)
        }
        Item::Cte(cte) => (add("cte", cte.alias.name.value.clone(), line_of(&cte.alias.name, line), parent), vec![Item::Query(&cte.query)]),
        Item::Body(body, query) => {
            let (at, mut items) = match body {
                SetExpr::Select(select) => (add("query", String::new(), line, parent), select_items(select)),
                SetExpr::SetOperation { left, op, set_quantifier, right } => {
                    let kind = match op { SetOperator::Union => "union", SetOperator::Except | SetOperator::Minus => "except", SetOperator::Intersect => "intersect" };
                    let all = matches!(set_quantifier, SetQuantifier::All | SetQuantifier::AllByName);
                    (add(kind, if all { "all" } else { "" }.into(), line, parent), vec![Item::Body(left, None), Item::Body(right, None)])
                }
                SetExpr::Query(inner) => (parent, vec![Item::Query(inner)]),
                SetExpr::Values(values) => {
                    let rows = values.rows.iter().map(|row| Item::Wrap("row", row.content.iter().map(Item::Expr).collect())).collect();
                    (add("values", String::new(), line, parent), rows)
                }
                _ => (parent, Vec::new()),
            };
            if let Some(q) = query {
                if let Some(order) = &q.order_by
                    && let OrderByKind::Expressions(sorts) = &order.kind
                {
                    items.push(Item::Wrap("orderby", sorts.iter().map(Item::Sort).collect()));
                }
                match &q.limit_clause {
                    Some(LimitClause::LimitOffset { limit, offset, .. }) => {
                        items.extend(limit.iter().map(|l| Item::Wrap("limit", vec![Item::Expr(l)])));
                        items.extend(offset.iter().map(|o| Item::Wrap("offset", vec![Item::Expr(&o.value)])));
                    }
                    Some(LimitClause::OffsetCommaLimit { offset, limit }) => {
                        items.push(Item::Wrap("limit", vec![Item::Expr(limit)]));
                        items.push(Item::Wrap("offset", vec![Item::Expr(offset)]));
                    }
                    None => {}
                }
            }
            (at, items)
        }
        Item::Joined(table, upto) => {
            if upto == 0 {
                return (parent, vec![Item::Factor(&table.relation)]);
            }
            let join = &table.joins[upto - 1];
            let (kind, constraint) = join_kind(&join.join_operator);
            let mut items = vec![Item::Joined(table, upto - 1), Item::Factor(&join.relation)];
            if let Some(JoinConstraint::On(on)) = constraint {
                items.push(Item::Wrap("on", vec![Item::Expr(on)]));
            }
            (add("join", kind.into(), line, parent), items)
        }
        Item::Factor(factor) => factor_node(factor, parent, line, add),
        Item::Select(select) => {
            let at = add("item", String::new(), line, parent);
            let items = match select {
                SelectItem::UnnamedExpr(e) => vec![Item::Expr(e)],
                SelectItem::ExprWithAlias { expr, alias } => vec![Item::Leaf("alias", alias.value.clone(), line_of(alias, line)), Item::Expr(expr)],
                SelectItem::ExprWithAliases { expr, .. } => vec![Item::Expr(expr)],
                SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::ObjectName(n), _) => vec![Item::Leaf("column", format!("{}.*", dotted(n)), line)],
                SelectItem::QualifiedWildcard(..) | SelectItem::Wildcard(_) => vec![Item::Leaf("column", "*".into(), line)],
            };
            (at, items)
        }
        Item::Sort(sort) => {
            let value = match sort.options.sort { Some(OrderBySort::Asc) => "asc", Some(OrderBySort::Desc) => "desc", _ => "" };
            (add("sort", value.into(), line, parent), vec![Item::Expr(&sort.expr)])
        }
        Item::Set(assignment) => {
            let at = add("set", String::new(), line, parent);
            let mut items: Vec<Item> = match &assignment.target {
                AssignmentTarget::ColumnName(n) => vec![Item::Leaf("column", dotted(n), line)],
                AssignmentTarget::Tuple(names) => names.iter().map(|n| Item::Leaf("column", dotted(n), line)).collect(),
            };
            items.push(Item::Expr(&assignment.value));
            (at, items)
        }
        Item::Merge(clause) => {
            let kind = match clause.clause_kind { MergeClauseKind::Matched => "matched", MergeClauseKind::NotMatchedBySource => "not matched by source", _ => "not matched" };
            let at = add("action", kind.into(), line, parent);
            let action = match &clause.action {
                MergeAction::Insert(insert) => {
                    let mut items: Vec<Item> = insert.columns.iter().map(|c| Item::Leaf("column", dotted(c), line)).collect();
                    if let MergeInsertKind::Values(values) = &insert.kind {
                        items.push(Item::Wrap("values", values.rows.iter().map(|r| Item::Wrap("row", r.content.iter().map(Item::Expr).collect())).collect()));
                    }
                    Item::Wrap("insert", items)
                }
                MergeAction::Update(update) => Item::Wrap("update", match &update.kind {
                    MergeUpdateKind::Set(assignments) => assignments.iter().map(Item::Set).collect(),
                    MergeUpdateKind::Wildcard => Vec::new(),
                }),
                MergeAction::Delete { .. } => Item::Wrap("delete", Vec::new()),
                MergeAction::DoNothing { .. } => Item::Wrap("nothing", Vec::new()),
            };
            (at, clause.predicate.iter().map(|p| Item::Wrap("where", vec![Item::Expr(p)])).chain([action]).collect())
        }
        Item::Function(function) => {
            let at = add("func", dotted(&function.name).to_lowercase(), parts(&function.name).first().map_or(line, |i| line_of(i, line)), parent);
            let mut items = Vec::new();
            match &function.args {
                FunctionArguments::List(list) => {
                    for arg in &list.args {
                        let (FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } | FunctionArg::Unnamed(arg)) = arg;
                        items.push(match arg {
                            FunctionArgExpr::Expr(e) => Item::Expr(e),
                            FunctionArgExpr::QualifiedWildcard(n) => Item::Leaf("column", format!("{}.*", dotted(n)), line),
                            _ => Item::Leaf("column", "*".into(), line),
                        });
                    }
                }
                FunctionArguments::Subquery(q) => items.push(Item::Wrap("subquery", vec![Item::Query(q)])),
                FunctionArguments::None => {}
            }
            if let Some(WindowType::WindowSpec(spec)) = &function.over {
                let mut over: Vec<Item> = spec.partition_by.iter().map(|p| Item::Wrap("partition", vec![Item::Expr(p)])).collect();
                if !spec.order_by.is_empty() {
                    over.push(Item::Wrap("orderby", spec.order_by.iter().map(Item::Sort).collect()));
                }
                items.push(Item::Wrap("over", over));
            }
            (at, items)
        }
        Item::Expr(e) => super::exprs::node(e, parent, line, add),
    }
}

/// What a SELECT body holds, in the order T-SQL's tree has it.
fn select_items(select: &Select) -> Vec<Item<'_>> {
    let mut items = Vec::new();
    if matches!(select.distinct, Some(Distinct::Distinct | Distinct::On(_))) {
        items.push(Item::Wrap("distinct", Vec::new()));
    }
    if let Some(TopQuantity::Expr(e)) = select.top.as_ref().and_then(|t| t.quantity.as_ref()) {
        items.push(Item::Wrap("top", vec![Item::Expr(e)]));
    }
    items.extend(select.projection.iter().map(Item::Select));
    if !select.from.is_empty() {
        items.push(Item::Wrap("from", select.from.iter().map(|t| Item::Joined(t, t.joins.len())).collect()));
    }
    if let Some(selection) = &select.selection {
        items.push(Item::Wrap("where", vec![Item::Expr(selection)]));
    }
    if let GroupByExpr::Expressions(group, _) = &select.group_by
        && !group.is_empty()
    {
        items.push(Item::Wrap("groupby", group.iter().map(Item::Expr).collect()));
    }
    if let Some(having) = &select.having {
        items.push(Item::Wrap("having", vec![Item::Expr(having)]));
    }
    items
}

fn join_kind(op: &JoinOperator) -> (&'static str, Option<&JoinConstraint>) {
    match op {
        JoinOperator::Join(c) | JoinOperator::Inner(c) | JoinOperator::StraightJoin(c) => ("inner", Some(c)),
        JoinOperator::Left(c) | JoinOperator::LeftOuter(c) | JoinOperator::LeftSemi(c) | JoinOperator::LeftAnti(c) => ("left", Some(c)),
        JoinOperator::Right(c) | JoinOperator::RightOuter(c) | JoinOperator::RightSemi(c) | JoinOperator::RightAnti(c) => ("right", Some(c)),
        JoinOperator::FullOuter(c) => ("full", Some(c)),
        JoinOperator::CrossJoin(c) => ("cross", Some(c)),
        JoinOperator::CrossApply => ("cross apply", None),
        JoinOperator::OuterApply => ("outer apply", None),
        JoinOperator::Semi(c) | JoinOperator::Anti(c) => ("inner", Some(c)),
        JoinOperator::AsOf { constraint, .. } => ("inner", Some(constraint)),
        _ => ("inner", None),
    }
}

fn factor_node<'a>(factor: &'a TableFactor, parent: usize, line: i64, add: &mut dyn FnMut(&str, String, i64, usize) -> usize) -> (usize, Vec<Item<'a>>) {
    let alias = |alias: &'a Option<sqlparser::ast::TableAlias>| alias.iter().map(|a| Item::Leaf("alias", a.name.value.clone(), line_of(&a.name, line))).collect::<Vec<_>>();
    match factor {
        TableFactor::Table { name, alias: a, .. } => {
            let Item::Wrap(_, mut items) = table_leaf(name, line) else { return (parent, Vec::new()) };
            let Item::Leaf(_, value, at_line) = items.remove(0) else { return (parent, Vec::new()) };
            items.extend(alias(a));
            (add("table", value, at_line, parent), items)
        }
        TableFactor::Derived { subquery, alias: a, .. } => {
            let mut items = alias(a);
            items.push(Item::Query(subquery));
            (add("subquery", String::new(), line, parent), items)
        }
        TableFactor::NestedJoin { table_with_joins, .. } => (parent, vec![Item::Joined(table_with_joins, table_with_joins.joins.len())]),
        TableFactor::Function { name, alias: a, .. } => (add("func", dotted(name).to_lowercase(), line, parent), alias(a)),
        _ => (parent, Vec::new()),
    }
}

/// A literal's text, as the T-SQL half spells one: the value without its quotes.
pub fn literal(value: &Value) -> Option<(&'static str, String)> {
    Some(match value {
        Value::Null => ("null", String::new()),
        Value::Placeholder(p) => ("param", p.clone()),
        Value::Number(n, _) => ("literal", n.clone()),
        Value::Boolean(b) => ("literal", b.to_string()),
        Value::SingleQuotedString(s) | Value::EscapedStringLiteral(s) | Value::NationalStringLiteral(s) | Value::DoubleQuotedString(s)
        | Value::HexStringLiteral(s) | Value::UnicodeStringLiteral(s) => ("literal", s.clone()),
        Value::DollarQuotedString(d) => ("literal", d.value.clone()),
        _ => ("literal", String::new()),
    })
}
