//! Literal expressions evaluated when the program compiles, with the JS
//! semantics of [`consteval`](crate::consteval) bit for bit, and branches of
//! constant conditions chosen. A result that is not certain to match JS, or
//! that JS would throw, stays a run-time computation.

use crate::consteval::{self, Const};
use crate::ir::{BinOp, Expr, ExprKind, Lit};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;

/// Folds `e` itself, its operands already folded; whether it changed.
pub fn fold(e: &mut Expr) -> bool {
    let folded = match &e.kind {
        ExprKind::Binary(op, l, r) => token(*op)
            .zip(constant(l).zip(constant(r)))
            .and_then(|(op, (l, r))| consteval::binary(op, l, r)),
        ExprKind::Not(inner) => constant(inner).and_then(|c| consteval::prefix(K::NotKw, c)),
        ExprKind::If(c, t, f) => match constant(c) {
            Some(Const::Bool(true)) => {
                *e = (**t).clone();
                return true;
            }
            Some(Const::Bool(false)) => {
                *e = (**f).clone();
                return true;
            }
            _ => None,
        },
        _ => None,
    };
    match folded.and_then(|c| expr(&c, e)) {
        Some(value) => {
            *e = value;
            true
        }
        None => false,
    }
}

fn token(op: BinOp) -> Option<K> {
    Some(match op {
        BinOp::Or => K::OrOr,
        BinOp::And => K::AndAnd,
        BinOp::Eq | BinOp::Same => K::EqEq,
        BinOp::NotEq | BinOp::NotSame => K::NotEq,
        BinOp::Lt => K::Lt,
        BinOp::Le => K::Le,
        BinOp::Gt => K::Gt,
        BinOp::Ge => K::Ge,
        BinOp::Concat => K::PlusPlus,
        BinOp::Add => K::Plus,
        BinOp::Sub => K::Minus,
        BinOp::Mul => K::Star,
        BinOp::Div => K::Slash,
        BinOp::Rem => K::Percent,
        BinOp::Pow => K::StarStar,
        BinOp::ListConcat => return None,
    })
}

/// The value of a literal, or of a negated number literal.
pub fn constant(e: &Expr) -> Option<Const> {
    match &e.kind {
        ExprKind::Lit(Lit::Number(n)) => literal::number_value(n).map(Const::Number),
        ExprKind::Lit(Lit::BigInt(n)) => literal::bigint_value(n).map(Const::BigInt),
        ExprKind::Lit(Lit::String(s)) => Some(Const::String(s.clone())),
        ExprKind::Lit(Lit::Bool(b)) => Some(Const::Bool(*b)),
        ExprKind::Neg(inner) => consteval::prefix(K::Minus, constant(inner)?),
        _ => None,
    }
}

/// `c` as an expression at the place of `at`: a literal, negated when the
/// value is negative (`-0` included), so the text stays a JS literal.
fn expr(c: &Const, at: &Expr) -> Option<Expr> {
    let lit = |l: Lit| Expr::new(ExprKind::Lit(l), at.span);
    let negated = |l: Lit| Expr::new(ExprKind::Neg(Box::new(lit(l))), at.span);
    Some(match c {
        Const::Number(x) if !x.is_finite() => return None,
        Const::Number(x) if x.is_sign_negative() => negated(Lit::Number(number_text(-x))),
        Const::Number(x) => lit(Lit::Number(number_text(*x))),
        Const::BigInt(x) if *x < 0 => negated(Lit::BigInt(format!("{}n", x.checked_neg()?))),
        Const::BigInt(x) => lit(Lit::BigInt(format!("{x}n"))),
        Const::String(s) => lit(Lit::String(s.clone())),
        Const::Bool(b) => lit(Lit::Bool(*b)),
    })
}

/// The shorter of the plain and the exponent spelling; both read back as
/// exactly `x`.
fn number_text(x: f64) -> String {
    let plain = format!("{x}");
    let exponent = format!("{x:e}");
    if exponent.len() < plain.len() {
        exponent
    } else {
        plain
    }
}
