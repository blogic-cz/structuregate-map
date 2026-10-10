//! THE TYPE OF A NAME INSIDE ONE FUNCTION, as far as the syntax says it: what lets `s.Load(` bind to `Store.Load`
//! (`rows.rs`) without a type checker. A name is typed by the receiver, a parameter, `x := F(...)` or `x := T{...}`
//! (with or without a `&`, `T` or `alias.T`), or `var x T`. A call form types x only when `F` declares a single result
//! that is a type of its OWN folder (`Index::results`).
//!
//! SOURCES: the receiver and the parameters come off the syntax tree; the locals come off the function's TOKENS
//! (`IDENT :=` and `var IDENT`), because that finds one inside a nested block or a literal as well, as `rows.rs` finds
//! a call. SCOPE IS THE WHOLE FUNCTION, not the block: a name typed twice to different types is unknown there, and so
//! is bound to nothing, rather than guessed from whichever came first.

use super::receiver;
use super::rows::Index;
use gosyn::ast::{Expression, FuncDecl};
use gosyn::token::{Keyword, LitKind, Operator, Token};
use gosyn::LexicalToken;
use std::collections::HashMap;

/// A name -> `(folder, type name)` of the type it holds.
pub(crate) type Typed = HashMap<String, (String, String)>;

/// The type an expression written as a type names: `T`, `*T`, `alias.T`, `*alias.T`.
fn type_of(typ: &Expression, own: &str, aliases: &HashMap<String, String>) -> Option<(String, String)> {
    match typ {
        Expression::Ident(id) => Some((own.to_string(), id.name.clone())),
        Expression::Star(star) => type_of(&star.right, own, aliases),
        Expression::TypePointer(pointer) => type_of(&pointer.typ, own, aliases),
        Expression::Paren(paren) => type_of(&paren.expr, own, aliases),
        Expression::Selector(sel) => match &*sel.x {
            Expression::Ident(alias) => aliases.get(&alias.name).map(|folder| (folder.clone(), sel.sel.name.clone())),
            _ => None,
        },
        _ => None,
    }
}

/// The single result type of a top-level func, when it is a plain `T` or `*T` - a type of the func's own folder.
pub(crate) fn result_of(func: &FuncDecl) -> Option<String> {
    let [field] = func.typ.result.list.as_slice() else { return None };
    if field.name.len() > 1 {
        return None;
    }
    match type_of(&field.typ, "", &HashMap::new()) {
        Some((_, name)) => Some(name),
        None => None,
    }
}

pub(crate) fn typed_names(func: &FuncDecl, tokens: &[LexicalToken], span: (usize, usize), own: &str,
                          aliases: &HashMap<String, String>, index: &Index) -> Typed {
    // None = typed to two types: unknown.
    let mut seen: HashMap<String, Option<(String, String)>> = HashMap::new();
    let mut put = |name: &str, typ: (String, String)| {
        if name == "_" {
            return;
        }
        let slot = seen.entry(name.to_string()).or_insert_with(|| Some(typ.clone()));
        if slot.as_ref() != Some(&typ) {
            *slot = None;
        }
    };
    if let Some(field) = func.recv.as_ref().and_then(|r| r.list.first()) {
        if let (Some(id), Some(cls)) = (field.name.first(), receiver(&field.typ)) {
            put(&id.name, (own.to_string(), cls));
        }
    }
    for field in &func.typ.params.list {
        if let Some(typ) = type_of(&field.typ, own, aliases) {
            field.name.iter().for_each(|id| put(&id.name, typ.clone()));
        }
    }
    let ident = |at: usize| match tokens.get(at).map(|t| &t.token) {
        Some(Token::Literal(LitKind::Ident, name)) => Some(name.as_str()),
        _ => None,
    };
    let is_op = |at: usize, op: Operator| matches!(tokens.get(at).map(|t| &t.token), Some(Token::Operator(o)) if *o == op);
    // `a` or `a.b` at `at`: the qualifier (if any), the name, and the token after it.
    let chain = |at: usize| -> Option<(Option<&str>, &str, usize)> {
        let first = ident(at)?;
        if is_op(at + 1, Operator::Dot) {
            Some((Some(first), ident(at + 2)?, at + 3))
        } else {
            Some((None, first, at + 1))
        }
    };
    let folder_of = |qual: Option<&str>| match qual {
        None => Some(own.to_string()),
        Some(alias) => aliases.get(alias).cloned(),
    };
    let from = tokens.partition_point(|t| t.start < span.0);
    for k in from..tokens.len() {
        if tokens[k].start > span.1 {
            break;
        }
        if matches!(tokens[k].token, Token::Keyword(Keyword::Var)) {
            let Some(name) = ident(k + 1) else { continue };
            let at = k + 2 + usize::from(is_op(k + 2, Operator::Star));
            if let Some((qual, typ, _)) = chain(at) {
                if let Some(folder) = folder_of(qual) {
                    put(name, (folder, typ.to_string()));
                }
            }
            continue;
        }
        let Some(name) = ident(k) else { continue };
        if !is_op(k + 1, Operator::Define) || (k > 0 && (is_op(k - 1, Operator::Dot) || is_op(k - 1, Operator::Comma))) {
            continue;
        }
        let amp = is_op(k + 2, Operator::And);
        let Some((qual, head, after)) = chain(k + 2 + usize::from(amp)) else { continue };
        let Some(folder) = folder_of(qual) else { continue };
        if is_op(after, Operator::BraceLeft) {
            put(name, (folder, head.to_string()));
        } else if !amp && is_op(after, Operator::ParenLeft) {
            if let Some(typ) = index.results.get(&(folder.clone(), head.to_string())) {
                put(name, (folder, typ.clone()));
            }
        }
    }
    seen.into_iter().filter_map(|(name, typ)| typ.map(|t| (name, t))).collect()
}
