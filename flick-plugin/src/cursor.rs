//! Reading a block: the text of its spans, its arguments and a cursor over
//! the tokens of its body, which asks the compiler for the user's code it
//! comes to and goes past it.

use crate::{Block, Compiler, Diagnostic, Span, Token, TokenKind};

impl Block {
    /// The text of `span`, empty outside the block.
    pub fn text_of(&self, span: Span) -> &str {
        (self.text)
            .get(span.start as usize..span.end as usize)
            .unwrap_or("")
    }

    /// The whole of the block: an error there is on the block, its head
    /// with it.
    pub fn whole(&self) -> Span {
        Span {
            start: 0,
            end: u32::try_from(self.text.len()).unwrap_or(u32::MAX),
        }
    }

    /// The tokens of the head's arguments, before the body.
    pub fn arguments(&self) -> &[Token] {
        let n = (self.tokens.iter())
            .take_while(|t| t.span.start < self.body)
            .count();
        &self.tokens[..n]
    }

    /// A cursor at the first token: the arguments, the body, then the rest
    /// of a declaration.
    pub fn cursor(&self) -> Cursor<'_> {
        Cursor {
            block: self,
            rest: &self.tokens,
        }
    }

    /// A cursor at the first token of the body.
    pub fn body(&self) -> Cursor<'_> {
        Cursor {
            block: self,
            rest: &self.tokens[self.arguments().len()..],
        }
    }

    /// The `///` lines right above the line of `offset`, each trimmed, joined
    /// by `separator`: the documentation of what starts there.
    pub fn docs_above(&self, offset: u32, separator: &str) -> String {
        let before = self.text.get(..offset as usize).unwrap_or(&self.text);
        let line = before.rfind('\n').unwrap_or(0);
        let mut lines: Vec<&str> = (before[..line].lines().rev())
            .map_while(|l| l.trim_start().strip_prefix("///").map(str::trim))
            .collect();
        lines.reverse();
        lines.join(separator)
    }
}

/// The tokens of a block from one on.
#[derive(Debug, Clone)]
pub struct Cursor<'b> {
    block: &'b Block,
    rest: &'b [Token],
}

impl<'b> Cursor<'b> {
    pub fn peek(&self) -> Option<Token> {
        self.rest.first().copied()
    }

    pub fn is_done(&self) -> bool {
        self.rest.is_empty()
    }

    /// Where the next token starts, or the end of the block after the last.
    pub fn start(&self) -> u32 {
        self.peek().map_or(self.block.whole().end, |t| t.span.start)
    }

    /// The text of `token`.
    pub fn text(&self, token: Token) -> &'b str {
        self.block.text_of(token.span)
    }

    /// Whether the next token is `text`, a word or punctuation.
    pub fn at(&self, text: &str) -> bool {
        self.peek().is_some_and(|t| self.text(t) == text)
    }

    /// The next token when it is `text`, past it.
    pub fn eat(&mut self, text: &str) -> Option<Token> {
        self.at(text).then(|| self.bump())
    }

    /// The next token when it is a name, past it.
    pub fn ident(&mut self) -> Option<Token> {
        (self.peek()?.kind == TokenKind::Ident).then(|| self.bump())
    }

    /// The strings that come next, past them.
    pub fn strings(&mut self) -> Vec<Span> {
        let mut out = Vec::new();
        while self.peek().is_some_and(|t| t.kind == TokenKind::String) {
            out.push(self.bump().span);
        }
        out
    }

    /// The user's expression from the next token on, the longest one, past
    /// it.
    pub fn expr(&mut self, compiler: &mut dyn Compiler) -> Result<Span, Diagnostic> {
        let hole = compiler.parse_expr(self.start(), self.block.whole().end)?;
        self.skip_to(hole.span.end);
        Ok(hole.span)
    }

    /// The user's type from the next token on, the longest one, past it.
    pub fn ty(&mut self, compiler: &mut dyn Compiler) -> Result<Span, Diagnostic> {
        let hole = compiler.parse_type(self.start(), self.block.whole().end)?;
        self.skip_to(hole.span.end);
        Ok(hole.span)
    }

    /// The user's pattern from the next token on, the longest one, past it.
    pub fn pat(&mut self, compiler: &mut dyn Compiler) -> Result<Span, Diagnostic> {
        let hole = compiler.parse_pat(self.start(), self.block.whole().end)?;
        self.skip_to(hole.span.end);
        Ok(hole.span)
    }

    /// The user's declaration from the next token on, past it.
    pub fn decl(&mut self, compiler: &mut dyn Compiler) -> Result<Span, Diagnostic> {
        let hole = compiler.parse_decl(self.start(), self.block.whole().end)?;
        self.skip_to(hole.span.end);
        Ok(hole.span)
    }

    /// The tokens left.
    pub fn rest(&self) -> &'b [Token] {
        self.rest
    }

    /// The tokens from here up to `end`.
    pub fn until(&self, end: u32) -> &'b [Token] {
        let n = self.rest.iter().take_while(|t| t.span.end <= end).count();
        &self.rest[..n]
    }

    fn skip_to(&mut self, end: u32) {
        self.rest = &self.rest[self.until(end).len()..];
    }

    fn bump(&mut self) -> Token {
        let (first, rest) = self.rest.split_first().expect("a token to take");
        self.rest = rest;
        *first
    }
}

impl Iterator for Cursor<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        (!self.is_done()).then(|| self.bump())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Block, Place, Span, Token, TokenKind};

    fn token(kind: TokenKind, start: u32, end: u32) -> Token {
        Token {
            kind,
            span: Span { start, end },
        }
    }

    #[test]
    fn a_cursor_reads_the_body_after_the_arguments() {
        let text = "\"x\" {\n  /// one\n  /// two\n  flag a \"b\" : Bool";
        let at = |s: &str| u32::try_from(text.find(s).expect("in the text")).expect("short");
        let flag = at("flag");
        let block = Block {
            head: "cli".into(),
            text: text.into(),
            body: at("{") + 1,
            place: Place::Expression,
            tokens: vec![
                token(TokenKind::String, 0, 3),
                token(TokenKind::Ident, flag, flag + 4),
                token(TokenKind::Ident, flag + 5, flag + 6),
                token(TokenKind::String, flag + 7, flag + 10),
                token(TokenKind::Punct, flag + 11, flag + 12),
            ],
        };
        assert_eq!(block.arguments().len(), 1);
        let mut c = block.body();
        assert!(c.eat("flag").is_some());
        assert_eq!(c.ident().map(|t| c.text(t)), Some("a"));
        assert_eq!(c.strings().len(), 1);
        assert!(c.at(":") && !c.at("="));
        assert_eq!(block.docs_above(flag, " "), "one two");
        assert_eq!(c.by_ref().count(), 1);
        assert_eq!(c.start(), block.whole().end);
    }
}
