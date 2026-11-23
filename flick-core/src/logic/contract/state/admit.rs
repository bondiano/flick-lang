//! Static admission of types, constructors, and non-contracted helpers for a
//! `PureTotalV2` contract.

use std::collections::HashSet;

use crate::logic::{Ctor, Data, DataDecl, Def, Program, TUPLES, Ty};
use crate::source::Span;

use super::super::defs::admit_v2_for_module;
use super::super::term::{LangTypes, Pat, Term, TermKind};
use super::super::{ContractFailure, ContractSpec, VerifiedContracts};
use super::{AdmittedContract, refuse};

/// Statically admits every type, constructor, and non-contracted definition
/// reachable from `spec`, `body`, and any transitively referenced verified
/// callee's `requires`/`ensures`.
///
/// Refuses (`FLICK-T082`):
/// - unsupported or recursive types in parameters, result, or terms;
/// - self-recursion (`(spec.module, spec.name)` reachable);
/// - calls to failed, cyclic, refused, or pending contracted functions;
/// - foreign code, dictionary-taking helpers, user `Eq`/`Ord`/`Numeric`
///   instances, and recursive helpers.
pub fn admit_contract(
    spec: &ContractSpec,
    body: &Term,
    data: &Data,
    program: &dyn Program,
    verified: &VerifiedContracts,
    lang: &LangTypes,
) -> Result<AdmittedContract, ContractFailure> {
    let mut merged_data = data.clone();
    let mut all_contracted = verified.all_contracted_keys();
    all_contracted.insert((spec.module.clone(), spec.name.clone()));

    let mut roots: Vec<&Term> = Vec::with_capacity(1 + spec.requires.len() + spec.ensures.len());
    roots.push(body);
    for (req, _) in &spec.requires {
        roots.push(req);
    }
    for (ens, _) in &spec.ensures {
        roots.push(ens);
    }
    let own_root_count = roots.len();

    // Transitively admit helpers referenced by verified callees' contracts too.
    let mut seen_summaries: HashSet<(String, String)> = HashSet::new();
    let mut defs = admit_v2_for_module(
        &roots,
        Some(&spec.module),
        program,
        &all_contracted,
        lang,
        spec.decl_span,
    )
    .map_err(ContractFailure::Refused)?;

    loop {
        let mut new_roots: Vec<&Term> = Vec::new();
        for (dep_mod, dep_name) in defs.contracted_deps() {
            if dep_mod == &spec.module && dep_name == &spec.name {
                let at = find_def_span(&roots[..own_root_count], dep_mod, dep_name)
                    .unwrap_or(spec.decl_span);
                return refuse(
                    at,
                    format!("`{dep_name}` names itself: recursion is outside the domain"),
                );
            }
            let Some(summary) = verified.get(dep_mod, dep_name) else {
                let at = find_def_span(&roots[..own_root_count], dep_mod, dep_name)
                    .unwrap_or(spec.decl_span);
                return refuse(
                    at,
                    format!("`{dep_name}`'s contract did not check, so it cannot be assumed"),
                );
            };
            if seen_summaries.insert((dep_mod.clone(), dep_name.clone())) {
                merged_data.extend(&summary.data);
                for (req, _) in &summary.requires {
                    new_roots.push(req);
                }
                for (ens, _) in &summary.ensures {
                    new_roots.push(ens);
                }
            }
        }
        if new_roots.is_empty() {
            break;
        }
        roots.extend(new_roots);
        defs = admit_v2_for_module(
            &roots,
            Some(&spec.module),
            program,
            &all_contracted,
            lang,
            spec.decl_span,
        )
        .map_err(ContractFailure::Refused)?;
    }

    let mut visiting = Vec::new();
    for (_, ty) in &spec.params {
        admit_ty(ty, &merged_data, lang, spec.decl_span, &mut visiting)?;
    }
    admit_ty(
        &spec.result_ty,
        &merged_data,
        lang,
        spec.decl_span,
        &mut visiting,
    )?;

    let clamp_err = |err: ContractFailure| match err {
        ContractFailure::Refused(mut r) => {
            r.span = spec.decl_span;
            ContractFailure::Refused(r)
        }
        other => other,
    };
    for root in &roots[..own_root_count] {
        admit_term_ctors(root, &merged_data, lang, &mut visiting)?;
    }
    for root in &roots[own_root_count..] {
        admit_term_ctors(root, &merged_data, lang, &mut visiting).map_err(clamp_err)?;
    }
    for (def, def_body) in defs.defs() {
        if defs.is_local(def) {
            admit_term_ctors(def_body, &merged_data, lang, &mut visiting)?;
        } else {
            admit_term_ctors(def_body, &merged_data, lang, &mut visiting).map_err(clamp_err)?;
        }
    }

    Ok(AdmittedContract {
        defs,
        data: merged_data,
    })
}

pub fn lookup_data<'a>(
    data: &'a Data,
    module: &str,
    name: &str,
    span: Span,
) -> Result<&'a DataDecl, ContractFailure> {
    let Some(decl) = data.get(&(module.to_owned(), name.to_owned())) else {
        return refuse(span, format!("the type `{name}` is outside the domain"));
    };
    if decl.recursive {
        return refuse(
            span,
            format!("`{name}` is recursive; recursion is outside the domain"),
        );
    }
    Ok(decl)
}

