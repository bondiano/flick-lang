//! Compile-time evaluation of expressions built only from literals, as
//! `ReScript` does: `60 * 60 * 24`, `"a" ++ "b"`, `not false`. The result is the
//! value the expression has at run time, with JS semantics bit for bit:
//! IEEE 754 arithmetic and remainder, `SameValueZero` for `==` on `Number`,
//! UTF-16 order for `<` on `String`.
//!
//! `None` means "evaluate at run time": the expression is not made of literals
//! only, mixes types (the checker reports that), or its result is not certain
//! to match JS exactly (e.g. `**` with a non-integer result) or would throw
//! (`1n / 0n` is a run-time `RangeError`, a defect in Effect).

use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Number(f64),
    BigInt(i128),
    String(String),
    Bool(bool),
}

/// Largest integer below which every integer is an exact binary64.
const EXACT: f64 = 9_007_199_254_740_992.0;

pub fn eval(node: &Node, src: &str) -> Option<Const> {
    let mut nodes = node.nodes();
    match node.kind {
        K::Literal => literal_value(first_token(node)?, src),
        K::ParenExpr => eval(nodes.next()?, src),
        // a block whose only statement is an expression, e.g. a `let` body
        K::Block => {
            let only = nodes.next()?;
            (nodes.next().is_none() && only.kind != K::LetDecl && only.kind != K::Signature)
                .then(|| eval(only, src))?
        }
        K::PrefixExpr => prefix(first_token(node)?.kind, eval(nodes.next()?, src)?),
        K::BinExpr => {
            let lhs = eval(nodes.next()?, src)?;
            let rhs = eval(nodes.next()?, src)?;
            binary(operator(node)?, lhs, rhs)
        }
        K::IfExpr => {
            let Const::Bool(condition) = eval(nodes.next()?, src)? else {
                return None;
            };
            let (then, otherwise) = (nodes.next()?, nodes.next()?);
            eval(if condition { then } else { otherwise }, src)
        }
        _ => None,
    }
}

fn first_token(node: &Node) -> Option<Token> {
    node.children.iter().find_map(|c| match c {
        Element::Token(t) if !t.kind.is_trivia() => Some(*t),
        _ => None,
    })
}

/// The operator token of a binary expression (the token between its operands).
fn operator(node: &Node) -> Option<K> {
    node.children.iter().find_map(|c| match c {
        Element::Token(t) if t.kind.is_binary_op() => Some(t.kind),
        _ => None,
    })
}

fn literal_value(token: Token, src: &str) -> Option<Const> {
    let text = token.text(src);
    Some(match token.kind {
        K::Number => Const::Number(literal::number_value(text)?),
        K::BigInt => Const::BigInt(literal::bigint_value(text)?),
        K::String => Const::String(literal::string_value(text)),
        K::TrueKw => Const::Bool(true),
        K::FalseKw => Const::Bool(false),
        _ => return None,
    })
}

fn prefix(op: K, value: Const) -> Option<Const> {
    Some(match (op, value) {
        (K::Minus, Const::Number(x)) => Const::Number(-x),
        (K::Minus, Const::BigInt(x)) => Const::BigInt(x.checked_neg()?),
        (K::NotKw, Const::Bool(b)) => Const::Bool(!b),
        _ => return None,
    })
}

#[allow(clippy::float_cmp, reason = "JS compares numbers exactly")]
fn binary(op: K, lhs: Const, rhs: Const) -> Option<Const> {
    use Const::{BigInt, Bool, Number, String};
    Some(match (lhs, rhs) {
        (Number(a), Number(b)) => match op {
            K::Plus => Number(a + b),
            K::Minus => Number(a - b),
            K::Star => Number(a * b),
            K::Slash => Number(a / b),
            // Rust `%` on f64 is C `fmod`: exact, sign of the dividend, like JS
            K::Percent => Number(a % b),
            K::StarStar => Number(power(a, b)?),
            // SameValueZero: NaN equals NaN, +0 equals -0
            K::EqEq => Bool(a == b || (a.is_nan() && b.is_nan())),
            K::NotEq => Bool(!(a == b || (a.is_nan() && b.is_nan()))),
            K::Lt => Bool(a < b),
            K::Le => Bool(a <= b),
            K::Gt => Bool(a > b),
            K::Ge => Bool(a >= b),
            _ => return None,
        },
        (BigInt(a), BigInt(b)) => match op {
            K::Plus => BigInt(a.checked_add(b)?),
            K::Minus => BigInt(a.checked_sub(b)?),
            K::Star => BigInt(a.checked_mul(b)?),
            // truncating division and remainder with the dividend's sign, like JS
            K::Slash => BigInt(a.checked_div(b)?),
            K::Percent => BigInt(a.checked_rem(b)?),
            K::StarStar => BigInt(a.checked_pow(u32::try_from(b).ok()?)?),
            K::EqEq => Bool(a == b),
            K::NotEq => Bool(a != b),
            K::Lt => Bool(a < b),
            K::Le => Bool(a <= b),
            K::Gt => Bool(a > b),
            K::Ge => Bool(a >= b),
            _ => return None,
        },
        (String(a), String(b)) => {
            let order = || a.encode_utf16().cmp(b.encode_utf16());
            match op {
                K::PlusPlus => String(a.clone() + &b),
                K::EqEq => Bool(a == b),
                K::NotEq => Bool(a != b),
                K::Lt => Bool(order().is_lt()),
                K::Le => Bool(order().is_le()),
                K::Gt => Bool(order().is_gt()),
                K::Ge => Bool(order().is_ge()),
                _ => return None,
            }
        }
        (Bool(a), Bool(b)) => match op {
            K::AndAnd => Bool(a && b),
            K::OrOr => Bool(a || b),
            K::EqEq => Bool(a == b),
            K::NotEq => Bool(a != b),
            _ => return None,
        },
        _ => return None,
    })
}

/// `a ** b` only where the result is certain to equal JS `Math.pow`: a zero
/// exponent, or an integer power with an exactly representable result.
/// Engines' `pow` may differ in the last bit elsewhere.
#[allow(clippy::float_cmp, reason = "exactness is the point")]
fn power(base: f64, exponent: f64) -> Option<f64> {
    if exponent == 0.0 {
        return Some(1.0);
    }
    let integral = |x: f64| x.is_finite() && x.fract() == 0.0 && x.abs() < EXACT;
    if !integral(base) || !integral(exponent) || exponent < 0.0 {
        return None;
    }
    let odd = exponent % 2.0 == 1.0;
    match base {
        // keeps the sign of `-0` for odd exponents, like JS
        0.0 => return Some(if odd { base } else { 0.0 }),
        1.0 => return Some(1.0),
        -1.0 => return Some(if odd { -1.0 } else { 1.0 }),
        _ => {}
    }
    // |base| >= 2, so the loop leaves the exact range within 53 steps
    let mut result = 1.0_f64;
    let mut left = exponent;
    while left > 0.0 {
        result *= base;
        left -= 1.0;
        if result.abs() > EXACT {
            return None;
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests;
