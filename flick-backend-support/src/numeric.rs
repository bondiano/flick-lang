//! Portable integer operations shared with frontend constant facts.

pub use flick_ir::{Arith, Num};

/// The value of canonical IR `BigInt` digits, when it fits in 128 bits.
pub fn bigint_digits_value(digits: &str) -> Option<i128> {
    let radix = match digits.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("0x") => Some(16),
        Some("0o") => Some(8),
        Some("0b") => Some(2),
        _ => None,
    };
    let (base, rest) = radix.map_or((10, digits), |base| (base, &digits[2..]));
    i128::from_str_radix(rest, base).ok()
}

/// Wrapping binary exponentiation mod 2^64 (which also yields the exact low
/// W bits mod 2^W for any W in {8, 16, 32, 64}).
fn pow_u64(mut base: u64, mut exp: u64) -> u64 {
    let mut acc: u64 = 1;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = acc.wrapping_mul(base);
        }
        exp >>= 1;
        if exp > 0 {
            base = base.wrapping_mul(base);
        }
    }
    acc
}

macro_rules! signed_arith {
    ($fn_name:ident, $ty:ty, $uty:ty) => {
        /// `a op b` on a fixed-width signed integer: wraps around, division/remainder
        /// by zero is zero, `min / -1 = min`, `min % -1 = 0`, and negative exponents
        /// truncate toward zero (`1 -> 1`, `-1 -> ±1`, else `0`).
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            clippy::cast_sign_loss,
            clippy::cast_lossless,
            reason = "fixed-width modular arithmetic truncates intentionally"
        )]
        pub fn $fn_name(op: Arith, a: $ty, b: $ty) -> $ty {
            match op {
                Arith::Add => a.wrapping_add(b),
                Arith::Sub => a.wrapping_sub(b),
                Arith::Mul => a.wrapping_mul(b),
                Arith::Div | Arith::Rem if b == 0 => 0,
                Arith::Div => a.wrapping_div(b),
                Arith::Rem => a.wrapping_rem(b),
                Arith::Pow => {
                    if b < 0 {
                        match a {
                            1 => 1,
                            -1 => {
                                if b & 1 == 0 {
                                    1
                                } else {
                                    -1
                                }
                            }
                            _ => 0,
                        }
                    } else {
                        pow_u64(a as $uty as u64, b as u64) as $ty
                    }
                }
            }
        }
    };
}

macro_rules! unsigned_arith {
    ($fn_name:ident, $ty:ty) => {
        /// `a op b` on a fixed-width unsigned integer: wraps around, and
        /// division/remainder by zero is zero.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_lossless,
            reason = "fixed-width modular arithmetic truncates intentionally"
        )]
        pub fn $fn_name(op: Arith, a: $ty, b: $ty) -> $ty {
            match op {
                Arith::Add => a.wrapping_add(b),
                Arith::Sub => a.wrapping_sub(b),
                Arith::Mul => a.wrapping_mul(b),
                Arith::Div | Arith::Rem if b == 0 => 0,
                Arith::Div => a / b,
                Arith::Rem => a % b,
                Arith::Pow => pow_u64(a as u64, b as u64) as $ty,
            }
        }
    };
}

signed_arith!(int8_arith, i8, u8);
signed_arith!(int16_arith, i16, u16);
signed_arith!(int_arith, i32, u32);
signed_arith!(int64_arith, i64, u64);

unsigned_arith!(uint8_arith, u8);
unsigned_arith!(uint16_arith, u16);
unsigned_arith!(uint32_arith, u32);
unsigned_arith!(uint64_arith, u64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_width_boundary_arithmetic() {
        assert_eq!(int8_arith(Arith::Add, i8::MAX, 1), i8::MIN);
        assert_eq!(int8_arith(Arith::Div, i8::MIN, -1), i8::MIN);
        assert_eq!(int8_arith(Arith::Rem, i8::MIN, -1), 0);
        assert_eq!(int8_arith(Arith::Div, 5, 0), 0);
        assert_eq!(int8_arith(Arith::Rem, -7, 2), -1);
        assert_eq!(int8_arith(Arith::Pow, -1, i8::MIN), 1);
        assert_eq!(int8_arith(Arith::Pow, -1, i8::MIN + 1), -1);

        assert_eq!(int16_arith(Arith::Add, i16::MAX, 1), i16::MIN);
        assert_eq!(int16_arith(Arith::Div, i16::MIN, -1), i16::MIN);

        assert_eq!(int64_arith(Arith::Add, i64::MAX, 1), i64::MIN);
        assert_eq!(int64_arith(Arith::Div, i64::MIN, -1), i64::MIN);
        assert_eq!(int64_arith(Arith::Rem, i64::MIN, -1), 0);
        // Full 64-bit exponent (must NOT truncate to u32!)
        assert_eq!(int64_arith(Arith::Pow, 2, 1_i64 << 32), 0);
        assert_eq!(int64_arith(Arith::Pow, -1, i64::MIN), 1);
        assert_eq!(int64_arith(Arith::Pow, -1, i64::MIN + 1), -1);

        assert_eq!(uint8_arith(Arith::Sub, 0, 1), 255);
        assert_eq!(uint8_arith(Arith::Add, 255, 1), 0);
        assert_eq!(uint8_arith(Arith::Div, 9, 0), 0);
        assert_eq!(uint8_arith(Arith::Pow, 2, 8), 0);

        assert_eq!(uint16_arith(Arith::Sub, 0, 1), u16::MAX);
        assert_eq!(uint32_arith(Arith::Sub, 0, 1), u32::MAX);
        assert_eq!(uint64_arith(Arith::Sub, 0, 1), u64::MAX);
        assert_eq!(uint64_arith(Arith::Pow, 2, 1_u64 << 32), 0);
        assert_eq!(uint64_arith(Arith::Pow, 0, 0), 1);
    }
}
