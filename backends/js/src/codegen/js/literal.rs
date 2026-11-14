//! Literals as JS.

use super::Gen;
use super::doc::{Head, Js};
use crate::ir::Lit;
use crate::numeric;

impl<'d> Gen<'_, 'd> {
    /// A literal as JS; a negative number in parentheses, as `-x` is printed.
    pub(super) fn literal(&self, lit: &Lit) -> Js<'d> {
        match lit {
            Lit::Int(n) if *n < 0 => self.negative(&n.to_string()),
            Lit::Int(n) => self.leaf(&n.to_string()),
            // JS globals, never shadowed: generated code reserves both names
            Lit::Float(x) if x.is_nan() => self.leaf("NaN"),
            Lit::Float(x) => {
                let text = if x.is_infinite() {
                    "Infinity".to_owned()
                } else {
                    numeric::float_text(*x)
                };
                if x.is_sign_negative() {
                    self.negative(&format!("-{text}"))
                } else {
                    self.leaf(&text)
                }
            }
            Lit::BigInt(n) if n.starts_with('-') => self.negative(&format!("{n}n")),
            Lit::BigInt(n) => self.leaf(&format!("{n}n")),
            Lit::String(s) => self.string(s),
            Lit::Bool(b) => self.leaf(&b.to_string()),
            Lit::Unit => self.leaf("undefined"),
        }
    }

    /// The negative number `text`, in parentheses.
    fn negative(&self, text: &str) -> Js<'d> {
        Js::code(self.a.text(self.a.alloc_str(text)), Head::Other, &[]).in_parens()
    }
}
