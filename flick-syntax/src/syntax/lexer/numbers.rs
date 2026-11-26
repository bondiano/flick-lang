//! Number literals: decimal, hexadecimal, octal and binary integers, floats
//! and big integers with an `n` suffix, with their `_` separators.

use fln_literals::{NumberKind, scan_number_recover};

use super::{Lexer, push_error};
use crate::diagnostics::codes;
use crate::syntax::kind::SyntaxKind;

impl Lexer<'_> {
    /// JS `NumericLiteral` minus the `.5` and `5.` forms, which would clash with
    /// `.field` access.
    pub(super) fn number(&mut self) -> SyntaxKind {
        let file = self.file;
        let errors = &mut self.errors;
        let scanned = scan_number_recover(self.text, self.pos, |err| {
            push_error(errors, file, codes::S006, err.start, err.end, err.message());
        });
        self.pos = scanned.end;
        match scanned.kind {
            NumberKind::BigInt { .. } => SyntaxKind::BigInt,
            NumberKind::Integer { .. } | NumberKind::Float { .. } => SyntaxKind::Number,
        }
    }
}
