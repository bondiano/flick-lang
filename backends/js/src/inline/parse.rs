//! Zero-dependency JavaScript parser for validating inline foreign expressions
//! and module preambles with exact byte offsets.

use super::lex::{Kind, Token, lex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub start: usize,
    pub end: usize,
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
            message: "expected a JavaScript expression in inline foreign binding".into(),
            start: 0,
            end: src.len(),
        });
    }
    p.expr()?;
    if !p.at(Kind::Eof) {
        let tok = p.cur();
        return Err(ParseError {
            message: format!(
                "unexpected `{}` after inline JavaScript expression",
                tok.text
            ),
            start: tok.start,
            end: tok.end,
        });
    }
    Ok(())
}

pub fn parse_module(src: &str) -> Result<(), ParseError> {
    let tokens = lex(src).map_err(|e| ParseError {
        message: e.message,
        start: e.start,
        end: e.end,
    })?;
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
    };
    while !p.at(Kind::Eof) {
        p.module_item()?;
    }
    Ok(())
}

pub(super) struct Parser<'a> {
    pub(super) tokens: &'a [Token<'a>],
    pub(super) pos: usize,
}

impl<'a> Parser<'a> {
    pub(super) fn cur(&self) -> &'a Token<'a> {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    pub(super) fn peek(&self, offset: usize) -> &'a Token<'a> {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx]
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

    pub(super) fn eat_kw(&mut self, kw: &str) -> bool {
        if self.at_kw(kw) {
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
        if self.at(Kind::Ident) {
            Ok(self.bump())
        } else {
            Err(self.err_here(format!("expected {what}")))
        }
    }

    fn end_stmt(&mut self) -> Result<(), ParseError> {
        if self.eat(Kind::Semi)
            || self.at(Kind::RBrace)
            || self.at(Kind::Eof)
            || self.cur().newline_before
        {
            Ok(())
        } else {
            Err(self.err_here(format!("unexpected token `{}`", self.cur().text)))
        }
    }

    fn module_item(&mut self) -> Result<(), ParseError> {
        if self.at_kw("import") && !matches!(self.peek(1).kind, Kind::LParen | Kind::Dot) {
            return self.import_decl();
        }
        if self.at_kw("export") {
            return self.export_decl();
        }
        self.stmt()
    }

    fn import_decl(&mut self) -> Result<(), ParseError> {
        self.bump(); // import
        if self.eat(Kind::String) {
            return self.end_stmt();
        }
        if self.at(Kind::Ident) {
            self.bump();
            if self.eat(Kind::Comma) {
                self.import_clause_rest()?;
            }
        } else {
            self.import_clause_rest()?;
        }
        if !self.eat_kw("from") {
            return Err(self.err_here("expected `from` in import declaration"));
        }
        self.expect(Kind::String, "module specifier string")?;
        self.end_stmt()
    }

    fn import_clause_rest(&mut self) -> Result<(), ParseError> {
        if self.eat(Kind::Star) {
            if !self.eat_kw("as") {
                return Err(self.err_here("expected `as` after `*`"));
            }
            self.expect_ident("namespace identifier")?;
            Ok(())
        } else if self.eat(Kind::LBrace) {
            while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
                if !matches!(self.cur().kind, Kind::Ident | Kind::String) {
                    return Err(self.err_here("expected import specifier"));
                }
                self.bump();
                if self.eat_kw("as") {
                    self.expect_ident("local import name")?;
                }
                if !self.eat(Kind::Comma) {
                    break;
                }
            }
            self.expect(Kind::RBrace, "`}`")?;
            Ok(())
        } else {
            Err(self.err_here("expected import bindings"))
        }
    }

    fn export_decl(&mut self) -> Result<(), ParseError> {
        self.bump(); // export
        if self.eat_kw("default") {
            if self.at_kw("function")
                || (self.at_kw("async")
                    && !self.peek(1).newline_before
                    && self.peek(1).kind == Kind::Ident
                    && self.peek(1).text == "function")
            {
                return self.function_decl(true);
            }
            if self.at_kw("class") {
                return self.class_decl(true);
            }
            self.assign_expr()?;
            return self.end_stmt();
        }
        if self.eat(Kind::Star) {
            if self.eat_kw("as") {
                self.expect_ident("export name")?;
            }
            if !self.eat_kw("from") {
                return Err(self.err_here("expected `from` in `export *`"));
            }
            self.expect(Kind::String, "module specifier string")?;
            return self.end_stmt();
        }
        if self.eat(Kind::LBrace) {
            while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
                if !matches!(self.cur().kind, Kind::Ident | Kind::String) {
                    return Err(self.err_here("expected export name"));
                }
                self.bump();
                if self.eat_kw("as") {
                    if !matches!(self.cur().kind, Kind::Ident | Kind::String) {
                        return Err(self.err_here("expected exported alias"));
                    }
                    self.bump();
                }
                if !self.eat(Kind::Comma) {
                    break;
                }
            }
            self.expect(Kind::RBrace, "`}`")?;
            if self.eat_kw("from") {
                self.expect(Kind::String, "module specifier string")?;
            }
            return self.end_stmt();
        }
        self.stmt()
    }

