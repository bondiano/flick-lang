//! Literal expressions evaluated when the program compiles, with the JS
//! semantics of [`consteval`](crate::consteval) bit for bit, and branches of
//! constant conditions chosen. A result that is not certain to match JS, or
//! that JS would throw, stays a run-time computation.

use std::fmt::Write as _;

use crate::consteval::{self, Const};
use crate::ir::{BinOp, Expr, ExprKind, Lit};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::Dict;

/// Folds `e` itself, its operands already folded; whether it changed.
pub fn fold(e: &mut Expr) -> bool {
    let folded = match &e.kind {
        ExprKind::Binary(op, l, r) => token(*op)
            .zip(constant(l).zip(constant(r)))
            .and_then(|(op, (l, r))| consteval::binary(op, l, r)),
        ExprKind::Not(inner) => constant(inner).and_then(|c| consteval::prefix(K::NotKw, c)),
        ExprKind::App { func, arg, .. } => match &func.kind {
            ExprKind::Method {
                dict: Dict::Instance { module, name, args },
                name: method,
            } if module == "Instances" && args.is_empty() && method == "show" => {
                constant(arg).and_then(|c| show(name, c)).map(Const::String)
            }
            _ => None,
        },
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

/// `show` of a built-in instance as the runtime `show` spells it.
fn show(instance: &str, c: Const) -> Option<String> {
    Some(match (instance, c) {
        // shortcut: integers only; Rust and JS spell other numbers differently (`1e21`)
        ("Show Number", Const::Number(x)) if x.fract() == 0.0 && x.abs() < consteval::EXACT => {
            if x == 0.0 && x.is_sign_negative() {
                "-0".to_owned()
            } else {
                format!("{x}")
            }
        }
        ("Show BigInt", Const::BigInt(x)) => format!("{x}n"),
        ("Show Bool", Const::Bool(b)) => b.to_string(),
        ("Show String", Const::String(s)) => json_string(&s),
        _ => return None,
    })
}

/// `JSON.stringify` of a string.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
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
