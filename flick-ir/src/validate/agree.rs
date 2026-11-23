//! Structural alpha-equality and scheme-instantiation matching for IR types.

use std::collections::BTreeMap;

use super::Validator;
use super::decls::TypeIndex;
use crate::{IrType, Lang, TypeInterner, TypeRef};

const NON_ERROR_LANGS: &[Lang] = &[
    Lang::Unit,
    Lang::Bool,
    Lang::Int,
    Lang::Float,
    Lang::BigInt,
    Lang::String,
    Lang::Unknown,
    Lang::JsValue,
    Lang::List,
    Lang::Seq,
    Lang::Ref,
    Lang::Redacted,
    Lang::Schema,
    Lang::Url,
    Lang::Form,
    Lang::PureCodec,
    Lang::Eff,
    Lang::Lazy,
    Lang::Layer,
    Lang::Codec,
    Lang::Ui,
    Lang::ServiceTag,
    Lang::ServiceImpl,
    Lang::Option,
    Lang::Result,
    Lang::Exit,
    Lang::Failure,
    Lang::Json,
];

impl Validator<'_> {
    pub(super) fn expect_agree(&mut self, expected: &IrType, actual: &IrType, what: &str) {
        if !types_agree(self.types, expected, actual, self.gadt_depth > 0) {
            let module = self.module;
            self.problems.push(format!(
                "`{module}` {what} has type `{actual}`, expected `{expected}`"
            ));
        }
    }
}

/// Structural alpha-equality of IR types (allowing `Var` refinement inside a
/// GADT match arm, erased `Errors`/`ServiceImpl` compatibility, opaque alias
/// unfolding, higher-rank `Forall` skolemization/instantiation, and stage
/// variance inside data containers).
pub(super) fn types_agree(
    types: &TypeIndex<'_>,
    a: &IrType,
    b: &IrType,
    allow_var_refinement: bool,
) -> bool {
    types_agree_env(types, a, b, &[], allow_var_refinement, false, 0)
}

fn is_error_compatible(types: &TypeIndex<'_>, ty: &IrType) -> bool {
    if types.is_lang_con(ty, Lang::Never) || matches!(ty, IrType::Var(_)) {
        return true;
    }
    let IrType::Named { module, name, .. } = ty else {
        return false;
    };
    if let Some(decl) = types.type_decls.get(&(module.as_str(), name.as_str())) {
        return decl.error;
    }
    !NON_ERROR_LANGS
        .iter()
        .any(|&role| types.is_lang_named(ty, role).is_some())
}

pub(super) fn unfold_opaque_alias(types: &TypeIndex<'_>, ty: &IrType) -> Option<TypeRef> {
    let IrType::Named { module, name, args } = ty else {
        return None;
    };
    let decl = types.type_decls.get(&(module.as_str(), name.as_str()))?;
    let body = decl.alias.as_ref()?;
    if args.len() != decl.params.len() {
        return None;
    }
    if decl.params.is_empty() {
        return Some(body.clone());
    }
    let mut interner = TypeInterner::new();
    let subst: Vec<(&str, &TypeRef)> = decl
        .params
        .iter()
        .map(String::as_str)
        .zip(args.iter())
        .collect();
    Some(body.substitute(&subst, &mut interner))
}

fn peel_forall(mut ty: &IrType) -> (Vec<String>, &IrType) {
    let mut vars = Vec::new();
    while let IrType::Forall { vars: vs, body, .. } = ty {
        vars.extend(vs.iter().cloned());
        ty = body;
    }
    (vars, ty)
}