    fn stmt(&mut self) -> Result<(), ParseError> {
        match self.cur().kind {
            Kind::Semi => {
                self.bump();
                Ok(())
            }
            Kind::LBrace => self.block(),
            Kind::Ident => self.ident_stmt(),
            _ => {
                self.expr()?;
                self.end_stmt()
            }
        }
    }

    fn ident_stmt(&mut self) -> Result<(), ParseError> {
        match self.cur().text {
            "const" | "let" | "var" => {
                if self.cur().text == "let"
                    && !matches!(
                        self.peek(1).kind,
                        Kind::Ident | Kind::LBrace | Kind::LBracket
                    )
                {
                    self.expr()?;
                    return self.end_stmt();
                }
                self.var_decl(true)
            }
            "function" => self.function_decl(false),
            "async"
                if !self.peek(1).newline_before
                    && self.peek(1).kind == Kind::Ident
                    && self.peek(1).text == "function" =>
            {
                self.function_decl(false)
            }
            "class" => self.class_decl(false),
            "if" => {
                self.bump();
                self.expect(Kind::LParen, "`(`")?;
                self.expr()?;
                self.expect(Kind::RParen, "`)`")?;
                self.stmt()?;
                if self.eat_kw("else") {
                    self.stmt()?;
                }
                Ok(())
            }
            "while" => {
                self.bump();
                self.expect(Kind::LParen, "`(`")?;
                self.expr()?;
                self.expect(Kind::RParen, "`)`")?;
                self.stmt()
            }
            "do" => {
                self.bump();
                self.stmt()?;
                if !self.eat_kw("while") {
                    return Err(self.err_here("expected `while` after `do` statement"));
                }
                self.expect(Kind::LParen, "`(`")?;
                self.expr()?;
                self.expect(Kind::RParen, "`)`")?;
                let _ = self.eat(Kind::Semi);
                Ok(())
            }
            "for" => self.for_stmt(),
            "switch" => self.switch_stmt(),
            "try" => self.try_stmt(),
            "return" | "break" | "continue" => {
                let is_return = self.cur().text == "return";
                self.bump();
                if !self.cur().newline_before
                    && !matches!(self.cur().kind, Kind::Semi | Kind::RBrace | Kind::Eof)
                {
                    if is_return {
                        self.expr()?;
                    } else {
                        self.expect_ident("label")?;
                    }
                }
                self.end_stmt()
            }
            "throw" => {
                self.bump();
                if self.cur().newline_before {
                    return Err(self.err_here("expected expression after `throw`"));
                }
                self.expr()?;
                self.end_stmt()
            }
            "debugger" => {
                self.bump();
                self.end_stmt()
            }
            _ => {
                if self.peek(1).kind == Kind::Colon {
                    self.bump();
                    self.bump();
                    return self.stmt();
                }
                self.expr()?;
                self.end_stmt()
            }
        }
    }

