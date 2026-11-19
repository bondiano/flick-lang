//! String literals: one-line strings with their `\{…}` holes, multi-line
//! `"""` strings, raw strings and escapes.

use std::iter;

use super::Lexer;
use crate::diagnostics::codes;
use crate::syntax::kind::SyntaxKind;

impl Lexer<'_> {
    /// Float of `#` of a raw string opening at `pos` (`r"` or `r#…#"`).
    pub(super) fn raw_string_start(&self) -> Option<usize> {
        let hashes = self.src[self.pos + 1..]
            .iter()
            .take_while(|&&b| b == b'#')
            .count();
        (self.peek(1 + hashes) == b'"').then_some(hashes)
    }

    pub(super) fn raw_string(&mut self) -> SyntaxKind {
        let start = self.pos;
        let hashes = self.raw_string_start().expect("raw string start");
        self.pos += 2 + hashes;
        let close: String = iter::once('"').chain(iter::repeat_n('#', hashes)).collect();
        if let Some(at) = self.rest().find(&close) {
            self.pos += at + close.len();
        } else {
            self.pos = self.src.len();
            self.error(
                codes::S004,
                start,
                start + 2 + hashes,
                format!("unterminated raw string: missing `{close}`"),
            );
        }
        SyntaxKind::String
    }

    pub(super) fn string(&mut self) -> SyntaxKind {
        let start = self.pos;
        let triple = self.rest().starts_with("\"\"\"");
        if triple {
            self.pos += 3;
            if !(self.peek(0) == b'\n' || self.rest().starts_with("\r\n")) {
                self.error(
                    codes::S005,
                    start,
                    start + 3,
                    "a multi-line string starts with `\"\"\"` followed by a line break",
                );
            }
        } else {
            self.pos += 1;
        }
        match self.string_body(start, triple) {
            Body::Closed => SyntaxKind::String,
            Body::Hole => {
                self.holes.push(0);
                SyntaxKind::StringStart
            }
        }
    }

    /// The part of a string after the `}` of a hole: up to the next hole,
    /// or to the end of the string.
    pub(super) fn string_rest(&mut self) -> SyntaxKind {
        let start = self.pos;
        self.pos += 1;
        match self.string_body(start, false) {
            Body::Hole => SyntaxKind::StringMid,
            Body::Closed => {
                self.holes.pop();
                SyntaxKind::StringEnd
            }
        }
    }

    /// The text of a string from `pos` to its closing quote, past it, or to
    /// a `\{` that opens a hole, past it; `start` is where the token began.
    fn string_body(&mut self, start: usize, triple: bool) -> Body {
        loop {
            if self.pos >= self.src.len() || (!triple && matches!(self.peek(0), b'\n' | b'\r')) {
                let len = if triple { 3 } else { 1 };
                self.error(
                    codes::S004,
                    start,
                    start + len,
                    "unterminated string: missing closing `\"`",
                );
                return Body::Closed;
            }
            if triple && self.rest().starts_with("\"\"\"") {
                self.pos += 3;
                return Body::Closed;
            }
            match self.peek(0) {
                b'"' if !triple => {
                    self.pos += 1;
                    return Body::Closed;
                }
                b'\\' if self.peek(1) == b'{' => {
                    self.pos += 2;
                    if !triple {
                        return Body::Hole;
                    }
                    self.error(
                        codes::S005,
                        self.pos - 2,
                        self.pos,
                        "`\\{…}` interpolates only in a one-line string `\"…\"`",
                    );
                }
                b'\\' => self.escape(),
                _ => self.pos += self.rest().chars().next().map_or(1, char::len_utf8),
            }
        }
    }

    fn escape(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let bad = |lexer: &mut Self, message: &str| {
            let end = lexer.pos.max(start + 2).min(lexer.src.len());
            lexer.error(codes::S005, start, end, message.to_owned());
        };
        match self.peek(0) {
            b'n' | b't' | b'r' | b'\\' | b'"' => self.pos += 1,
            b'0' => {
                self.pos += 1;
                if self.peek(0).is_ascii_digit() {
                    bad(self, "`\\0` cannot be followed by a digit; use `\\u{…}`");
                }
            }
            b'u' => {
                self.pos += 1;
                if self.peek(0) != b'{' {
                    return bad(self, "expected `\\u{…}` with 1 to 6 hex digits");
                }
                self.pos += 1;
                let digits = self.src[self.pos..]
                    .iter()
                    .take_while(|b| b.is_ascii_hexdigit())
                    .count();
                self.pos += digits;
                let closed = self.peek(0) == b'}';
                if closed {
                    self.pos += 1;
                }
                if !closed || !(1..=6).contains(&digits) {
                    return bad(self, "expected `\\u{…}` with 1 to 6 hex digits");
                }
                let hex = &self.text[self.pos - 1 - digits..self.pos - 1];
                let value = u32::from_str_radix(hex, 16).unwrap_or(u32::MAX);
                if char::from_u32(value).is_none() {
                    bad(
                        self,
                        "`\\u{…}` must be a Unicode scalar value (no surrogates, at most 10FFFF)",
                    );
                }
            }
            0 | b'\n' | b'\r' => bad(self, "unknown escape sequence"),
            _ => {
                self.pos += self.rest().chars().next().map_or(1, char::len_utf8);
                bad(self, "unknown escape sequence");
            }
        }
    }
}

/// Where the text of a string stopped.
enum Body {
    /// At its closing quote, or where a missing one was reported.
    Closed,
    /// After the `\{` of a hole.
    Hole,
}
