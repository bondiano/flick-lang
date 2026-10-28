//! Literals as JS.

use super::names;
use crate::ir::Lit;
use crate::numeric;

/// A literal as JS; a negative number in parentheses, as `-x` is printed.
pub fn literal(lit: &Lit) -> String {
    match lit {
        Lit::Int(n) if *n < 0 => format!("({n})"),
        Lit::Int(n) => n.to_string(),
        // JS globals, never shadowed: generated code reserves both names
        Lit::Float(x) if x.is_nan() => "NaN".to_owned(),
        Lit::Float(x) => {
            let text = if x.is_infinite() {
                "Infinity".to_owned()
            } else {
                numeric::float_text(*x)
            };
            if x.is_sign_negative() {
                format!("(-{text})")
            } else {
                text
            }
        }
        Lit::BigInt(n) if n.starts_with('-') => format!("({n}n)"),
        Lit::BigInt(n) => format!("{n}n"),
        Lit::String(s) => names::string(s),
        Lit::Bool(b) => b.to_string(),
        Lit::Unit => "undefined".to_owned(),
    }
}
