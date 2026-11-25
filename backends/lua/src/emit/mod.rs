//! Lua source from the IR, in three steps:
//!
//! 1. [`lower`] builds a structured Lua program ([`ast`]) from the pure core
//!    of the IR, refusing what the target cannot compute exactly;
//! 2. [`limits`] counts what each generated function needs of the dialect's
//!    VM, refusing what it would not load;
//! 3. [`print`] writes deterministic text and the Flick span of each line.
//!
//! Every refusal is `P012` at the Flick source of what is refused; in a
//! built-in module, at the project's expression that reached it.

mod ast;
mod build;
mod limits;
mod lower;
mod names;
mod print;

use flick_backend::{Diagnostic, Span as SourceSpan};
use flick_ir::Span;

use crate::profiles::Dialect;
use crate::refusal;
pub(crate) use build::build;

/// What the target cannot run or load, at its Flick source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub message: String,
    pub span: Span,
}

impl Refusal {
    /// The `P012` diagnostic of the refusal, at its span.
    pub(crate) fn diagnostic(self) -> Diagnostic {
        let span = SourceSpan {
            file: self.span.file.index(),
            start: self.span.start,
            end: self.span.end,
        };
        refusal(self.message, Some(span), vec![])
    }
}

/// The target code is generated for: its name in messages (`lua/love`)
/// and the dialect its environment selects.
pub(crate) struct Target<'a> {
    pub name: &'a str,
    pub dialect: &'static Dialect,
}

#[cfg(test)]
mod tests;
