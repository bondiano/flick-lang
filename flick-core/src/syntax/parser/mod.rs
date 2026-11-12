//! Recursive-descent parser with Pratt operators over the lexer's tokens.
//!
//! Layout (`docs/src/content/docs/reference/grammar.md`) is resolved here rather than by
//! inserting tokens: a stack of contexts decides whether a token that starts a
//! line separates statements ([`Sep::Next`]), closes the block ([`Sep::End`]) or
//! continues the current construct. The parser sees a boundary as the virtual
//! [`SyntaxKind::LayoutSep`], so every construct stops at it without special code.
//!
//! The parser emits start/token/finish events; [`build`] turns them into the
//! lossless tree, attaching trivia between nodes.

mod build;
mod conditional;
mod control;
mod decl;
mod expr;
mod fragment;
mod pattern;
mod plugin;
mod types;

use std::{iter, mem};

use SyntaxKind as K;
use build::{Event, build};

pub use fragment::{
    Fragment, Heads, parse_body_at, parse_decl_at, parse_decls_at, parse_expr_at, parse_lines_at,
    parse_pat_at, parse_type_at,
};

use super::cst::Node;
use super::kind::{SyntaxKind, describe};
use super::lexer::{RawToken, lex};
use crate::diagnostics::{Code, Diagnostic, codes};
use crate::source::{FileId, Span};

pub struct Parse {
    pub tree: Node,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(text: &str, file: FileId) -> Parse {
    parse_with_heads_mode(text, file, &[], false)
}

/// [`parse`] of a part of a file whose imports expose the plugin heads
/// `heads`, as a declaration the formatter laid out.
pub fn parse_with_heads<'t>(text: &'t str, file: FileId, heads: Heads<'t>) -> Parse {
    parse_with_heads_mode(text, file, heads, true)
}

fn parse_with_heads_mode<'t>(
    text: &'t str,
    file: FileId,
    heads: Heads<'t>,
    allow_trailing_attribute: bool,
) -> Parse {
    let (raw, mut diagnostics) = lex(text, file);
    let mut p = Parser::new(text, file, &raw);
    p.plugin_heads.extend(heads);
    decl::source_file(&mut p);
    diagnostics.append(&mut p.errors);
    let tree = build(&raw, p.events);
    diagnostics.extend(super::validate::check(
        &tree,
        text,
        file,
        allow_trailing_attribute,
    ));
    Diagnostic::sort_by_span(&mut diagnostics);
    Parse { tree, diagnostics }
}

/// A non-trivia token with the layout facts the parser needs.
#[derive(Debug, Clone, Copy)]
struct Tok {
    kind: SyntaxKind,
    start: u32,
    end: u32,
    /// Column in characters from the start of its line.
    col: u32,
    /// First non-trivia token on its line.
    line_start: bool,
    /// Column of the first token of the line this token is on.
    line_indent: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sep {
    Next,
    End,
}

#[derive(Debug, Clone, Copy)]
enum Ctx {
    /// Statements start at `col`; a line at a smaller column ends the block.
    Aligned { col: u32 },
    /// A block that began mid-line: lines indented deeper than the line it
    /// began on continue it, anything else ends it.
    Inline { threshold: u32 },
    /// Inside `( )`, `[ ]` and row braces line breaks are insignificant.
    Bracket,
}

pub(crate) struct Marker(usize);
#[derive(Clone, Copy)]
pub(crate) struct Done(usize);

const MAX_DEPTH: u32 = 200;

pub(crate) struct Parser<'t> {
    text: &'t str,
    file: FileId,
    toks: Vec<Tok>,
    pos: usize,
    events: Vec<Event>,
    ctx: Vec<(Ctx, bool)>,
    /// Token whose `Sep::Next` the current block already consumed.
    sep_taken: Option<usize>,
    errors: Vec<Diagnostic>,
    last_error_at: Option<usize>,
    /// The current statement already has a syntax error; later ones in it are
    /// consequences and stay quiet.
    failed: bool,
    /// `provide e { … }` and `codec e { … }`: `{` ends the head expression.
    no_brace: bool,
    /// The heads of plugin blocks the imports so far exposed.
    plugin_heads: Vec<&'t str>,
    /// The lines under a call are its arguments, one each: a plugin asked
    /// for its body to be read so.
    line_children: bool,
    depth: u32,
}

