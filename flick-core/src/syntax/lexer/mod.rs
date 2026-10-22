//! Lexer: splits source text into tokens that cover every byte (trivia
//! included), so the tree built over them is lossless. Malformed literals still
//! become one token plus a diagnostic, so the parser sees no cascade.

use super::kind::{SyntaxKind, keyword};
use crate::diagnostics::{Code, Diagnostic, codes};
use crate::source::{FileId, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawToken {
    pub kind: SyntaxKind,
    pub start: u32,
    pub end: u32,
}

pub fn lex(text: &str, file: FileId) -> (Vec<RawToken>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        src: text.as_bytes(),
        text,
        pos: 0,
        file,
        tokens: Vec::new(),
        errors: Vec::new(),
    };
    while lexer.pos < lexer.src.len() {
        let start = lexer.pos;
        let kind = lexer.token();
        debug_assert!(lexer.pos > start, "lexer makes progress");
        lexer.tokens.push(RawToken {
            kind,
            start: offset(start),
            end: offset(lexer.pos),
        });
    }
    (lexer.tokens, lexer.errors)
}

struct Lexer<'a> {
    src: &'a [u8],
    text: &'a str,
    pos: usize,
    file: FileId,
    tokens: Vec<RawToken>,
    errors: Vec<Diagnostic>,
}

