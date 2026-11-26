//! String literals: one-line strings with their `\{…}` holes, multi-line
//! `"""` strings, raw strings and escapes.

use fln_literals::{
    InterpolationMode, StringError, StringErrorKind, StringScanOutcome, raw_string_hashes,
    scan_raw_string_recover, scan_string_continuation_recover, scan_string_recover,
};

use super::{Lexer, push_error};
use crate::diagnostics::{Code, Diagnostic, codes};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind;

impl Lexer<'_> {
    /// Count of `#` of a raw string opening at `pos` (`r"` or `r#…#"`).
    pub(super) fn raw_string_start(&self) -> Option<usize> {
        raw_string_hashes(self.text, self.pos)
    }

    pub(super) fn raw_string(&mut self) -> SyntaxKind {
        let file = self.file;
        let errors = &mut self.errors;
        let outcome = scan_raw_string_recover(self.text, self.pos, |err| {
            report_string_error(errors, file, err);
        });
        self.pos = match outcome {
            StringScanOutcome::Closed { end, .. } | StringScanOutcome::Hole { end } => end,
        };
        SyntaxKind::String
    }

    pub(super) fn string(&mut self) -> SyntaxKind {
        let file = self.file;
        let errors = &mut self.errors;
        let outcome =
            scan_string_recover(self.text, self.pos, InterpolationMode::StopAtHole, |err| {
                report_string_error(errors, file, err);
            });
        match outcome {
            StringScanOutcome::Closed { end, .. } => {
                self.pos = end;
                SyntaxKind::String
            }
            StringScanOutcome::Hole { end } => {
                self.pos = end;
                self.holes.push(0);
                SyntaxKind::StringStart
            }
        }
    }

    /// The part of a string after the `}` of a hole: up to the next hole,
    /// or to the end of the string.
    pub(super) fn string_rest(&mut self) -> SyntaxKind {
        let file = self.file;
        let errors = &mut self.errors;
        let outcome = scan_string_continuation_recover(self.text, self.pos, |err| {
            report_string_error(errors, file, err);
        });
        match outcome {
            StringScanOutcome::Hole { end } => {
                self.pos = end;
                SyntaxKind::StringMid
            }
            StringScanOutcome::Closed { end, .. } => {
                self.pos = end;
                self.holes.pop();
                SyntaxKind::StringEnd
            }
        }
    }
}

fn report_string_error(errors: &mut Vec<Diagnostic>, file: FileId, err: StringError) {
    let (code, message): (Code, String) = match err.kind {
        StringErrorKind::Unterminated => (
            codes::S004,
            "unterminated string: missing closing `\"`".to_owned(),
        ),
        StringErrorKind::UnterminatedRaw { hashes } => (
            codes::S004,
            format!(
                "unterminated raw string: missing `\"{}`",
                "#".repeat(hashes)
            ),
        ),
        StringErrorKind::MultilineMissingNewline => (
            codes::S005,
            "a multi-line string starts with `\"\"\"` followed by a line break".to_owned(),
        ),
        StringErrorKind::InterpolationNotAllowed => (
            codes::S005,
            "`\\{…}` interpolates only in a one-line string `\"…\"`".to_owned(),
        ),
        StringErrorKind::NullEscapeFollowedByDigit => (
            codes::S005,
            "`\\0` cannot be followed by a digit; use `\\u{…}`".to_owned(),
        ),
        StringErrorKind::InvalidUnicodeEscape => (
            codes::S005,
            "expected `\\u{…}` with 1 to 6 hex digits".to_owned(),
        ),
        StringErrorKind::InvalidUnicodeScalar => (
            codes::S005,
            "`\\u{…}` must be a Unicode scalar value (no surrogates, at most 10FFFF)".to_owned(),
        ),
        StringErrorKind::UnknownEscape => (codes::S005, "unknown escape sequence".to_owned()),
    };
    push_error(errors, file, code, err.start, err.end, message);
}
