//! The built-in numeric types. Operators, literals and constant folding read
//! what a type does from its kind, so another fixed-width type is one more
//! variant here and one more form in each backend.

use crate::syntax::literal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Num {
    Int,
    Float,
    BigInt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumKind {
    /// Two's complement of `bits` bits; arithmetic wraps around.
    Integer { bits: u32, signed: bool },
    /// IEEE 754 binary of `bits` bits.
    Float { bits: u32 },
    /// An integer of any size.
    Arbitrary,
}

/// The arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}

impl Num {
    /// The type an operand nothing determines becomes comes first.
    pub const ALL: [Num; 3] = [Num::Int, Num::Float, Num::BigInt];

    pub fn name(self) -> &'static str {
        match self {
            Num::Int => "Int",
            Num::Float => "Float",
            Num::BigInt => "BigInt",
        }
    }

    pub fn of(name: &str) -> Option<Num> {
        Num::ALL.into_iter().find(|n| n.name() == name)
    }

    pub fn kind(self) -> NumKind {
        match self {
            Num::Int => NumKind::Integer {
                bits: 32,
                signed: true,
            },
            Num::Float => NumKind::Float { bits: 64 },
            Num::BigInt => NumKind::Arbitrary,
        }
    }

    /// Whether the integer `value` is one of this type's values; a float
    /// type holds every integer literal, rounded.
    pub fn holds(self, value: i128) -> bool {
        match self.kind() {
            NumKind::Integer { bits, signed: true } => {
                let max = (1_i128 << (bits - 1)) - 1;
                (-max - 1..=max).contains(&value)
            }
            NumKind::Integer {
                bits,
                signed: false,
            } => (0..1_i128 << bits).contains(&value),
            NumKind::Float { .. } | NumKind::Arbitrary => true,
        }
    }

    /// Whether a literal written with a fraction or an exponent may be one.
    pub fn fractional(self) -> bool {
        matches!(self.kind(), NumKind::Float { .. })
    }
}

/// The type a number literal nothing determines becomes: `Int`, or `Float`
/// for one with a fraction.
pub fn default_num(fractional: bool) -> Num {
    Num::ALL
        .into_iter()
        .find(|n| n.fractional() || !fractional)
        .unwrap_or(Num::Float)
}

impl Arith {
    pub fn symbol(self) -> &'static str {
        match self {
            Arith::Add => "+",
            Arith::Sub => "-",
            Arith::Mul => "*",
            Arith::Div => "/",
            Arith::Rem => "%",
            Arith::Pow => "**",
        }
    }
}

/// Whether a number token is written as an integer: in a radix, or with
/// neither a fraction nor an exponent.
pub fn is_integer_literal(text: &str) -> bool {
    let digits: String = text.chars().filter(|&c| c != '_').collect();
    literal::radix(&digits).is_some() || digits.bytes().all(|b| b.is_ascii_digit())
}

/// The value of a number token written as an integer.
pub fn integer_value(text: &str) -> Option<i128> {
    let digits: String = text.chars().filter(|&c| c != '_').collect();
    match literal::radix(&digits) {
        Some((base, rest)) => i128::from_str_radix(rest, base).ok(),
        None if digits.bytes().all(|b| b.is_ascii_digit()) => digits.parse().ok(),
        None => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_arithmetic_wraps_and_divides_by_zero_to_zero() {
        assert_eq!(int_arith(Arith::Add, i32::MAX, 1), i32::MIN);
        assert_eq!(int_arith(Arith::Mul, 65_536, 65_536), 0);
        assert_eq!(int_arith(Arith::Div, 7, 2), 3);
        assert_eq!(int_arith(Arith::Div, -7, 2), -3);
        assert_eq!(int_arith(Arith::Div, 5, 0), 0);
        assert_eq!(int_arith(Arith::Rem, -7, 2), -1);
        assert_eq!(int_arith(Arith::Rem, 5, 0), 0);
        assert_eq!(int_arith(Arith::Div, i32::MIN, -1), i32::MIN);
        assert_eq!(int_arith(Arith::Rem, i32::MIN, -1), 0);
        assert_eq!(int_arith(Arith::Pow, 2, 31), i32::MIN);
        assert_eq!(int_arith(Arith::Pow, 2, -1), 0);
        assert_eq!(int_arith(Arith::Pow, -1, -3), -1);
    }

    #[test]
    fn literals_and_ranges() {
        assert_eq!(integer_value("1_000"), Some(1000));
        assert_eq!(integer_value("0xFF"), Some(255));
        assert_eq!(integer_value("0b1e"), None);
        assert_eq!(integer_value("1.5"), None);
        assert_eq!(integer_value("1e3"), None);
        assert!(is_integer_literal("0xFF") && is_integer_literal("1_000"));
        assert!(!is_integer_literal("1.5") && !is_integer_literal("1e3"));
        assert!(Num::Int.holds(-2_147_483_648));
        assert!(!Num::Int.holds(2_147_483_648));
        assert!(Num::Float.holds(1 << 100));
        assert_eq!(default_num(false), Num::Int);
        assert_eq!(default_num(true), Num::Float);
    }
}
