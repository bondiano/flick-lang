//! `List.length` and `List.get` of a list literal, through the list
//! primitives a target reviewed: the count, or `Some` of the element or
//! `None`. The elements that are not data are named first, in order, so
//! each still runs once, where it was written, selected or not.

use super::eval::{NativePrimitive, PrimitiveEval};
use super::known::name_parts;
use crate::ir::{Constructor, Expr, ExprKind, Lit, Pat, Stmt};
use flick_ir::{IrType, Stage, TypeRef};

/// Folds the known lengths and lookups in `e`; whether anything changed.
pub fn lists(
    e: &mut Expr,
    fresh: &mut usize,
    primitives: &dyn PrimitiveEval,
    preserve_types: bool,
) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= lists(c, fresh, primitives, preserve_types));
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        return changed | bound(stmts, result, fresh, primitives);
    }
    match folded(e, fresh, primitives, preserve_types) {
        Some(folded) => *e = folded,
        None => return changed,
    }
    true
}

/// A full application of `List.length`, or of `List.get` at a literal
/// index: the index and the list it is given.
fn known_call<'e>(e: &'e Expr, primitives: &dyn PrimitiveEval) -> Option<(Option<i32>, &'e Expr)> {
    let (head, args) = e.pure_spine();
    let ExprKind::Global { module, name, .. } = &head.kind else {
        return None;
    };
    match (primitives.list_primitive(module, name)?, &args[..]) {
        (NativePrimitive::ListLength, [list]) => Some((None, *list)),
        (NativePrimitive::ListGet, [index, list]) => match index.kind {
            ExprKind::Lit(Lit::Int(k)) => Some((Some(k), *list)),
            _ => None,
        },
        _ => None,
    }
}

fn folded(
    e: &Expr,
    fresh: &mut usize,
    primitives: &dyn PrimitiveEval,
    preserve_types: bool,
) -> Option<Expr> {
    let (index, list) = known_call(e, primitives)?;
    if !matches!(list.kind, ExprKind::List(_)) {
        return None;
    }
    let ty = preserve_types.then(|| e.ty.clone()).flatten();
    // the element type, from the call's `Option a`, so `Some` is not `Option Int`
    let type_args: Vec<TypeRef> = match ty.as_deref() {
        Some(IrType::Named { args, .. }) if args.len() == 1 => args.clone(),
        _ if preserve_types && index.is_some() => return None,
        _ => Vec::new(),
    };
    let options = primitives.option_constructors();
    if index.is_some() && options.is_none() {
        return None;
    }
    let (mut list, mut parts) = (list.clone(), Vec::new());
    name_parts(&mut list, fresh, &mut parts);
    let ExprKind::List(items) = list.kind else {
        return None;
    };
    let span = e.span;
    let typed = |c: &Constructor, fields| Constructor {
        fields,
        type_args: type_args.clone(),
        ..c.clone()
    };
    let value = match index.zip(options) {
        None => Expr::with_ty(
            ExprKind::Lit(Lit::Int(items.len().try_into().ok()?)),
            span,
            ty.clone(),
        ),
        Some((k, (some, none))) => match usize::try_from(k)
            .ok()
            .and_then(|k| items.into_iter().nth(k))
        {
            Some(item) => {
                let kind = ExprKind::App {
                    func: Box::new(Expr::new(
                        ExprKind::Con(typed(some, type_args.clone())),
                        span,
                    )),
                    arg: Box::new(item),
                    stage: Stage::Pure,
                    arg_first: false,
                };
                Expr::with_ty(kind, span, ty.clone())
            }
            None => Expr::with_ty(ExprKind::Con(typed(none, Vec::new())), span, ty.clone()),
        },
    };
    if parts.is_empty() {
        return Some(value);
    }
    Some(Expr::with_ty(
        ExprKind::Block(parts, Box::new(value)),
        span,
        ty,
    ))
}

/// `let xs = [..]` right before a fold over `xs`, as an inlined `List.head`
/// gives: its elements that are not data are named first, so the list,
/// data now, may move to its use.
fn bound(
    stmts: &mut Vec<Stmt>,
    result: &Expr,
    fresh: &mut usize,
    primitives: &dyn PrimitiveEval,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let next = match stmts.get(i + 1) {
            Some(Stmt::Let { value: v, .. } | Stmt::Expr(v)) => Some(v),
            Some(_) => None,
            None => Some(result),
        };
        let mut parts = Vec::new();
        if let (Stmt::Let { pat, value, .. }, Some(next)) = (&stmts[i], next)
            && let (Pat::Bind(x), ExprKind::List(_)) = (pat, &value.kind)
            && folds(next, x, primitives)
            && let Stmt::Let { value, .. } = &mut stmts[i]
        {
            name_parts(value, fresh, &mut parts);
        }
        let n = parts.len();
        stmts.splice(i..i, parts);
        changed |= n > 0;
        i += n + 1;
    }
    changed
}

/// Whether `e` folds over the local `x`.
fn folds(e: &Expr, x: &str, primitives: &dyn PrimitiveEval) -> bool {
    let over_x = |(_, list): (_, &Expr)| matches!(&list.kind, ExprKind::Local(l) if l == x);
    if known_call(e, primitives).is_some_and(over_x) {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= folds(c, x, primitives));
    found
}