#[allow(clippy::too_many_lines)]
fn types_agree_env(
    types: &TypeIndex<'_>,
    a: &IrType,
    b: &IrType,
    renames: &[(&str, &str)],
    allow_var_refinement: bool,
    in_data: bool,
    unfold: u8,
) -> bool {
    if renames.is_empty() && a == b {
        return true;
    }
    if allow_var_refinement && (matches!(a, IrType::Var(_)) || matches!(b, IrType::Var(_))) {
        return true;
    }
    if (types.is_lang_con(a, Lang::Errors) && is_error_compatible(types, b))
        || (types.is_lang_con(b, Lang::Errors) && is_error_compatible(types, a))
    {
        return true;
    }
    if (types.is_lang_con(a, Lang::ServiceImpl) && matches!(b, IrType::Record { .. }))
        || (types.is_lang_con(b, Lang::ServiceImpl) && matches!(a, IrType::Record { .. }))
    {
        return true;
    }
    if matches!(a, IrType::Forall { .. }) && !matches!(b, IrType::Forall { .. }) {
        let (vars, inner) = peel_forall(a);
        return matches_scheme(types, inner, &vars, b, allow_var_refinement, in_data);
    }
    if matches!(b, IrType::Forall { .. }) && !matches!(a, IrType::Forall { .. }) {
        let (vars, inner) = peel_forall(b);
        return matches_scheme(types, inner, &vars, a, allow_var_refinement, in_data);
    }
    let matched = match (a, b) {
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
        ) => {
            m1 == m2
                && n1 == n2
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| {
                    types_agree_env(types, x, y, renames, allow_var_refinement, true, unfold)
                })
        }
        (IrType::Var(v1), IrType::Var(v2)) => {
            let mapped = renames
                .iter()
                .rev()
                .find(|&&(l, _)| l == v1)
                .map_or(v1.as_str(), |&(_, r)| r);
            mapped == v2
        }
        (IrType::App { head: h1, args: a1 }, IrType::App { head: h2, args: a2 }) => {
            types_agree_env(
                types,
                h1,
                h2,
                renames,
                allow_var_refinement,
                in_data,
                unfold,
            ) && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| {
                    types_agree_env(types, x, y, renames, allow_var_refinement, true, unfold)
                })
        }
        (
            IrType::Fun {
                param: p1,
                stage: s1,
                result: r1,
            },
            IrType::Fun {
                param: p2,
                stage: s2,
                result: r2,
            },
        ) => {
            (in_data || s1 == s2)
                && types_agree_env(
                    types,
                    p1,
                    p2,
                    renames,
                    allow_var_refinement,
                    in_data,
                    unfold,
                )
                && types_agree_env(
                    types,
                    r1,
                    r2,
                    renames,
                    allow_var_refinement,
                    in_data,
                    unfold,
                )
        }
        (IrType::Tuple(t1), IrType::Tuple(t2)) => {
            t1.len() == t2.len()
                && t1.iter().zip(t2).all(|(x, y)| {
                    types_agree_env(types, x, y, renames, allow_var_refinement, true, unfold)
                })
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
            let tails_match = match (t1, t2) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    let mapped = renames
                        .iter()
                        .rev()
                        .find(|&&(l, _)| l == a)
                        .map_or(a.as_str(), |&(_, r)| r);
                    mapped == b
                }
                _ => false,
            };
            tails_match
                && f1.len() == f2.len()
                && f1.iter().zip(f2).all(|((l1, ty1), (l2, ty2))| {
                    l1 == l2
                        && types_agree_env(
                            types,
                            ty1,
                            ty2,
                            renames,
                            allow_var_refinement,
                            true,
                            unfold,
                        )
                })
        }
        (
            IrType::Forall {
                vars: v1,
                constraints: c1,
                body: b1,
            },
            IrType::Forall {
                vars: v2,
                constraints: c2,
                body: b2,
            },
        ) => {
            if v1.len() != v2.len() || c1.len() != c2.len() {
                false
            } else {
                let mut next = renames.to_vec();
                for (x, y) in v1.iter().zip(v2) {
                    next.push((x.as_str(), y.as_str()));
                }
                c1.iter().zip(c2).all(|(x, y)| {
                    x.module == y.module
                        && x.class == y.class
                        && x.args.len() == y.args.len()
                        && x.args.iter().zip(&y.args).all(|(a, b)| {
                            types_agree_env(types, a, b, &next, allow_var_refinement, true, unfold)
                        })
                }) && types_agree_env(types, b1, b2, &next, allow_var_refinement, in_data, unfold)
            }
        }
        (
            IrType::Exists {
                binder: k1,
                contract: c1,
                body: b1,
            },
            IrType::Exists {
                binder: k2,
                contract: c2,
                body: b2,
            },
        ) => {
            k1 == k2
                && types_agree_env(types, c1, c2, renames, allow_var_refinement, true, unfold)
                && types_agree_env(
                    types,
                    b1,
                    b2,
                    renames,
                    allow_var_refinement,
                    in_data,
                    unfold,
                )
        }
        _ => false,
    };
    if matched || unfold >= 4 {
        return matched;
    }
    if let Some(ua) = unfold_opaque_alias(types, a)
        && types_agree_env(
            types,
            &ua,
            b,
            renames,
            allow_var_refinement,
            in_data,
            unfold + 1,
        )
    {
        return true;
    }
    if let Some(ub) = unfold_opaque_alias(types, b)
        && types_agree_env(
            types,
            a,
            &ub,
            renames,
            allow_var_refinement,
            in_data,
            unfold + 1,
        )
    {
        return true;
    }
    false
}

