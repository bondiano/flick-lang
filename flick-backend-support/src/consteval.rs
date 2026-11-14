//! Exact literal operations shared by frontend facts and backend folding.

#[derive(Clone, Copy)]
pub enum Op {
    Minus,
    NotKw,
    Plus,
    Star,
    Slash,
    Percent,
    StarStar,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    PlusPlus,
    AndAnd,
    OrOr,
}
use Op as K;

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Number(f64),
    BigInt(i128),
    String(String),
    Bool(bool),
}

/// Largest integer below which every integer is an exact binary64.
pub const EXACT: f64 = 9_007_199_254_740_992.0;

pub fn prefix(op: K, value: Const) -> Option<Const> {
    Some(match (op, value) {
        (K::Minus, Const::Number(x)) => Const::Number(-x),
        (K::Minus, Const::BigInt(x)) => Const::BigInt(x.checked_neg()?),
        (K::NotKw, Const::Bool(b)) => Const::Bool(!b),
        _ => return None,
    })
}

#[allow(clippy::float_cmp, reason = "JS compares numbers exactly")]
pub fn binary(op: K, lhs: Const, rhs: Const) -> Option<Const> {
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
            // truncating division and remainder with the dividend's sign, like
            // JS; by zero they are zero, as the operators define
            K::Slash | K::Percent if b == 0 => BigInt(0),
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
