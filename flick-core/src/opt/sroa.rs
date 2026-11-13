//! Scalar replacement of temporary immutable records and tuples: a local
//! record whose every use reads a field, or a local tuple whose every use
//! destructures it, becomes one local per part. Its identity is never
//! observed, so no object needs to exist.

use crate::ir::{Expr, ExprKind, Pat, Stmt};

/// Splits the records and tuples bound in `stmts` that only feed reads of
/// their parts; whether anything changed.
pub fn split(stmts: &mut Vec<Stmt>, result: &mut Expr) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let parts = match &stmts[i] {
            Stmt::Let(Pat::Bind(name), value) => match &value.kind {
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
        let lets: Vec<Stmt> = parts
            .iter()
            .map(|(part, value)| Stmt::Let(Pat::Bind(local(part)), value.clone()))
            .collect();
        let count = lets.len();
        let rest: Vec<Stmt> = stmts
            .drain(i + 1..)
            .flat_map(|stmt| replace_stmt(stmt, &name, &local))
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
            Stmt::Let(
                Pat::Tuple(_),
                Expr {
                    kind: ExprKind::Local(l),
                    ..
                },
            ) if l == name => {
                total += 1;
                parts += 1;
            }
            Stmt::Let(_, v)
            | Stmt::Var(_, v)
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
fn replace_stmt(stmt: Stmt, name: &str, local: &dyn Fn(&str) -> String) -> Vec<Stmt> {
    match stmt {
        Stmt::Let(
            Pat::Tuple(pats),
            Expr {
                kind: ExprKind::Local(l),
                span,
                ..
            },
        ) if l == name => pats
            .into_iter()
            .enumerate()
            .map(|(k, p)| Stmt::Let(p, Expr::new(ExprKind::Local(local(&k.to_string())), span)))
            .collect(),
        Stmt::Let(p, mut v) => {
            replace(&mut v, name, local);
            vec![Stmt::Let(p, v)]
        }
        Stmt::Expr(mut v) => {
            replace(&mut v, name, local);
            vec![Stmt::Expr(v)]
        }
        Stmt::Var(x, mut v) => {
            replace(&mut v, name, local);
            vec![Stmt::Var(x, v)]
        }
        Stmt::Assign(x, mut v) => {
            replace(&mut v, name, local);
            vec![Stmt::Assign(x, v)]
        }
        Stmt::Fun {
            name: f,
            rec,
            dicts,
            mut value,
        } => {
            replace(&mut value, name, local);
            vec![Stmt::Fun {
                name: f,
                rec,
                dicts,
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
