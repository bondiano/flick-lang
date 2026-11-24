//! The numeric coercions and fixed-width helpers the standard numeric
//! instances and modules declare as foreign.

use num_bigint::{BigInt, Sign};

use crate::syntax::{Lit, Term};

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "exact bit-preserving modular reduction for finite f64"
)]
fn f64_to_u64_wrapping(x: f64) -> u64 {
    if !x.is_finite() || x == 0.0 {
        return 0;
    }
    let m = x.trunc() % 18_446_744_073_709_551_616.0;
    let mag = m.abs() as u64;
    if m < 0.0 { mag.wrapping_neg() } else { mag }
}

fn bigint_to_u64_wrapping(n: &BigInt) -> u64 {
    let (sign, digits) = n.to_u64_digits();
    let low = digits.first().copied().unwrap_or(0);
    if sign == Sign::Minus {
        low.wrapping_neg()
    } else {
        low
    }
}

#[expect(
    clippy::cast_sign_loss,
    reason = "two's-complement bit-preserving modular conversion"
)]
fn to_u64_bits(arg: &Term) -> Result<u64, String> {
    Ok(match arg {
        Term::Lit(Lit::Int(n)) => i64::from(*n) as u64,
        Term::Lit(Lit::Int8(n)) => i64::from(*n) as u64,
        Term::Lit(Lit::Int16(n)) => i64::from(*n) as u64,
        Term::Lit(Lit::Int64(n)) => *n as u64,
        Term::Lit(Lit::UInt8(n)) => u64::from(*n),
        Term::Lit(Lit::UInt16(n)) => u64::from(*n),
        Term::Lit(Lit::UInt32(n)) => u64::from(*n),
        Term::Lit(Lit::UInt64(n)) => *n,
        Term::Lit(Lit::Float(x)) => f64_to_u64_wrapping(*x),
        Term::Lit(Lit::Float32(x)) => f64_to_u64_wrapping(f64::from(*x)),
        Term::Lit(Lit::BigInt(n)) => bigint_to_u64_wrapping(n),
        _ => return Err(format!("numeric conversion of {arg:?}")),
    })
}

/// Exact single-rounded conversion from `BigInt` to IEEE 754 binary32.
#[expect(
    clippy::cast_possible_truncation,
    reason = "mantissa and exponent are explicitly bounded for binary32"
)]
fn bigint_to_f32(n: &BigInt) -> f32 {
    let (sign, digits) = n.to_u64_digits();
    if digits.is_empty() {
        return 0.0;
    }
    let sign_bit = if sign == Sign::Minus { 1_u32 << 31 } else { 0 };
    let bit_len = n.bits();
    if bit_len > 128 {
        return f32::from_bits(sign_bit | 0x7f80_0000);
    }
    let mut exp = (bit_len - 1) as u32;
    let abs = n.magnitude();
    let mut sig = if bit_len <= 24 {
        let low = digits[0] as u32;
        low << (24 - bit_len)
    } else {
        let shift = bit_len - 24;
        let top = (abs >> shift).to_u64_digits().first().copied().unwrap_or(0) as u32;
        let guard = abs.bit(shift - 1);
        let sticky = (0..(shift - 1)).any(|i| abs.bit(i));
        let mut s = top;
        if guard && (sticky || (s & 1) != 0) {
            s += 1;
            if s == (1 << 24) {
                s >>= 1;
                exp += 1;
            }
        }
        s
    };
    if exp > 127 {
        return f32::from_bits(sign_bit | 0x7f80_0000);
    }
    sig &= 0x007f_ffff;
    f32::from_bits(sign_bit | ((exp + 127) << 23) | sig)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "IEEE 754 rounding from 64-bit integers to binary64"
)]
pub(super) fn number(args: &[Term], _: &mut String) -> Result<Term, String> {
    let value = match args {
        [Term::Lit(Lit::Int(n))] => f64::from(*n),
        [Term::Lit(Lit::Int8(n))] => f64::from(*n),
        [Term::Lit(Lit::Int16(n))] => f64::from(*n),
        [Term::Lit(Lit::Int64(n))] => *n as f64,
        [Term::Lit(Lit::UInt8(n))] => f64::from(*n),
        [Term::Lit(Lit::UInt16(n))] => f64::from(*n),
        [Term::Lit(Lit::UInt32(n))] => f64::from(*n),
        [Term::Lit(Lit::UInt64(n))] => *n as f64,
        [Term::Lit(Lit::Float(n))] => *n,
        [Term::Lit(Lit::Float32(n))] => f64::from(*n),
        [Term::Lit(Lit::BigInt(n))] => n.to_string().parse().map_err(|e| format!("{e}"))?,
        _ => return Err(format!("Number of {args:?}")),
    };
    Ok(Term::Lit(Lit::Float(value)))
}

