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

/// `a op b` on `Int`: wraps around, and division by zero is zero.
pub fn int_arith(op: Arith, a: i32, b: i32) -> i32 {
    match op {
        Arith::Add => a.wrapping_add(b),
        Arith::Sub => a.wrapping_sub(b),
        Arith::Mul => a.wrapping_mul(b),
        Arith::Div | Arith::Rem if b == 0 => 0,
        Arith::Div => a.wrapping_div(b),
        Arith::Rem => a.wrapping_rem(b),
        Arith::Pow => int_pow(a, b),
    }
}

/// `a ** b` on `Int`, truncated like division for a negative exponent.
fn int_pow(a: i32, b: i32) -> i32 {
    match (a, u32::try_from(b)) {
        (_, Ok(b)) => a.wrapping_pow(b),
        (1, Err(_)) => 1,
        (-1, Err(_)) => {
            if b % 2 == 0 {
                1
            } else {
                -1
            }
        }
        (_, Err(_)) => 0,
    }
}
