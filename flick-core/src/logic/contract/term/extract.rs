//! Strict IR-to-`PureTotalV2` term extraction.

use std::rc::Rc;

use flick_ir::{Expr, ExprKind, Lit, Num, Pat as IrPat, Stage, Stmt};

use crate::logic::{Ctor, Def, LEMMAS, Program, RULES, Refused, term::tuple};
use crate::source::Span;

use super::binary::extract_binary;
use super::infer::{INFER_DEPTH, infer_prim};
use super::{Arm, LangTypes, Pat, PrimHint, Term, TermKind};

pub(super) fn refuse<T>(span: Span, why: impl Into<String>) -> Result<T, Refused> {
    Err(Refused {
        span,
        why: why.into(),
    })
}

/// Extracts `expr` into a `PureTotalV2` spanned `Term`.
///
/// `ty_env` maps in-scope local names to known `PrimHint`s (e.g. function
/// parameters from `ContractInfo::params`).
pub fn extract(
    expr: &Expr,
    ty_env: &[(Rc<str>, PrimHint)],
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    let mut env: Vec<(Rc<str>, Option<PrimHint>)> = ty_env
        .iter()
        .map(|(name, hint)| (name.clone(), Some(*hint)))
        .collect();
    extract_in(expr, &mut env, program, lang)
}

/// Extracts a `Numeric Int` method body, defaulting untyped lambda parameters
/// to `PrimHint::Int` (in case a synthetic IR body omits `Param::ty`).
pub(in super::super) fn extract_numeric_int_method(
    expr: &Expr,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    let mut env = Vec::new();
    if let ExprKind::Lambda { params, body } = &expr.kind {
        let mut names = Vec::with_capacity(params.len());
        for (i, param) in params.iter().enumerate() {
            if param.stage != Stage::Pure {
                return refuse(expr.span, "a function with effects is outside the domain");
            }
            let name: Rc<str> = match &param.pat {
                IrPat::Bind(n) => n.as_str().into(),
                IrPat::Wild | IrPat::Lit(Lit::Unit) => format!("$unused{i}").into(),
                _ => {
                    return refuse(
                        expr.span,
                        "a parameter pattern is not supported in a contract",
                    );
                }
            };
            let hint = param
                .ty
                .as_ref()
                .map_or(PrimHint::Int, |t| lang.classify_ty(t));
            env.push((name.clone(), Some(hint)));
            names.push(name);
        }
        let mut out = extract_in(body, &mut env, program, lang)?;
        for name in names.into_iter().rev() {
            out = Term::new(TermKind::Lam(name, Rc::new(out)), expr.span);
        }
        return Ok(out);
    }
    extract_in(expr, &mut env, program, lang)
}

pub(super) fn extract_in(
    expr: &Expr,
    env: &mut Vec<(Rc<str>, Option<PrimHint>)>,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    let span = expr.span;
    let kind = match &expr.kind {
        ExprKind::Lit(Lit::Int(n)) => TermKind::IntLit(i128::from(*n)),
        ExprKind::Lit(Lit::Bool(b)) => TermKind::BoolLit(*b),
        ExprKind::Lit(Lit::Unit) => TermKind::UnitLit,
        ExprKind::Lit(Lit::Float(_)) => return refuse(span, "`Float` is outside the domain"),
        ExprKind::Lit(Lit::BigInt(_)) => return refuse(span, "`BigInt` is outside the domain"),
        ExprKind::Lit(Lit::String(_)) => return refuse(span, "`String` is outside the domain"),
        ExprKind::Neg(Num::Float, _) => {
            return refuse(span, "`Float` arithmetic is outside the domain");
        }
        ExprKind::Neg(Num::BigInt, _) => {
            return refuse(span, "`BigInt` arithmetic is outside the domain");
        }
        ExprKind::Neg(Num::Int, inner) => {
            TermKind::IntNeg(Rc::new(extract_in(inner, env, program, lang)?))
        }
        ExprKind::Not(inner) => TermKind::Not(Rc::new(extract_in(inner, env, program, lang)?)),
        ExprKind::If(cond, then_branch, else_branch) => TermKind::If(
            Rc::new(extract_in(cond, env, program, lang)?),
            Rc::new(extract_in(then_branch, env, program, lang)?),
            Rc::new(extract_in(else_branch, env, program, lang)?),
        ),
        ExprKind::App {
            func,
            arg,
            stage: Stage::Pure,
            arg_first,
        } => TermKind::App {
            func: Rc::new(extract_in(func, env, program, lang)?),
            arg: Rc::new(extract_in(arg, env, program, lang)?),
            arg_first: *arg_first,
        },
        ExprKind::App { .. } => return refuse(span, "a call with effects is outside the domain"),
        ExprKind::Local(name) => TermKind::Var(name.as_str().into()),
        ExprKind::Global { module, .. } if module == RULES || module == LEMMAS => {
            return refuse(span, "a proof rule or lemma is outside a contract");
        }
        ExprKind::Global { module, name, .. } => TermKind::Def(Rc::new(Def::Value {
            module: module.clone(),
            name: name.clone(),
        })),
        ExprKind::Method { dict, name, .. } => TermKind::Def(Rc::new(Def::Method {
            dict: dict.clone(),
            name: name.clone(),
        })),
        ExprKind::Con(c) => TermKind::Con(Rc::new(Ctor {
            module: c.module.clone(),
            ty: c.ty.clone(),
            name: c.name.clone(),
            arity: c.arity,
        })),
        ExprKind::Binary(op, lhs, rhs) => {
            return extract_binary(*op, lhs, rhs, span, env, program, lang);
        }
        ExprKind::Lambda { params, body } => {
            return extract_lambda(params, body, span, env, program, lang);
        }
        ExprKind::Block(stmts, result) => {
            return extract_block(stmts, result, span, env, program, lang);
        }
        ExprKind::Match(scrutinee, arms) => {
            return extract_match(scrutinee, arms, span, env, program, lang);
        }
        ExprKind::Tuple(items) => {
            return items.iter().try_fold(
                Term::new(TermKind::Con(tuple(items.len())), span),
                |acc, item| {
                    let arg = extract_in(item, env, program, lang)?;
                    Ok(Term::new(
                        TermKind::App {
                            func: Rc::new(acc),
                            arg: Rc::new(arg),
                            arg_first: false,
                        },
                        span,
                    ))
                },
            );
        }
        ExprKind::Dicts(..) | ExprKind::Abstract(..) | ExprKind::Dictionary(_) => {
            return refuse(
                span,
                "a constrained call or user class dictionary is outside the domain",
            );
        }
        other => {
            return refuse(span, format!("{} is outside the domain", describe(other)));
        }
    };
    Ok(Term::new(kind, span))
}

