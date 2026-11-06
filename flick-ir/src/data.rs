//! Constructors of data and how their values are represented.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constructor {
    pub module: String,
    pub ty: String,
    pub name: String,
    pub arity: usize,
    /// How its values are represented.
    pub rep: Rep,
    /// The dictionaries its values hold after the fields: one per class
    /// constraint of a constructor of a GADT.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dicts: usize,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// How the values of one constructor are represented.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rep {
    #[default]
    Tagged,
    Unboxed,
    Typeof(JsClass),
    /// The constructor is its literal: a string or an integer.
    Literal(EnumLit),
}

/// What JS can tell a value apart by, without a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JsClass {
    Number,
    String,
    Boolean,
    Bigint,
    /// A list or a tuple.
    Array,
}

impl JsClass {
    /// The name the runtime and the generated tests use.
    pub fn name(self) -> &'static str {
        match self {
            JsClass::Number => "number",
            JsClass::String => "string",
            JsClass::Boolean => "boolean",
            JsClass::Bigint => "bigint",
            JsClass::Array => "array",
        }
    }
}

/// The literal a constructor of an enum is: all of a type's are strings, or
/// all are integers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnumLit {
    Int(i32),
    String(String),
}

/// A type constructor by its module and name: the type of an expression
/// when that is a constructor on its own (`Float`, `String`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeName {
    pub module: String,
    pub name: String,
}