/// Token kinds that continue the construct even at the column of the block.
fn continues(kind: SyntaxKind) -> bool {
    (kind.is_binary_op() && kind != K::Minus)
        || kind.is_closer()
        || matches!(
            kind,
            K::Pipe | K::ThenKw | K::ElseKw | K::WithKw | K::YieldKw
        )
}

impl<'t> Parser<'t> {
    fn new(text: &'t str, file: FileId, raw: &[RawToken]) -> Self {
        let mut toks = Vec::new();
        let mut col = 0;
        let mut line_has_token = false;
        let mut indent = 0;
        for t in raw {
            if !t.kind.is_trivia() {
                let kind = t.kind;
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
            }
            let token = &text[t.start as usize..t.end as usize];
            if let Some((_, tail)) = token.rsplit_once('\n') {
                // A multi-line string still occupies its final line; trivia does not.
                col = u32::try_from(tail.chars().count()).expect("columns fit in u32");
                line_has_token = !t.kind.is_trivia();
            } else {
                col += u32::try_from(token.chars().count()).expect("columns fit in u32");
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
        Parser {
            text,
            file,
            toks,
            pos: 0,
            events: Vec::new(),
            ctx: Vec::new(),
            sep_taken: None,
            errors: Vec::new(),
            last_error_at: None,
            failed: false,
            no_brace: false,
            plugin_heads: Vec::new(),
            line_children: false,
            depth: 0,
        }
    }

    // ---- layout ----

    fn sep_at(&self, i: usize) -> Option<Sep> {
        let t = self.toks[i];
        if t.kind == K::Eof {
            return Some(Sep::End);
        }
        if !t.line_start || self.sep_taken == Some(i) {
            return None;
        }
        match self.ctx.last().map_or(Ctx::Aligned { col: 0 }, |c| c.0) {
            Ctx::Aligned { col } if t.col < col => Some(Sep::End),
            Ctx::Aligned { col } if t.col == col && !continues(t.kind) => Some(Sep::Next),
            Ctx::Aligned { .. } => None,
            Ctx::Inline { threshold } => (t.col <= threshold).then_some(Sep::End),
            Ctx::Bracket if t.kind.is_closer() => None,
            Ctx::Bracket => (t.col == 0 || t.col < self.aligned_col()).then_some(Sep::End),
        }
    }

    /// Column of the innermost aligned block (the top level is column 0).
    fn aligned_col(&self) -> u32 {
        self.ctx
            .iter()
            .rev()
            .find_map(|c| match c.0 {
                Ctx::Aligned { col } => Some(col),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn sep(&self) -> Option<Sep> {
        self.sep_at(self.pos)
    }

    fn push(&mut self, ctx: Ctx) {
        self.ctx.push((ctx, self.no_brace));
        self.no_brace = false;
    }

    /// Starts a block aligned on the current token, which is its first item.
    fn push_aligned_here(&mut self) {
        self.push(Ctx::Aligned { col: self.col() });
        self.sep_taken = Some(self.pos);
    }

    fn pop(&mut self) {
        let (_, no_brace) = self.ctx.pop().expect("balanced layout contexts");
        self.no_brace = no_brace;
    }

    /// The current token's column, for blocks that align on it.
    fn col(&self) -> u32 {
        self.toks[self.pos].col
    }

    // ---- token access ----

    /// Kind of the `n`-th token ahead, or `LayoutSep` if a boundary comes first.
    fn nth(&self, n: usize) -> SyntaxKind {
        for k in 0..=n {
            let i = (self.pos + k).min(self.toks.len() - 1);
            if self.sep_at(i).is_some() {
                return K::LayoutSep;
            }
            if k == n {
                return self.toks[i].kind;
            }
        }
        unreachable!()
    }

    fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    /// The current token ignoring layout.
    fn raw(&self) -> Tok {
        self.toks[self.pos]
    }

    fn prev_end(&self) -> u32 {
        self.pos.checked_sub(1).map_or(0, |i| self.toks[i].end)
    }

    /// Whether the current token touches the previous one (no trivia between).
    fn adjacent(&self) -> bool {
        self.pos > 0 && self.raw().start == self.prev_end()
    }

    fn text_of(&self, tok: Tok) -> &'t str {
        &self.text[tok.start as usize..tok.end as usize]
    }

    /// Whether the current (visible) token is the identifier `word`.
    fn at_word(&self, word: &str) -> bool {
        self.at(K::Ident) && self.text_of(self.raw()) == word
    }

    fn bump(&mut self) {
        debug_assert_ne!(self.raw().kind, K::Eof, "bump at end of file");
        self.events.push(Event::Token(None));
        self.pos += 1;
    }

    /// Consumes the current token as `kind`: a word that is a keyword here.
    fn bump_as(&mut self, kind: SyntaxKind) {
        self.events.push(Event::Token(Some(kind)));
        self.pos += 1;
    }

    fn eat(&mut self, kind: SyntaxKind) -> bool {
        let hit = self.at(kind);
        if hit {
            self.bump();
        }
        hit
    }

    fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error_expected(describe(kind));
        false
    }

    /// Closing delimiters close their bracket even when dedented, so they are
    /// matched ignoring layout.
    fn expect_closer(&mut self, kind: SyntaxKind, opener: Tok) {
        if self.raw().kind == kind {
            self.bump();
            return;
        }
        let open = self.text_of(opener);
        let span = self.here_span();
        if !self.failed && self.last_error_at != Some(self.pos) {
            self.failed = true;
            self.last_error_at = Some(self.pos);
            let found = self.found();
            self.errors.push(
                Diagnostic::error(
                    codes::S008,
                    format!(
                        "expected {} to close `{open}`, found {found}",
                        describe(kind)
                    ),
                    span,
                )
                .with_related(self.span(opener.start, opener.end), "unclosed delimiter"),
            );
        }
    }

    // ---- markers ----

    fn start(&mut self) -> Marker {
        self.events.push(Event::Start {
            kind: None,
            forward_parent: None,
        });
        Marker(self.events.len() - 1)
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "taking the marker completes it once"
    )]
    fn complete(&mut self, m: Marker, kind: SyntaxKind) -> Done {
        let Marker(at) = m;
        if let Event::Start { kind: k, .. } = &mut self.events[at] {
            *k = Some(kind);
        }
        self.events.push(Event::Finish);
        Done(at)
    }

    /// Starts a node that will wrap the already completed `done`.
    fn precede(&mut self, done: Done) -> Marker {
        let m = self.start();
        if let Event::Start { forward_parent, .. } = &mut self.events[done.0] {
            *forward_parent = Some(m.0);
        }
        m
    }

    // ---- diagnostics ----

    fn span(&self, start: u32, end: u32) -> Span {
        Span::new(self.file, start, end)
    }

    /// Span of the current token, or the end of the previous one at a boundary.
    fn here_span(&self) -> Span {
        let t = self.raw();
        if t.kind == K::Eof || self.sep().is_some() {
            let end = self.prev_end();
            self.span(end, end)
        } else {
            self.span(t.start, t.end)
        }
    }

    fn found(&self) -> String {
        let t = self.raw();
        if t.kind == K::Eof {
            "end of file".into()
        } else if self.sep().is_some() {
            "the end of the line".into()
        } else {
            format!("`{}`", self.text_of(t))
        }
    }

    fn error_at(&mut self, code: Code, span: Span, message: String) {
        if self.failed || self.last_error_at == Some(self.pos) {
            return;
        }
        self.failed = true;
        self.last_error_at = Some(self.pos);
        self.errors.push(Diagnostic::error(code, message, span));
    }

    fn error_expected(&mut self, what: &str) {
        let message = format!("expected {what}, found {}", self.found());
        self.error_at(codes::S007, self.here_span(), message);
    }

    /// Wraps the rest of the current statement in an `Error` node.
    fn recover(&mut self) {
        let m = self.start();
        while self.sep().is_none() && !(self.raw().kind.is_closer() && self.in_brackets()) {
            self.bump();
        }
        self.complete(m, K::Error);
    }

    fn in_brackets(&self) -> bool {
        !self.ctx.is_empty()
    }

    /// Reports an unexpected token after a complete statement, as an
    /// indentation problem when it starts a line.
    fn unexpected(&mut self) {
        self.failed = false;
        self.report_unexpected();
    }

    fn report_unexpected(&mut self) {
        let t = self.raw();
        if t.line_start {
            let cols: Vec<String> = self
                .ctx
                .iter()
                .filter_map(|c| match c.0 {
                    Ctx::Aligned { col } => Some((col + 1).to_string()),
                    _ => None,
                })
                .collect();
            let message = format!(
                "unexpected indentation: column {} continues nothing here; \
                 statements of the enclosing blocks start at column {}",
                t.col + 1,
                iter::once("1".to_owned())
                    .chain(cols)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            self.error_at(codes::S009, self.span(t.start, t.end), message);
        } else {
            let message = format!("unexpected {}", self.found());
            self.error_at(codes::S007, self.here_span(), message);
        }
    }

    fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            self.error_expected("less deeply nested code");
            if self.raw().kind != K::Eof {
                self.skip_too_deep();
            }
            return false;
        }
        self.depth += 1;
        true
    }

    /// The rest of a too deeply nested construct, to the end of its line or
    /// brackets, as one error: a long `a <| b <| …` nests no deeper.
    fn skip_too_deep(&mut self) {
        let (m, mut open) = (self.start(), 0u32);
        loop {
            match self.raw().kind {
                K::LParen | K::LBracket | K::LBrace => open += 1,
                kind if kind.is_closer() => open = open.saturating_sub(1),
                _ => {}
            }
            self.bump();
            let next = self.current();
            if matches!(next, K::Eof | K::LayoutSep) || next.is_closer() && open == 0 {
                break;
            }
        }
        self.complete(m, K::Error);
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    // ---- blocks ----

    /// Statements separated by line breaks at the block column, or by `;` in
    /// braces. `glue` lets an item start without a separator (e.g. SQL clauses).
    fn items(&mut self, item: fn(&mut Self), braces: bool, glue: fn(&Self) -> bool) {
        let mut may_start = true;
        let mut item_failed = false;
        loop {
            if braces && self.raw().kind == K::RBrace {
                break;
            }
            match self.sep() {
                Some(Sep::End) => break,
                Some(Sep::Next) => self.sep_taken = Some(self.pos),
                None if may_start || glue(self) => {}
                // the closer belongs to an enclosing bracket
                None if self.raw().kind.is_closer() && self.in_brackets() => break,
                // a `;` after a block on its own lines ends that block and
                // separates the items of the braces around it
                None if !braces && self.raw().kind == K::Semi && self.in_brackets() => break,
                None => {
                    if !item_failed {
                        self.unexpected();
                    }
                    self.recover();
                    continue;
                }
            }
            let before = self.pos;
            let outer = mem::replace(&mut self.failed, false);
            item(self);
            item_failed = self.failed;
            self.failed = outer || item_failed;
            may_start = false;
            while braces && self.eat(K::Semi) {
                may_start = true;
            }
            if self.pos == before {
                let t = self.raw();
                if t.kind == K::Eof || (t.kind.is_closer() && self.in_brackets()) {
                    break;
                }
                let m = self.start();
                self.bump();
                self.complete(m, K::Error);
            }
        }
    }

    /// A block after `=`, `->`, `then`, `else` or `try`: aligned on its first
    /// token when that starts a line, otherwise one inline statement.
    fn body(&mut self, kind: SyntaxKind, item: fn(&mut Self)) {
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
            self.items(item, false, |_| false);
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
    fn braces(&mut self, kind: SyntaxKind, item: fn(&mut Self), glue: fn(&Self) -> bool) -> bool {
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
    fn delimited(&mut self, close: SyntaxKind, item: fn(&mut Self)) {
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

#[cfg(test)]
mod branch_tests;
#[cfg(test)]
mod exposing_tests;
#[cfg(test)]
mod operator_tests;
#[cfg(test)]
mod plugin_tests;
#[cfg(test)]
mod tests;