    pub(super) fn block(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            self.stmt()?;
        }
        self.expect(Kind::RBrace, "`}`")?;
        Ok(())
    }

    fn var_decl(&mut self, require_semi: bool) -> Result<(), ParseError> {
        let is_const = self.cur().text == "const";
        self.bump();
        loop {
            self.pattern()?;
            if self.eat(Kind::Assign) {
                self.assign_expr()?;
            } else if is_const && require_semi {
                return Err(self.err_here("expected `=` in `const` declaration"));
            }
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        if require_semi {
            self.end_stmt()?;
        }
        Ok(())
    }

    fn for_stmt(&mut self) -> Result<(), ParseError> {
        self.bump(); // for
        let _ = self.eat_kw("await");
        self.expect(Kind::LParen, "`(`")?;
        if !self.at(Kind::Semi) {
            if matches!(self.cur().text, "const" | "let" | "var") && self.at(Kind::Ident) {
                self.bump();
                self.pattern()?;
                if self.eat_kw("of") || self.eat_kw("in") {
                    self.expr()?;
                    self.expect(Kind::RParen, "`)`")?;
                    return self.stmt();
                }
                if self.eat(Kind::Assign) {
                    self.assign_expr()?;
                }
                while self.eat(Kind::Comma) {
                    self.pattern()?;
                    if self.eat(Kind::Assign) {
                        self.assign_expr()?;
                    }
                }
            } else {
                self.expr()?;
                if self.eat_kw("of") || self.eat_kw("in") {
                    self.expr()?;
                    self.expect(Kind::RParen, "`)`")?;
                    return self.stmt();
                }
            }
        }
        self.expect(Kind::Semi, "`;`")?;
        if !self.at(Kind::Semi) {
            self.expr()?;
        }
        self.expect(Kind::Semi, "`;`")?;
        if !self.at(Kind::RParen) {
            self.expr()?;
        }
        self.expect(Kind::RParen, "`)`")?;
        self.stmt()
    }

    fn switch_stmt(&mut self) -> Result<(), ParseError> {
        self.bump();
        self.expect(Kind::LParen, "`(`")?;
        self.expr()?;
        self.expect(Kind::RParen, "`)`")?;
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.eat_kw("case") {
                self.expr()?;
                self.expect(Kind::Colon, "`:`")?;
            } else if self.eat_kw("default") {
                self.expect(Kind::Colon, "`:`")?;
            } else {
                self.stmt()?;
            }
        }
        self.expect(Kind::RBrace, "`}`")?;
        Ok(())
    }

    fn try_stmt(&mut self) -> Result<(), ParseError> {
        self.bump();
        self.block()?;
        let mut had_handler = false;
        if self.eat_kw("catch") {
            had_handler = true;
            if self.eat(Kind::LParen) {
                self.pattern()?;
                self.expect(Kind::RParen, "`)`")?;
            }
            self.block()?;
        }
        if self.eat_kw("finally") {
            had_handler = true;
            self.block()?;
        }
        if !had_handler {
            return Err(self.err_here("expected `catch` or `finally` after `try`"));
        }
        Ok(())
    }

    pub(super) fn function_decl(&mut self, optional_name: bool) -> Result<(), ParseError> {
        let _ = self.eat_kw("async");
        self.bump(); // function
        let _ = self.eat(Kind::Star);
        if self.at(Kind::Ident) {
            self.bump();
        } else if !optional_name {
            return Err(self.err_here("expected function name"));
        }
        self.param_list()?;
        self.block()
    }

    pub(super) fn class_decl(&mut self, optional_name: bool) -> Result<(), ParseError> {
        self.bump(); // class
        if self.at(Kind::Ident) && !self.at_kw("extends") {
            self.bump();
        } else if !optional_name {
            return Err(self.err_here("expected class name"));
        }
        if self.eat_kw("extends") {
            self.lhs_expr()?;
        }
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.eat(Kind::Semi) {
                continue;
            }
            if self.at_kw("static") && self.peek(1).kind == Kind::LBrace {
                self.bump();
                self.block()?;
                continue;
            }
            let _ = self.eat_kw("static");
            let _ = self.eat_kw("async");
            let _ = self.eat(Kind::Star);
            if (self.at_kw("get") || self.at_kw("set"))
                && !matches!(self.peek(1).kind, Kind::LParen | Kind::Assign | Kind::Semi)
            {
                self.bump();
            }
            self.prop_name()?;
            if self.at(Kind::LParen) {
                self.param_list()?;
                self.block()?;
            } else {
                if self.eat(Kind::Assign) {
                    self.assign_expr()?;
                }
                self.end_stmt()?;
            }
        }
        self.expect(Kind::RBrace, "`}`")?;
        Ok(())
    }

    pub(super) fn param_list(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LParen, "`(`")?;
        while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
            let rest = self.eat(Kind::DotDotDot);
            self.pattern()?;
            if !rest && self.eat(Kind::Assign) {
                self.assign_expr()?;
            }
            if !self.eat(Kind::Comma) {
                break;
            }
            if rest {
                return Err(self.err_here("rest parameter must be last"));
            }
        }
        self.expect(Kind::RParen, "`)`")?;
        Ok(())
    }
}
