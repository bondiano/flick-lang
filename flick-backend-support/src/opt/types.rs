//! Type substitution and template matching helpers for optimization passes.

use std::collections::HashMap;

#[cfg(feature = "typed-ir")]
use flick_ir::{Arm, Constructor, ExprKind, Stmt, TypeInterner};
use flick_ir::{Expr, IrType, Pat, TypeRef};

/// Substitutes free type variables in `e` according to `subst`.
#[cfg(not(feature = "typed-ir"))]
pub(super) fn subst_types(_e: &mut Expr, _subst: &[(&str, &TypeRef)]) {}

/// Substitutes free type variables in `e` according to `subst`.
#[cfg(feature = "typed-ir")]
pub(super) fn subst_types(e: &mut Expr, subst: &[(&str, &TypeRef)]) {
    if subst.is_empty() {
        return;
    }
    let mut interner = TypeInterner::new();
    subst_expr(e, subst, &mut interner);
}

#[cfg(feature = "typed-ir")]
fn subst_expr(e: &mut Expr, subst: &[(&str, &TypeRef)], interner: &mut TypeInterner) {
    if let Some(ty) = &mut e.ty {
        *ty = ty.substitute(subst, interner);
    }
    match &mut e.kind {
        ExprKind::Global { type_args, .. } | ExprKind::Method { type_args, .. } => {
            for arg in type_args {
                *arg = arg.substitute(subst, interner);
            }
        }
        ExprKind::Con(con) => subst_con(con, subst, interner),
        ExprKind::Lambda { params, body } => {
            for p in params {
                if let Some(ty) = &mut p.ty {
                    *ty = ty.substitute(subst, interner);
                }
                for (_, ty) in &mut p.binder_types {
                    *ty = ty.substitute(subst, interner);
                }
                subst_pat(&mut p.pat, subst, interner);
            }
            subst_expr(body, subst, interner);
        }
        ExprKind::Block(stmts, result) => {
            for stmt in stmts {
                subst_stmt(stmt, subst, interner);
            }
            subst_expr(result, subst, interner);
        }
        ExprKind::Match(scrutinee, arms) => {
            subst_expr(scrutinee, subst, interner);
            for arm in arms {
                subst_arm(arm, subst, interner);
            }
        }
        ExprKind::Handle(h) => {
            for handled in &mut h.effects {
                for op in &mut handled.operations {
                    subst_expr(&mut op.function, subst, interner);
                }
            }
            subst_expr(&mut h.body, subst, interner);
            if let Some(ret) = &mut h.ret {
                subst_expr(ret, subst, interner);
            }
            for arm in &mut h.errors {
                subst_arm(arm, subst, interner);
            }
        }
        ExprKind::Each {
            stop,
            pat,
            binder_types,
            source,
            body,
        } => {
            if let Some(stop) = stop {
                subst_expr(stop, subst, interner);
            }
            subst_pat(pat, subst, interner);
            for (_, ty) in binder_types {
                *ty = ty.substitute(subst, interner);
            }
            subst_expr(source, subst, interner);
            subst_expr(body, subst, interner);
        }
        _ => e.each_child_mut(&mut |c| subst_expr(c, subst, interner)),
    }
}

#[cfg(feature = "typed-ir")]
fn subst_stmt(stmt: &mut Stmt, subst: &[(&str, &TypeRef)], interner: &mut TypeInterner) {
    match stmt {
        Stmt::Let {
            pat,
            binder_types,
            value,
        } => {
            subst_pat(pat, subst, interner);
            for (_, ty) in binder_types {
                *ty = ty.substitute(subst, interner);
            }
            subst_expr(value, subst, interner);
        }
        Stmt::Var { ty, value, .. } => {
            if let Some(ty) = ty {
                *ty = ty.substitute(subst, interner);
            }
            subst_expr(value, subst, interner);
        }
        Stmt::Assign(_, value) | Stmt::Expr(value) => subst_expr(value, subst, interner),
        Stmt::Fun {
            type_params,
            dict_constraints,
            ty,
            value,
            ..
        } => {
            let filtered: Vec<(&str, &TypeRef)> = if type_params.is_empty() {
                subst.to_vec()
            } else {
                subst
                    .iter()
                    .copied()
                    .filter(|&(k, _)| !type_params.iter().any(|p| p == k))
                    .collect()
            };
            if filtered.is_empty() {
                return;
            }
            for c in dict_constraints {
                for arg in &mut c.args {
                    *arg = arg.substitute(&filtered, interner);
                }
            }
            if let Some(ty) = ty {
                *ty = ty.substitute(&filtered, interner);
            }
            subst_expr(value, &filtered, interner);
        }
    }
}

#[cfg(feature = "typed-ir")]
fn subst_arm(arm: &mut Arm, subst: &[(&str, &TypeRef)], interner: &mut TypeInterner) {
    subst_pat(&mut arm.pat, subst, interner);
    for (_, ty) in &mut arm.binder_types {
        *ty = ty.substitute(subst, interner);
    }
    if let Some(guard) = &mut arm.guard {
        subst_expr(guard, subst, interner);
    }
    subst_expr(&mut arm.body, subst, interner);
}