fn extract_lambda(
    params: &[flick_ir::Param],
    body: &Expr,
    span: Span,
    env: &mut Vec<(Rc<str>, Option<PrimHint>)>,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    let mark = env.len();
    let mut names = Vec::with_capacity(params.len());
    for (idx, param) in params.iter().enumerate() {
        if param.stage != Stage::Pure {
            env.truncate(mark);
            return refuse(span, "a function with effects is outside the domain");
        }
        let name: Rc<str> = match &param.pat {
            IrPat::Bind(bound) => {
                let rc: Rc<str> = bound.as_str().into();
                let hint = param.ty.as_ref().map(|ty| lang.classify_ty(ty));
                env.push((rc.clone(), hint));
                rc
            }
            IrPat::Wild | IrPat::Lit(Lit::Unit) => format!("$unused{idx}").into(),
            _ => {
                env.truncate(mark);
                return refuse(span, "a parameter pattern is not supported in a contract");
            }
        };
        names.push(name);
    }
    let body = extract_in(body, env, program, lang);
    env.truncate(mark);
    let mut out = body?;
    for name in names.into_iter().rev() {
        out = Term::new(TermKind::Lam(name, Rc::new(out)), span);
    }
    Ok(out)
}

fn extract_block(
    stmts: &[Stmt],
    result: &Expr,
    span: Span,
    env: &mut Vec<(Rc<str>, Option<PrimHint>)>,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    let mark = env.len();
    let mut bindings = Vec::with_capacity(stmts.len());
    for (idx, stmt) in stmts.iter().enumerate() {
        match stmt {
            Stmt::Let {
                pat: IrPat::Bind(name),
                value,
                ..
            } => {
                let val_term = match extract_in(value, env, program, lang) {
                    Ok(term) => term,
                    Err(err) => {
                        env.truncate(mark);
                        return Err(err);
                    }
                };
                let hint = infer_prim(value, env, program, lang, INFER_DEPTH);
                let rc: Rc<str> = name.as_str().into();
                env.push((rc.clone(), hint));
                bindings.push((rc, val_term));
            }
            Stmt::Let {
                pat: IrPat::Wild | IrPat::Lit(Lit::Unit),
                value,
                ..
            } => {
                let val_term = match extract_in(value, env, program, lang) {
                    Ok(term) => term,
                    Err(err) => {
                        env.truncate(mark);
                        return Err(err);
                    }
                };
                bindings.push((format!("$unused{idx}").into(), val_term));
            }
            Stmt::Fun {
                name,
                rec: false,
                dicts,
                value,
                ..
            } if dicts.is_empty() => {
                let val_term = match extract_in(value, env, program, lang) {
                    Ok(term) => term,
                    Err(err) => {
                        env.truncate(mark);
                        return Err(err);
                    }
                };
                let rc: Rc<str> = name.as_str().into();
                env.push((rc.clone(), Some(PrimHint::Other)));
                bindings.push((rc, val_term));
            }
            Stmt::Fun { rec: true, .. } => {
                env.truncate(mark);
                return refuse(span, "recursion is outside the domain");
            }
            Stmt::Fun { .. } => {
                env.truncate(mark);
                return refuse(
                    span,
                    "a local function with class constraints is outside the domain",
                );
            }
            Stmt::Var { .. } | Stmt::Assign(..) => {
                env.truncate(mark);
                return refuse(span, "a mutable variable is outside the domain");
            }
            Stmt::Expr(_) => {
                env.truncate(mark);
                return refuse(span, "only `let` bindings are supported in a contract");
            }
            Stmt::Let { .. } => {
                env.truncate(mark);
                return refuse(span, "only `let name = value` is supported in a contract");
            }
        }
    }
    let res = extract_in(result, env, program, lang);
    env.truncate(mark);
    let mut out = res?;
    for (name, val) in bindings.into_iter().rev() {
        out = Term::new(TermKind::Let(name, Rc::new(val), Rc::new(out)), span);
    }
    Ok(out)
}

