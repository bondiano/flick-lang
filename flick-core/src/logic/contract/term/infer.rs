//! Primitive type hint inference for checked IR expressions during extraction.

use std::rc::Rc;

use flick_ir::{BinOp, Expr, ExprKind, ItemKind, Lit, Num, Pat as IrPat, Stage, Stmt};

use crate::logic::Program;

use super::{LangTypes, PrimHint};

pub(super) const INFER_DEPTH: usize = 4;

/// Infers the primitive type of `expr` when it can be determined positively
/// from `expr.ty`, syntactic IR form, `env`, or pure `program` definitions.
pub(super) fn infer_prim(
    expr: &Expr,
    env: &[(Rc<str>, Option<PrimHint>)],
    program: &dyn Program,
    lang: &LangTypes,
    depth: usize,
) -> Option<PrimHint> {
    if let Some(ty) = &expr.ty {
        return Some(lang.classify_ty(ty));
    }
    match &expr.kind {
        ExprKind::Lit(Lit::Int(_))
        | ExprKind::Binary(BinOp::Arith(_, Num::Int), ..)
        | ExprKind::Neg(Num::Int, _) => Some(PrimHint::Int),
        ExprKind::Lit(Lit::Bool(_))
        | ExprKind::Not(_)
        | ExprKind::Binary(
            BinOp::And
            | BinOp::Or
            | BinOp::Eq
            | BinOp::NotEq
            | BinOp::Same
            | BinOp::NotSame
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge,
            ..,
        ) => Some(PrimHint::Bool),
        ExprKind::Lit(Lit::Float(_))
        | ExprKind::Binary(BinOp::Arith(_, Num::Float), ..)
        | ExprKind::Neg(Num::Float, _) => Some(PrimHint::Float),
        ExprKind::Lit(Lit::BigInt(_))
        | ExprKind::Binary(BinOp::Arith(_, Num::BigInt), ..)
        | ExprKind::Neg(Num::BigInt, _) => Some(PrimHint::BigInt),
        ExprKind::Lit(Lit::String(_)) | ExprKind::Binary(BinOp::Concat, ..) => {
            Some(PrimHint::String)
        }
        ExprKind::Lit(Lit::Unit) => Some(PrimHint::Unit),
        ExprKind::Binary(BinOp::ListConcat, ..)
        | ExprKind::List(_)
        | ExprKind::Tuple(_)
        | ExprKind::Record(_)
        | ExprKind::Con(_) => Some(PrimHint::Other),
        ExprKind::Local(name) => env
            .iter()
            .rev()
            .find(|(bound, _)| &**bound == name.as_str())
            .and_then(|(_, hint)| *hint),
        ExprKind::If(_, then_branch, else_branch) => {
            let then_hint = infer_prim(then_branch, env, program, lang, depth);
            let else_hint = infer_prim(else_branch, env, program, lang, depth);
            match (then_hint, else_hint) {
                (Some(left), Some(right)) if left == right => Some(left),
                (Some(hint), None) | (None, Some(hint)) => Some(hint),
                _ => None,
            }
        }
        ExprKind::Block(stmts, res) => infer_block_prim(stmts, res, env, program, lang, depth),
        ExprKind::Match(scrutinee, arms) => {
            infer_match_prim(scrutinee, arms, env, program, lang, depth)
        }
        ExprKind::Global { module, name, .. } if depth > 0 => {
            let item = program.item(module, name)?;
            let ItemKind::Value { dicts, body, .. } = &item.kind else {
                return None;
            };
            if !dicts.is_empty() || matches!(body.kind, ExprKind::Lambda { .. }) {
                return None;
            }
            infer_prim(body, &[], program, lang, depth - 1)
        }
        ExprKind::App { .. } if depth > 0 => infer_app_prim(expr, env, program, lang, depth),
        _ => None,
    }
}

fn infer_block_prim(
    stmts: &[Stmt],
    res: &Expr,
    env: &[(Rc<str>, Option<PrimHint>)],
    program: &dyn Program,
    lang: &LangTypes,
    depth: usize,
) -> Option<PrimHint> {
    let mut local = env.to_vec();
    for stmt in stmts {
        match stmt {
            Stmt::Let {
                pat: IrPat::Bind(name),
                value: val,
                ..
            } => {
                let hint = infer_prim(val, &local, program, lang, depth);
                local.push((name.as_str().into(), hint));
            }
            Stmt::Fun { name, .. } => {
                local.push((name.as_str().into(), Some(PrimHint::Other)));
            }
            _ => {}
        }
    }
    infer_prim(res, &local, program, lang, depth)
}

fn infer_match_prim(
    scrutinee: &Expr,
    arms: &[flick_ir::Arm],
    env: &[(Rc<str>, Option<PrimHint>)],
    program: &dyn Program,
    lang: &LangTypes,
    depth: usize,
) -> Option<PrimHint> {
    let scrut_hint = infer_prim(scrutinee, env, program, lang, depth);
    let mut found: Option<PrimHint> = None;
    for arm in arms {
        let mut local = env.to_vec();
        match &arm.pat {
            IrPat::Bind(name) => local.push((name.as_str().into(), scrut_hint)),
            other => {
                let mut binders = Vec::new();
                other.binders(&mut binders);
                for binder in binders {
                    local.push((binder.into(), None));
                }
            }
        }
        if let Some(hint) = infer_prim(&arm.body, &local, program, lang, depth) {
            match found {
                None => found = Some(hint),
                Some(prev) if prev == hint => {}
                Some(_) => return None,
            }
        }
    }
    found
}

fn infer_app_prim(
    e: &Expr,
    env: &[(Rc<str>, Option<PrimHint>)],
    program: &dyn Program,
    lang: &LangTypes,
    depth: usize,
) -> Option<PrimHint> {
    let mut args = Vec::new();
    let mut head = e;
    while let ExprKind::App {
        func,
        arg,
        stage: Stage::Pure,
        ..
    } = &head.kind
    {
        args.push(arg.as_ref());
        head = func;
    }
    args.reverse();
    match &head.kind {
        ExprKind::Global { module, name, .. } => {
            let item = program.item(module, name)?;
            let ItemKind::Value { dicts, body, .. } = &item.kind else {
                return None;
            };
            if !dicts.is_empty() {
                return None;
            }
            let mut callee_env = Vec::new();
            let mut cursor = body;
            let mut arg_idx = 0;
            while let ExprKind::Lambda { params, body: next } = &cursor.kind {
                if arg_idx + params.len() > args.len() {
                    return None;
                }
                for param in params {
                    let arg = args[arg_idx];
                    arg_idx += 1;
                    if let IrPat::Bind(p) = &param.pat {
                        let hint = param
                            .ty
                            .as_ref()
                            .map(|t| lang.classify_ty(t))
                            .or_else(|| infer_prim(arg, env, program, lang, depth - 1));
                        callee_env.push((p.as_str().into(), hint));
                    }
                }
                cursor = next;
            }
            if arg_idx == args.len() {
                infer_prim(cursor, &callee_env, program, lang, depth - 1)
            } else {
                None
            }
        }
        ExprKind::Method { dict, name, .. } => {
            if let flick_ir::Dict::Instance {
                module,
                name: instance,
                args: dict_args,
            } = dict
                && dict_args.is_empty()
                && lang.is_numeric_int_instance(module, instance)
            {
                match (name.as_str(), args.len()) {
                    ("add" | "sub", 2) | ("negate" | "from_int", 1) => Some(PrimHint::Int),
                    ("lt" | "le" | "gt" | "ge", 2) => Some(PrimHint::Bool),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}
