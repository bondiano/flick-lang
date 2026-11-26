//! Type predicates, deterministic template matching, and substitution helpers
//! for polymorphic specialization.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{Constructor, Dict, Expr, ExprKind, IrType, TypeInterner, TypeRef};

use super::classify::ProgramIndex;

/// Returns `true` if `arg` is a compound type containing a free type variable
/// from `src_params` (e.g. `List a`, `(a, a)`, `Option a`, `{ x : a | r }`),
/// indicating type-argument growth along a recursive cycle.
pub(crate) fn is_growing_type_arg(arg: &IrType, src_params: &BTreeSet<&str>) -> bool {
    if matches!(arg, IrType::Var(_)) {
        return false;
    }
    mentions_any_free_var(arg, src_params)
}

/// Checks whether a local `Stmt::Fun { rec: true, .. }` performs polymorphic
/// recursion by comparing its declared scheme against recursive `Local(name)`
/// occurrences inside `value`.
pub(crate) fn is_local_polymorphic_recursion(
    name: &str,
    type_params: &[String],
    declared_ty: Option<&TypeRef>,
    value: &Expr,
) -> bool {
    if type_params.is_empty() {
        return false;
    }
    let Some(decl_ty) = declared_ty else {
        return true;
    };
    let param_set: BTreeSet<&str> = type_params.iter().map(String::as_str).collect();
    let mut found = false;
    walk_expr_for_local_rec(value, name, decl_ty, &param_set, &mut found);
    found
}

fn walk_expr_for_local_rec(
    expr: &Expr,
    name: &str,
    decl_ty: &IrType,
    params: &BTreeSet<&str>,
    found: &mut bool,
) {
    if *found {
        return;
    }
    if let ExprKind::Local(local) = &expr.kind
        && local == name
        && let Some(call_ty) = &expr.ty
    {
        let mut matched = BTreeMap::new();
        match_type_map(decl_ty, call_ty, &mut matched);
        if matched.values().any(|arg| is_growing_type_arg(arg, params)) {
            *found = true;
            return;
        }
    }
    expr.each_child(&mut |child| walk_expr_for_local_rec(child, name, decl_ty, params, found));
}

/// Whether `ty` has any `IrType::Forall` with non-empty quantified type
/// variables in a parameter position (higher-rank callback).
pub(crate) fn has_higher_rank_param(ty: &IrType) -> bool {
    match ty {
        IrType::Fun { param, result, .. } => {
            contains_quantified_forall(param) || has_higher_rank_param(result)
        }
        IrType::Forall { body, .. } => has_higher_rank_param(body),
        _ => false,
    }
}

/// Whether `ty` contains an `IrType::Forall` with at least one quantified type
/// variable.
pub(crate) fn contains_quantified_forall(ty: &IrType) -> bool {
    if let IrType::Forall { vars, body, .. } = ty {
        !vars.is_empty() || contains_quantified_forall(body)
    } else {
        let mut found = false;
        ty.each_child(&mut |c| {
            if !found && contains_quantified_forall(c) {
                found = true;
            }
        });
        found
    }
}

/// Whether `ty` contains `IrType::Exists` or `Prelude.Module`.
pub(crate) fn contains_exists_or_module(ty: &IrType, index: &ProgramIndex<'_>) -> bool {
    match ty {
        IrType::Exists { .. } => true,
        IrType::Named { .. } if index.is_packed_module_type(ty) => true,
        _ => {
            let mut found = false;
            ty.each_child(&mut |c| {
                if !found && contains_exists_or_module(c, index) {
                    found = true;
                }
            });
            found
        }
    }
}

/// Whether `ty` contains any open record row (`tail.is_some()`).
pub(crate) fn contains_open_record(ty: &IrType) -> bool {
    if let IrType::Record { fields, tail } = ty {
        tail.is_some() || fields.iter().any(|(_, t)| contains_open_record(t))
    } else {
        let mut found = false;
        ty.each_child(&mut |c| {
            if !found && contains_open_record(c) {
                found = true;
            }
        });
        found
    }
}

/// Whether `ty` has any free type variable (respecting `Forall` binders) or
/// open record row tail variable.
pub(crate) fn has_free_type_vars(ty: &IrType) -> bool {
    has_free_vars_inner(ty, &BTreeSet::new())
}

fn has_free_vars_inner(ty: &IrType, bound: &BTreeSet<&str>) -> bool {
    match ty {
        IrType::Var(v) => !bound.contains(v.as_str()),
        IrType::Record { fields, tail } => {
            if let Some(t) = tail
                && !bound.contains(t.as_str())
            {
                return true;
            }
            fields.iter().any(|(_, f)| has_free_vars_inner(f, bound))
        }
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut inner = bound.clone();
            inner.extend(vars.iter().map(String::as_str));
            constraints
                .iter()
                .any(|c| c.args.iter().any(|a| has_free_vars_inner(a, &inner)))
                || has_free_vars_inner(body, &inner)
        }
        _ => {
            let mut found = false;
            ty.each_child(&mut |c| {
                if !found && has_free_vars_inner(c, bound) {
                    found = true;
                }
            });
            found
        }
    }
}

