//! A POSTGRESQL EXPRESSION AS NODES, in the kinds the T-SQL half's expressions are (`column`, `literal`, `param`,
//! `op`, `func`, `in`, `exists`, `case`, `when`, `subquery`). An operator is named by its symbol, a function by its
//! name lowercased; parentheses are transparent.

use super::tree::{dotted, literal, Item};
use sqlparser::ast::{BinaryOperator, CastKind, Expr, UnaryOperator};

fn binary(op: &BinaryOperator) -> String {
    match op {
        BinaryOperator::Plus => "+",
        BinaryOperator::Minus => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Modulo => "%",
        BinaryOperator::StringConcat => "||",
        BinaryOperator::Gt => ">",
        BinaryOperator::Lt => "<",
        BinaryOperator::GtEq => ">=",
        BinaryOperator::LtEq => "<=",
        BinaryOperator::Eq => "=",
        BinaryOperator::NotEq => "<>",
        BinaryOperator::And => "and",
        BinaryOperator::Or => "or",
        BinaryOperator::BitwiseOr => "|",
        BinaryOperator::BitwiseAnd => "&",
        BinaryOperator::BitwiseXor | BinaryOperator::PGBitwiseXor => "^",
        BinaryOperator::PGBitwiseShiftLeft => "<<",
        BinaryOperator::PGBitwiseShiftRight => ">>",
        BinaryOperator::Custom(op) => return op.to_lowercase(),
        _ => "op",
    }
    .into()
}

fn unary(op: &UnaryOperator) -> &'static str {
    match op {
        UnaryOperator::Not | UnaryOperator::BangNot => "not",
        UnaryOperator::Minus => "-",
        UnaryOperator::Plus => "+",
        UnaryOperator::BitwiseNot => "~",
        _ => "op",
    }
}

