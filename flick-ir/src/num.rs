//! The built-in numeric types and arithmetic operators. What a type does
//! comes from its kind, so another fixed-width type is one more variant here
//! and one more form in each backend.

use serde::{Deserialize, Serialize};

use crate::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Num {
    Int,
    Float,
    BigInt,
    Int8,
    Int16,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
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
    pub const ALL: [Num; 11] = [
        Num::Int,
        Num::Float,
        Num::BigInt,
        Num::Int8,
        Num::Int16,
        Num::Int64,
        Num::UInt8,
        Num::UInt16,
        Num::UInt32,
        Num::UInt64,
        Num::Float32,
    ];

    /// The `@lang` role for this numeric type.
    pub fn lang(self) -> Lang {
        match self {
            Num::Int => Lang::Int,
            Num::Float => Lang::Float,
            Num::BigInt => Lang::BigInt,
            Num::Int8 => Lang::Int8,
            Num::Int16 => Lang::Int16,
            Num::Int64 => Lang::Int64,
            Num::UInt8 => Lang::UInt8,
            Num::UInt16 => Lang::UInt16,
            Num::UInt32 => Lang::UInt32,
            Num::UInt64 => Lang::UInt64,
            Num::Float32 => Lang::Float32,
        }
    }

    pub fn kind(self) -> NumKind {
        match self {
            Num::Int => NumKind::Integer {
                bits: 32,
                signed: true,
            },
            Num::Int8 => NumKind::Integer {
                bits: 8,
                signed: true,
            },
            Num::Int16 => NumKind::Integer {
                bits: 16,
                signed: true,
            },
            Num::Int64 => NumKind::Integer {
                bits: 64,
                signed: true,
            },
            Num::UInt8 => NumKind::Integer {
                bits: 8,
                signed: false,
            },
            Num::UInt16 => NumKind::Integer {
                bits: 16,
                signed: false,
            },
            Num::UInt32 => NumKind::Integer {
                bits: 32,
                signed: false,
            },
            Num::UInt64 => NumKind::Integer {
                bits: 64,
                signed: false,
            },
            Num::Float => NumKind::Float { bits: 64 },
            Num::Float32 => NumKind::Float { bits: 32 },
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