/// Whether `actual` is a valid instantiation of `scheme` quantified over `params`.
pub(super) fn matches_scheme(
    types: &TypeIndex<'_>,
    scheme: &IrType,
    params: &[String],
    actual: &IrType,
    allow_var_refinement: bool,
    in_data: bool,
) -> bool {
    let mut subst: BTreeMap<&str, &IrType> = BTreeMap::new();
    match_scheme_rec(
        types,
        scheme,
        params,
        actual,
        &mut subst,
        allow_var_refinement,
        in_data,
    )
}

fn match_scheme_rec<'a>(
    types: &TypeIndex<'_>,
    scheme: &'a IrType,
    params: &[String],
    actual: &'a IrType,
    subst: &mut BTreeMap<&'a str, &'a IrType>,
    allow_var_refinement: bool,
    in_data: bool,
) -> bool {
    if let IrType::Var(v) = scheme
        && params.iter().any(|p| p == v)
    {
        if let Some(prev) = subst.get(v.as_str()) {
            return types_agree_env(types, prev, actual, &[], allow_var_refinement, in_data, 0);
        }
        subst.insert(v.as_str(), actual);
        return true;
    }
    if allow_var_refinement && matches!(actual, IrType::Var(_)) {
        return true;
    }
    match (scheme, actual) {
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
        ) => {
            m1 == m2
                && n1 == n2
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| {
                    match_scheme_rec(types, x, params, y, subst, allow_var_refinement, true)
                })
        }
        (IrType::Var(v1), IrType::Var(v2)) => v1 == v2,
        (
            IrType::Fun {
                param: p1,
                stage: s1,
                result: r1,
            },
            IrType::Fun {
                param: p2,
                stage: s2,
                result: r2,
            },
        ) => {
            (in_data || s1 == s2)
                && match_scheme_rec(types, p1, params, p2, subst, allow_var_refinement, in_data)
                && match_scheme_rec(types, r1, params, r2, subst, allow_var_refinement, in_data)
        }
        (IrType::Tuple(t1), IrType::Tuple(t2)) => {
            t1.len() == t2.len()
                && t1.iter().zip(t2).all(|(x, y)| {
                    match_scheme_rec(types, x, params, y, subst, allow_var_refinement, true)
                })
        }
        (
            IrType::Record {
                fields: f1,
                tail: None,
            },
            IrType::Record {
                fields: f2,
                tail: None,
            },
        ) => {
            f1.len() == f2.len()
                && f1.iter().zip(f2).all(|((l1, ty1), (l2, ty2))| {
                    l1 == l2
                        && match_scheme_rec(
                            types,
                            ty1,
                            params,
                            ty2,
                            subst,
                            allow_var_refinement,
                            true,
                        )
                })
        }
        (IrType::Record { tail: Some(t), .. }, IrType::Record { .. })
            if params.iter().any(|p| p == t) =>
        {
            true
        }
        (IrType::App { head, .. }, _) if matches!(&**head, IrType::Var(v) if params.iter().any(|p| p == v)) => {
            true
        }
        (IrType::App { head: h1, args: a1 }, IrType::App { head: h2, args: a2 }) => {
            match_scheme_rec(types, h1, params, h2, subst, allow_var_refinement, in_data)
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| {
                    match_scheme_rec(types, x, params, y, subst, allow_var_refinement, true)
                })
        }
        _ => types_agree_env(types, scheme, actual, &[], allow_var_refinement, in_data, 0),
    }
}
