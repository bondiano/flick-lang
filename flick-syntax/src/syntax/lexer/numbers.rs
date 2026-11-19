//! Number literals: decimal, hexadecimal, octal and binary integers, floats
//! and big integers with an `n` suffix, with their `_` separators.

use super::{Lexer, is_ident_byte};
use crate::diagnostics::codes;
use crate::syntax::kind::SyntaxKind;

impl Lexer<'_> {
    /// JS `NumericLiteral` minus the `.5` and `5.` forms, which would clash with
    /// `.field` access.
    pub(super) fn number(&mut self) -> SyntaxKind {
        let start = self.pos;
        let mut problem: Option<(usize, usize, &'static str)> = None;
        let note = |p: &mut Option<_>, at: usize, end: usize, msg| {
            p.get_or_insert((at, end, msg));
        };
        let radix = match (self.peek(0), self.peek(1)) {
            (b'0', b'x' | b'X') => 16,
            (b'0', b'o' | b'O') => 8,
            (b'0', b'b' | b'B') => 2,
            _ => 10,
        };
        let mut integer_only = true;
        if radix == 10 {
            let leading_zero =
                self.peek(0) == b'0' && (self.peek(1).is_ascii_digit() || self.peek(1) == b'_');
            self.digits(10, &mut problem);
            if leading_zero {
                note(
                    &mut problem,
                    start,
                    self.pos,
                    "a decimal number cannot have a leading zero (legacy octal is not supported; use `0o`)",
                );
            }
            if self.peek(0) == b'.' && self.peek(1).is_ascii_digit() {
                integer_only = false;
                self.pos += 1;
                self.digits(10, &mut problem);
            } else if self.peek(0) == b'.' && self.peek(1) != b'.' {
                self.pos += 1;
                note(
                    &mut problem,
                    start,
                    self.pos,
                    "a number cannot end with `.`; write a digit after it, e.g. `5.0`",
                );
            }
            if matches!(self.peek(0), b'e' | b'E') {
                integer_only = false;
                self.pos += 1;
                if matches!(self.peek(0), b'+' | b'-') {
                    self.pos += 1;
                }
                if !self.digits(10, &mut problem) {
                    note(
                        &mut problem,
                        start,
                        self.pos,
                        "missing digits in the exponent",
                    );
                }
            }
        } else {
            self.pos += 2;
            if !self.digits(radix, &mut problem) {
                note(
                    &mut problem,
                    start,
                    self.pos,
                    "missing digits after the radix prefix",
                );
            }
        }
        let mut kind = SyntaxKind::Number;
        if self.peek(0) == b'n' {
            self.pos += 1;
            kind = SyntaxKind::BigInt;
            let legacy = radix == 10 && self.src[start] == b'0' && self.pos - start > 2;
            if !integer_only || legacy {
                note(
                    &mut problem,
                    start,
                    self.pos,
                    "a BigInt literal is an integer without fraction, exponent or leading zero",
                );
            }
        }
        if is_ident_byte(self.peek(0)) {
            let at = self.pos;
            while is_ident_byte(self.peek(0)) {
                self.pos += 1;
            }
            note(
                &mut problem,
                at,
                self.pos,
                "a number cannot be directly followed by a letter or digit of another base",
            );
        }
        if let Some((at, end, message)) = problem {
            self.error(codes::S006, at, end, message);
        }
        kind
    }

    /// A digit run with `_` separators; returns whether it had any digit.
    fn digits(&mut self, radix: u32, problem: &mut Option<(usize, usize, &'static str)>) -> bool {
        let is_digit = |b: u8| char::from(b).is_digit(radix);
        let start = self.pos;
        while is_digit(self.peek(0)) || self.peek(0) == b'_' {
            if self.peek(0) == b'_' {
                let after_digit = self.pos > start && is_digit(self.src[self.pos - 1]);
                if !after_digit || !is_digit(self.peek(1)) {
                    problem.get_or_insert((
                        self.pos,
                        self.pos + 1,
                        "`_` separates two digits; it cannot start, end or repeat",
                    ));
                }
            }
            self.pos += 1;
        }
        self.src[start..self.pos].iter().any(|&b| is_digit(b))
    }
}
