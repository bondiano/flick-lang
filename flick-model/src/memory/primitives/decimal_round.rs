//! Exact `BigDecimal` rounding, scaling, and division helpers (`runtime-go/flickrt/decimal.go`).

use std::cmp::Ordering;

use num_bigint::BigInt;
use num_traits::{Signed, Zero};

use super::Error;

pub(super) const MAX_DECIMAL_SCALE: i64 = 10_000;
pub(super) const MAX_DECIMAL_ALIGNMENT: i64 = 2 * MAX_DECIMAL_SCALE;

pub(super) fn pow10(n: i64) -> BigInt {
    if n <= 0 {
        return BigInt::from(1);
    }
    BigInt::from(10_u32).pow(u32::try_from(n).unwrap_or(u32::MAX))
}

pub(super) fn power_of_ten(n: i64) -> Result<BigInt, Error> {
    if n > MAX_DECIMAL_ALIGNMENT {
        return Err(Error::RangeError(format!(
            "Scale alignment must not exceed {MAX_DECIMAL_ALIGNMENT} digits, got {n}"
        )));
    }
    Ok(pow10(n))
}

#[derive(Clone, Debug)]
pub(super) struct Dec {
    pub value: BigInt,
    pub scale: i64,
}

impl Dec {
    pub fn zero() -> Self {
        Self {
            value: BigInt::zero(),
            scale: 0,
        }
    }

    pub fn normalize(&self) -> Self {
        if self.value.is_zero() {
            return Self::zero();
        }
        let digits = self.value.to_str_radix(10);
        let trimmed = digits.trim_end_matches('0');
        let dropped = i64::try_from(digits.len() - trimmed.len()).unwrap_or(0);
        if dropped == 0 {
            return self.clone();
        }
        let v = BigInt::parse_bytes(trimmed.as_bytes(), 10).unwrap_or_else(BigInt::zero);
        Self {
            value: v,
            scale: self.scale - dropped,
        }
    }

