//! Known shapes taken apart: `List.length` and `List.get` of a list
//! literal, through the list primitives a target reviewed, are the count,
//! or `Some` of the element or `None`; a field read of a record literal is
//! that field, and one of a block is read at its end. The elements or
//! fields that are not data are named first, in order, so each still runs
//! once, where it was written, selected or not. A tuple has no field read:
//! only a pattern takes it apart, as `devirt::known_lets` does.

use std::mem;

use super::eval::{NativePrimitive, PrimitiveEval};
use super::known::name_parts;
use crate::ir::{Constructor, Expr, ExprKind, Lit, Pat, Stmt};
use flick_ir::{IrType, Stage, TypeRef};

/// Folds the known lengths, lookups and field reads in `e`; whether
/// anything changed.
pub fn shapes(
    e: &mut Expr,
    fresh: &mut usize,
    primitives: &dyn PrimitiveEval,
    preserve_types: bool,
) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= shapes(c, fresh, primitives, preserve_types));
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        return changed | bound(stmts, result, fresh, primitives);
    }
    if field(e, fresh) {
        return true;
    }
    match folded(e, fresh, primitives, preserve_types) {
        Some(folded) => *e = folded,
        None => return changed,
    }
    true
}

/// `r.l` of a record literal `r`, or of a block, which [`field`] folds.
fn read(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Field(r, label) => match &r.kind {
            ExprKind::Block(..) => true,
            ExprKind::Record(fields) => fields.iter().any(|(l, _)| l == label),
            _ => false,
        },
        _ => false,
    }
}

/// `{ …; l = v; … }.l` is `v`, after the fields that are not data, `v`
/// among them, are named in order; `{ stmts; r }.l` is `{ stmts; r.l }`,
/// as the read comes after the block either way. Whether `e` was one.
fn field(e: &mut Expr, fresh: &mut usize) -> bool {
    if !read(e) {
        return false;
    }
    let span = e.span;
    let ExprKind::Field(record, label) = mem::replace(&mut e.kind, ExprKind::Lit(Lit::Unit)) else {
        unreachable!("a field read, as tested")
    };
    let (parts, value) = if let ExprKind::Block(stmts, end) = record.kind {
        let mut read = Expr::with_ty(ExprKind::Field(end, label), span, e.ty.clone());
        field(&mut read, fresh);
        (stmts, read)
    } else {
        selected(*record, &label, fresh)
    };
    *e = if parts.is_empty() {
        value
    } else {
        Expr::with_ty(ExprKind::Block(parts, Box::new(value)), span, e.ty.take())
    };
    true
}

/// The field `label` of the record literal `record`, after the `let`s
/// that name its parts that are not data.
fn selected(mut record: Expr, label: &str, fresh: &mut usize) -> (Vec<Stmt>, Expr) {
    let mut parts = Vec::new();
    name_parts(&mut record, fresh, &mut parts);
    let ExprKind::Record(fields) = record.kind else {
        unreachable!("a record, as tested")
    };
    let found = fields.into_iter().find(|(name, _)| name == label);
    let value = found.expect("a field of the record, as tested").1;
    // named last, the field runs last either way: it stays the result
    match (parts.pop(), &value.kind) {
        (
            Some(Stmt::Let {
                pat, value: part, ..
            }),
            ExprKind::Local(local),
        ) if matches!(&pat, Pat::Bind(name) if name == local) => (parts, part),
        (last, _) => {
            parts.extend(last);
            (parts, value)
        }
    }
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
