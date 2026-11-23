//! Go statement, expression, and composite-literal parsing for inline foreign
//! validation.

use super::lex::Kind;
use super::parse::{ParseError, Parser, is_go_keyword};

impl Parser<'_> {
    pub(super) fn block(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            self.stmt()?;
        }
        self.expect(Kind::RBrace, "`}`")?;
        Ok(())
    }

    fn stmt(&mut self) -> Result<(), ParseError> {
        if self.eat(Kind::Semi) {
            return Ok(());
        }
        if self.at(Kind::LBrace) {
            self.block()?;
            return self.end_stmt();
        }
        if self.at(Kind::Ident) {
            match self.cur().text {
                "var" => return self.var_or_const_decl(false),
                "const" => return self.var_or_const_decl(true),
                "type" => return self.type_decl(),
                "if" => return self.if_stmt(),
                "for" => return self.for_stmt(),
                "switch" => return self.switch_stmt(),
                "select" => return self.select_stmt(),
                "return" => {
                    self.bump();
                    if !matches!(self.cur().kind, Kind::Semi | Kind::RBrace | Kind::Eof) {
                        self.expr_list(true)?;
                    }
                    return self.end_stmt();
                }
                "defer" | "go" => {
                    self.bump();
                    self.expr(true)?;
                    return self.end_stmt();
                }
                "break" | "continue" => {
                    self.bump();
                    if self.at(Kind::Ident) && !is_go_keyword(self.cur().text) {
                        self.bump();
                    }
                    return self.end_stmt();
                }
                "goto" => {
                    self.bump();
                    self.expect_ident("label")?;
                    return self.end_stmt();
                }
                "fallthrough" => {
                    self.bump();
                    return self.end_stmt();
                }
                _ if !is_go_keyword(self.cur().text) && self.peek(1).kind == Kind::Colon => {
                    self.bump();
                    self.bump();
                    if !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
                        return self.stmt();
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        self.simple_stmt(true)?;
        self.end_stmt()
    }

    fn simple_stmt(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        self.expr(allow_struct_lit)?;
        if self.eat(Kind::Inc) || self.eat(Kind::Dec) {
            return Ok(());
        }
        if self.eat(Kind::Arrow) {
            self.expr(allow_struct_lit)?;
            return Ok(());
        }
        while self.eat(Kind::Comma) {
            self.expr(allow_struct_lit)?;
        }
        if matches!(
            self.cur().kind,
            Kind::Define | Kind::Assign | Kind::AssignOp
        ) {
            self.bump();
            if self.at_kw("range") {
                self.bump();
                self.expr(allow_struct_lit)?;
            } else {
                self.expr_list(allow_struct_lit)?;
            }
        }
        Ok(())
    }

    fn if_stmt(&mut self) -> Result<(), ParseError> {
        self.bump(); // if
        self.simple_stmt(false)?;
        if self.eat(Kind::Semi) {
            self.expr(false)?;
        }
        self.block()?;
        if self.at_kw("else") {
            self.bump();
            if self.at_kw("if") {
                return self.if_stmt();
            }
            self.block()?;
        }
        self.end_stmt()
    }

    fn for_stmt(&mut self) -> Result<(), ParseError> {
        self.bump(); // for
        if !self.at(Kind::LBrace) {
            if self.at_kw("range") {
                self.bump();
                self.expr(false)?;
            } else {
                if !self.at(Kind::Semi) {
                    self.simple_stmt(false)?;
                }
                if self.eat(Kind::Semi) {
                    if !self.at(Kind::Semi) {
                        self.expr(false)?;
                    }
                    self.expect(Kind::Semi, "`;`")?;
                    if !self.at(Kind::LBrace) {
                        self.simple_stmt(false)?;
                    }
                }
            }
        }
        self.block()?;
        self.end_stmt()
    }

    fn switch_stmt(&mut self) -> Result<(), ParseError> {
        self.bump(); // switch
        if !self.at(Kind::LBrace) {
            if !self.at(Kind::Semi) {
                self.simple_stmt(false)?;
            }
            if self.eat(Kind::Semi) && !self.at(Kind::LBrace) {
                self.simple_stmt(false)?;
            }
        }
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.at_kw("case") {
                self.bump();
                self.type_or_expr_list(true)?;
                self.expect(Kind::Colon, "`:`")?;
            } else if self.at_kw("default") {
                self.bump();
                self.expect(Kind::Colon, "`:`")?;
            } else {
                self.stmt()?;
            }
        }
        self.expect(Kind::RBrace, "`}`")?;
        self.end_stmt()
    }

    fn select_stmt(&mut self) -> Result<(), ParseError> {
        self.bump(); // select
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.at_kw("case") {
                self.bump();
                self.simple_stmt(true)?;
                self.expect(Kind::Colon, "`:`")?;
            } else if self.at_kw("default") {
                self.bump();
                self.expect(Kind::Colon, "`:`")?;
            } else {
                self.stmt()?;
            }
        }
        self.expect(Kind::RBrace, "`}`")?;
        self.end_stmt()
    }

    pub(super) fn expr_list(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        self.expr(allow_struct_lit)?;
        while self.eat(Kind::Comma) {
            self.expr(allow_struct_lit)?;
        }
        Ok(())
    }

    fn type_or_expr_list(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        self.type_or_expr(allow_struct_lit)?;
        while self.eat(Kind::Comma) {
            self.type_or_expr(allow_struct_lit)?;
        }
        Ok(())
    }

    fn type_or_expr(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        if self.at(Kind::LBracket)
            || (self.at(Kind::Ident)
                && matches!(self.cur().text, "map" | "struct" | "interface" | "chan"))
        {
            self.type_expr()?;
            if allow_struct_lit && self.at(Kind::LBrace) {
                self.composite_lit()?;
            }
            return Ok(());
        }
        self.expr(allow_struct_lit)
    }

    pub(super) fn expr(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        self.bin_expr(1, allow_struct_lit)
    }

    fn bin_prec(&self) -> Option<u8> {
        match self.cur().kind {
            Kind::PipePipe => Some(1),
            Kind::AmpAmp => Some(2),
            Kind::Eq | Kind::Ne | Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge => Some(3),
            Kind::Plus | Kind::Minus | Kind::Pipe | Kind::Caret => Some(4),
            Kind::Star
            | Kind::Slash
            | Kind::Percent
            | Kind::Shl
            | Kind::Shr
            | Kind::Amp
            | Kind::AndNot => Some(5),
            _ => None,
        }
    }

    fn bin_expr(&mut self, min_prec: u8, allow_struct_lit: bool) -> Result<(), ParseError> {
        self.unary_expr(allow_struct_lit)?;
        while let Some(prec) = self.bin_prec() {
            if prec < min_prec {
                break;
            }
            self.bump();
            self.bin_expr(prec + 1, allow_struct_lit)?;
        }
        Ok(())
    }

    fn unary_expr(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        if matches!(
            self.cur().kind,
            Kind::Plus
                | Kind::Minus
                | Kind::Bang
                | Kind::Caret
                | Kind::Star
                | Kind::Amp
                | Kind::Arrow
        ) {
            self.bump();
            return self.unary_expr(allow_struct_lit);
        }
        self.primary_expr(allow_struct_lit)
    }

    fn primary_expr(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        self.primary_atom()?;
        self.postfix_ops(allow_struct_lit)
    }

    fn primary_atom(&mut self) -> Result<(), ParseError> {
        match self.cur().kind {
            Kind::Number | Kind::String | Kind::Rune => {
                self.bump();
            }
            Kind::LParen => {
                self.bump();
                if self.at(Kind::Ident)
                    && matches!(self.cur().text, "map" | "struct" | "interface" | "chan")
                {
                    self.type_expr()?;
                } else {
                    self.expr(true)?;
                }
                self.expect(Kind::RParen, "`)`")?;
            }
            Kind::LBracket => {
                self.type_expr()?;
                if self.at(Kind::LBrace) {
                    self.composite_lit()?;
                }
            }
            Kind::Ident => match self.cur().text {
                "func" => {
                    self.bump();
                    self.signature()?;
                    if self.at(Kind::LBrace) {
                        self.block()?;
                    }
                }
                "map" | "struct" | "interface" | "chan" => {
                    self.type_expr()?;
                    if self.at(Kind::LBrace) {
                        self.composite_lit()?;
                    }
                }
                kw if is_go_keyword(kw) => {
                    return Err(self.err_here(format!("expected Go expression, found `{kw}`")));
                }
                _ => {
                    self.bump();
                }
            },
            _ => {
                return Err(self.err_here(format!(
                    "expected Go expression, found `{}`",
                    if self.at(Kind::Eof) {
                        "end of input"
                    } else {
                        self.cur().text
                    }
                )));
            }
        }
        Ok(())
    }

    fn postfix_ops(&mut self, allow_struct_lit: bool) -> Result<(), ParseError> {
        loop {
            match self.cur().kind {
                Kind::Dot => {
                    self.bump();
                    if self.eat(Kind::LParen) {
                        if self.at_kw("type") {
                            self.bump();
                        } else {
                            self.type_expr()?;
                        }
                        self.expect(Kind::RParen, "`)`")?;
                    } else {
                        self.expect_ident("selector name")?;
                    }
                }
                Kind::LBracket => {
                    self.bump();
                    if self.eat(Kind::Colon) {
                        if !matches!(self.cur().kind, Kind::Colon | Kind::RBracket) {
                            self.expr(true)?;
                        }
                        if self.eat(Kind::Colon) {
                            self.expr(true)?;
                        }
                    } else {
                        self.type_or_expr(true)?;
                        if self.eat(Kind::Colon) {
                            if !matches!(self.cur().kind, Kind::Colon | Kind::RBracket) {
                                self.expr(true)?;
                            }
                            if self.eat(Kind::Colon) {
                                self.expr(true)?;
                            }
                        } else {
                            while self.eat(Kind::Comma) {
                                if self.at(Kind::RBracket) {
                                    break;
                                }
                                self.type_or_expr(true)?;
                            }
                        }
                    }
                    self.expect(Kind::RBracket, "`]`")?;
                }
                Kind::LParen => {
                    self.bump();
                    while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
                        self.type_or_expr(true)?;
                        let _ = self.eat(Kind::Ellipsis);
                        if !self.eat(Kind::Comma) {
                            break;
                        }
                    }
                    self.expect(Kind::RParen, "`)`")?;
                }
                Kind::LBrace if allow_struct_lit => {
                    self.composite_lit()?;
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn composite_lit(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.at(Kind::LBrace) {
                self.composite_lit()?;
            } else {
                self.expr(true)?;
                if self.eat(Kind::Colon) {
                    if self.at(Kind::LBrace) {
                        self.composite_lit()?;
                    } else {
                        self.expr(true)?;
                    }
                }
            }
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect(Kind::RBrace, "`}`")?;
        Ok(())
    }
}
