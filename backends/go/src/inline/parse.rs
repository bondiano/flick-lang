//! Zero-dependency Go parser for validating inline foreign bindings and
//! preambles, plus extracting hoisted `import` declarations from preambles.

use super::lex::{Kind, Token, lex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSpec {
    pub alias: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPreamble {
    pub imports: Vec<ImportSpec>,
    /// Byte offset inside the preamble string where non-import declarations begin.
    pub body_offset: usize,
}

pub fn parse_expression(src: &str) -> Result<(), ParseError> {
    let tokens = lex(src).map_err(|e| ParseError {
        message: e.message,
        start: e.start,
        end: e.end,
    })?;
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
    };
    if p.at(Kind::Eof) {
        return Err(ParseError {
            message: "expected a Go expression in inline foreign binding".into(),
            start: 0,
            end: src.len(),
        });
    }
    p.expr(true)?;
    // A trailing newline in the literal produces an implicit ASI semicolon.
    if p.at(Kind::Semi) && p.cur().implicit {
        p.bump();
    }
    if !p.at(Kind::Eof) {
        let tok = p.cur();
        return Err(ParseError {
            message: format!("unexpected `{}` after inline Go expression", tok.text),
            start: tok.start,
            end: tok.end,
        });
    }
    Ok(())
}

pub fn parse_preamble(src: &str) -> Result<ParsedPreamble, ParseError> {
    let tokens = lex(src).map_err(|e| ParseError {
        message: e.message,
        start: e.start,
        end: e.end,
    })?;
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let mut imports = Vec::new();
    let mut seen_non_import = false;
    let mut after_imports = 0usize;
    while !p.at(Kind::Eof) {
        if p.eat(Kind::Semi) {
            if !seen_non_import {
                after_imports = p.prev_end();
            }
            continue;
        }
        if p.at_kw("package") {
            return Err(p.err_here(
                "inline Go preamble must not declare a `package` clause; Flick generates `package main`",
            ));
        }
        if p.at_kw("import") {
            if seen_non_import {
                return Err(p.err_here(
                    "`import` declarations must appear before other declarations in an inline Go preamble",
                ));
            }
            p.import_decl(&mut imports)?;
            after_imports = p.prev_end();
            continue;
        }
        seen_non_import = true;
        p.top_decl()?;
    }
    let body_offset = if seen_non_import {
        skip_leading_blank_lines(src, after_imports)
    } else {
        src.len()
    };
    Ok(ParsedPreamble {
        imports,
        body_offset,
    })
}

fn skip_leading_blank_lines(src: &str, mut offset: usize) -> usize {
    let bytes = src.as_bytes();
    while offset < bytes.len() {
        let line_start = offset;
        while offset < bytes.len() && matches!(bytes[offset], b' ' | b'\t' | b'\r') {
            offset += 1;
        }
        if offset < bytes.len() && bytes[offset] == b'\n' {
            offset += 1;
            continue;
        }
        return line_start;
    }
    src.len()
}

pub(super) struct Parser<'a> {
    tokens: &'a [Token<'a>],
    pos: usize,
}

impl<'a> Parser<'a> {
    pub(super) fn cur(&self) -> &'a Token<'a> {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    pub(super) fn peek(&self, offset: usize) -> &'a Token<'a> {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx]
    }

    fn prev_end(&self) -> usize {
        if self.pos == 0 {
            0
        } else {
            self.tokens[self.pos - 1].end
        }
    }

    pub(super) fn at(&self, kind: Kind) -> bool {
        self.cur().kind == kind
    }

    pub(super) fn at_kw(&self, kw: &str) -> bool {
        self.cur().kind == Kind::Ident && self.cur().text == kw
    }

    pub(super) fn bump(&mut self) -> &'a Token<'a> {
        let tok = self.cur();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    pub(super) fn eat(&mut self, kind: Kind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(super) fn err_here(&self, message: impl Into<String>) -> ParseError {
        let t = self.cur();
        ParseError {
            message: message.into(),
            start: t.start,
            end: t.end,
        }
    }

    pub(super) fn expect(&mut self, kind: Kind, what: &str) -> Result<&'a Token<'a>, ParseError> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(self.err_here(format!("expected {what}")))
        }
    }

