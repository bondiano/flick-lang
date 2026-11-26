//! Lexer: splits source text into tokens that cover every byte (trivia
//! included), so the tree built over them is lossless. Malformed literals still
//! become one token plus a diagnostic, so the parser sees no cascade.

use fln_literals::{CommentKind, scan_block_comment_recover, scan_line_comment};

use super::kind::{SyntaxKind, keyword};
use crate::diagnostics::{Code, Diagnostic, codes};
use crate::source::{FileId, Span};

mod numbers;
mod strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawToken {
    pub kind: SyntaxKind,
    pub start: u32,
    pub end: u32,
}

pub fn lex(text: &str, file: FileId) -> (Vec<RawToken>, Vec<Diagnostic>) {
    lex_with(text, file, false)
}

/// [`lex`] of code a plugin generated, where `Owner$member`, the companion
/// `Owner.member` of a type, is one name: no source writes it.
pub fn lex_generated(text: &str, file: FileId) -> (Vec<RawToken>, Vec<Diagnostic>) {
    lex_with(text, file, true)
}

fn lex_with(text: &str, file: FileId, generated: bool) -> (Vec<RawToken>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        src: text.as_bytes(),
        text,
        pos: 0,
        file,
        generated,
        tokens: Vec::with_capacity(text.len() / 3 + 1),
        errors: Vec::new(),
        holes: Vec::new(),
    };
    while lexer.pos < lexer.src.len() {
        let start = lexer.pos;
        let kind = lexer.token();
        debug_assert!(lexer.pos > start, "lexer makes progress");
        // a hole ends with the line of its string, also inside a comment
        // or a multi-line string that crosses it
        if !lexer.holes.is_empty() && lexer.src[start..lexer.pos].contains(&b'\n') {
            lexer.holes.clear();
        }
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
    generated: bool,
    tokens: Vec<RawToken>,
    errors: Vec<Diagnostic>,
    /// The `\{…}` of strings open around `pos`, innermost last: how many
    /// `{` each has open, so its own `}` resumes the string.
    holes: Vec<u32>,
}

fn offset(n: usize) -> u32 {
    u32::try_from(n).expect("source offsets fit in u32")
}

#[inline]
const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn push_error(
    errors: &mut Vec<Diagnostic>,
    file: FileId,
    code: Code,
    start: usize,
    end: usize,
    message: impl Into<String>,
) {
    errors.push(Diagnostic::error(
        code,
        message,
        Span::new(file, offset(start), offset(end)),
    ));
}