    pub fn rescale(&self, scale: i64) -> Result<Self, Error> {
        match scale.cmp(&self.scale) {
            Ordering::Greater => Ok(Self {
                value: &self.value * power_of_ten(scale - self.scale)?,
                scale,
            }),
            Ordering::Less => Ok(Self {
                value: &self.value / power_of_ten(self.scale - scale)?,
                scale,
            }),
            Ordering::Equal => Ok(self.clone()),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.value.is_zero()
    }

    pub fn is_negative(&self) -> bool {
        self.value.is_negative()
    }

    pub fn is_positive(&self) -> bool {
        self.value.is_positive()
    }
}

pub(super) fn dec_sum(a: &Dec, b: &Dec) -> Result<Dec, Error> {
    if b.is_zero() {
        return Ok(a.clone());
    }
    if a.is_zero() {
        return Ok(b.clone());
    }
    let s = a.scale.max(b.scale);
    let av = a.rescale(s)?.value;
    let bv = b.rescale(s)?.value;
    Ok(Dec {
        value: av + bv,
        scale: s,
    })
}

pub(super) fn dec_compare(a: &Dec, b: &Dec) -> Result<Ordering, Error> {
    if a.scale == b.scale {
        return Ok(a.value.cmp(&b.value));
    }
    let sa = a.value.sign();
    let sb = b.value.sign();
    if sa != sb {
        return Ok(sa.cmp(&sb));
    }
    if a.is_zero() {
        return Ok(Ordering::Equal);
    }
    let s = a.scale.max(b.scale);
    Ok(a.rescale(s)?.value.cmp(&b.rescale(s)?.value))
}

pub(super) fn dec_truncate(d: &Dec, s: i64) -> Result<Dec, Error> {
    if d.scale <= s {
        Ok(d.clone())
    } else {
        d.rescale(s)
    }
}

pub(super) fn dec_ceil(d: &Dec, s: i64) -> Result<Dec, Error> {
    let t = dec_truncate(d, s)?;
    if d.is_positive() && dec_compare(&t, d)? == Ordering::Less {
        dec_sum(
            &t,
            &Dec {
                value: BigInt::from(1),
                scale: s,
            },
        )
    } else {
        Ok(t)
    }
}

pub(super) fn dec_floor(d: &Dec, s: i64) -> Result<Dec, Error> {
    let t = dec_truncate(d, s)?;
    if d.is_negative() && dec_compare(&t, d)? == Ordering::Greater {
        dec_sum(
            &t,
            &Dec {
                value: BigInt::from(-1),
                scale: s,
            },
        )
    } else {
        Ok(t)
    }
}

pub(super) fn dec_round(d: &Dec, s: i64, mode: &str) -> Result<Dec, Error> {
    if d.scale <= s {
        return Ok(d.clone());
    }
    let half_ceil = || -> Result<Dec, Error> {
        let sum = dec_sum(
            d,
            &Dec {
                value: BigInt::from(5),
                scale: s + 1,
            },
        )?;
        dec_floor(&sum, s)
    };
    let half_floor = || -> Result<Dec, Error> {
        let sum = dec_sum(
            d,
            &Dec {
                value: BigInt::from(-5),
                scale: s + 1,
            },
        )?;
        dec_ceil(&sum, s)
    };
    match mode {
        "Ceiling" => dec_ceil(d, s),
        "Floor" => dec_floor(d, s),
        "TowardZero" => dec_truncate(d, s),
        "AwayFromZero" => {
            if d.is_positive() {
                dec_ceil(d, s)
            } else {
                dec_floor(d, s)
            }
        }
        "HalfCeiling" => half_ceil(),
        "HalfFloor" => half_floor(),
        "HalfTowardZero" => {
            if d.is_negative() {
                half_ceil()
            } else {
                half_floor()
            }
        }
        "HalfAwayFromZero" => {
            if d.is_negative() {
                half_floor()
            } else {
                half_ceil()
            }
        }
        _ => {
            let up = half_ceil()?;
            let down = half_floor()?;
            if dec_compare(&up, &down)? == Ordering::Equal {
                return Ok(up);
            }
            let digit = if up.scale >= s {
                (&up.value / pow10(up.scale - s)) % BigInt::from(10)
            } else {
                BigInt::zero()
            };
            let even = (&digit % BigInt::from(2)).is_zero();
            if (mode == "HalfEven") == even {
                Ok(up)
            } else {
                Ok(down)
            }
        }
    }
}

/// Computes `num / den` rounded to an integer by `mode`; `None` when `Exact` cannot.
pub(super) fn round_quotient(num: &BigInt, den: &BigInt, mode: &str) -> Option<BigInt> {
    let q = num / den;
    let r = num % den;
    if r.is_zero() {
        return Some(q);
    }
    let sign = if num.is_negative() == den.is_negative() {
        1_i64
    } else {
        -1_i64
    };
    let half = (BigInt::from(2) * r.abs()).cmp(&den.abs());
    let step = |up: bool| {
        if up {
            Some(&q + BigInt::from(sign))
        } else {
            Some(q.clone())
        }
    };
    let even = (&q % BigInt::from(2)).is_zero();
    match mode {
        "TowardZero" => Some(q),
        "AwayFromZero" => step(true),
        "Floor" => step(sign < 0),
        "Ceiling" => step(sign > 0),
        "HalfAwayFromZero" => step(half != Ordering::Less),
        "HalfTowardZero" => step(half == Ordering::Greater),
        "HalfFloor" => step(half == Ordering::Greater || (half == Ordering::Equal && sign < 0)),
        "HalfCeiling" => step(half == Ordering::Greater || (half == Ordering::Equal && sign > 0)),
        "HalfEven" => step(half == Ordering::Greater || (half == Ordering::Equal && !even)),
        "HalfOdd" => step(half == Ordering::Greater || (half == Ordering::Equal && even)),
        _ => None,
    }
}

/// Computes `self / that` to 100 significant digits, rounding half away from zero.
pub(super) fn default_divide(a: &Dec, b: &Dec) -> Option<Dec> {
    if b.is_zero() {
        return None;
    }
    if a.is_zero() {
        return Some(Dec::zero());
    }
    let mut scale = a.scale - b.scale;
    if a.value == b.value {
        return Some(Dec {
            value: BigInt::from(1),
            scale,
        });
    }
    let mut num = a.value.abs();
    let den = b.value.abs();
    let ten = BigInt::from(10);
    let negative = a.is_negative() != b.is_negative();
    while num < den {
        num *= &ten;
        scale += 1;
    }
    let mut q = &num / &den;
    let mut r = &num % &den;
    if !r.is_zero() {
        let mut count = q.to_str_radix(10).len();
        r *= &ten;
        while !r.is_zero() && count < 100 {
            let d = &r / &den;
            let rest = &r % &den;
            q = q * &ten + d;
            r = rest * &ten;
            count += 1;
            scale += 1;
        }
        if !r.is_zero() {
            let next_digit = (&r / &den).to_str_radix(10);
            if next_digit.as_bytes().first().is_some_and(|&c| c >= b'5') {
                q += 1;
            }
        }
    }
    if negative {
        q = -q;
    }
    Some(Dec { value: q, scale })
}