fn offset(n: usize) -> u32 {
    u32::try_from(n).expect("source offsets fit in u32")
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

impl Lexer<'_> {
    fn peek(&self, n: usize) -> u8 {
        self.src.get(self.pos + n).copied().unwrap_or(0)
    }

    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn error(&mut self, code: Code, start: usize, end: usize, message: impl Into<String>) {
        self.errors.push(Diagnostic::error(
            code,
            message,
            Span::new(self.file, offset(start), offset(end)),
        ));
    }

    fn token(&mut self) -> SyntaxKind {
        use SyntaxKind as K;
        let start = self.pos;
        let b = self.peek(0);
        match b {
            b' ' | b'\t' => {
                while matches!(self.peek(0), b' ' | b'\t') {
                    if self.peek(0) == b'\t' {
                        self.error(
                            codes::S009,
                            self.pos,
                            self.pos + 1,
                            "tab character: Flick layout is indented with spaces",
                        );
                    }
                    self.pos += 1;
                }
                K::Whitespace
            }
            b'\n' => {
                self.pos += 1;
                K::Newline
            }
            b'\r' if self.peek(1) == b'\n' => {
                self.pos += 2;
                K::Newline
            }
            b'/' if self.peek(1) == b'/' => {
                let doc = self.rest().starts_with("///") && !self.rest().starts_with("////");
                while self.pos < self.src.len()
                    && self.peek(0) != b'\n'
                    && !self.rest().starts_with("\r\n")
                {
                    self.pos += self.rest().chars().next().map_or(1, char::len_utf8);
                }
                if doc { K::DocComment } else { K::LineComment }
            }
            b'/' if self.peek(1) == b'*' => self.block_comment(),
            b'"' => self.string(),
            b'r' if self.raw_string_start().is_some() => self.raw_string(),
            b'0'..=b'9' => self.number(),
            b'.' if self.peek(1).is_ascii_digit() => {
                self.pos += 1;
                self.number();
                self.error(
                    codes::S006,
                    start,
                    self.pos,
                    "a number cannot start with `.`; write a leading `0`, e.g. `0.5`",
                );
                K::Number
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                while is_ident_byte(self.peek(0)) {
                    self.pos += 1;
                }
                let text = &self.text[start..self.pos];
                if text == "_" {
                    K::Underscore
                } else if b.is_ascii_uppercase() {
                    K::Con
                } else {
                    keyword(text).unwrap_or(K::Ident)
                }
            }
            _ => {
                if let Some((len, kind)) = self.punct() {
                    self.pos += len;
                    return kind;
                }
                // one token and one diagnostic for a whole run of stray characters
                loop {
                    self.pos += self.rest().chars().next().map_or(1, char::len_utf8);
                    let next = self.peek(0);
                    let known = next.is_ascii_alphanumeric()
                        || matches!(next, b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'_')
                        || self.punct().is_some();
                    if self.pos >= self.src.len() || known {
                        break;
                    }
                }
                let shown = &self.text[start..self.pos];
                self.error(
                    codes::S003,
                    start,
                    self.pos,
                    format!("unexpected character{} `{shown}`", plural(shown)),
                );
                K::Unknown
            }
        }
    }

    /// Length and kind of the operator or delimiter at `pos`, if any.
    fn punct(&self) -> Option<(usize, SyntaxKind)> {
        use SyntaxKind as K;
        const TABLE: [(&str, SyntaxKind); 32] = [
            ("..", K::DotDot),
            ("->", K::Arrow),
            ("=>", K::FatArrow),
            ("==", K::EqEq),
            ("|>", K::PipeGt),
            ("||", K::OrOr),
            ("&&", K::AndAnd),
            ("!=", K::NotEq),
            ("<=", K::Le),
            (">=", K::Ge),
            ("++", K::PlusPlus),
            ("**", K::StarStar),
            ("(", K::LParen),
            (")", K::RParen),
            ("[", K::LBracket),
            ("]", K::RBracket),
            ("{", K::LBrace),
            ("}", K::RBrace),
            (",", K::Comma),
            (";", K::Semi),
            (":", K::Colon),
            (".", K::Dot),
            ("=", K::Eq),
            ("|", K::Pipe),
            ("<", K::Lt),
            (">", K::Gt),
            ("+", K::Plus),
            ("-", K::Minus),
            ("*", K::Star),
            ("/", K::Slash),
            ("%", K::Percent),
            ("^", K::Caret),
        ];
        let rest = self.rest();
        let (text, kind) = TABLE.iter().find(|(text, _)| rest.starts_with(text))?;
        Some((text.len(), *kind))
    }

    fn block_comment(&mut self) -> SyntaxKind {
        let start = self.pos;
        self.pos += 2;
        let mut depth = 1;
        while depth > 0 {
            if self.pos >= self.src.len() {
                self.error(
                    codes::S004,
                    start,
                    start + 2,
                    "unterminated block comment: missing `*/`",
                );
                break;
            }
            if self.rest().starts_with("/*") {
                depth += 1;
                self.pos += 2;
            } else if self.rest().starts_with("*/") {
                depth -= 1;
                self.pos += 2;
            } else {
                self.pos += self.rest().chars().next().map_or(1, char::len_utf8);
            }
        }
        SyntaxKind::BlockComment
    }

    /// Number of `#` of a raw string opening at `pos` (`r"` or `r#…#"`).
    fn raw_string_start(&self) -> Option<usize> {
        let hashes = self.src[self.pos + 1..]
            .iter()
            .take_while(|&&b| b == b'#')
            .count();
        (self.peek(1 + hashes) == b'"').then_some(hashes)
    }

    fn raw_string(&mut self) -> SyntaxKind {
        let start = self.pos;
        let hashes = self.raw_string_start().expect("raw string start");
        self.pos += 2 + hashes;
        let close: String = std::iter::once('"')
            .chain(std::iter::repeat_n('#', hashes))
            .collect();
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

    fn string(&mut self) -> SyntaxKind {
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
        loop {
            if self.pos >= self.src.len() || (!triple && matches!(self.peek(0), b'\n' | b'\r')) {
                let len = if triple { 3 } else { 1 };
                self.error(
                    codes::S004,
                    start,
                    start + len,
                    "unterminated string: missing closing `\"`",
                );
                break;
            }
            if triple && self.rest().starts_with("\"\"\"") {
                self.pos += 3;
                break;
            }
            match self.peek(0) {
                b'"' if !triple => {
                    self.pos += 1;
                    break;
                }
                b'\\' => self.escape(),
                _ => self.pos += self.rest().chars().next().map_or(1, char::len_utf8),
            }
        }
        SyntaxKind::String
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

    /// JS `NumericLiteral` minus the `.5` and `5.` forms, which would clash with
    /// `.field` access.
    fn number(&mut self) -> SyntaxKind {
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

fn plural(s: &str) -> &'static str {
    if s.chars().count() > 1 { "s" } else { "" }
}

#[cfg(test)]
mod tests;