pub(super) fn bigint(args: &[Term], _: &mut String) -> Result<Term, String> {
    let value = match args {
        [Term::Lit(Lit::Int(n))] => BigInt::from(*n),
        [Term::Lit(Lit::Int8(n))] => BigInt::from(*n),
        [Term::Lit(Lit::Int16(n))] => BigInt::from(*n),
        [Term::Lit(Lit::Int64(n))] => BigInt::from(*n),
        [Term::Lit(Lit::UInt8(n))] => BigInt::from(*n),
        [Term::Lit(Lit::UInt16(n))] => BigInt::from(*n),
        [Term::Lit(Lit::UInt32(n))] => BigInt::from(*n),
        [Term::Lit(Lit::UInt64(n))] => BigInt::from(*n),
        [Term::Lit(Lit::BigInt(n))] => n.clone(),
        [Term::Lit(Lit::Float(n))] if n.is_finite() && n.trunc().to_bits() == n.to_bits() => {
            format!("{n:.0}").parse().map_err(|e| format!("{e}"))?
        }
        [Term::Lit(Lit::Float32(n))] if n.is_finite() && n.trunc().to_bits() == n.to_bits() => {
            format!("{:.0}", f64::from(*n))
                .parse()
                .map_err(|e| format!("{e}"))?
        }
        [Term::Lit(Lit::Float(_) | Lit::Float32(_))] => {
            return Ok(Term::Die("not an integer".into()));
        }
        _ => return Err(format!("BigInt of {args:?}")),
    };
    Ok(Term::Lit(Lit::BigInt(value)))
}

pub(super) fn show_int64(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(n))] => Ok(Term::string(&n.to_string())),
        _ => Err(format!("showInt64 of {args:?}")),
    }
}

pub(super) fn show_uint64(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::UInt64(n))] => Ok(Term::string(&n.to_string())),
        _ => Err(format!("showUInt64 of {args:?}")),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn to_int8(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::Int8(to_u64_bits(&args[0])? as i8)))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn to_int16(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::Int16(to_u64_bits(&args[0])? as i16)))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn to_int32(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::Int(to_u64_bits(&args[0])? as i32)))
}

#[expect(
    clippy::cast_possible_wrap,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn to_int64(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::Int64(to_u64_bits(&args[0])? as i64)))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn to_uint8(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::UInt8(to_u64_bits(&args[0])? as u8)))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn to_uint16(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::UInt16(to_u64_bits(&args[0])? as u16)))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "explicit two's-complement wrapping conversion"
)]
pub(super) fn to_uint32(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::UInt32(to_u64_bits(&args[0])? as u32)))
}

pub(super) fn to_uint64(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::UInt64(to_u64_bits(&args[0])?)))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "IEEE 754 single-rounded conversion to binary32"
)]
pub(super) fn to_float32(args: &[Term], _: &mut String) -> Result<Term, String> {
    let value = match args {
        [Term::Lit(Lit::Int(n))] => *n as f32,
        [Term::Lit(Lit::Int8(n))] => f32::from(*n),
        [Term::Lit(Lit::Int16(n))] => f32::from(*n),
        [Term::Lit(Lit::Int64(n))] => *n as f32,
        [Term::Lit(Lit::UInt8(n))] => f32::from(*n),
        [Term::Lit(Lit::UInt16(n))] => f32::from(*n),
        [Term::Lit(Lit::UInt32(n))] => *n as f32,
        [Term::Lit(Lit::UInt64(n))] => *n as f32,
        [Term::Lit(Lit::Float(x))] => *x as f32,
        [Term::Lit(Lit::Float32(x))] => *x,
        [Term::Lit(Lit::BigInt(n))] => bigint_to_f32(n),
        _ => return Err(format!("toFloat32 of {args:?}")),
    };
    Ok(Term::Lit(Lit::Float32(value)))
}

pub(super) fn int64_bit_and(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(a)), Term::Lit(Lit::Int64(b))] => Ok(Term::Lit(Lit::Int64(a & b))),
        _ => Err(format!("int64BitAnd of {args:?}")),
    }
}

pub(super) fn int64_bit_or(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(a)), Term::Lit(Lit::Int64(b))] => Ok(Term::Lit(Lit::Int64(a | b))),
        _ => Err(format!("int64BitOr of {args:?}")),
    }
}

pub(super) fn int64_bit_xor(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(a)), Term::Lit(Lit::Int64(b))] => Ok(Term::Lit(Lit::Int64(a ^ b))),
        _ => Err(format!("int64BitXor of {args:?}")),
    }
}

pub(super) fn int64_bit_not(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(a))] => Ok(Term::Lit(Lit::Int64(!a))),
        _ => Err(format!("int64BitNot of {args:?}")),
    }
}

#[expect(clippy::cast_sign_loss, reason = "shift count masked modulo 64")]
pub(super) fn int64_shift_left(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(a)), Term::Lit(Lit::Int(n))] => {
            Ok(Term::Lit(Lit::Int64(a.wrapping_shl((*n as u32) & 63))))
        }
        _ => Err(format!("int64ShiftLeft of {args:?}")),
    }
}

#[expect(clippy::cast_sign_loss, reason = "shift count masked modulo 64")]
pub(super) fn int64_shift_right(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(a)), Term::Lit(Lit::Int(n))] => {
            Ok(Term::Lit(Lit::Int64(a.wrapping_shr((*n as u32) & 63))))
        }
        _ => Err(format!("int64ShiftRight of {args:?}")),
    }
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "logical unsigned 64-bit right shift with count masked modulo 64"
)]
pub(super) fn int64_shift_right_unsigned(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Lit(Lit::Int64(a)), Term::Lit(Lit::Int(n))] => Ok(Term::Lit(Lit::Int64(
            ((*a as u64).wrapping_shr((*n as u32) & 63)) as i64,
        ))),
        _ => Err(format!("int64ShiftRightUnsigned of {args:?}")),
    }
}
