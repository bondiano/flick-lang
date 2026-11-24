//! Zero-dependency JavaScript lexer for validating inline foreign code.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ident,
    PrivateIdent,
    Number,
    String,
    Regex,
    TemplateNoSub,
    TemplateHead,
    TemplateMiddle,
    TemplateTail,
    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    // Punctuators
    Dot,
    DotDotDot,
    Comma,
    Semi,
    Colon,
    Question,
    QuestionDot,
    Arrow,
    Assign,
    AssignOp,
    PlusPlus,
    MinusMinus,
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Bang,
    AmpAmp,
    PipePipe,
    QuestionQuestion,
    EqEq,
    EqEqEq,
    BangEq,
    BangEqEq,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    UShr,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
    pub newline_before: bool,
}

pub fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
    let mut lexer = Lexer {
        src,
        bytes: src.as_bytes(),
        pos: 0,
        regex_allowed: true,
        template_braces: Vec::new(),
        newline_before: false,
    };
    lexer.run()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    regex_allowed: bool,
    template_braces: Vec<usize>,
    newline_before: bool,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) -> Result<Vec<Token<'a>>, LexError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws_and_comments()?;
            let nl = self.newline_before;
            self.newline_before = false;
            if self.pos >= self.bytes.len() {
                out.push(Token {
                    kind: Kind::Eof,
                    text: "",
                    start: self.pos,
                    end: self.pos,
                    newline_before: nl,
                });
                return Ok(out);
            }
            let tok = self.next_token(nl)?;
            self.update_regex_state(&tok);
            out.push(tok);
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            while let Some(b) = self.peek() {
                match b {
                    b' ' | b'\t' | 0x0b | 0x0c => self.pos += 1,
                    b'\n' | b'\r' => {
                        self.newline_before = true;
                        self.pos += 1;
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
                    if b == b'\n' || b == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
                continue;
            }
            if self.peek() == Some(b'/') && self.peek_at(1) == Some(b'*') {
                let start = self.pos;
                self.pos += 2;
                let mut closed = false;
                while self.pos + 1 < self.bytes.len() {
                    if self.bytes[self.pos] == b'\n' || self.bytes[self.pos] == b'\r' {
                        self.newline_before = true;
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
                continue;
            }
            break;
        }
        Ok(())
    }

    fn next_token(&mut self, nl: bool) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        let b = self.bytes[self.pos];
        match b {
            b'\'' | b'"' => self.lex_string(nl),
            b'`' => self.lex_template(nl, true),
            b'}' => {
                self.pos += 1;
                if let Some(depth) = self.template_braces.last_mut() {
                    if *depth == 0 {
                        self.template_braces.pop();
                        return self.lex_template(nl, false);
                    }
                    *depth -= 1;
                }
                Ok(self.tok(Kind::RBrace, start, nl))
            }
            b'{' => {
                self.pos += 1;
                if let Some(depth) = self.template_braces.last_mut() {
                    *depth += 1;
                }
                Ok(self.tok(Kind::LBrace, start, nl))
            }
            b'/' if self.regex_allowed => self.lex_regex(nl),
            b'0'..=b'9' => self.lex_number(nl),
            b'.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.lex_number(nl),
            b'#' => {
                self.pos += 1;
                if self.peek().is_some_and(is_ident_start_byte)
                    || self.src[self.pos..]
                        .chars()
                        .next()
                        .is_some_and(is_unicode_ident_start)
                {
                    self.consume_ident_rest();
                    Ok(self.tok(Kind::PrivateIdent, start, nl))
                } else {
                    Err(lex_err(start, self.pos, "unexpected `#` in JavaScript"))
                }
            }
            b if is_ident_start_byte(b) => {
                self.consume_ident_rest();
                Ok(self.tok(Kind::Ident, start, nl))
            }
            b if !b.is_ascii() => {
                let Some(ch) = self.src[self.pos..].chars().next() else {
                    return Err(lex_err(
                        start,
                        self.pos,
                        "unexpected end of input in JavaScript",
                    ));
                };
                if is_unicode_ident_start(ch) {
                    self.consume_ident_rest();
                    Ok(self.tok(Kind::Ident, start, nl))
                } else {
                    Err(lex_err(
                        start,
                        start + ch.len_utf8(),
                        format!("unexpected character `{ch}` in JavaScript"),
                    ))
                }
            }
            _ => self.lex_punct(nl),
        }
    }

    fn tok(&self, kind: Kind, start: usize, nl: bool) -> Token<'a> {
        Token {
            kind,
            text: &self.src[start..self.pos],
            start,
            end: self.pos,
            newline_before: nl,
        }
    }

    fn lex_string(&mut self, nl: bool) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        let quote = self.bytes[self.pos];
        self.pos += 1;
        while let Some(b) = self.peek() {
            if b == quote {
                self.pos += 1;
                return Ok(self.tok(Kind::String, start, nl));
            }
            if b == b'\\' {
                self.pos += 1;
                let Some(ch) = self.src[self.pos..].chars().next() else {
                    return Err(lex_err(start, self.pos, "unterminated string literal"));
                };
                if ch == '\r' && self.peek_at(1) == Some(b'\n') {
                    self.pos += 2;
                } else {
                    self.pos += ch.len_utf8();
                }
                continue;
            }
            if b == b'\n' || b == b'\r' {
                return Err(lex_err(start, self.pos, "unterminated string literal"));
            }
            self.pos += 1;
        }
        Err(lex_err(start, self.pos, "unterminated string literal"))
    }

    fn lex_template(&mut self, nl: bool, fresh: bool) -> Result<Token<'a>, LexError> {
        let start = if fresh { self.pos } else { self.pos - 1 };
        if fresh {
            self.pos += 1; // opening backtick
        }
        while let Some(b) = self.peek() {
            if b == b'`' {
                self.pos += 1;
                let kind = if fresh {
                    Kind::TemplateNoSub
                } else {
                    Kind::TemplateTail
                };
                return Ok(self.tok(kind, start, nl));
            }
            if b == b'$' && self.peek_at(1) == Some(b'{') {
                self.pos += 2;
                self.template_braces.push(0);
                let kind = if fresh {
                    Kind::TemplateHead
                } else {
                    Kind::TemplateMiddle
                };
                return Ok(self.tok(kind, start, nl));
            }
            if b == b'\\' {
                self.pos += 1;
                let Some(ch) = self.src[self.pos..].chars().next() else {
                    return Err(lex_err(start, self.pos, "unterminated template literal"));
                };
                if ch == '\r' && self.peek_at(1) == Some(b'\n') {
                    self.pos += 2;
                } else {
                    self.pos += ch.len_utf8();
                }
                continue;
            }
            self.pos += 1;
        }
        Err(lex_err(start, self.pos, "unterminated template literal"))
    }

    fn lex_regex(&mut self, nl: bool) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        self.pos += 1; // opening /
        let mut in_class = false;
        while let Some(b) = self.peek() {
            if b == b'\n' || b == b'\r' {
                return Err(lex_err(
                    start,
                    self.pos,
                    "unterminated regular expression literal",
                ));
            }
            if b == b'\\' {
                self.pos += 1;
                match self.src[self.pos..].chars().next() {
                    Some('\n' | '\r') | None => {
                        return Err(lex_err(
                            start,
                            self.pos,
                            "unterminated regular expression literal",
                        ));
                    }
                    Some(ch) => self.pos += ch.len_utf8(),
                }
                continue;
            }
            if b == b'[' {
                in_class = true;
            } else if b == b']' {
                in_class = false;
            } else if b == b'/' && !in_class {
                self.pos += 1;
                while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    self.pos += 1;
                }
                return Ok(self.tok(Kind::Regex, start, nl));
            }
            self.pos += 1;
        }
        Err(lex_err(
            start,
            self.pos,
            "unterminated regular expression literal",
        ))
    }

    fn lex_number(&mut self, nl: bool) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        if self.peek() == Some(b'0')
            && let Some(prefix @ (b'x' | b'X' | b'b' | b'B' | b'o' | b'O')) = self.peek_at(1)
        {
            self.pos += 2;
            let digits_start = self.pos;
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
            if self.pos == digits_start {
                return Err(lex_err(start, self.pos, "invalid numeric literal"));
            }
            if self.peek() == Some(b'n') {
                self.pos += 1;
            }
            return Ok(self.tok(Kind::Number, start, nl));
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'_') {
                self.pos += 1;
            }
            if self.pos == exp_start {
                return Err(lex_err(
                    start,
                    self.pos,
                    "missing exponent digits in number",
                ));
            }
        }
        if self.peek() == Some(b'n') {
            self.pos += 1;
        }
        Ok(self.tok(Kind::Number, start, nl))
    }

    fn consume_ident_rest(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' {
                self.pos += 1;
            } else if !b.is_ascii() {
                let Some(ch) = self.src[self.pos..].chars().next() else {
                    break;
                };
                if is_unicode_ident_part(ch) {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn lex_punct(&mut self, nl: bool) -> Result<Token<'a>, LexError> {
        let start = self.pos;
        let rest = &self.src[self.pos..];
        let (len, kind) = if rest.starts_with(">>>=") {
            (4, Kind::AssignOp)
        } else if rest.starts_with("...") {
            (3, Kind::DotDotDot)
        } else if rest.starts_with("===") {
            (3, Kind::EqEqEq)
        } else if rest.starts_with("!==") {
            (3, Kind::BangEqEq)
        } else if rest.starts_with(">>>") {
            (3, Kind::UShr)
        } else if rest.starts_with("<<=")
            || rest.starts_with(">>=")
            || rest.starts_with("**=")
            || rest.starts_with("&&=")
            || rest.starts_with("||=")
            || rest.starts_with("??=")
        {
            (3, Kind::AssignOp)
        } else if rest.starts_with("=>") {
            (2, Kind::Arrow)
        } else if rest.starts_with("?.") && !self.peek_at(2).is_some_and(|c| c.is_ascii_digit()) {
            (2, Kind::QuestionDot)
        } else if rest.starts_with("++") {
            (2, Kind::PlusPlus)
        } else if rest.starts_with("--") {
            (2, Kind::MinusMinus)
        } else if rest.starts_with("**") {
            (2, Kind::StarStar)
        } else if rest.starts_with("&&") {
            (2, Kind::AmpAmp)
        } else if rest.starts_with("||") {
            (2, Kind::PipePipe)
        } else if rest.starts_with("??") {
            (2, Kind::QuestionQuestion)
        } else if rest.starts_with("==") {
            (2, Kind::EqEq)
        } else if rest.starts_with("!=") {
            (2, Kind::BangEq)
        } else if rest.starts_with("<=") {
            (2, Kind::Le)
        } else if rest.starts_with(">=") {
            (2, Kind::Ge)
        } else if rest.starts_with("<<") {
            (2, Kind::Shl)
        } else if rest.starts_with(">>") {
            (2, Kind::Shr)
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
                b'[' => Kind::LBracket,
                b']' => Kind::RBracket,
                b'.' => Kind::Dot,
                b',' => Kind::Comma,
                b';' => Kind::Semi,
                b':' => Kind::Colon,
                b'?' => Kind::Question,
                b'=' => Kind::Assign,
                b'+' => Kind::Plus,
                b'-' => Kind::Minus,
                b'*' => Kind::Star,
                b'/' => Kind::Slash,
                b'%' => Kind::Percent,
                b'&' => Kind::Amp,
                b'|' => Kind::Pipe,
                b'^' => Kind::Caret,
                b'~' => Kind::Tilde,
                b'!' => Kind::Bang,
                b'<' => Kind::Lt,
                b'>' => Kind::Gt,
                b => {
                    return Err(lex_err(
                        start,
                        start + 1,
                        format!("unexpected character `{}` in JavaScript", b as char),
                    ));
                }
            };
            (1, kind)
        };
        self.pos += len;
        Ok(self.tok(kind, start, nl))
    }

    fn update_regex_state(&mut self, tok: &Token<'a>) {
        self.regex_allowed = match tok.kind {
            Kind::Ident => matches!(
                tok.text,
                "return"
                    | "throw"
                    | "case"
                    | "delete"
                    | "void"
                    | "typeof"
                    | "in"
                    | "instanceof"
                    | "new"
                    | "of"
                    | "yield"
                    | "await"
                    | "else"
                    | "do"
            ),
            Kind::PrivateIdent
            | Kind::Number
            | Kind::String
            | Kind::Regex
            | Kind::TemplateNoSub
            | Kind::TemplateTail
            | Kind::RParen
            | Kind::RBracket
            | Kind::PlusPlus
            | Kind::MinusMinus => false,
            _ => true,
        };
    }
}

fn lex_err(start: usize, end: usize, msg: impl Into<String>) -> LexError {
    LexError {
        message: msg.into(),
        start,
        end: end.max(start),
    }
}

fn is_ident_start_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_unicode_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_unicode_ident_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}