fn admit_ty(
    ty: &Ty,
    data: &Data,
    lang: &LangTypes,
    span: Span,
    visiting: &mut Vec<(String, String)>,
) -> Result<(), ContractFailure> {
    match ty {
        Ty::Int | Ty::Bool | Ty::Rigid(_) => Ok(()),
        Ty::Refused(why) => refuse(span, why.clone()),
        Ty::Fun(..) => refuse(span, "function-typed values are outside the domain"),
        Ty::Data { module, name, args } => {
            if module == &lang.unit.0 && name == &lang.unit.1 && args.is_empty() {
                return Ok(());
            }
            if module == TUPLES {
                for arg in args {
                    admit_ty(arg, data, lang, span, visiting)?;
                }
                return Ok(());
            }
            let decl = lookup_data(data, module, name, span)?;
            for arg in args {
                admit_ty(arg, data, lang, span, visiting)?;
            }
            let key = (module.clone(), name.clone());
            if visiting.contains(&key) {
                return refuse(
                    span,
                    format!("`{name}` is recursive; recursion is outside the domain"),
                );
            }
            visiting.push(key);
            for (_, fields) in &decl.constructors {
                for field_ty in fields {
                    admit_ty(field_ty, data, lang, span, visiting)?;
                }
            }
            visiting.pop();
            Ok(())
        }
    }
}

fn admit_term_ctors(
    term: &Term,
    data: &Data,
    lang: &LangTypes,
    visiting: &mut Vec<(String, String)>,
) -> Result<(), ContractFailure> {
    match &term.kind {
        TermKind::Var(_)
        | TermKind::IntLit(_)
        | TermKind::BoolLit(_)
        | TermKind::UnitLit
        | TermKind::Def(_) => Ok(()),
        TermKind::Con(ctor) => admit_ctor(ctor, data, lang, term.span, visiting),
        TermKind::IntNeg(x) | TermKind::Not(x) | TermKind::Lam(_, x) => {
            admit_term_ctors(x, data, lang, visiting)
        }
        TermKind::IntBin(_, a, b)
        | TermKind::IntCmp(_, a, b)
        | TermKind::BoolEq { lhs: a, rhs: b, .. }
        | TermKind::TagEq { lhs: a, rhs: b, .. }
        | TermKind::And(a, b)
        | TermKind::Or(a, b)
        | TermKind::App {
            func: a, arg: b, ..
        }
        | TermKind::Let(_, a, b) => {
            admit_term_ctors(a, data, lang, visiting)?;
            admit_term_ctors(b, data, lang, visiting)
        }
        TermKind::If(c, t, f) => {
            admit_term_ctors(c, data, lang, visiting)?;
            admit_term_ctors(t, data, lang, visiting)?;
            admit_term_ctors(f, data, lang, visiting)
        }
        TermKind::Match(s, arms) => {
            admit_term_ctors(s, data, lang, visiting)?;
            for arm in arms.iter() {
                admit_pat_ctors(&arm.pat, data, lang, arm.body.span, visiting)?;
                admit_term_ctors(&arm.body, data, lang, visiting)?;
            }
            Ok(())
        }
    }
}

fn admit_pat_ctors(
    pat: &Pat,
    data: &Data,
    lang: &LangTypes,
    span: Span,
    visiting: &mut Vec<(String, String)>,
) -> Result<(), ContractFailure> {
    match pat {
        Pat::Con(ctor, fields) => {
            admit_ctor(ctor, data, lang, span, visiting)?;
            for field in fields {
                admit_pat_ctors(field, data, lang, span, visiting)?;
            }
            Ok(())
        }
        Pat::Wild | Pat::Bind(_) | Pat::Int(_) | Pat::Bool(_) | Pat::Unit => Ok(()),
    }
}

fn admit_ctor(
    ctor: &Ctor,
    data: &Data,
    lang: &LangTypes,
    span: Span,
    visiting: &mut Vec<(String, String)>,
) -> Result<(), ContractFailure> {
    if ctor.module == TUPLES || (ctor.module == lang.unit.0 && ctor.ty == lang.unit.1) {
        return Ok(());
    }
    let decl = lookup_data(data, &ctor.module, &ctor.ty, span)?;
    let key = (ctor.module.clone(), ctor.ty.clone());
    if visiting.contains(&key) {
        return refuse(
            span,
            format!(
                "`{}` is recursive; recursion is outside the domain",
                ctor.ty
            ),
        );
    }
    visiting.push(key);
    for (_, fields) in &decl.constructors {
        for field_ty in fields {
            admit_ty(field_ty, data, lang, span, visiting)?;
        }
    }
    visiting.pop();
    Ok(())
}

fn find_def_span(terms: &[&Term], module: &str, name: &str) -> Option<Span> {
    for t in terms {
        if let Some(span) = find_def_span_in(t, module, name) {
            return Some(span);
        }
    }
    None
}

fn find_def_span_in(term: &Term, module: &str, name: &str) -> Option<Span> {
    match &term.kind {
        TermKind::Def(d) => match d.as_ref() {
            Def::Value { module: m, name: n } if m == module && n == name => Some(term.span),
            _ => None,
        },
        TermKind::IntNeg(x) | TermKind::Not(x) | TermKind::Lam(_, x) => {
            find_def_span_in(x, module, name)
        }
        TermKind::IntBin(_, a, b)
        | TermKind::IntCmp(_, a, b)
        | TermKind::BoolEq { lhs: a, rhs: b, .. }
        | TermKind::TagEq { lhs: a, rhs: b, .. }
        | TermKind::And(a, b)
        | TermKind::Or(a, b)
        | TermKind::App {
            func: a, arg: b, ..
        }
        | TermKind::Let(_, a, b) => {
            find_def_span_in(a, module, name).or_else(|| find_def_span_in(b, module, name))
        }
        TermKind::If(c, t, f) => find_def_span_in(c, module, name)
            .or_else(|| find_def_span_in(t, module, name))
            .or_else(|| find_def_span_in(f, module, name)),
        TermKind::Match(s, arms) => find_def_span_in(s, module, name).or_else(|| {
            arms.iter()
                .find_map(|a| find_def_span_in(&a.body, module, name))
        }),
        _ => None,
    }
}