pub(crate) fn mentions_any_free_var(ty: &IrType, wanted: &BTreeSet<&str>) -> bool {
    mentions_inner(ty, wanted, &BTreeSet::new())
}

fn mentions_inner(ty: &IrType, wanted: &BTreeSet<&str>, bound: &BTreeSet<&str>) -> bool {
    match ty {
        IrType::Var(v) => !bound.contains(v.as_str()) && wanted.contains(v.as_str()),
        IrType::Record { fields, tail } => {
            if let Some(t) = tail
                && !bound.contains(t.as_str())
                && wanted.contains(t.as_str())
            {
                return true;
            }
            fields.iter().any(|(_, f)| mentions_inner(f, wanted, bound))
        }
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut inner = bound.clone();
            inner.extend(vars.iter().map(String::as_str));
            constraints
                .iter()
                .any(|c| c.args.iter().any(|a| mentions_inner(a, wanted, &inner)))
                || mentions_inner(body, wanted, &inner)
        }
        _ => {
            let mut found = false;
            ty.each_child(&mut |c| {
                if !found && mentions_inner(c, wanted, bound) {
                    found = true;
                }
            });
            found
        }
    }
}

/// Whether `ty` is completely monomorphic and suitable for `LayoutEngine`
/// (no free type variables, no open record tails, no `Exists`, no `Module`).
pub(crate) fn is_concrete_layout_type(ty: &IrType, index: &ProgramIndex<'_>) -> bool {
    !has_free_type_vars(ty) && !contains_open_record(ty) && !contains_exists_or_module(ty, index)
}

/// Deterministic template matching of `template` against `concrete` into a
/// `BTreeMap` of variable bindings.
#[allow(clippy::too_many_lines)]
pub(crate) fn match_type_map(
    template: &IrType,
    concrete: &TypeRef,
    out: &mut BTreeMap<String, TypeRef>,
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
                match_type_map(t, c, out);
            }
        }
        (IrType::App { head: h1, args: a1 }, IrType::App { head: h2, args: a2 })
            if a1.len() == a2.len() =>
        {
            match_type_map(h1, h2, out);
            for (t, c) in a1.iter().zip(a2) {
                match_type_map(t, c, out);
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
            match_type_map(p1, p2, out);
            match_type_map(r1, r2, out);
        }
        (IrType::Tuple(a1), IrType::Tuple(a2)) if a1.len() == a2.len() => {
            for (t, c) in a1.iter().zip(a2) {
                match_type_map(t, c, out);
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
                    match_type_map(ty1, ty2, out);
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
            let mut inner = BTreeMap::new();
            match_type_map(b1, b2, &mut inner);
            for (k, v) in inner {
                if !vars.iter().any(|bv| bv == &k) {
                    out.entry(k).or_insert(v);
                }
            }
        }
        _ => {}
    }
}

/// Substitutes a `Dict::Given` mapping through `dict`.
pub(crate) fn subst_dict(dict: &Dict, env: &BTreeMap<String, Dict>) -> Dict {
    match dict {
        Dict::Given(name) => env.get(name).cloned().unwrap_or_else(|| dict.clone()),
        Dict::Instance { module, name, args } => Dict::Instance {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|a| subst_dict(a, env)).collect(),
        },
        Dict::Super { dict: inner, class } => Dict::Super {
            dict: Box::new(subst_dict(inner, env)),
            class: class.clone(),
        },
        Dict::Structural { class, fields } => Dict::Structural {
            class: class.clone(),
            fields: fields.iter().map(|f| subst_dict(f, env)).collect(),
        },
        Dict::Value { .. }
        | Dict::Opened { .. }
        | Dict::Field(_)
        | Dict::Primitive { .. }
        | Dict::Site { .. }
        | Dict::Error => dict.clone(),
    }
}

/// Helper to substitute a type using a `BTreeMap<String, TypeRef>`.
pub(crate) fn subst_type(
    ty: &TypeRef,
    subst: &BTreeMap<String, TypeRef>,
    interner: &mut TypeInterner,
) -> TypeRef {
    if subst.is_empty() {
        return ty.clone();
    }
    let pairs: Vec<(&str, &TypeRef)> = subst.iter().map(|(k, v)| (k.as_str(), v)).collect();
    ty.substitute(&pairs, interner)
}

/// Helper to substitute a constructor's field/type args using `subst`.
pub(crate) fn subst_constructor(
    ctor: &Constructor,
    subst: &BTreeMap<String, TypeRef>,
    interner: &mut TypeInterner,
) -> Constructor {
    if subst.is_empty() {
        return ctor.clone();
    }
    let pairs: Vec<(&str, &TypeRef)> = subst.iter().map(|(k, v)| (k.as_str(), v)).collect();
    let mut out = ctor.clone();
    for f in &mut out.fields {
        *f = f.substitute(&pairs, interner);
    }
    for a in &mut out.type_args {
        *a = a.substitute(&pairs, interner);
    }
    out
}
