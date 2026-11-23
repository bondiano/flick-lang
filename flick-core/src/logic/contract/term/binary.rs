//! Binary operator extraction and primitive operand type confirmation.

use std::rc::Rc;

use flick_ir::{Arith, BinOp, Expr, ExprKind, Num};

use crate::logic::{Program, Refused};
use crate::source::Span;

use super::extract::{extract_in, refuse};
use super::infer::{INFER_DEPTH, infer_prim};
use super::{IntBinOp, IntCmpOp, LangTypes, PrimHint, Term, TermKind};

pub(super) fn extract_binary(
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
    span: Span,
    env: &mut Vec<(Rc<str>, Option<PrimHint>)>,
    program: &dyn Program,
    lang: &LangTypes,
) -> Result<Term, Refused> {
    let kind = match op {
        BinOp::Arith(Arith::Add, Num::Int) => {
            let l = extract_in(lhs, env, program, lang)?;
            let r = extract_in(rhs, env, program, lang)?;
            TermKind::IntBin(IntBinOp::Add, Rc::new(l), Rc::new(r))
        }
        BinOp::Arith(Arith::Sub, Num::Int) => {
            let l = extract_in(lhs, env, program, lang)?;
            let r = extract_in(rhs, env, program, lang)?;
            TermKind::IntBin(IntBinOp::Sub, Rc::new(l), Rc::new(r))
        }
        BinOp::Arith(op @ (Arith::Mul | Arith::Div | Arith::Rem | Arith::Pow), _) => {
            return refuse(span, format!("`{}` is outside the domain", op.symbol()));
        }
        BinOp::Arith(_, Num::Float) => {
            return refuse(span, "`Float` arithmetic is outside the domain");
        }
        BinOp::Arith(_, Num::BigInt) => {
            return refuse(span, "`BigInt` arithmetic is outside the domain");
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let tl = infer_prim(lhs, env, program, lang, INFER_DEPTH);
            let tr = infer_prim(rhs, env, program, lang, INFER_DEPTH);
            if !confirmed_prim(tl, tr, PrimHint::Int) {
                return refuse(
                    span,
                    format!(
                        "`{}` on non-`Int` operands is outside the domain",
                        op.symbol()
                    ),
                );
            }
            let l = extract_in(lhs, env, program, lang)?;
            let r = extract_in(rhs, env, program, lang)?;
            let cmp = match op {
                BinOp::Lt => IntCmpOp::Lt,
                BinOp::Le => IntCmpOp::Le,
                BinOp::Gt => IntCmpOp::Gt,
                BinOp::Ge => IntCmpOp::Ge,
                _ => unreachable!(),
            };
            TermKind::IntCmp(cmp, Rc::new(l), Rc::new(r))
        }
        BinOp::Same | BinOp::NotSame => {
            let eq = op == BinOp::Same;
            if let (ExprKind::Field(l_inner, lf), ExprKind::Field(r_inner, rf)) =
                (&lhs.kind, &rhs.kind)
                && lf == "_tag"
                && rf == "_tag"
                && lhs.ty.is_none()
                && rhs.ty.is_none()
            {
                let l = extract_in(l_inner, env, program, lang)?;
                let r = extract_in(r_inner, env, program, lang)?;
                TermKind::TagEq {
                    eq,
                    lhs: Rc::new(l),
                    rhs: Rc::new(r),
                }
            } else {
                let tl = infer_prim(lhs, env, program, lang, INFER_DEPTH);
                let tr = infer_prim(rhs, env, program, lang, INFER_DEPTH);
                if confirmed_prim(tl, tr, PrimHint::Int) {
                    let l = extract_in(lhs, env, program, lang)?;
                    let r = extract_in(rhs, env, program, lang)?;
                    let cmp = if eq { IntCmpOp::Eq } else { IntCmpOp::Ne };
                    TermKind::IntCmp(cmp, Rc::new(l), Rc::new(r))
                } else if confirmed_prim(tl, tr, PrimHint::Bool) {
                    let l = extract_in(lhs, env, program, lang)?;
                    let r = extract_in(rhs, env, program, lang)?;
                    TermKind::BoolEq {
                        eq,
                        lhs: Rc::new(l),
                        rhs: Rc::new(r),
                    }
                } else {
                    let sym = if eq { "==" } else { "!=" };
                    return refuse(span, format!("`{sym}` on this type is outside the domain"));
                }
            }
        }
        BinOp::Eq | BinOp::NotEq => {
            let sym = if op == BinOp::Eq { "==" } else { "!=" };
            return refuse(span, format!("`{sym}` on this type is outside the domain"));
        }
        BinOp::And => {
            let l = extract_in(lhs, env, program, lang)?;
            let r = extract_in(rhs, env, program, lang)?;
            TermKind::And(Rc::new(l), Rc::new(r))
        }
        BinOp::Or => {
            let l = extract_in(lhs, env, program, lang)?;
            let r = extract_in(rhs, env, program, lang)?;
            TermKind::Or(Rc::new(l), Rc::new(r))
        }
        BinOp::Concat => return refuse(span, "`++` on `String` is outside the domain"),
        BinOp::ListConcat => return refuse(span, "`++` on `List` is outside the domain"),
    };
    Ok(Term::new(kind, span))
}

fn confirmed_prim(tl: Option<PrimHint>, tr: Option<PrimHint>, expected: PrimHint) -> bool {
    match (tl, tr) {
        (Some(a), Some(b)) => a == expected && b == expected,
        (Some(a), None) | (None, Some(a)) => a == expected,
        (None, None) => false,
    }
}
