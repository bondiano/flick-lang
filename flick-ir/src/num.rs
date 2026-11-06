//! The built-in numeric types and arithmetic operators. What a type does
//! comes from its kind, so another fixed-width type is one more variant here
//! and one more form in each backend.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
