//! A plugin block as its plugin sees it, and the holes it asks the compiler
//! to parse: the text and tokens of the head's arguments and the body, with
//! offsets from the first argument (or from the body, without arguments),
//! and the user's expressions and types parsed where they stand.

use std::sync::Arc;

use flick_plugin::{Block, Compiler, Hole, HoleKind, Span, Token as PluginToken, TokenKind};

use crate::diagnostics::Diagnostic;
use crate::source::FileId;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::{Fragment, parse_expr_at, parse_type_at};

/// A plugin block of a module and what its plugin asked of it.
#[derive(Default)]
pub struct BlockSource {
    text: Arc<str>,
    file: FileId,
    head: String,
    /// Where the frame starts (at the first argument, or at the body) and
    /// where the body ends, in the module text.
    start: u32,
    end: u32,
    /// Where the body starts in the frame.
    body: u32,
    /// The tokens of the arguments and the body, trivia included and `{`
    /// left out, at their places in the text.
    tokens: Vec<Token>,
    /// What parsing the holes the plugin asked for reported.
    pub diagnostics: Vec<Diagnostic>,
}

impl BlockSource {
    /// The block `node`, a `PluginExpr` of the module text `text`; `None`
    /// for a block without its closing brace. Its frame starts at the first
    /// argument of the head, or at the body without arguments.
    pub fn new(text: Arc<str>, file: FileId, node: &Node) -> Option<Self> {
        let mut direct = node.child_tokens().filter(|t| !t.kind.is_trivia());
        let head = direct.next().filter(|t| t.kind == K::Ident)?;
        let first = direct.next()?;
        let open = node.child_tokens().find(|t| t.kind == K::LBrace)?;
        let close = node.child_tokens().find(|t| t.kind == K::RBrace)?;
        let start = if first.kind == K::LBrace {
            open.end
        } else {
            first.start
        };
        let tokens = (node.tokens().into_iter())
            .filter(|t| start <= t.start && t.end <= close.start && t.start != open.start)
            .collect();
        Some(BlockSource {
            head: head.text(&text).to_owned(),
            text,
            file,
            start,
            body: open.end - start,
            end: close.start,
            tokens,
            diagnostics: Vec::new(),
        })
    }

    pub fn head(&self) -> &str {
        &self.head
    }

    /// Where the frame starts in the module text: spans the plugin gives
    /// count from here.
    pub fn start(&self) -> u32 {
        self.start
    }

    pub fn file(&self) -> FileId {
        self.file
    }

    /// The column the frame starts at and the indent of its line, in
    /// characters: where the holes stand, which their layout depends on.
    pub fn layout(&self) -> (u32, u32) {
        let start = self.start as usize;
        let line = self.text[..start].rfind('\n').map_or(0, |i| i + 1);
        let before = &self.text[line..start];
        let count = |s: &str| u32::try_from(s.chars().count()).expect("columns fit in u32");
        (
            count(before),
            count(&before[..before.len() - before.trim_start().len()]),
        )
    }

    /// The block as the plugin reads it.
    pub fn block(&self) -> Block {
        let tokens = self
            .tokens
            .iter()
            .filter(|t| !t.kind.is_trivia())
            .map(|t| PluginToken {
                kind: token_kind(t.kind),
                span: self.relative(t.start, t.end),
            })
            .collect();
        Block {
            head: self.head.clone(),
            text: self.text[self.start as usize..self.end as usize].to_owned(),
            body: self.body,
            tokens,
        }
    }

    /// The user's code at `hole`, parsed again where it stands; `None` when
    /// it does not parse into exactly that span.
    pub fn hole_tree(&self, hole: Hole) -> Option<Node> {
        let fragment = self.fragment(hole.kind, hole.span.start, hole.span.end)?;
        (fragment.diagnostics.is_empty()
            && fragment.tree.range().0 == self.start + hole.span.start
            && fragment.end == self.start + hole.span.end)
            .then_some(fragment.tree)
    }

    /// The longest expression or type of the body from `start` on, before
    /// `end`, offsets relative to the body; syntax errors go to
    /// [`BlockSource::diagnostics`].
    fn parse(
        &mut self,
        kind: HoleKind,
        start: u32,
        end: u32,
    ) -> Result<Hole, flick_plugin::Diagnostic> {
        let refuse = |message: &str| flick_plugin::Diagnostic::error(message, Span { start, end });
        let what = match kind {
            HoleKind::Expr => "an expression",
            HoleKind::Ty => "a type",
        };
        let Some(fragment) = self.fragment(kind, start, end) else {
            return Err(refuse(&format!("expected {what} here")));
        };
        if let Some(first) = fragment.diagnostics.first() {
            let message = first.message.clone();
            self.diagnostics.extend(fragment.diagnostics);
            return Err(refuse(&message));
        }
        let (from, _) = fragment.tree.range();
        Ok(Hole {
            kind,
            span: self.relative(from, fragment.end),
        })
    }

    fn fragment(&self, kind: HoleKind, start: u32, end: u32) -> Option<Fragment> {
        if start > end || end > self.end - self.start {
            return None;
        }
        let (from, to) = (self.start + start, self.start + end);
        let first = self.tokens.iter().position(|t| t.start >= from)?;
        let last = self.tokens.iter().rposition(|t| t.end <= to)? + 1;
        let tokens = self.tokens.get(first..last)?;
        match kind {
            HoleKind::Expr => parse_expr_at(&self.text, self.file, tokens),
            HoleKind::Ty => parse_type_at(&self.text, self.file, tokens),
        }
    }

    fn relative(&self, start: u32, end: u32) -> Span {
        Span {
            start: start - self.start,
            end: end - self.start,
        }
    }
}

impl Compiler for BlockSource {
    fn parse_expr(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Expr, start, end)
    }

    fn parse_type(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Ty, start, end)
    }
}

fn token_kind(kind: K) -> TokenKind {
    match kind {
        K::Ident => TokenKind::Ident,
        K::Con => TokenKind::Con,
        K::Number | K::BigInt => TokenKind::Number,
        K::String => TokenKind::String,
        k if k.is_keyword() => TokenKind::Keyword,
        _ => TokenKind::Punct,
    }
}
