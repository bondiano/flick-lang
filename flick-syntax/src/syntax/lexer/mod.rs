//! Lexer: splits source text into tokens that cover every byte (trivia
//! included), so the tree built over them is lossless. Malformed literals still
//! become one token plus a diagnostic, so the parser sees no cascade.

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
        tokens: Vec::new(),
        errors: Vec::new(),
        holes: Vec::new(),
    };
    while lexer.pos < lexer.src.len() {
        let start = lexer.pos;
        let kind = lexer.token();
        debug_assert!(lexer.pos > start, "lexer makes progress");
        // a hole ends with the line of its string, also inside a comment
        // or a multi-line string that crosses it
        if !lexer.holes.is_empty() && text[start..lexer.pos].contains('\n') {
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
        if let Some(&open) = self.holes.last() {
            match b {
                b'}' if open == 0 => return self.string_rest(),
                // a hole ends with the line of its string
                b'\n' | b'\r' if b == b'\n' || self.peek(1) == b'\n' => self.holes.clear(),
                _ => {}
            }
        }
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
            b'\'' if matches!(self.peek(1), b'a'..=b'z' | b'_') => {
                self.pos += 1;
                while is_ident_byte(self.peek(0))
                    || (self.generated && self.peek(0) == b'$' && is_ident_byte(self.peek(1)))
                {
                    self.pos += 1;
                }
                K::Label
            }
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
                while is_ident_byte(self.peek(0))
                    || (self.generated && self.peek(0) == b'$' && is_ident_byte(self.peek(1)))
                {
                    self.pos += 1;
                }
                let text = &self.text[start..self.pos];
                // `Owner$member` is named by its member
                let b = text.rsplit('$').next().map_or(b, |m| m.as_bytes()[0]);
                if text == "_" {
                    K::Underscore
                } else if b.is_ascii_uppercase() {
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
            _ => self.symbol(start),
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
        let bytes = self.text.as_bytes();
        if start >= 2 && bytes[start - 1] == b'.' && is_ident_byte(bytes[start - 2]) {
            return true;
        }
        let line_start = start == 0 || bytes[start - 1] == b'\n';
        if line_start {
            let rest = self.text[self.pos..].trim_start_matches(' ');
            return rest.starts_with(':') && !rest.starts_with("::");
        }
        let before: Vec<&str> = self.text[..start]
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .collect();
        // the debugging keywords stay usable as labels: `{ echo = … }`
        if matches!(word, "todo" | "echo") {
            let rest = self.text[self.pos..].trim_start_matches(' ');
            if rest.starts_with(':') && !rest.starts_with("::")
                || rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>")
            {
                return true;
            }
        }
        let foreign_preamble = word == "module"
            && matches!(before[..], [.., "foreign"])
            && self.text[self.pos..]
                .trim_start_matches([' ', '\t'])
                .strip_prefix("inline")
                .is_some_and(|rest| rest.as_bytes().first().is_none_or(|b| !is_ident_byte(*b)));
        (!foreign_preamble
            && matches!(
                before[..],
                [.., "foreign"] | [.., "foreign", "pure" | "sync" | "async" | "effect"]
            ))
            || (matches!(
                before[..],
                ["let"] | ["let", "rec" | "mut" | "lazy"] | ["let", "lazy", "mut"]
            ) && !matches!(word, "rec" | "assert" | "mut" | "lazy" | "catch"))
    }

    /// Length and kind of the operator or delimiter at `pos`, if any.
    fn punct(&self) -> Option<(usize, SyntaxKind)> {
        use SyntaxKind as K;
        const TABLE: [(&str, SyntaxKind); 43] = [
            ("...", K::DotDotDot),
            ("..<", K::DotDotLt),
            ("..", K::DotDot),
            ("->", K::Arrow),
            ("=>", K::FatArrow),
            ("==", K::EqEq),
            ("|>", K::PipeGt),
            ("<|", K::LtPipe),
            (">>", K::GtGt),
            ("<<", K::LtLt),
            ("||", K::OrOr),
            ("&&", K::AndAnd),
            ("!=", K::NotEq),
            ("!", K::Bang),
            ("<=", K::Le),
            ("<-", K::LArrow),
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
            ("$", K::Dollar),
            ("?", K::Question),
            ("@", K::At),
            ("~", K::Tilde),
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