#[cfg(feature = "typed-ir")]
fn subst_con(con: &mut Constructor, subst: &[(&str, &TypeRef)], interner: &mut TypeInterner) {
    for f in &mut con.fields {
        *f = f.substitute(subst, interner);
    }
    for a in &mut con.type_args {
        *a = a.substitute(subst, interner);
    }
}

#[cfg(feature = "typed-ir")]
fn subst_pat(pat: &mut Pat, subst: &[(&str, &TypeRef)], interner: &mut TypeInterner) {
    match pat {
        Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => {}
        Pat::Tuple(items) | Pat::Or(items) => {
            for p in items {
                subst_pat(p, subst, interner);
            }
        }
        Pat::List(items, rest) => {
            for p in items.iter_mut().chain(rest.as_deref_mut()) {
                subst_pat(p, subst, interner);
            }
        }
        Pat::Record(fields) => {
            for (_, p) in fields {
                subst_pat(p, subst, interner);
            }
        }
        Pat::Con(con, items) => {
            subst_con(con, subst, interner);
            for p in items {
                subst_pat(p, subst, interner);
            }
        }
    }
}

/// Matches free type variables in `template` against `concrete` into `out`.
#[cfg(not(feature = "typed-ir"))]
pub(super) fn match_type(
    _template: &IrType,
    _concrete: &TypeRef,
    _out: &mut HashMap<String, TypeRef>,
) {
}

/// Matches free type variables in `template` against `concrete` into `out`.
#[cfg(feature = "typed-ir")]
#[allow(clippy::too_many_lines)]
pub(super) fn match_type(
    template: &IrType,
    concrete: &TypeRef,
    out: &mut HashMap<String, TypeRef>,
) {
    match (template, &**concrete) {
        (IrType::Var(v), _) => {
            out.entry(v.clone()).or_insert_with(|| concrete.clone());
        }
        (
            IrType::Named {
                module: m1,
                name: n1,
                args: a1,
            },
            IrType::Named {
                module: m2,
                name: n2,
                args: a2,
            },
        ) if m1 == m2 && n1 == n2 && a1.len() == a2.len() => {
            for (t, c) in a1.iter().zip(a2) {
                match_type(t, c, out);
            }
        }
        (IrType::App { head: h1, args: a1 }, IrType::App { head: h2, args: a2 })
            if a1.len() == a2.len() =>
        {
            match_type(h1, h2, out);
            for (t, c) in a1.iter().zip(a2) {
                match_type(t, c, out);
            }
        }
        (
            IrType::Fun {
                param: p1,
                result: r1,
                ..
            },
            IrType::Fun {
                param: p2,
                result: r2,
                ..
            },
        ) => {
            match_type(p1, p2, out);
            match_type(r1, r2, out);
        }
        (IrType::Tuple(a1), IrType::Tuple(a2)) if a1.len() == a2.len() => {
            for (t, c) in a1.iter().zip(a2) {
                match_type(t, c, out);
            }
        }
        (
            IrType::Record {
                fields: f1,
                tail: t1,
            },
            IrType::Record {
                fields: f2,
                tail: t2,
            },
        ) => {
            for (l1, ty1) in f1 {
                if let Some((_, ty2)) = f2.iter().find(|(l2, _)| l2 == l1) {
                    match_type(ty1, ty2, out);
                }
            }
            if let Some(tail_var) = t1 {
                let extra: Vec<(String, TypeRef)> = f2
                    .iter()
                    .filter(|(l2, _)| !f1.iter().any(|(l1, _)| l1 == l2))
                    .cloned()
                    .collect();
                if extra.is_empty() {
                    if let Some(tv2) = t2 {
                        out.entry(tail_var.clone())
                            .or_insert_with(|| TypeRef::new(IrType::Var(tv2.clone())));
                    } else {
                        out.entry(tail_var.clone()).or_insert_with(|| {
                            TypeRef::new(IrType::Record {
                                fields: Vec::new(),
                                tail: None,
                            })
                        });
                    }
                } else {
                    out.entry(tail_var.clone()).or_insert_with(|| {
                        TypeRef::new(IrType::Record {
                            fields: extra,
                            tail: t2.clone(),
                        })
                    });
                }
            }
        }
        (IrType::Forall { vars, body: b1, .. }, IrType::Forall { body: b2, .. }) => {
            let mut inner = HashMap::new();
            match_type(b1, b2, &mut inner);
            for (k, v) in inner {
                if !vars.iter().any(|bv| bv == &k) {
                    out.entry(k).or_insert(v);
                }
            }
        }
        _ => {}
    }
}

/// Filters `binder_types` to only the binders present in `pat`.
#[cfg(not(feature = "typed-ir"))]
pub(super) fn filter_binder_types(
    _pat: &Pat,
    _binder_types: &[(String, TypeRef)],
) -> Vec<(String, TypeRef)> {
    Vec::new()
}

/// Filters `binder_types` to only the binders present in `pat`.
#[cfg(feature = "typed-ir")]
pub(super) fn filter_binder_types(
    pat: &Pat,
    binder_types: &[(String, TypeRef)],
) -> Vec<(String, TypeRef)> {
    if binder_types.is_empty() {
        return Vec::new();
    }
    let mut names = Vec::new();
    pat.binders(&mut names);
    binder_types
        .iter()
        .filter(|(n, _)| names.contains(&n.as_str()))
        .cloned()
        .collect()
}
