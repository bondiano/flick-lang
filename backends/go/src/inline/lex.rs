//! Zero-dependency Go lexer with automatic semicolon insertion for
//! validating inline foreign Go bindings and preambles.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ident,
    Number,
    String,
    Rune,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Ellipsis,
    Semi,
    Colon,
    Define,
    Assign,
    AssignOp,
    Arrow,
    Inc,
    Dec,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    AndNot,
    Shl,
    Shr,
    AmpAmp,
    PipePipe,
    Bang,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Tilde,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
    /// True when this `Kind::Semi` was inserted automatically by Go's ASI rule.
    pub implicit: bool,
}

pub fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
    let mut lexer = Lexer {
        src,
        bytes: src.as_bytes(),
        pos: 0,
        insert_semi: false,
    };
    lexer.run()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    insert_semi: bool,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) -> Result<Vec<Token<'a>>, LexError> {
        let mut out = Vec::new();
        loop {
            if let Some(semi) = self.skip_ws_and_comments()? {
                out.push(semi);
                self.insert_semi = false;
                continue;
            }
            if self.pos >= self.bytes.len() {
                if self.insert_semi {
                    out.push(Token {
                        kind: Kind::Semi,
                        text: ";",
                        start: self.pos,
                        end: self.pos,
                        implicit: true,
                    });
                    self.insert_semi = false;
                }
                out.push(Token {
                    kind: Kind::Eof,
                    text: "",
                    start: self.pos,
                    end: self.pos,
                    implicit: false,
                });
                return Ok(out);
            }
            let tok = self.next_token()?;
            self.insert_semi = ends_with_asi(&tok);
            out.push(tok);
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn skip_ws_and_comments(&mut self) -> Result<Option<Token<'a>>, LexError> {
        loop {
            while let Some(b) = self.peek() {
                match b {
                    b' ' | b'\t' | b'\r' => self.pos += 1,
                    b'\n' => {
                        let at = self.pos;
                        self.pos += 1;
                        if self.insert_semi {
                            return Ok(Some(Token {
                                kind: Kind::Semi,
                                text: ";",
                                start: at,
                                end: at,
                                implicit: true,
                            }));
                        }
                    }
                    0xef if self.bytes[self.pos..].starts_with(&[0xef, 0xbb, 0xbf]) => {
                        self.pos += 3;
                    }
                    _ => break,
                }
            }
            if self.peek() == Some(b'/') && self.peek_at(1) == Some(b'/') {
                self.pos += 2;
                while let Some(b) = self.peek() {
                    if b == b'\n' {
                        break;
                    }
                    self.pos += 1;
                }
                continue;
            }
            if self.peek() == Some(b'/') && self.peek_at(1) == Some(b'*') {
                let start = self.pos;
                self.pos += 2;
                let mut saw_nl = false;
                let mut closed = false;
                while self.pos + 1 < self.bytes.len() {
                    if self.bytes[self.pos] == b'\n' {
                        saw_nl = true;
                    }
                    if self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/' {
                        self.pos += 2;
                        closed = true;
                        break;
                    }
                    self.pos += 1;
                }
                if !closed {
                    return Err(lex_err(start, start + 2, "unterminated block comment"));
                }
                if saw_nl && self.insert_semi {
                    return Ok(Some(Token {
                        kind: Kind::Semi,
                        text: ";",
                        start: self.pos,
                        end: self.pos,
                        implicit: true,
                    }));
                }
                continue;
            }
            return Ok(None);
        }
    }

    fn next_token(&mut self) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        let b = self.bytes[self.pos];
        match b {
            b'"' => self.lex_string(),
            b'`' => self.lex_raw_string(),
            b'\'' => self.lex_rune(),
            b'0'..=b'9' => self.lex_number(),
            b'.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.lex_number(),
            b if b.is_ascii_alphabetic() || b == b'_' => {
                self.consume_ident();
                Ok(self.tok(Kind::Ident, start))
            }
            b if !b.is_ascii() => {
                let Some(ch) = self.src[self.pos..].chars().next() else {
                    return Err(lex_err(start, self.pos, "unexpected end of input in Go"));
                };
                if ch.is_alphabetic() {
                    self.consume_ident();
                    Ok(self.tok(Kind::Ident, start))
                } else {
                    Err(lex_err(
                        start,
                        start + ch.len_utf8(),
                        format!("unexpected character `{ch}` in Go"),
                    ))
                }
            }
            _ => self.lex_punct(),
        }
    }

    fn tok(&self, kind: Kind, start: usize) -> Token<'a> {
        Token {
            kind,
            text: &self.src[start..self.pos],
            start,
            end: self.pos,
            implicit: false,
        }
    }

    fn lex_string(&mut self) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        self.pos += 1;
        while let Some(b) = self.peek() {
            if b == b'"' {
                self.pos += 1;
                return Ok(self.tok(Kind::String, start));
            }
            if b == b'\\' {
                self.pos += 1;
                if self.peek().is_none() || self.peek() == Some(b'\n') {
                    return Err(lex_err(start, self.pos, "unterminated Go string literal"));
                }
                self.pos += 1;
                continue;
            }
            if b == b'\n' {
                return Err(lex_err(start, self.pos, "unterminated Go string literal"));
            }
            self.pos += 1;
        }
        Err(lex_err(start, self.pos, "unterminated Go string literal"))
    }

    fn lex_raw_string(&mut self) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        self.pos += 1;
        while let Some(b) = self.peek() {
            self.pos += 1;
            if b == b'`' {
                return Ok(self.tok(Kind::String, start));
            }
        }
        Err(lex_err(
            start,
            self.pos,
            "unterminated Go raw string literal",
        ))
    }

    fn lex_rune(&mut self) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        self.pos += 1;
        let mut count = 0usize;
        while let Some(b) = self.peek() {
            if b == b'\'' {
                self.pos += 1;
                if count == 0 {
                    return Err(lex_err(start, self.pos, "empty Go rune literal"));
                }
                return Ok(self.tok(Kind::Rune, start));
            }
            if b == b'\n' {
                return Err(lex_err(start, self.pos, "unterminated Go rune literal"));
            }
            if b == b'\\' {
                self.pos += 1;
                if self.peek().is_none() || self.peek() == Some(b'\n') {
                    return Err(lex_err(start, self.pos, "unterminated Go rune literal"));
                }
                self.pos += 1;
                count += 1;
                continue;
            }
            let Some(ch) = self.src[self.pos..].chars().next() else {
                break;
            };
            self.pos += ch.len_utf8();
            count += 1;
        }
        Err(lex_err(start, self.pos, "unterminated Go rune literal"))
    }

    fn lex_number(&mut self) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        if self.peek() == Some(b'0')
            && let Some(prefix @ (b'x' | b'X' | b'b' | b'B' | b'o' | b'O')) = self.peek_at(1)
        {
            self.pos += 2;
            let digits = self.pos;
            while let Some(b) = self.peek() {
                let ok = match prefix {
                    b'x' | b'X' => b.is_ascii_hexdigit() || b == b'_',
                    b'b' | b'B' => b == b'0' || b == b'1' || b == b'_',
                    _ => (b'0'..=b'7').contains(&b) || b == b'_',
                };
                if ok {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == digits {
                return Err(lex_err(start, self.pos, "invalid Go numeric literal"));
            }
            if matches!(prefix, b'x' | b'X') && self.peek() == Some(b'.') {
                self.pos += 1;
                while self
                    .peek()
                    .is_some_and(|b| b.is_ascii_hexdigit() || b == b'_')
                {
                    self.pos += 1;
                }
            }
            if matches!(prefix, b'x' | b'X') && matches!(self.peek(), Some(b'p' | b'P')) {
                self.consume_exponent(start)?;
            }
            if self.peek() == Some(b'i') {
                self.pos += 1;
            }
            return Ok(self.tok(Kind::Number, start));
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') && self.peek_at(1) != Some(b'.') {
            self.pos += 1;
            while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.consume_exponent(start)?;
        }
        if self.peek() == Some(b'i') {
            self.pos += 1;
        }
        Ok(self.tok(Kind::Number, start))
    }

    fn consume_exponent(&mut self, start: usize) -> Result<(), LexError> {
        self.pos += 1;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        let digits = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
            self.pos += 1;
        }
        if self.pos == digits {
            return Err(lex_err(
                start,
                self.pos,
                "missing exponent digits in Go number",
            ));
        }
        Ok(())
    }

    fn consume_ident(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else if !b.is_ascii() {
                let Some(ch) = self.src[self.pos..].chars().next() else {
                    break;
                };
                if ch.is_alphanumeric() {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn lex_punct(&mut self) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        let rest = &self.src[self.pos..];
        let (len, kind) = if rest.starts_with("...") {
            (3, Kind::Ellipsis)
        } else if rest.starts_with("<<=") || rest.starts_with(">>=") || rest.starts_with("&^=") {
            (3, Kind::AssignOp)
        } else if rest.starts_with(":=") {
            (2, Kind::Define)
        } else if rest.starts_with("<-") {
            (2, Kind::Arrow)
        } else if rest.starts_with("++") {
            (2, Kind::Inc)
        } else if rest.starts_with("--") {
            (2, Kind::Dec)
        } else if rest.starts_with("&&") {
            (2, Kind::AmpAmp)
        } else if rest.starts_with("||") {
            (2, Kind::PipePipe)
        } else if rest.starts_with("==") {
            (2, Kind::Eq)
        } else if rest.starts_with("!=") {
            (2, Kind::Ne)
        } else if rest.starts_with("<=") {
            (2, Kind::Le)
        } else if rest.starts_with(">=") {
            (2, Kind::Ge)
        } else if rest.starts_with("<<") {
            (2, Kind::Shl)
        } else if rest.starts_with(">>") {
            (2, Kind::Shr)
        } else if rest.starts_with("&^") {
            (2, Kind::AndNot)
        } else if rest.starts_with("+=")
            || rest.starts_with("-=")
            || rest.starts_with("*=")
            || rest.starts_with("/=")
            || rest.starts_with("%=")
            || rest.starts_with("&=")
            || rest.starts_with("|=")
            || rest.starts_with("^=")
        {
            (2, Kind::AssignOp)
        } else {
            let kind = match self.bytes[self.pos] {
                b'(' => Kind::LParen,
                b')' => Kind::RParen,
                b'{' => Kind::LBrace,
                b'}' => Kind::RBrace,
                b'[' => Kind::LBracket,
                b']' => Kind::RBracket,
                b',' => Kind::Comma,
                b'.' => Kind::Dot,
                b';' => Kind::Semi,
                b':' => Kind::Colon,
                b'=' => Kind::Assign,
                b'+' => Kind::Plus,
                b'-' => Kind::Minus,
                b'*' => Kind::Star,
                b'/' => Kind::Slash,
                b'%' => Kind::Percent,
                b'&' => Kind::Amp,
                b'|' => Kind::Pipe,
                b'^' => Kind::Caret,
                b'!' => Kind::Bang,
                b'<' => Kind::Lt,
                b'>' => Kind::Gt,
                b'~' => Kind::Tilde,
                b => {
                    return Err(lex_err(
                        start,
                        start + 1,
                        format!("unexpected character `{}` in Go", b as char),
                    ));
                }
            };
            (1, kind)
        };
        self.pos += len;
        Ok(self.tok(kind, start))
    }
}

fn lex_err(start: usize, end: usize, msg: impl Into<String>) -> LexError {
    LexError {
        message: msg.into(),
        start,
        end: end.max(start),
    }
}

fn ends_with_asi(tok: &Token<'_>) -> bool {
    match tok.kind {
        Kind::Ident => !matches!(
            tok.text,
            "case"
                | "chan"
                | "const"
                | "default"
                | "defer"
                | "else"
                | "for"
                | "func"
                | "go"
                | "goto"
                | "if"
                | "import"
                | "interface"
                | "map"
                | "package"
                | "range"
                | "select"
                | "struct"
                | "switch"
                | "type"
                | "var"
        ),
        Kind::Number
        | Kind::String
        | Kind::Rune
        | Kind::Inc
        | Kind::Dec
        | Kind::RParen
        | Kind::RBracket
        | Kind::RBrace => true,
        _ => false,
    }
}
