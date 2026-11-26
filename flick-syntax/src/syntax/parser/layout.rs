//! Block, brace, and delimited layout combinators for [`Parser`].

use super::{Ctx, K, Parser, RawToken, SyntaxKind, Tok, codes};

#[inline]
fn char_width(slice: &str) -> u32 {
    if slice.is_ascii() {
        u32::try_from(slice.len()).expect("columns fit in u32")
    } else {
        u32::try_from(slice.chars().count()).expect("columns fit in u32")
    }
}

fn advance_multiline_col(slice: &str, col: &mut u32, line_has_token: &mut bool, is_trivia: bool) {
    if let Some((_, tail)) = slice.rsplit_once('\n') {
        *col = char_width(tail);
        *line_has_token = !is_trivia;
    } else {
        *col += char_width(slice);
    }
}

/// Drops trivia and computes each token's column and line-start facts.
pub(super) fn collect_tokens(text: &str, raw: &[RawToken]) -> Vec<Tok> {
    let mut toks = Vec::with_capacity(raw.len() / 2 + 1);
    let mut col = 0_u32;
    let mut line_has_token = false;
    let mut indent = 0_u32;
    for &t in raw {
        let kind = t.kind;
        match kind {
            K::Newline => {
                col = 0;
                line_has_token = false;
                continue;
            }
            K::Whitespace => {
                col += t.end - t.start;
                continue;
            }
            K::LineComment | K::DocComment => {
                col += char_width(&text[t.start as usize..t.end as usize]);
                continue;
            }
            K::BlockComment | K::Unknown => {
                advance_multiline_col(
                    &text[t.start as usize..t.end as usize],
                    &mut col,
                    &mut line_has_token,
                    true,
                );
                continue;
            }
            _ => {}
        }
        let line_start = !line_has_token;
        if line_start {
            indent = col;
        }
        toks.push(Tok {
            kind,
            start: t.start,
            end: t.end,
            col,
            line_start,
            line_indent: indent,
        });
        line_has_token = true;
        match kind {
            K::String => {
                let token = &text[t.start as usize..t.end as usize];
                if token.as_bytes().contains(&b'\n') {
                    advance_multiline_col(token, &mut col, &mut line_has_token, false);
                } else {
                    col += char_width(token);
                }
            }
            K::StringStart | K::StringMid | K::StringEnd => {
                col += char_width(&text[t.start as usize..t.end as usize]);
            }
            _ => {
                // Identifiers, constructors, keywords, numbers, labels, and punctuation are single-line ASCII.
                col += t.end - t.start;
            }
        }
    }
    let end = u32::try_from(text.len()).expect("source fits in u32");
    toks.push(Tok {
        kind: K::Eof,
        start: end,
        end,
        col: 0,
        line_start: true,
        line_indent: 0,
    });
    toks
}

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
        self.body_with(kind, item, glue, |_| false);
    }

    pub(super) fn body_stop(
        &mut self,
        kind: SyntaxKind,
        item: fn(&mut Self),
        stop: fn(&Self) -> bool,
    ) {
        self.body_with(kind, item, |_| false, stop);
    }

    fn body_with(
        &mut self,
        kind: SyntaxKind,
        item: fn(&mut Self),
        glue: fn(&Self) -> bool,
        stop: fn(&Self) -> bool,
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
            self.items_with_stop(item, false, glue, stop);
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
