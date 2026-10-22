//! Diagnostics for types that do not unify.

use super::Checker;
use super::ty::Type;
use super::unify::Mismatch;
use crate::diagnostics::codes;
use crate::source::Span;

impl Checker<'_> {
    /// Unifies the type an expression has with the one its position expects;
    /// reports a mismatch at `span`. `false` after a diagnostic.
    pub fn expect(&mut self, actual: &Type, expected: &Type, span: Span) -> bool {
        match self.unify(actual, expected) {
            Ok(()) => true,
            Err(mismatch) => {
                self.mismatch(&mismatch, actual, expected, span);
                false
            }
        }
    }

    pub fn mismatch(&mut self, mismatch: &Mismatch, actual: &Type, expected: &Type, span: Span) {
        let (expected, actual) = (self.show(expected), self.show(actual));
        let head = format!("expected `{expected}`, found `{actual}`");
        match mismatch {
            Mismatch::Types(a, b) => {
                let (a, b) = (self.zonk(a), self.zonk(b));
                let rigid = |t: &Type| matches!(t, Type::Skolem(_));
                if rigid(&a) || rigid(&b) {
                    let (var, other) = if rigid(&b) { (&b, &a) } else { (&a, &b) };
                    let (var, other) = (self.show(var), self.show(other));
                    self.error(
                        codes::T008,
                        span,
                        format!(
                            "{head}: `{var}` stands for any type the caller chooses, so it cannot be `{other}`"
                        ),
                    );
                    return;
                }
                let detail = match (&a, &b) {
                    (Type::Fun { effects: e1, .. }, Type::Fun { effects: e2, .. })
                        if e1.is_some() != e2.is_some() =>
                    {
                        ": a pure function `->` and a runtime function `->{…}` are different types"
                            .to_owned()
                    }
                    _ => {
                        let (sa, sb) = (self.show(&a), self.show(&b));
                        if sa == actual && sb == expected || sa == expected && sb == actual {
                            String::new()
                        } else {
                            format!(": `{sa}` is not `{sb}`")
                        }
                    }
                };
                self.error(codes::T003, span, format!("{head}{detail}"));
            }
            Mismatch::Label { label, left } => {
                let detail = if *left {
                    format!("the value has field `{label}`, which the expected type does not")
                } else {
                    format!("field `{label}` is missing")
                };
                self.error(codes::T003, span, format!("{head}: {detail}"));
            }
            Mismatch::Occurs(t) => {
                let t = self.show(t);
                self.error(
                    codes::T016,
                    span,
                    format!("{head}: the type would contain itself (`{t}`)"),
                );
            }
            Mismatch::Escape(name) => self.error(
                codes::T007,
                span,
                format!(
                    "{head}: the type variable `{name}` would escape the scope of its `forall`"
                ),
            ),
            Mismatch::Polymorphic(t) => {
                let t = self.show(t);
                self.error(
                    codes::T003,
                    span,
                    format!(
                        "{head}: a type variable cannot stand for the polymorphic type `{t}`; write the `forall` in a signature"
                    ),
                );
            }
        }
    }
}
