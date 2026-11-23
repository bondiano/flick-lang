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

use crate::numeric::{self, Arith};

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Number(f64),
    Float32(f32),
    Int8(i8),
    Int16(i16),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    BigInt(i128),
    String(String),
    Bool(bool),
}

/// Largest integer below which every integer is an exact binary64.
pub const EXACT: f64 = 9_007_199_254_740_992.0;

/// Largest integer below which every integer is an exact binary32.
pub const EXACT_F32: f32 = 16_777_216.0;

pub fn prefix(op: K, value: Const) -> Option<Const> {
    Some(match (op, value) {
        (K::Minus, Const::Number(x)) => Const::Number(-x),
        (K::Minus, Const::Float32(x)) => Const::Float32(-x),
        (K::Minus, Const::Int8(x)) => Const::Int8(x.wrapping_neg()),
        (K::Minus, Const::Int16(x)) => Const::Int16(x.wrapping_neg()),
        (K::Minus, Const::Int64(x)) => Const::Int64(x.wrapping_neg()),
        (K::Minus, Const::UInt8(x)) => Const::UInt8(x.wrapping_neg()),
        (K::Minus, Const::UInt16(x)) => Const::UInt16(x.wrapping_neg()),
        (K::Minus, Const::UInt32(x)) => Const::UInt32(x.wrapping_neg()),
        (K::Minus, Const::UInt64(x)) => Const::UInt64(x.wrapping_neg()),
        (K::Minus, Const::BigInt(x)) => Const::BigInt(x.checked_neg()?),
        (K::NotKw, Const::Bool(b)) => Const::Bool(!b),
        _ => return None,
    })
}

fn arith_op(op: K) -> Option<Arith> {
    Some(match op {
        K::Plus => Arith::Add,
        K::Minus => Arith::Sub,
        K::Star => Arith::Mul,
        K::Slash => Arith::Div,
        K::Percent => Arith::Rem,
        K::StarStar => Arith::Pow,
        _ => return None,
    })
}

macro_rules! fixed_int_binary {
    ($op:expr, $a:expr, $b:expr, $variant:ident, $arith:ident) => {
        if let Some(arith) = arith_op($op) {
            Const::$variant(numeric::$arith(arith, $a, $b))
        } else {
            match $op {
                K::EqEq => Const::Bool($a == $b),
                K::NotEq => Const::Bool($a != $b),
                K::Lt => Const::Bool($a < $b),
                K::Le => Const::Bool($a <= $b),
                K::Gt => Const::Bool($a > $b),
                K::Ge => Const::Bool($a >= $b),
                _ => return None,
            }
        }
    };
}

#[allow(clippy::float_cmp, reason = "JS compares numbers exactly")]
#[allow(
    clippy::too_many_lines,
    reason = "exhaustive binary dispatch across fixed-width numeric variants"
)]
pub fn binary(op: K, lhs: Const, rhs: Const) -> Option<Const> {
    use Const::{
        BigInt, Bool, Float32, Int8, Int16, Int64, Number, String, UInt8, UInt16, UInt32, UInt64,
    };
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
        (Float32(a), Float32(b)) => match op {
            K::Plus => Float32(a + b),
            K::Minus => Float32(a - b),
            K::Star => Float32(a * b),
            K::Slash => Float32(a / b),
            K::Percent => Float32(a % b),
            K::StarStar => Float32(power_f32(a, b)?),
            K::EqEq => Bool(a == b || (a.is_nan() && b.is_nan())),
            K::NotEq => Bool(!(a == b || (a.is_nan() && b.is_nan()))),
            K::Lt => Bool(a < b),
            K::Le => Bool(a <= b),
            K::Gt => Bool(a > b),
            K::Ge => Bool(a >= b),
            _ => return None,
        },
        (Int8(a), Int8(b)) => fixed_int_binary!(op, a, b, Int8, int8_arith),
        (Int16(a), Int16(b)) => fixed_int_binary!(op, a, b, Int16, int16_arith),
        (Int64(a), Int64(b)) => fixed_int_binary!(op, a, b, Int64, int64_arith),
        (UInt8(a), UInt8(b)) => fixed_int_binary!(op, a, b, UInt8, uint8_arith),
        (UInt16(a), UInt16(b)) => fixed_int_binary!(op, a, b, UInt16, uint16_arith),
        (UInt32(a), UInt32(b)) => fixed_int_binary!(op, a, b, UInt32, uint32_arith),
        (UInt64(a), UInt64(b)) => fixed_int_binary!(op, a, b, UInt64, uint64_arith),
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

/// `a ** b` on `Float32` only where the result is an exact binary32 integer
/// power (matching `Math.fround(a ** b)` and `float32(PowFloat(a, b))`).
#[allow(clippy::float_cmp, reason = "exactness is the point")]
fn power_f32(base: f32, exponent: f32) -> Option<f32> {
    if exponent == 0.0 {
        return Some(1.0);
    }
    let integral = |x: f32| x.is_finite() && x.fract() == 0.0 && x.abs() < EXACT_F32;
    if !integral(base) || !integral(exponent) || exponent < 0.0 {
        return None;
    }
    let odd = exponent % 2.0 == 1.0;
    match base {
        0.0 => return Some(if odd { base } else { 0.0 }),
        1.0 => return Some(1.0),
        -1.0 => return Some(if odd { -1.0 } else { 1.0 }),
        _ => {}
    }
    let mut result = 1.0_f32;
    let mut left = exponent;
    while left > 0.0 {
        result *= base;
        left -= 1.0;
        if result.abs() > EXACT_F32 {
            return None;
        }
    }
    Some(result)
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
