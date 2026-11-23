//! The operators of Γ₀, as the runtimes compute them: `Int` wraps around
//! (`flick_core::numeric::int_arith`), `Float` is IEEE, `==` is structural
//! with `SameValueZero` on numbers, strings order by UTF-16 code units.

use num_bigint::{BigInt, Sign};
use std::cmp::Ordering;

use flick_core::numeric::{
    Arith, Num, int_arith, int8_arith, int16_arith, int64_arith, uint8_arith, uint16_arith,
    uint32_arith, uint64_arith,
};

use crate::subst::lit_eq;
use crate::syntax::{Head, Lit, Prim, Term};

pub fn run(p: Prim, args: &[Term]) -> Result<Term, String> {
    let lit = |t: Lit| Ok(Term::Lit(t));
    match (p, args) {
        (Prim::Arith(op, Num::Int), [Term::Lit(Lit::Int(a)), Term::Lit(Lit::Int(b))]) => {
            lit(Lit::Int(int_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::Int8), [Term::Lit(Lit::Int8(a)), Term::Lit(Lit::Int8(b))]) => {
            lit(Lit::Int8(int8_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::Int16), [Term::Lit(Lit::Int16(a)), Term::Lit(Lit::Int16(b))]) => {
            lit(Lit::Int16(int16_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::Int64), [Term::Lit(Lit::Int64(a)), Term::Lit(Lit::Int64(b))]) => {
            lit(Lit::Int64(int64_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::UInt8), [Term::Lit(Lit::UInt8(a)), Term::Lit(Lit::UInt8(b))]) => {
            lit(Lit::UInt8(uint8_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::UInt16), [Term::Lit(Lit::UInt16(a)), Term::Lit(Lit::UInt16(b))]) => {
            lit(Lit::UInt16(uint16_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::UInt32), [Term::Lit(Lit::UInt32(a)), Term::Lit(Lit::UInt32(b))]) => {
            lit(Lit::UInt32(uint32_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::UInt64), [Term::Lit(Lit::UInt64(a)), Term::Lit(Lit::UInt64(b))]) => {
            lit(Lit::UInt64(uint64_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::Float), [Term::Lit(Lit::Float(a)), Term::Lit(Lit::Float(b))]) => {
            lit(Lit::Float(float_arith(op, *a, *b)))
        }
        (
            Prim::Arith(op, Num::Float32),
            [Term::Lit(Lit::Float32(a)), Term::Lit(Lit::Float32(b))],
        ) => lit(Lit::Float32(float32_arith(op, *a, *b))),
        (Prim::Arith(op, Num::BigInt), [Term::Lit(Lit::BigInt(a)), Term::Lit(Lit::BigInt(b))]) => {
            Ok(big_arith(op, a, b)
                .map_or_else(|why| Term::Die(why.into()), |n| Term::Lit(Lit::BigInt(n))))
        }
        (Prim::Neg(Num::Int), [Term::Lit(Lit::Int(a))]) => lit(Lit::Int(a.wrapping_neg())),
        (Prim::Neg(Num::Int8), [Term::Lit(Lit::Int8(a))]) => lit(Lit::Int8(a.wrapping_neg())),
        (Prim::Neg(Num::Int16), [Term::Lit(Lit::Int16(a))]) => lit(Lit::Int16(a.wrapping_neg())),
        (Prim::Neg(Num::Int64), [Term::Lit(Lit::Int64(a))]) => lit(Lit::Int64(a.wrapping_neg())),
        (Prim::Neg(Num::UInt8), [Term::Lit(Lit::UInt8(a))]) => lit(Lit::UInt8(a.wrapping_neg())),
        (Prim::Neg(Num::UInt16), [Term::Lit(Lit::UInt16(a))]) => lit(Lit::UInt16(a.wrapping_neg())),
        (Prim::Neg(Num::UInt32), [Term::Lit(Lit::UInt32(a))]) => lit(Lit::UInt32(a.wrapping_neg())),
        (Prim::Neg(Num::UInt64), [Term::Lit(Lit::UInt64(a))]) => lit(Lit::UInt64(a.wrapping_neg())),
        (Prim::Neg(Num::Float), [Term::Lit(Lit::Float(a))]) => lit(Lit::Float(-a)),
        (Prim::Neg(Num::Float32), [Term::Lit(Lit::Float32(a))]) => lit(Lit::Float32(-a)),
        (Prim::Neg(Num::BigInt), [Term::Lit(Lit::BigInt(a))]) => lit(Lit::BigInt(-a)),
        (Prim::Eq | Prim::NotEq, [a, b]) => match equal(a, b) {
            Some(same) => lit(Lit::Bool(same == (p == Prim::Eq))),
            None => Err(format!("`==` of {a:?} and {b:?}")),
        },
        (Prim::Same | Prim::NotSame, [Term::Lit(a), Term::Lit(b)]) => {
            lit(Lit::Bool(lit_eq(a, b) == (p == Prim::Same)))
        }
        // the values of an enum, which the builds compare as their literals
        (
            Prim::Same | Prim::NotSame,
            [Term::Apply(Head::Con(a), x), Term::Apply(Head::Con(b), y)],
        ) if x.is_empty() && y.is_empty() && a.ty == b.ty => {
            lit(Lit::Bool((a.name == b.name) == (p == Prim::Same)))
        }
        (Prim::Lt | Prim::Le | Prim::Gt | Prim::Ge, [Term::Lit(a), Term::Lit(b)]) => {
            let order = order(a, b).ok_or_else(|| format!("`<` of {a:?} and {b:?}"))?;
            lit(Lit::Bool(match (p, order) {
                (_, None) => false,
                (Prim::Lt, Some(o)) => o == Ordering::Less,
                (Prim::Le, Some(o)) => o != Ordering::Greater,
                (Prim::Gt, Some(o)) => o == Ordering::Greater,
                (_, Some(o)) => o != Ordering::Less,
            }))
        }
        (Prim::Not, [Term::Lit(Lit::Bool(b))]) => lit(Lit::Bool(!b)),
        (Prim::Concat, [Term::Lit(Lit::String(a)), Term::Lit(Lit::String(b))]) => {
            lit(Lit::String(format!("{a}{b}").into()))
        }
        (Prim::ListConcat, [Term::List(a), Term::List(b)]) => {
            Ok(Term::List(a.iter().chain(b.iter()).cloned().collect()))
        }
        (Prim::TagOf, [Term::Apply(Head::Con(c), _)]) => {
            lit(Lit::String(format!("{}.{}", c.ty, c.name).into()))
        }
        (Prim::Atom, [Term::Lit(Lit::String(s))]) => lit(Lit::String(atom(s).into())),
        _ => Err(format!("{p:?} of {args:?}")),
    }
}

fn float_arith(op: Arith, a: f64, b: f64) -> f64 {
    match op {
        Arith::Add => a + b,
        Arith::Sub => a - b,
        Arith::Mul => a * b,
        Arith::Div => a / b,
        // JS `%` keeps the sign of the dividend, as Rust's does
        Arith::Rem => a % b,
        Arith::Pow if b.is_nan() || (a.abs().to_bits() == 1.0_f64.to_bits() && b.is_infinite()) => {
            f64::NAN
        }
        Arith::Pow => a.powf(b),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "binary32 pow rounds f64::powf once to f32, matching JS Math.fround and Go"
)]
fn float32_arith(op: Arith, a: f32, b: f32) -> f32 {
    match op {
        Arith::Add => a + b,
        Arith::Sub => a - b,
        Arith::Mul => a * b,
        Arith::Div => a / b,
        Arith::Rem => a % b,
        Arith::Pow if b.is_nan() || (a.abs().to_bits() == 1.0_f32.to_bits() && b.is_infinite()) => {
            f32::NAN
        }
        Arith::Pow => f64::from(a).powf(f64::from(b)) as f32,
    }
}

fn big_arith(op: Arith, a: &BigInt, b: &BigInt) -> Result<BigInt, String> {
    Ok(match op {
        Arith::Add => a + b,
        Arith::Sub => a - b,
        Arith::Mul => a * b,
        Arith::Div | Arith::Rem if *b == BigInt::ZERO => BigInt::ZERO,
        Arith::Div => a / b,
        Arith::Rem => a % b,
        Arith::Pow => {
            if b.sign() == Sign::Minus {
                return Err("Exponent must be non-negative".into());
            }
            let (mut base, mut power) = (a.clone(), b.clone());
            let mut result = BigInt::from(1u8);
            while power != BigInt::ZERO {
                if (&power & BigInt::from(1u8)) != BigInt::ZERO {
                    result *= &base;
                }
                power >>= 1u8;
                if power != BigInt::ZERO {
                    base = &base * &base;
                }
            }
            result
        }
    })
}

/// `<` as JS orders primitives: `None` inside when a `NaN` makes every
/// comparison false; `None` outside for values `<` does not compare.
#[expect(
    clippy::option_option,
    reason = "the two `None`s mean different things, as above"
)]
fn order(a: &Lit, b: &Lit) -> Option<Option<Ordering>> {
    Some(match (a, b) {
        (Lit::Int(x), Lit::Int(y)) => Some(x.cmp(y)),
        (Lit::Int8(x), Lit::Int8(y)) => Some(x.cmp(y)),
        (Lit::Int16(x), Lit::Int16(y)) => Some(x.cmp(y)),
        (Lit::Int64(x), Lit::Int64(y)) => Some(x.cmp(y)),
        (Lit::UInt8(x), Lit::UInt8(y)) => Some(x.cmp(y)),
        (Lit::UInt16(x), Lit::UInt16(y)) => Some(x.cmp(y)),
        (Lit::UInt32(x), Lit::UInt32(y)) => Some(x.cmp(y)),
        (Lit::UInt64(x), Lit::UInt64(y)) => Some(x.cmp(y)),
        (Lit::BigInt(x), Lit::BigInt(y)) => Some(x.cmp(y)),
        (Lit::Float(x), Lit::Float(y)) => x.partial_cmp(y),
        (Lit::Float32(x), Lit::Float32(y)) => x.partial_cmp(y),
        (Lit::String(x), Lit::String(y)) => Some(x.encode_utf16().cmp(y.encode_utf16())),
        (Lit::Bool(x), Lit::Bool(y)) => Some(x.cmp(y)),
        _ => return None,
    })
}

/// Structural equality of two values; `None` for values without it.
#[expect(
    clippy::float_cmp,
    reason = "`==` on `Float` and `Float32` is `SameValueZero`, exact"
)]
pub fn equal(a: &Term, b: &Term) -> Option<bool> {
    Some(match (a, b) {
        (Term::Lit(Lit::Float(x)), Term::Lit(Lit::Float(y))) => {
            x == y || (x.is_nan() && y.is_nan())
        }
        (Term::Lit(Lit::Float32(x)), Term::Lit(Lit::Float32(y))) => {
            x == y || (x.is_nan() && y.is_nan())
        }
        (Term::Lit(x), Term::Lit(y)) => x == y,
        // the dictionaries a value of a GADT holds are not compared
        (Term::Apply(Head::Con(c), xs), Term::Apply(Head::Con(d), ys)) => {
            let held = c.dicts.min(xs.len()).min(ys.len());
            c == d && all_equal(&xs[held..], &ys[held..])?
        }
        (Term::Tuple(xs), Term::Tuple(ys)) | (Term::List(xs), Term::List(ys)) => {
            xs.len() == ys.len() && all_equal(xs, ys)?
        }
        (Term::Record(xs), Term::Record(ys)) => {
            xs.len() == ys.len()
                && xs.iter().try_fold(true, |acc, (l, x)| {
                    let (_, y) = ys.iter().find(|(m, _)| m == l)?;
                    Some(acc && equal(x, y)?)
                })?
        }
        (Term::Loc(x), Term::Loc(y)) => x == y,
        _ => return None,
    })
}

fn all_equal(xs: &[Term], ys: &[Term]) -> Option<bool> {
    xs.iter()
        .zip(ys)
        .try_fold(true, |acc, (x, y)| Some(acc && equal(x, y)?))
}

/// `atom` of `runtime/src/core/data.ts`: a shown argument of a constructor
/// in parentheses when it has a space and is not already bracketed.
pub fn atom(text: &str) -> String {
    let bracketed = text.starts_with(['[', '(', '{', '"']);
    if bracketed || !text.contains(' ') {
        text.to_owned()
    } else {
        format!("({text})")
    }
}

#[cfg(test)]
mod numeric_tests {
    use super::*;

    fn big(n: &str) -> BigInt {
        n.parse().expect("integer test oracle")
    }

    #[test]
    fn bigint_arithmetic_keeps_precision_and_zero_division_policy() {
        assert_eq!(
            big_arith(
                Arith::Mul,
                &big("18446744073709551616"),
                &big("18446744073709551616")
            )
            .expect("product"),
            big("340282366920938463463374607431768211456")
        );
        assert_eq!(
            big_arith(Arith::Div, &big("-7"), &big("2")).expect("quotient"),
            big("-3")
        );
        assert_eq!(
            big_arith(Arith::Rem, &big("-7"), &big("2")).expect("remainder"),
            big("-1")
        );
        assert_eq!(
            big_arith(Arith::Div, &big("7"), &big("0")).expect("zero quotient"),
            big("0")
        );
        assert_eq!(
            big_arith(Arith::Rem, &big("7"), &big("0")).expect("zero remainder"),
            big("0")
        );
        assert_eq!(
            big_arith(Arith::Pow, &big("2"), &big("128")).expect("power"),
            big("340282366920938463463374607431768211456")
        );
        assert_eq!(
            big_arith(Arith::Pow, &big("0"), &big("0")).expect("zero power"),
            big("1")
        );
        assert!(big_arith(Arith::Pow, &big("2"), &big("-1")).is_err());
    }

    #[test]
    fn float_powers_preserve_the_nan_boundaries() {
        assert!(float_arith(Arith::Pow, 1.0, f64::INFINITY).is_nan());
        assert!(float_arith(Arith::Pow, -1.0, f64::NEG_INFINITY).is_nan());
        assert!(float_arith(Arith::Pow, 1.0, f64::NAN).is_nan());
        assert_eq!(
            float_arith(Arith::Pow, f64::NAN, 0.0).to_bits(),
            1.0_f64.to_bits()
        );
    }
}
