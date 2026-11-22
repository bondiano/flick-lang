//! Block, brace, and delimited layout combinators for [`Parser`].

use super::{Ctx, K, Parser, SyntaxKind, codes};

impl Parser<'_> {
    /// Whether a `catch` token starts a new line at the current aligned block
    /// column: used as `glue` in records and foreign metadata where `catch` is
    /// a field or metadata name rather than a postfix handler continuation.
    pub(super) fn at_aligned_catch(&self) -> bool {
        self.raw().line_start && self.at(K::CatchKw) && self.col() == self.aligned_col()
    }

    /// A block after `=`, `->`, `then`, `else` or `try`: aligned on its first
    /// token when that starts a line, otherwise one inline statement.
    pub(super) fn body(&mut self, kind: SyntaxKind, item: fn(&mut Self)) {
        self.body_glued(kind, item, |_| false);
    }

    pub(super) fn body_glued(
        &mut self,
        kind: SyntaxKind,
        item: fn(&mut Self),
        glue: fn(&Self) -> bool,
    ) {
        let m = self.start();
        if self.sep().is_some() {
            let span = self.here_span();
            self.error_at(
                codes::S009,
                span,
                "expected an indented block on the next line".into(),
            );
        } else if self.raw().line_start {
            self.push(Ctx::Aligned { col: self.col() });
            self.items(item, false, glue);
            self.pop();
        } else {
            // a line at the column of an enclosing mid-line block is its next item
            self.push(Ctx::Inline {
                threshold: self.raw().line_indent.max(self.aligned_col()),
            });
            item(self);
            self.pop();
        }
        self.complete(m, kind);
    }

    /// `{ items }`, aligned on the first item; returns false without `{`.
    pub(super) fn braces(
        &mut self,
        kind: SyntaxKind,
        item: fn(&mut Self),
        glue: fn(&Self) -> bool,
    ) -> bool {
        let opener = self.raw();
        if !self.expect(K::LBrace) {
            return false;
        }
        let m = self.start();
        if self.raw().kind != K::RBrace {
            self.push(Ctx::Aligned { col: self.col() });
            self.items(item, true, glue);
            self.pop();
        }
        self.complete(m, kind);
        self.expect_closer(K::RBrace, opener);
        true
    }

    /// `open items close` separated by `,`, with line breaks insignificant.
    pub(super) fn delimited(&mut self, close: SyntaxKind, item: fn(&mut Self)) {
        let opener = self.raw();
        self.bump();
        self.push(Ctx::Bracket);
        while !self.at(close) && self.sep().is_none() && self.raw().kind != close {
            let before = self.pos;
            item(self);
            if !self.eat(K::Comma) {
                break;
            }
            if self.pos == before {
                break;
            }
        }
        self.pop();
        self.expect_closer(close, opener);
    }
}