impl Lexer<'_> {
    #[inline]
    fn peek(&self, n: usize) -> u8 {
        self.src.get(self.pos + n).copied().unwrap_or(0)
    }

    #[inline]
    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn error(&mut self, code: Code, start: usize, end: usize, message: impl Into<String>) {
        push_error(&mut self.errors, self.file, code, start, end, message);
    }

    #[inline]
    fn token(&mut self) -> SyntaxKind {
        use SyntaxKind as K;
        let start = self.pos;
        let bytes = self.src;
        let len = bytes.len();
        let b = bytes[start];
        if let Some(&open) = self.holes.last() {
            match b {
                b'}' if open == 0 => return self.string_rest(),
                // a hole ends with the line of its string
                b'\n' | b'\r' if b == b'\n' || self.peek(1) == b'\n' => self.holes.clear(),
                _ => {}
            }
        }
        match b {
            b' ' => {
                let mut p = start + 1;
                while p < len && bytes[p] == b' ' {
                    p += 1;
                }
                self.pos = p;
                if p < len && bytes[p] == b'\t' {
                    self.scan_whitespace_with_tabs();
                }
                K::Whitespace
            }
            b'\t' => {
                self.scan_whitespace_with_tabs();
                K::Whitespace
            }
            b'\n' => {
                self.pos = start + 1;
                K::Newline
            }
            b'\r' if bytes.get(start + 1) == Some(&b'\n') => {
                self.pos = start + 2;
                K::Newline
            }
            b'/' if bytes.get(start + 1) == Some(&b'/') => {
                let (end, kind) = scan_line_comment(self.text, start).expect("starts with //");
                self.pos = end;
                match kind {
                    CommentKind::Doc => K::DocComment,
                    CommentKind::Line | CommentKind::Block => K::LineComment,
                }
            }
            b'/' if bytes.get(start + 1) == Some(&b'*') => self.block_comment(),
            b'\'' if matches!(bytes.get(start + 1), Some(b'a'..=b'z' | b'_')) => self.label(start),
            b'"' => self.string(),
            b'r' if self.raw_string_start().is_some() => self.raw_string(),
            b'0'..=b'9' => self.number(),
            b'.' if bytes.get(start + 1).is_some_and(u8::is_ascii_digit) => self.number(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.ident(start, b),
            _ => self.symbol(start),
        }
    }

    fn label(&mut self, start: usize) -> SyntaxKind {
        let bytes = self.src;
        let len = bytes.len();
        let mut p = start + 2;
        if self.generated {
            while p < len {
                if is_ident_byte(bytes[p]) {
                    p += 1;
                } else if bytes[p] == b'$' && bytes.get(p + 1).copied().is_some_and(is_ident_byte) {
                    p += 2;
                } else {
                    break;
                }
            }
        } else {
            while p < len && is_ident_byte(bytes[p]) {
                p += 1;
            }
        }
        self.pos = p;
        SyntaxKind::Label
    }

    fn ident(&mut self, start: usize, first: u8) -> SyntaxKind {
        use SyntaxKind as K;
        let bytes = self.src;
        let len = bytes.len();
        let mut p = start + 1;
        if self.generated {
            while p < len {
                if is_ident_byte(bytes[p]) {
                    p += 1;
                } else if bytes[p] == b'$' && bytes.get(p + 1).copied().is_some_and(is_ident_byte) {
                    p += 2;
                } else {
                    break;
                }
            }
        } else {
            while p < len && is_ident_byte(bytes[p]) {
                p += 1;
            }
        }
        self.pos = p;
        let text = &self.text[start..p];
        // `Owner$member` is named by its member
        let lead = if self.generated {
            text.rsplit('$').next().map_or(first, |m| m.as_bytes()[0])
        } else {
            first
        };
        if text == "_" {
            K::Underscore
        } else if lead.is_ascii_uppercase() {
            K::Con
        } else if let Some(kind) = keyword(text) {
            if self.member_name(start) {
                K::Ident
            } else {
                kind
            }
        } else {
            K::Ident
        }
    }

    fn scan_whitespace_with_tabs(&mut self) {
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
    }

    fn symbol(&mut self, start: usize) -> SyntaxKind {
        use SyntaxKind as K;
        if let Some((len, kind)) = self.punct() {
            self.pos += len;
            if let Some(open) = self.holes.last_mut() {
                match kind {
                    K::LBrace => *open += 1,
                    K::RBrace => *open -= 1,
                    _ => {}
                }
            }
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

    /// Whether the word at `start` names a value even if it spells a
    /// keyword, as `HashSet.with` does: right after `Qualifier.`, as the
    /// name of a top-level signature (`with : …` at the start of a line), of
    /// a `let` or of a `foreign` declaration.
    fn member_name(&self, start: usize) -> bool {
        let word = &self.text[start..self.pos];
        if word == "orelse" {
            return false;
        }
        let bytes = self.src;
        if start >= 2 && bytes[start - 1] == b'.' && is_ident_byte(bytes[start - 2]) {
            return true;
        }
        let line_start = start == 0 || bytes[start - 1] == b'\n';
        if line_start {
            let rest = self.text[self.pos..].trim_start_matches(' ');
            return rest.starts_with(':') && !rest.starts_with("::");
        }
        // the debugging keywords stay usable as labels: `{ echo = … }`,
        // `(make ()).echo`, the accessor `.echo` and the optional step
        // `.?echo`, but not after the dots of a range (`0..echo x`)
        if matches!(word, "todo" | "echo") {
            let dot =
                |at: usize| at >= 1 && bytes[at - 1] == b'.' && (at == 1 || bytes[at - 2] != b'.');
            if dot(start) || start >= 1 && bytes[start - 1] == b'?' && dot(start - 1) {
                return true;
            }
            let rest = self.text[self.pos..].trim_start_matches(' ');
            if rest.starts_with(':') && !rest.starts_with("::")
                || rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>")
            {
                return true;
            }
        }
        let line_begin = bytes[..start]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        let mut before = self.text[line_begin..start].split_whitespace();
        let Some(last) = before.next_back() else {
            return false;
        };
        match last {
            "foreign" => {
                let foreign_preamble = word == "module"
                    && self.text[self.pos..]
                        .trim_start_matches([' ', '\t'])
                        .strip_prefix("inline")
                        .is_some_and(|rest| {
                            rest.as_bytes().first().is_none_or(|b| !is_ident_byte(*b))
                        });
                !foreign_preamble
            }
            "pure" | "sync" | "async" | "effect" => before.next_back() == Some("foreign"),
            "let" => {
                before.next_back().is_none()
                    && !matches!(word, "rec" | "assert" | "mut" | "lazy" | "catch")
            }
            "rec" | "lazy" => {
                before.next_back() == Some("let")
                    && before.next_back().is_none()
                    && !matches!(word, "rec" | "assert" | "mut" | "lazy" | "catch")
            }
            "mut" => {
                let is_let_head = match before.next_back() {
                    Some("let") => before.next_back().is_none(),
                    Some("lazy") => {
                        before.next_back() == Some("let") && before.next_back().is_none()
                    }
                    _ => false,
                };
                is_let_head && !matches!(word, "rec" | "assert" | "mut" | "lazy" | "catch")
            }
            _ => false,
        }
    }

    /// Length and kind of the operator or delimiter at `pos`, if any.
    fn punct(&self) -> Option<(usize, SyntaxKind)> {
        use SyntaxKind as K;
        let b0 = self.peek(0);
        let hit = match b0 {
            b'(' => (1, K::LParen),
            b')' => (1, K::RParen),
            b'[' => (1, K::LBracket),
            b']' => (1, K::RBracket),
            b'{' => (1, K::LBrace),
            b'}' => (1, K::RBrace),
            b',' => (1, K::Comma),
            b';' => (1, K::Semi),
            b':' => (1, K::Colon),
            b'/' => (1, K::Slash),
            b'%' => (1, K::Percent),
            b'^' => (1, K::Caret),
            b'$' => (1, K::Dollar),
            b'?' => (1, K::Question),
            b'@' => (1, K::At),
            b'~' => (1, K::Tilde),
            b'.' => match (self.peek(1), self.peek(2)) {
                (b'.', b'.') => (3, K::DotDotDot),
                (b'.', b'<') => (3, K::DotDotLt),
                (b'.', _) => (2, K::DotDot),
                _ => (1, K::Dot),
            },
            b'=' => match self.peek(1) {
                b'>' => (2, K::FatArrow),
                b'=' => (2, K::EqEq),
                _ => (1, K::Eq),
            },
            b'-' => match self.peek(1) {
                b'>' => (2, K::Arrow),
                _ => (1, K::Minus),
            },
            b'|' => match self.peek(1) {
                b'>' => (2, K::PipeGt),
                b'|' => (2, K::OrOr),
                _ => (1, K::Pipe),
            },
            b'<' => match self.peek(1) {
                b'|' => (2, K::LtPipe),
                b'<' => (2, K::LtLt),
                b'=' => (2, K::Le),
                b'-' => (2, K::LArrow),
                _ => (1, K::Lt),
            },
            b'>' => match self.peek(1) {
                b'>' => (2, K::GtGt),
                b'=' => (2, K::Ge),
                _ => (1, K::Gt),
            },
            b'&' if self.peek(1) == b'&' => (2, K::AndAnd),
            b'!' => match self.peek(1) {
                b'=' => (2, K::NotEq),
                _ => (1, K::Bang),
            },
            b'+' => match self.peek(1) {
                b'+' => (2, K::PlusPlus),
                _ => (1, K::Plus),
            },
            b'*' => match self.peek(1) {
                b'*' => (2, K::StarStar),
                _ => (1, K::Star),
            },
            _ => return None,
        };
        Some(hit)
    }

    fn block_comment(&mut self) -> SyntaxKind {
        let file = self.file;
        let errors = &mut self.errors;
        self.pos = scan_block_comment_recover(self.text, self.pos, |err| {
            push_error(
                errors,
                file,
                codes::S004,
                err.start,
                err.start + 2,
                "unterminated block comment: missing `*/`",
            );
        });
        SyntaxKind::BlockComment
    }
}

fn plural(s: &str) -> &'static str {
    if s.chars().count() > 1 { "s" } else { "" }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod label_tests {
    use super::*;
    use crate::source::SourceMap;

    #[test]
    fn lex_loop_label() {
        let mut map = SourceMap::default();
        let file = map.add("t.fl", "'outer 'inner_1 'a");
        let (tokens, errors) = lex("'outer 'inner_1 'a", file);
        assert!(errors.is_empty());
        let kinds: Vec<_> = tokens
            .into_iter()
            .filter(|t| !t.kind.is_trivia())
            .map(|t| t.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![SyntaxKind::Label, SyntaxKind::Label, SyntaxKind::Label]
        );
    }

    #[test]
    fn stray_quote_is_error() {
        let mut map = SourceMap::default();
        let file = map.add("t.fl", "'123");
        let (_tokens, errors) = lex("'123", file);
        assert!(!errors.is_empty());
    }
}