pub(super) fn node<'a>(e: &'a Expr, parent: usize, line: i64, add: &mut dyn FnMut(&str, String, i64, usize) -> usize) -> (usize, Vec<Item<'a>>) {
    let at_line = |ident: &sqlparser::ast::Ident| if ident.span.start.line > 0 { ident.span.start.line as i64 } else { line };
    let mut op = |value: &str, children: Vec<Item<'a>>| (add("op", value.into(), line, parent), children);
    match e {
        Expr::Identifier(ident) => (add("column", ident.value.clone(), at_line(ident), parent), Vec::new()),
        Expr::CompoundIdentifier(parts) => {
            let value = parts.iter().map(|p| p.value.as_str()).collect::<Vec<_>>().join(".");
            (add("column", value, parts.first().map_or(line, at_line), parent), Vec::new())
        }
        Expr::Wildcard(_) => (add("column", "*".into(), line, parent), Vec::new()),
        Expr::QualifiedWildcard(name, _) => (add("column", format!("{}.*", dotted(name)), line, parent), Vec::new()),
        Expr::Value(v) => {
            let (kind, value) = literal(&v.value).unwrap_or(("literal", String::new()));
            let line = if v.span.start.line > 0 { v.span.start.line as i64 } else { line };
            (add(kind, value, line, parent), Vec::new())
        }
        Expr::BinaryOp { left, op: o, right } => op(&binary(o), vec![Item::Expr(left), Item::Expr(right)]),
        Expr::UnaryOp { op: o, expr } => op(unary(o), vec![Item::Expr(expr)]),
        Expr::IsNull(x) => op("is null", vec![Item::Expr(x)]),
        Expr::IsNotNull(x) => op("is not null", vec![Item::Expr(x)]),
        Expr::IsTrue(x) => op("is true", vec![Item::Expr(x)]),
        Expr::IsFalse(x) => op("is false", vec![Item::Expr(x)]),
        Expr::IsNotTrue(x) => op("is not true", vec![Item::Expr(x)]),
        Expr::IsNotFalse(x) => op("is not false", vec![Item::Expr(x)]),
        Expr::IsDistinctFrom(a, b) => op("is distinct from", vec![Item::Expr(a), Item::Expr(b)]),
        Expr::IsNotDistinctFrom(a, b) => op("is not distinct from", vec![Item::Expr(a), Item::Expr(b)]),
        Expr::Between { expr, negated, low, high } => {
            op(if *negated { "not between" } else { "between" }, vec![Item::Expr(expr), Item::Expr(low), Item::Expr(high)])
        }
        Expr::Like { negated, expr, pattern, .. } => op(if *negated { "not like" } else { "like" }, vec![Item::Expr(expr), Item::Expr(pattern)]),
        Expr::ILike { negated, expr, pattern, .. } => op(if *negated { "not ilike" } else { "ilike" }, vec![Item::Expr(expr), Item::Expr(pattern)]),
        Expr::SimilarTo { negated, expr, pattern, .. } => {
            op(if *negated { "not similar to" } else { "similar to" }, vec![Item::Expr(expr), Item::Expr(pattern)])
        }
        Expr::AnyOp { left, compare_op, right, .. } => op(&format!("{} any", binary(compare_op)), vec![Item::Expr(left), Item::Expr(right)]),
        Expr::AllOp { left, compare_op, right } => op(&format!("{} all", binary(compare_op)), vec![Item::Expr(left), Item::Expr(right)]),
        Expr::InList { expr, list, negated } => {
            let mut items = vec![Item::Expr(expr)];
            items.extend(list.iter().map(Item::Expr));
            (add("in", if *negated { "not" } else { "" }.into(), line, parent), items)
        }
        Expr::InSubquery { expr, subquery, negated } => (
            add("in", if *negated { "not" } else { "" }.into(), line, parent),
            vec![Item::Expr(expr), Item::Wrap("subquery", vec![Item::Query(subquery)])],
        ),
        Expr::Exists { subquery, negated } => {
            let exists = Item::Wrap("subquery", vec![Item::Query(subquery)]);
            if *negated {
                op("not", vec![Item::Wrap("exists", vec![exists])])
            } else {
                (add("exists", String::new(), line, parent), vec![exists])
            }
        }
        Expr::Subquery(q) => (add("subquery", String::new(), line, parent), vec![Item::Query(q)]),
        Expr::Function(f) => (parent, vec![Item::Function(f)]),
        Expr::Cast { kind, expr, .. } => {
            let name = match kind { CastKind::TryCast | CastKind::SafeCast => "try_cast", _ => "cast" };
            (add("func", name.into(), line, parent), vec![Item::Expr(expr)])
        }
        Expr::Case { operand, conditions, else_result, .. } => {
            let mut items: Vec<Item> = operand.iter().map(|o| Item::Expr(o)).collect();
            items.extend(conditions.iter().map(|w| Item::Wrap("when", vec![Item::Expr(&w.condition), Item::Expr(&w.result)])));
            items.extend(else_result.iter().map(|x| Item::Expr(x)));
            (add("case", String::new(), line, parent), items)
        }
        Expr::Interval(interval) => (add("func", "interval".into(), line, parent), vec![Item::Expr(&interval.value)]),
        Expr::Extract { expr, .. } => (add("func", "extract".into(), line, parent), vec![Item::Expr(expr)]),
        Expr::Substring { expr, substring_from, substring_for, .. } => {
            let mut items = vec![Item::Expr(expr)];
            items.extend(substring_from.iter().chain(substring_for.iter()).map(|x| Item::Expr(x)));
            (add("func", "substring".into(), line, parent), items)
        }
        Expr::AtTimeZone { timestamp, time_zone } => op("at time zone", vec![Item::Expr(timestamp), Item::Expr(time_zone)]),
        Expr::Collate { expr, .. } | Expr::Nested(expr) => (parent, vec![Item::Expr(expr)]),
        Expr::Tuple(items) => (parent, items.iter().map(Item::Expr).collect()),
        _ => (parent, Vec::new()),
    }
}
