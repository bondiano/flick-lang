//! Scalar replacement of temporary immutable records and tuples: a local
//! record whose every use reads a field, or a local tuple whose every use
//! destructures it, becomes one local per part. Its identity is never
//! observed, so no object needs to exist.

use std::collections::HashMap;

use super::types::filter_binder_types;
use crate::ir::{Expr, ExprKind, Pat, Stmt};
use flick_ir::{IrType, TypeRef};

/// Splits the records and tuples bound in `stmts` that only feed reads of
/// their parts; whether anything changed.
pub fn split(stmts: &mut Vec<Stmt>, result: &mut Expr) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let parts = match &stmts[i] {
            Stmt::Let {
                pat: Pat::Bind(name),
                value,
                ..
            } => match &value.kind {
                ExprKind::Record(fields)
                    if only(stmts, i, result, name, &|e| field_of(e, name)) =>
                {
                    Some((name.clone(), fields.clone()))
                }
                ExprKind::Tuple(items) if only(stmts, i, result, name, &|_| false) => Some((
                    name.clone(),
                    items
                        .iter()
                        .enumerate()
                        .map(|(k, e)| (k.to_string(), e.clone()))
                        .collect(),
                )),
                _ => None,
            },
            _ => None,
        };
        let Some((name, parts)) = parts else {
            i += 1;
            continue;
        };
        let local = |part: &str| format!("{name}${part}");
        let part_types: HashMap<String, Option<TypeRef>> = parts
            .iter()
            .map(|(part, value)| (part.clone(), value.ty.clone()))
            .collect();
        let lets: Vec<Stmt> = parts
            .iter()
            .map(|(part, value)| {
                let n = local(part);
                let bt = value
                    .ty
                    .clone()
                    .map(|t| vec![(n.clone(), t)])
                    .unwrap_or_default();
                Stmt::let_typed(Pat::Bind(n), bt, value.clone())
            })
            .collect();
        let count = lets.len();
        let rest: Vec<Stmt> = stmts
            .drain(i + 1..)
            .flat_map(|stmt| replace_stmt(stmt, &name, &local, &part_types))
            .collect();
        stmts.splice(i..=i, lets);
        stmts.extend(rest);
        replace(result, &name, &local);
        i += count;
        changed = true;
    }
    changed
}

/// Whether every read of `name` after statement `i` is a part of it: a
/// field read `is_part` accepts, or `let (a, b) = name`.
fn only(
    stmts: &[Stmt],
    i: usize,
    result: &Expr,
    name: &str,
    is_part: &dyn Fn(&Expr) -> bool,
) -> bool {
    let mut total = result.uses(name);
    let mut parts = parts(result, is_part);
    for stmt in &stmts[i + 1..] {
        match stmt {
            Stmt::Let {
                pat: Pat::Tuple(_),
                value:
                    Expr {
                        kind: ExprKind::Local(l),
                        ..
                    },
                ..
            } if l == name => {
                total += 1;
                parts += 1;
            }
            Stmt::Let { value: v, .. }
            | Stmt::Var { value: v, .. }
            | Stmt::Assign(_, v)
            | Stmt::Expr(v)
            | Stmt::Fun { value: v, .. } => {
                total += v.uses(name);
                parts += self::parts(v, is_part);
            }
        }
    }
    total > 0 && total == parts
}

fn parts(e: &Expr, is_part: &dyn Fn(&Expr) -> bool) -> usize {
    if is_part(e) {
        return 1;
    }
    let mut n = 0;
    e.each_child(&mut |c| n += parts(c, is_part));
    n
}

fn field_of(e: &Expr, name: &str) -> bool {
    matches!(&e.kind, ExprKind::Field(inner, _) if matches!(&inner.kind, ExprKind::Local(l) if l == name))
}

/// `stmt` with the parts of `name` read from their locals; `let (a, b) = t`
/// of a split tuple becomes one `let` per part.
fn replace_stmt(
    stmt: Stmt,
    name: &str,
    local: &dyn Fn(&str) -> String,
    part_types: &HashMap<String, Option<TypeRef>>,
) -> Vec<Stmt> {
    match stmt {
        Stmt::Let {
            pat: Pat::Tuple(pats),
            binder_types,
            value:
                Expr {
                    kind: ExprKind::Local(l),
                    span,
                    ty: tup_ty,
                },
        } if l == name => {
            let elem_tys = match tup_ty.as_deref() {
                Some(IrType::Tuple(ts)) if ts.len() == pats.len() => Some(ts),
                _ => None,
            };
            pats.into_iter()
                .enumerate()
                .map(|(k, p)| {
                    let key = k.to_string();
                    let bt = filter_binder_types(&p, &binder_types);
                    let ty = elem_tys
                        .and_then(|ts| ts.get(k).cloned())
                        .or_else(|| part_types.get(&key).cloned().flatten());
                    Stmt::let_typed(p, bt, Expr::with_ty(ExprKind::Local(local(&key)), span, ty))
                })
                .collect()
        }
        Stmt::Let {
            pat: p,
            binder_types,
            value: mut v,
        } => {
            replace(&mut v, name, local);
            vec![Stmt::Let {
                pat: p,
                binder_types,
                value: v,
            }]
        }
        Stmt::Expr(mut v) => {
            replace(&mut v, name, local);
            vec![Stmt::Expr(v)]
        }
        Stmt::Var {
            name: x,
            ty,
            value: mut v,
        } => {
            replace(&mut v, name, local);
            vec![Stmt::Var {
                name: x,
                ty,
                value: v,
            }]
        }
        Stmt::Assign(x, mut v) => {
            replace(&mut v, name, local);
            vec![Stmt::Assign(x, v)]
        }
        Stmt::Fun {
            name: f,
            rec,
            type_params,
            dicts,
            dict_constraints,
            ty,
            mut value,
        } => {
            replace(&mut value, name, local);
            vec![Stmt::Fun {
                name: f,
                rec,
                type_params,
                dicts,
                dict_constraints,
                ty,
                value,
            }]
        }
    }
}

fn replace(e: &mut Expr, name: &str, local: &dyn Fn(&str) -> String) {
    if let ExprKind::Field(inner, label) = &e.kind
        && matches!(&inner.kind, ExprKind::Local(l) if l == name)
    {
        e.kind = ExprKind::Local(local(label));
        return;
    }
    e.each_child_mut(&mut |c| replace(c, name, local));
}
