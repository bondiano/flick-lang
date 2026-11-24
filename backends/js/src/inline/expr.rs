//! JavaScript expression, pattern, and object literal parsing for inline
//! foreign validation.

use super::lex::Kind;
use super::parse::{ParseError, Parser};

impl Parser<'_> {
    pub(super) fn pattern(&mut self) -> Result<(), ParseError> {
        match self.cur().kind {
            Kind::Ident => {
                self.bump();
                Ok(())
            }
            Kind::LBrace => {
                self.bump();
                while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
                    if self.eat(Kind::DotDotDot) {
                        self.pattern()?;
                        let _ = self.eat(Kind::Comma);
                        break;
                    }
                    self.prop_name()?;
                    if self.eat(Kind::Colon) {
                        self.pattern()?;
                    }
                    if self.eat(Kind::Assign) {
                        self.assign_expr()?;
                    }
                    if !self.eat(Kind::Comma) {
                        break;
                    }
                }
                self.expect(Kind::RBrace, "`}`")?;
                Ok(())
            }
            Kind::LBracket => {
                self.bump();
                while !self.at(Kind::RBracket) && !self.at(Kind::Eof) {
                    if self.eat(Kind::Comma) {
                        continue;
                    }
                    if self.eat(Kind::DotDotDot) {
                        self.pattern()?;
                        let _ = self.eat(Kind::Comma);
                        break;
                    }
                    self.pattern()?;
                    if self.eat(Kind::Assign) {
                        self.assign_expr()?;
                    }
                    if !self.eat(Kind::Comma) {
                        break;
                    }
                }
                self.expect(Kind::RBracket, "`]`")?;
                Ok(())
            }
            _ => Err(self.err_here("expected binding pattern")),
        }
    }

    pub(super) fn prop_name(&mut self) -> Result<(), ParseError> {
        match self.cur().kind {
            Kind::Ident | Kind::PrivateIdent | Kind::String | Kind::Number => {
                self.bump();
                Ok(())
            }
            Kind::LBracket => {
                self.bump();
                self.assign_expr()?;
                self.expect(Kind::RBracket, "`]`")?;
                Ok(())
            }
            _ => Err(self.err_here("expected property name")),
        }
    }

    pub(super) fn expr(&mut self) -> Result<(), ParseError> {
        self.assign_expr()?;
        while self.eat(Kind::Comma) {
            self.assign_expr()?;
        }
        Ok(())
    }

    pub(super) fn assign_expr(&mut self) -> Result<(), ParseError> {
        if self.at_kw("yield") {
            self.bump();
            let _ = self.eat(Kind::Star);
            if !self.cur().newline_before
                && !matches!(
                    self.cur().kind,
                    Kind::Semi
                        | Kind::RParen
                        | Kind::RBracket
                        | Kind::RBrace
                        | Kind::Comma
                        | Kind::Eof
                )
            {
                self.assign_expr()?;
            }
            return Ok(());
        }
        if self.is_arrow_head() {
            return self.arrow_fn();
        }
        self.cond_expr()?;
        if matches!(self.cur().kind, Kind::Assign | Kind::AssignOp) {
            self.bump();
            self.assign_expr()?;
        }
        Ok(())
    }

    fn is_arrow_head(&self) -> bool {
        let mut i = self.pos;
        if self.tokens[i].kind == Kind::Ident && self.tokens[i].text == "async" {
            let next = &self.tokens[(i + 1).min(self.tokens.len() - 1)];
            if !next.newline_before && matches!(next.kind, Kind::Ident | Kind::LParen) {
                i += 1;
            }
        }
        if self.tokens[i].kind == Kind::Ident {
            let next = &self.tokens[(i + 1).min(self.tokens.len() - 1)];
            return next.kind == Kind::Arrow && !next.newline_before;
        }
        if self.tokens[i].kind == Kind::LParen {
            let mut depth = 0usize;
            let mut j = i;
            while j < self.tokens.len() {
                match self.tokens[j].kind {
                    Kind::LParen => depth += 1,
                    Kind::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            let next = &self.tokens[(j + 1).min(self.tokens.len() - 1)];
                            return next.kind == Kind::Arrow && !next.newline_before;
                        }
                    }
                    Kind::Eof => return false,
                    _ => {}
                }
                j += 1;
            }
        }
        false
    }

    fn arrow_fn(&mut self) -> Result<(), ParseError> {
        if self.at_kw("async") && matches!(self.peek(1).kind, Kind::Ident | Kind::LParen) {
            self.bump();
        }
        if self.at(Kind::Ident) {
            self.bump();
        } else {
            self.param_list()?;
        }
        self.expect(Kind::Arrow, "`=>`")?;
        if self.at(Kind::LBrace) {
            self.block()
        } else {
            self.assign_expr()
        }
    }

    fn cond_expr(&mut self) -> Result<(), ParseError> {
        self.bin_expr(1)?;
        if self.eat(Kind::Question) {
            self.assign_expr()?;
            self.expect(Kind::Colon, "`:`")?;
            self.assign_expr()?;
        }
        Ok(())
    }

    fn bin_prec(&self) -> Option<u8> {
        match self.cur().kind {
            Kind::QuestionQuestion => Some(1),
            Kind::PipePipe => Some(2),
            Kind::AmpAmp => Some(3),
            Kind::Pipe => Some(4),
            Kind::Caret => Some(5),
            Kind::Amp => Some(6),
            Kind::EqEq | Kind::BangEq | Kind::EqEqEq | Kind::BangEqEq => Some(7),
            Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge => Some(8),
            Kind::Ident if matches!(self.cur().text, "in" | "instanceof") => Some(8),
            Kind::Shl | Kind::Shr | Kind::UShr => Some(9),
            Kind::Plus | Kind::Minus => Some(10),
            Kind::Star | Kind::Slash | Kind::Percent => Some(11),
            Kind::StarStar => Some(12),
            _ => None,
        }
    }

    fn bin_expr(&mut self, min_prec: u8) -> Result<(), ParseError> {
        self.unary_expr()?;
        while let Some(prec) = self.bin_prec() {
            if prec < min_prec {
                break;
            }
            let right_assoc = self.at(Kind::StarStar);
            self.bump();
            self.bin_expr(if right_assoc { prec } else { prec + 1 })?;
        }
        Ok(())
    }

    fn unary_expr(&mut self) -> Result<(), ParseError> {
        if matches!(
            self.cur().kind,
            Kind::Plus | Kind::Minus | Kind::Tilde | Kind::Bang | Kind::PlusPlus | Kind::MinusMinus
        ) || (self.at(Kind::Ident)
            && matches!(self.cur().text, "typeof" | "void" | "delete" | "await"))
        {
            self.bump();
            return self.unary_expr();
        }
        self.lhs_expr()?;
        if !self.cur().newline_before
            && matches!(self.cur().kind, Kind::PlusPlus | Kind::MinusMinus)
        {
            self.bump();
        }
        Ok(())
    }

    pub(super) fn lhs_expr(&mut self) -> Result<(), ParseError> {
        if self.eat_kw("new") {
            if self.eat(Kind::Dot) {
                self.expect_ident("`target`")?;
            } else {
                self.lhs_expr()?;
            }
            return Ok(());
        }
        self.primary_expr()?;
        loop {
            match self.cur().kind {
                Kind::Dot => {
                    self.bump();
                    if !matches!(self.cur().kind, Kind::Ident | Kind::PrivateIdent) {
                        return Err(self.err_here("expected property name after `.`"));
                    }
                    self.bump();
                }
                Kind::QuestionDot => {
                    self.bump();
                    if self.at(Kind::LParen) {
                        self.arg_list()?;
                    } else if self.eat(Kind::LBracket) {
                        self.expr()?;
                        self.expect(Kind::RBracket, "`]`")?;
                    } else if matches!(self.cur().kind, Kind::Ident | Kind::PrivateIdent) {
                        self.bump();
                    } else {
                        return Err(self.err_here("expected property or call after `?.`"));
                    }
                }
                Kind::LBracket => {
                    self.bump();
                    self.expr()?;
                    self.expect(Kind::RBracket, "`]`")?;
                }
                Kind::LParen => self.arg_list()?,
                Kind::TemplateNoSub => {
                    self.bump();
                }
                Kind::TemplateHead => self.template_expr()?,
                _ => break,
            }
        }
        Ok(())
    }

    fn arg_list(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LParen, "`(`")?;
        while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
            let _ = self.eat(Kind::DotDotDot);
            self.assign_expr()?;
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect(Kind::RParen, "`)`")?;
        Ok(())
    }

    fn template_expr(&mut self) -> Result<(), ParseError> {
        self.bump(); // TemplateHead
        loop {
            self.expr()?;
            match self.cur().kind {
                Kind::TemplateMiddle => {
                    self.bump();
                }
                Kind::TemplateTail => {
                    self.bump();
                    return Ok(());
                }
                _ => return Err(self.err_here("unterminated template expression")),
            }
        }
    }

    fn primary_expr(&mut self) -> Result<(), ParseError> {
        match self.cur().kind {
            Kind::Number | Kind::String | Kind::Regex | Kind::TemplateNoSub => {
                self.bump();
                Ok(())
            }
            Kind::TemplateHead => self.template_expr(),
            Kind::Ident => match self.cur().text {
                "function" => self.function_decl(true),
                "async"
                    if !self.peek(1).newline_before
                        && self.peek(1).kind == Kind::Ident
                        && self.peek(1).text == "function" =>
                {
                    self.function_decl(true)
                }
                "class" => self.class_decl(true),
                _ => {
                    self.bump();
                    Ok(())
                }
            },
            Kind::LParen => {
                self.bump();
                self.expr()?;
                self.expect(Kind::RParen, "`)`")?;
                Ok(())
            }
            Kind::LBracket => {
                self.bump();
                while !self.at(Kind::RBracket) && !self.at(Kind::Eof) {
                    if self.eat(Kind::Comma) {
                        continue;
                    }
                    let _ = self.eat(Kind::DotDotDot);
                    self.assign_expr()?;
                    if !self.eat(Kind::Comma) {
                        break;
                    }
                }
                self.expect(Kind::RBracket, "`]`")?;
                Ok(())
            }
            Kind::LBrace => self.object_lit(),
            _ => Err(self.err_here(format!(
                "expected JavaScript expression, found `{}`",
                if self.at(Kind::Eof) {
                    "end of input"
                } else {
                    self.cur().text
                }
            ))),
        }
    }

    fn object_lit(&mut self) -> Result<(), ParseError> {
        self.expect(Kind::LBrace, "`{`")?;
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if self.eat(Kind::DotDotDot) {
                self.assign_expr()?;
            } else {
                let is_async = self.at_kw("async")
                    && !self.peek(1).newline_before
                    && !matches!(
                        self.peek(1).kind,
                        Kind::Colon | Kind::Comma | Kind::RBrace | Kind::LParen
                    );
                if is_async {
                    self.bump();
                }
                let _ = self.eat(Kind::Star);
                if (self.at_kw("get") || self.at_kw("set"))
                    && !matches!(
                        self.peek(1).kind,
                        Kind::Colon | Kind::Comma | Kind::RBrace | Kind::LParen
                    )
                {
                    self.bump();
                }
                let starts_ident = self.at(Kind::Ident);
                self.prop_name()?;
                if self.eat(Kind::Colon) {
                    self.assign_expr()?;
                } else if self.at(Kind::LParen) {
                    self.param_list()?;
                    self.block()?;
                } else if !starts_ident {
                    return Err(self.err_here("expected `:` in object literal"));
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