    pub(super) fn expect_ident(&mut self, what: &str) -> Result<&'a Token<'a>, ParseError> {
        if self.at(Kind::Ident) && !is_go_keyword(self.cur().text) {
            Ok(self.bump())
        } else {
            Err(self.err_here(format!("expected {what}")))
        }
    }

    pub(super) fn end_stmt(&mut self) -> Result<(), ParseError> {
        if self.eat(Kind::Semi)
            || self.at(Kind::RBrace)
            || self.at(Kind::RParen)
            || self.at(Kind::Eof)
        {
            Ok(())
        } else {
            Err(self.err_here(format!("unexpected token `{}`", self.cur().text)))
        }
    }

    fn import_decl(&mut self, out: &mut Vec<ImportSpec>) -> Result<(), ParseError> {
        self.bump(); // import
        if self.eat(Kind::LParen) {
            while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
                if self.eat(Kind::Semi) {
                    continue;
                }
                out.push(self.import_spec()?);
                self.end_stmt()?;
            }
            self.expect(Kind::RParen, "`)`")?;
        } else {
            out.push(self.import_spec()?);
        }
        self.end_stmt()
    }

    fn import_spec(&mut self) -> Result<ImportSpec, ParseError> {
        let alias = if self.at(Kind::Dot) {
            self.bump();
            Some(".".to_owned())
        } else if self.at(Kind::Ident) {
            Some(self.bump().text.to_owned())
        } else {
            None
        };
        let path_tok = self.expect(Kind::String, "import path string literal")?;
        let path = unquote_go_string(path_tok.text);
        Ok(ImportSpec { alias, path })
    }

    fn top_decl(&mut self) -> Result<(), ParseError> {
        match self.cur().kind {
            Kind::Ident => match self.cur().text {
                "func" => self.func_decl(),
                "type" => self.type_decl(),
                "var" => self.var_or_const_decl(false),
                "const" => self.var_or_const_decl(true),
                other => Err(self.err_here(format!(
                    "expected top-level Go declaration (`import`, `func`, `type`, `var`, or `const`), found `{other}`"
                ))),
            },
            _ => Err(self.err_here(format!(
                "expected top-level Go declaration, found `{}`",
                self.cur().text
            ))),
        }
    }

    fn func_decl(&mut self) -> Result<(), ParseError> {
        self.bump(); // func
        if self.at(Kind::LParen) {
            self.param_list(false)?; // method receiver
        }
        self.expect_ident("function name")?;
        if self.at(Kind::LBracket) {
            self.type_param_list()?;
        }
        self.signature()?;
        if self.at(Kind::LBrace) {
            self.block()?;
        }
        self.end_stmt()
    }

    fn type_param_list(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LBracket, "`[`")?;
        while !self.at(Kind::RBracket) && !self.at(Kind::Eof) {
            self.expect_ident("type parameter")?;
            if !self.at(Kind::Comma) && !self.at(Kind::RBracket) {
                self.type_constraint()?;
            }
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect(Kind::RBracket, "`]`")?;
        Ok(())
    }

    fn type_constraint(&mut self) -> Result<(), ParseError> {
        loop {
            let _ = self.eat(Kind::Tilde);
            self.type_expr()?;
            if !self.eat(Kind::Pipe) {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn signature(&mut self) -> Result<(), ParseError> {
        self.param_list(true)?;
        if self.at(Kind::LParen) {
            self.param_list(false)?;
        } else if self.starts_type() {
            self.type_expr()?;
        }
        Ok(())
    }

    fn param_list(&mut self, allow_variadic: bool) -> Result<(), ParseError> {
        self.expect(Kind::LParen, "`(`")?;
        while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
            if allow_variadic && self.eat(Kind::Ellipsis) {
                self.type_expr()?;
            } else {
                let first_is_ident = self.at(Kind::Ident) && !is_go_keyword(self.cur().text);
                let is_named = first_is_ident
                    && match self.peek(1).kind {
                        Kind::Comma | Kind::RParen | Kind::Dot => false,
                        Kind::LBracket => self.bracketed_is_named_type(),
                        _ => true,
                    };
                if is_named {
                    self.bump(); // param name
                    if allow_variadic {
                        let _ = self.eat(Kind::Ellipsis);
                    }
                    self.type_expr()?;
                } else {
                    self.type_expr()?;
                }
            }
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect(Kind::RParen, "`)`")?;
        Ok(())
    }

    pub(super) fn type_decl(&mut self) -> Result<(), ParseError> {
        self.bump(); // type
        if self.eat(Kind::LParen) {
            while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
                if self.eat(Kind::Semi) {
                    continue;
                }
                self.type_spec()?;
                self.end_stmt()?;
            }
            self.expect(Kind::RParen, "`)`")?;
        } else {
            self.type_spec()?;
        }
        self.end_stmt()
    }

    fn type_spec(&mut self) -> Result<(), ParseError> {
        self.expect_ident("type name")?;
        if self.at(Kind::LBracket) {
            self.type_param_list()?;
        }
        let _ = self.eat(Kind::Assign);
        self.type_expr()
    }

    pub(super) fn var_or_const_decl(&mut self, is_const: bool) -> Result<(), ParseError> {
        self.bump(); // var | const
        if self.eat(Kind::LParen) {
            while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
                if self.eat(Kind::Semi) {
                    continue;
                }
                self.value_spec(is_const)?;
                self.end_stmt()?;
            }
            self.expect(Kind::RParen, "`)`")?;
        } else {
            self.value_spec(is_const)?;
        }
        self.end_stmt()
    }

    fn value_spec(&mut self, is_const: bool) -> Result<(), ParseError> {
        self.expect_ident("identifier")?;
        while self.eat(Kind::Comma) {
            self.expect_ident("identifier")?;
        }
        if !self.at(Kind::Assign)
            && !self.at(Kind::Semi)
            && !self.at(Kind::RParen)
            && !self.at(Kind::RBrace)
            && !self.at(Kind::Eof)
        {
            self.type_expr()?;
        }
        if self.eat(Kind::Assign) {
            self.expr_list(true)?;
        } else if !is_const
            && matches!(
                self.cur().kind,
                Kind::Semi | Kind::RParen | Kind::RBrace | Kind::Eof
            )
        {
            // `var x T` without initializer is valid
        }
        Ok(())
    }

    fn starts_type(&self) -> bool {
        self.starts_type_at(self.pos)
    }

    fn starts_type_at(&self, idx: usize) -> bool {
        let tok = &self.tokens[idx.min(self.tokens.len() - 1)];
        match tok.kind {
            Kind::Star | Kind::LBracket | Kind::Arrow => true,
            Kind::Ident => {
                !is_go_keyword(tok.text)
                    || matches!(tok.text, "func" | "struct" | "interface" | "map" | "chan")
            }
            _ => false,
        }
    }

    /// Distinguishes a named slice/array parameter or field (`ident []T`,
    /// `ident [N]T`, `ident [...]T`) from an unnamed generic instantiation
    /// (`Box[T]`) by checking whether a type follows the matching `]`.
    fn bracketed_is_named_type(&self) -> bool {
        let mut idx = self.pos + 1;
        let mut depth = 0usize;
        while idx < self.tokens.len() {
            match self.tokens[idx].kind {
                Kind::LBracket => depth += 1,
                Kind::RBracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let after = (idx + 1).min(self.tokens.len() - 1);
                        return self.tokens[after].kind == Kind::LParen
                            || self.starts_type_at(after);
                    }
                }
                Kind::Eof => return false,
                _ => {}
            }
            idx += 1;
        }
        false
    }

    pub(super) fn type_expr(&mut self) -> Result<(), ParseError> {
        match self.cur().kind {
            Kind::Star => {
                self.bump();
                self.type_expr()
            }
            Kind::LParen => {
                self.bump();
                self.type_expr()?;
                self.expect(Kind::RParen, "`)`")?;
                Ok(())
            }
            Kind::LBracket => {
                self.bump();
                if !self.at(Kind::RBracket) && !self.eat(Kind::Ellipsis) {
                    self.expr(true)?;
                }
                self.expect(Kind::RBracket, "`]`")?;
                self.type_expr()
            }
            Kind::Arrow => {
                self.bump();
                if !self.at_kw("chan") {
                    return Err(self.err_here("expected `chan` after `<-`"));
                }
                self.bump();
                self.type_expr()
            }
            Kind::Ident => match self.cur().text {
                "map" => {
                    self.bump();
                    self.expect(Kind::LBracket, "`[`")?;
                    self.type_expr()?;
                    self.expect(Kind::RBracket, "`]`")?;
                    self.type_expr()
                }
                "chan" => {
                    self.bump();
                    let _ = self.eat(Kind::Arrow);
                    self.type_expr()
                }
                "func" => {
                    self.bump();
                    self.signature()
                }
                "struct" => self.struct_type(),
                "interface" => self.interface_type(),
                kw if is_go_keyword(kw) => {
                    Err(self.err_here(format!("expected Go type, found `{kw}`")))
                }
                _ => {
                    self.bump();
                    if self.eat(Kind::Dot) {
                        self.expect_ident("type name")?;
                    }
                    if self.at(Kind::LBracket) {
                        self.type_arg_list()?;
                    }
                    Ok(())
                }
            },
            _ => Err(self.err_here(format!("expected Go type, found `{}`", self.cur().text))),
        }
    }

    fn struct_type(&mut self) -> Result<(), ParseError> {
        self.bump();
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.eat(Kind::Semi) {
                continue;
            }
            if self.at(Kind::Star) {
                self.type_expr()?;
            } else {
                self.type_or_field_decl()?;
            }
            let _ = self.eat(Kind::String); // optional struct tag
            self.end_stmt()?;
        }
        self.expect(Kind::RBrace, "`}`")?;
        Ok(())
    }

    fn interface_type(&mut self) -> Result<(), ParseError> {
        self.bump();
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.eat(Kind::Semi) {
                continue;
            }
            if self.at(Kind::Ident)
                && !is_go_keyword(self.cur().text)
                && self.peek(1).kind == Kind::LParen
            {
                self.bump();
                self.signature()?;
            } else {
                self.type_constraint()?;
            }
            self.end_stmt()?;
        }
        self.expect(Kind::RBrace, "`}`")?;
        Ok(())
    }

    fn type_or_field_decl(&mut self) -> Result<(), ParseError> {
        let first_ident = self.at(Kind::Ident) && !is_go_keyword(self.cur().text);
        if first_ident && matches!(self.peek(1).kind, Kind::Comma) {
            self.bump();
            while self.eat(Kind::Comma) {
                self.expect_ident("field name")?;
            }
            return self.type_expr();
        }
        let is_named = first_ident
            && match self.peek(1).kind {
                Kind::Dot | Kind::Semi | Kind::RBrace | Kind::String => false,
                Kind::LBracket => self.bracketed_is_named_type(),
                _ => true,
            };
        if is_named {
            self.bump();
            return self.type_expr();
        }
        self.type_expr()
    }

    fn type_arg_list(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LBracket, "`[`")?;
        self.type_expr()?;
        while self.eat(Kind::Comma) {
            if self.at(Kind::RBracket) {
                break;
            }
            self.type_expr()?;
        }
        self.expect(Kind::RBracket, "`]`")?;
        Ok(())
    }
}

pub(super) fn is_go_keyword(s: &str) -> bool {
    matches!(
        s,
        "break"
            | "case"
            | "chan"
            | "const"
            | "continue"
            | "default"
            | "defer"
            | "else"
            | "fallthrough"
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
            | "return"
            | "select"
            | "struct"
            | "switch"
            | "type"
            | "var"
    )
}

fn unquote_go_string(raw: &str) -> String {
    if let Some(inner) = raw.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
        return inner.to_owned();
    }
    let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return raw.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(esc) = chars.next() {
                match esc {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    other => out.push(other),
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