fn extract_match(
    scrutinee: &Expr,
    arms: &[flick_ir::Arm],
    span: Span,
    env: &mut Vec<(Rc<str>, Option<PrimHint>)>,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    let scrut_hint = infer_prim(scrutinee, env, program, lang, INFER_DEPTH);
    let scrut = extract_in(scrutinee, env, program, lang)?;
    let mut out_arms = Vec::with_capacity(arms.len());
    for arm in arms {
        if arm.guard.is_some() {
            return refuse(arm.body.span, "a guard is not supported in a contract");
        }
        let pat = extract_pat(&arm.pat, arm.body.span)?;
        let mark = env.len();
        push_pat_hints(&pat, scrut_hint, env);
        let body = extract_in(&arm.body, env, program, lang);
        env.truncate(mark);
        out_arms.push(Arm { pat, body: body? });
    }
    Ok(Term::new(
        TermKind::Match(Rc::new(scrut), out_arms.into()),
        span,
    ))
}

fn push_pat_hints(
    pat: &Pat,
    scrut_hint: Option<PrimHint>,
    env: &mut Vec<(Rc<str>, Option<PrimHint>)>,
) {
    match pat {
        Pat::Bind(name) => env.push((name.clone(), scrut_hint)),
        Pat::Con(_, fields) => {
            for field in fields {
                push_pat_hints(field, None, env);
            }
        }
        Pat::Wild | Pat::Int(_) | Pat::Bool(_) | Pat::Unit => {}
    }
}

fn extract_pat(p: &IrPat, span: Span) -> Result<Pat, Refused> {
    Ok(match p {
        IrPat::Wild => Pat::Wild,
        IrPat::Bind(n) => Pat::Bind(n.as_str().into()),
        IrPat::Lit(Lit::Int(n)) => Pat::Int(i128::from(*n)),
        IrPat::Lit(Lit::Bool(b)) => Pat::Bool(*b),
        IrPat::Lit(Lit::Unit) => Pat::Unit,
        IrPat::Lit(Lit::Float(_)) => return refuse(span, "`Float` is outside the domain"),
        IrPat::Lit(Lit::BigInt(_)) => return refuse(span, "`BigInt` is outside the domain"),
        IrPat::Lit(Lit::String(_)) => return refuse(span, "`String` is outside the domain"),
        IrPat::Con(c, fields) => Pat::Con(
            Rc::new(Ctor {
                module: c.module.clone(),
                ty: c.ty.clone(),
                name: c.name.clone(),
                arity: c.arity,
            }),
            fields
                .iter()
                .map(|f| extract_pat(f, span))
                .collect::<Result<_, _>>()?,
        ),
        IrPat::Tuple(items) => Pat::Con(
            tuple(items.len()),
            items
                .iter()
                .map(|p| extract_pat(p, span))
                .collect::<Result<_, _>>()?,
        ),
        _ => return refuse(span, "this pattern is not supported in a contract"),
    })
}

fn describe(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Var(_) => "a mutable variable",
        ExprKind::Perform { .. } => "an operation of an effect",
        ExprKind::Generate { .. } | ExprKind::Each { .. } | ExprKind::Yield(_) => "a sequence",
        ExprKind::Loop { .. } | ExprKind::ControlLoop { .. } | ExprKind::LoopExit { .. } => {
            "a loop"
        }
        ExprKind::List(_) => "a list",
        ExprKind::Record(_) | ExprKind::Update(..) | ExprKind::Field(..) => "a record",
        ExprKind::Await(_)
        | ExprKind::Task(_)
        | ExprKind::Handle(_)
        | ExprKind::Lift(..)
        | ExprKind::Within(..) => "a runtime computation",
        _ => "this form",
    }
}
