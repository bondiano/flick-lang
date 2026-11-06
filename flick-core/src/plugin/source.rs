//! A plugin block as its plugin sees it, and the holes it asks the compiler
//! to parse: the text and tokens of the head's arguments, the body and the
//! rest of a declaration, with offsets from the first argument (or from the
//! body, without arguments), and the user's expressions, types, patterns
//! and declarations parsed where they stand.

use std::collections::BTreeMap;
use std::sync::Arc;

use flick_plugin::{
    Block, Compiler, Hole, HoleKind, Place, ReachError, Request, Response, Signature, Span,
    Token as PluginToken, TokenKind,
};

use super::reach::Reach;
use crate::diagnostics::Diagnostic;
use crate::source::FileId;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::{
    Fragment, parse_body_at, parse_decl_at, parse_expr_at, parse_pat_at, parse_type_at,
};
use crate::types::{let_parts, signature_name};

/// Where the spans of a hole in the signature above a block start: past
/// any of its own text.
const SIGNED: u32 = 1 << 31;

/// The hole of the user's type at `span` of the signature above a block.
pub(crate) fn signed(span: Span) -> Hole {
    Hole {
        kind: HoleKind::Ty,
        span: Span {
            start: span.start.saturating_add(SIGNED),
            end: span.end.saturating_add(SIGNED),
        },
    }
}

/// Where in the signature above its block `hole` is, if it is there.
pub(crate) fn in_signature(hole: Hole) -> Option<Span> {
    (hole.span.start >= SIGNED).then(|| Span {
        start: hole.span.start - SIGNED,
        end: hole.span.end - SIGNED,
    })
}

/// The signature right above a declaration block, at its place in the
/// module text.
#[derive(Clone)]
struct Above {
    whole: (u32, u32),
    tokens: Vec<Token>,
}

mod seen;

pub use seen::Seen;

/// A plugin block of a module and what its plugin asked of it.
#[derive(Default)]
pub struct BlockSource {
    text: Arc<str>,
    file: FileId,
    head: String,
    /// Where the head starts in the module text.
    head_at: u32,
    /// Where the frame starts (at the first argument, or at the body) and
    /// where the body ends, in the module text.
    start: u32,
    end: u32,
    /// Where the body starts in the frame; its length without a body.
    body: u32,
    closed: bool,
    place: Place,
    /// The tokens of the arguments and the body, trivia included and `{`
    /// left out, at their places in the text.
    tokens: Vec<Token>,
    /// The `///` lines right above a declaration, or above its signature.
    docs: String,
    signature: Option<Above>,
    module: Arc<str>,
    /// The heads the module imports, which start blocks in the holes.
    heads: Arc<[String]>,
    /// A declaration of another module, read by a derive: no span of it is
    /// a place of this module.
    foreign: bool,
    /// What parsing the holes the plugin asked for reported.
    pub diagnostics: Vec<Diagnostic>,
    /// What the plugin reached outside the block, as the project grants it.
    pub reach: Reach,
    /// The enums the module sees, as it writes them, with their base types.
    enums: Arc<BTreeMap<String, String>>,
    /// What the plugin published of the names the module sees, as it
    /// writes them.
    published: Arc<BTreeMap<String, Vec<u8>>>,
}

impl BlockSource {
    /// The block `node`, a `PluginExpr` or a `PluginDecl` of the module text
    /// `text`. Its frame starts at the first argument of the head, or at the
    /// body without arguments, and ends at `}`, or at the end of the node for
    /// a block without a body or not closed.
    pub fn new(text: Arc<str>, file: FileId, node: &Node, heads: Arc<[String]>) -> Option<Self> {
        let head = super::head_token(node)?;
        let all = node.tokens();
        let first = (all.iter()).find(|t| !t.kind.is_trivia() && t.start > head.start)?;
        // the braces of the body, not of an argument or of the rest
        let open = node.child_tokens().find(|t| t.kind == K::LBrace);
        let close = node.child_tokens().find(|t| t.kind == K::RBrace);
        let (_, node_end) = node.range();
        let start = match open {
            Some(o) if o.start == first.start => o.end,
            _ => first.start,
        };
        let (end, body, closed) = match (open, close) {
            (Some(o), Some(c)) => (c.start, o.end, true),
            (Some(o), None) => (node_end, o.end, false),
            (None, _) => (node_end, node_end, true),
        };
        let tokens = (all.into_iter())
            .filter(|t| start <= t.start && t.end <= end)
            .filter(|t| open.is_none_or(|o| t.start != o.start))
            .collect();
        let place = if node.kind == K::PluginDecl {
            Place::Declaration
        } else {
            Place::Expression
        };
        let docs = match place {
            Place::Declaration => docs_above(&text, head.start),
            Place::Expression => String::new(),
        };
        Some(BlockSource {
            docs,
            signature: None,
            module: Arc::from(""),
            heads,
            foreign: false,
            head: head.text(&text).to_owned(),
            head_at: head.start,
            text,
            file,
            start,
            body: body - start,
            end,
            closed,
            place,
            tokens,
            diagnostics: Vec::new(),
            reach: Reach::default(),
            enums: Arc::default(),
            published: Arc::default(),
        })
    }

    /// The declaration `node` as the block a derive reads: its whole text, a
    /// declaration of the module whose holes are its types. A declaration
    /// of another module, `holes` false, has none.
    pub fn declaration(
        text: Arc<str>,
        file: FileId,
        node: &Node,
        heads: Arc<[String]>,
        holes: bool,
    ) -> Self {
        let (start, end) = node.range();
        BlockSource {
            docs: docs_above(&text, start),
            signature: None,
            module: Arc::from(""),
            head: String::new(),
            head_at: start,
            file,
            start,
            end,
            body: end - start,
            closed: true,
            place: Place::Declaration,
            tokens: if holes { node.tokens() } else { Vec::new() },
            diagnostics: Vec::new(),
            reach: Reach::default(),
            enums: Arc::default(),
            published: Arc::default(),
            heads,
            foreign: !holes,
            text,
        }
    }

    /// Whether this is a declaration of another module.
    pub fn foreign(&self) -> bool {
        self.foreign
    }

    /// The block `node` inside the user's code of this one.
    pub fn nested(&self, node: &Node) -> Option<Self> {
        let mut nested = Self::new(
            Arc::clone(&self.text),
            self.file,
            node,
            Arc::clone(&self.heads),
        )?;
        nested.module = Arc::clone(&self.module);
        nested.enums = Arc::clone(&self.enums);
        nested.published = Arc::clone(&self.published);
        Some(nested)
    }

    /// The block, whose module sees the enums `enums`, written as it writes
    /// them, with their base types.
    #[must_use]
    pub fn with_enums(mut self, enums: Arc<BTreeMap<String, String>>) -> Self {
        self.enums = enums;
        self
    }

    /// The block in the module `module` whose tree is `tree`, with the
    /// signature right above it when it is a declaration of the module, or
    /// above the `let` of the module whose whole body it is.
    #[must_use]
    pub fn in_module(mut self, module: &str, tree: &Node) -> Self {
        self.module = Arc::from(module);
        let mut before: Option<&Node> = None;
        let mut within = None;
        for n in tree.nodes() {
            let (start, end) = n.range();
            if start <= self.start && self.start < end {
                within = Some(n);
                break;
            }
            before = Some(n);
        }
        let Some(sig) = before.filter(|n| n.kind == K::Signature) else {
            return self;
        };
        let decl = match self.place {
            Place::Declaration => self.head_at,
            Place::Expression => match within.filter(|n| self.whole_body_of(n, sig)) {
                Some(decl) => decl.range().0,
                None => return self,
            },
        };
        // nothing but blank lines and comments between it and the block
        let gap = &self.text[sig.range().1 as usize..decl as usize];
        if !gap
            .lines()
            .all(|l| l.trim().is_empty() || l.trim_start().starts_with("//"))
        {
            return self;
        }
        if self.place == Place::Declaration {
            self.docs = docs_above(&self.text, sig.range().0);
        }
        self.signature = Some(Above {
            whole: sig.range(),
            tokens: sig.tokens(),
        });
        self
    }

    /// Whether the block is the whole body of `decl`, a `let` that `sig`
    /// declares.
    fn whole_body_of(&self, decl: &Node, sig: &Node) -> bool {
        if decl.kind != K::LetDecl {
            return false;
        }
        let parts = let_parts(decl, &self.text);
        let named =
            parts.name.is_some() && parts.name.as_deref() == signature_name(sig, &self.text);
        let items: Vec<&Node> = parts.body.iter().flat_map(|b| b.nodes()).collect();
        named && matches!(items[..], [only] if only.range().0 == self.head_at)
    }

    /// The range of the signature above the block in the module text.
    pub fn signature(&self) -> Option<(u32, u32)> {
        self.signature.as_ref().map(|s| s.whole)
    }

    /// The signature above the block, as written.
    pub fn signature_text(&self) -> Option<&str> {
        let (start, end) = self.signature.as_ref()?.whole;
        self.text.get(start as usize..end as usize)
    }

    /// Where the block stands.
    pub fn place(&self) -> Place {
        self.place
    }

    pub fn head(&self) -> &str {
        &self.head
    }

    /// Where the frame starts in the module text: spans the plugin gives
    /// count from here.
    pub fn start(&self) -> u32 {
        self.start
    }

    /// Where the frame ends in the module text: at `}`.
    pub fn end(&self) -> u32 {
        self.end
    }

    /// Whether the block has its `}`: one without is only for editors.
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// The text of the module the block is in.
    pub fn text(&self) -> &str {
        &self.text
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
            place: self.place,
            tokens,
            docs: self.docs.clone(),
            signature: self.signature.as_ref().map(|s| {
                let (start, end) = s.whole;
                Signature {
                    text: self.text[start as usize..end as usize].to_owned(),
                    tokens: (s.tokens.iter())
                        .filter(|t| !t.kind.is_trivia())
                        .map(|t| PluginToken {
                            kind: token_kind(t.kind),
                            span: Span {
                                start: t.start - start,
                                end: t.end - start,
                            },
                        })
                        .collect(),
                }
            }),
            module: self.module.to_string(),
        }
    }

    /// The user's code at `hole`, parsed again where it stands; `None` when
    /// it does not parse into exactly that span.
    pub fn hole_tree(&self, hole: Hole) -> Option<Node> {
        if let Some(span) = in_signature(hole) {
            let above = self.signature.as_ref()?;
            let (start, end) = (above.whole.0 + span.start, above.whole.0 + span.end);
            if start >= end || end > above.whole.1 {
                return None;
            }
            let tokens: Vec<Token> = (above.tokens.iter())
                .filter(|t| start <= t.start && t.end <= end)
                .copied()
                .collect();
            let heads: Vec<&str> = self.heads.iter().map(String::as_str).collect();
            let fragment = parse_type_at(&self.text, self.file, &tokens, &heads)?;
            return (fragment.diagnostics.is_empty()
                && fragment.tree.range().0 == start
                && fragment.end == end)
                .then_some(fragment.tree);
        }
        // an expression's hole may be a body, the lines of a block
        let exact = |body: bool| {
            let fragment = self.fragment(hole.kind, body, hole.span.start, hole.span.end)?;
            (fragment.diagnostics.is_empty()
                && fragment.tree.range().0 == self.start + hole.span.start
                && fragment.end == self.start + hole.span.end)
                .then_some(fragment.tree)
        };
        exact(false).or_else(|| (hole.kind == HoleKind::Expr).then(|| exact(true)).flatten())
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
        self.parse_as(kind, false, start, end)
    }

    /// [`BlockSource::parse`], an expression parsed as a body when `body`.
    fn parse_as(
        &mut self,
        kind: HoleKind,
        body: bool,
        start: u32,
        end: u32,
    ) -> Result<Hole, flick_plugin::Diagnostic> {
        let refuse = |message: &str| flick_plugin::Diagnostic::error(message, Span { start, end });
        let what = match kind {
            HoleKind::Expr => "an expression",
            HoleKind::Ty => "a type",
            HoleKind::Pat => "a pattern",
            HoleKind::Decl => "a declaration",
        };
        let Some(fragment) = self.fragment(kind, body, start, end) else {
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

    fn fragment(&self, kind: HoleKind, body: bool, start: u32, end: u32) -> Option<Fragment> {
        if start > end || end > self.end - self.start {
            return None;
        }
        let (from, to) = (self.start + start, self.start + end);
        let first = self.tokens.iter().position(|t| t.start >= from)?;
        let last = self.tokens.iter().rposition(|t| t.end <= to)? + 1;
        let tokens = self.tokens.get(first..last)?;
        let heads: Vec<&str> = self.heads.iter().map(String::as_str).collect();
        let parse = match kind {
            HoleKind::Expr if body => parse_body_at,
            HoleKind::Expr => parse_expr_at,
            HoleKind::Ty => parse_type_at,
            HoleKind::Pat => parse_pat_at,
            HoleKind::Decl => parse_decl_at,
        };
        parse(&self.text, self.file, tokens, &heads)
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

    fn parse_pat(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Pat, start, end)
    }

    fn parse_decl(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Decl, start, end)
    }

    fn parse_body(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse_as(HoleKind::Expr, true, start, end)
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, ReachError> {
        self.reach.read_file(path)
    }

    fn env(&mut self, name: &str) -> Result<Option<String>, ReachError> {
        self.reach.env(name)
    }

    fn fetch(&mut self, request: &Request) -> Result<Response, ReachError> {
        self.reach.fetch(request)
    }

    fn literal_of(&mut self, name: &str) -> Option<String> {
        let answer = self.enums.get(name).cloned();
        self.reach.literal_of(name, answer)
    }

    fn published(&mut self, name: &str) -> Option<Vec<u8>> {
        let answer = self.published.get(name).cloned();
        self.reach.published(name, answer)
    }

    fn published_names(&mut self) -> Vec<String> {
        let names: Vec<String> = self.published.keys().cloned().collect();
        self.reach.published_names(names)
    }
}

pub(crate) fn token_kind(kind: K) -> TokenKind {
    match kind {
        K::Ident => TokenKind::Ident,
        K::Con => TokenKind::Con,
        K::Number | K::BigInt => TokenKind::Number,
        K::String => TokenKind::String,
        k if k.is_keyword() => TokenKind::Keyword,
        _ => TokenKind::Punct,
    }
}

/// The `///` lines right above the line of `offset`, each without `///`
/// and one space.
fn docs_above(text: &str, offset: u32) -> String {
    let line = text[..offset as usize].rfind('\n').map_or(0, |i| i + 1);
    let mut lines: Vec<&str> = (text[..line].lines().rev())
        .map_while(|l| l.trim_start().strip_prefix("///"))
        .map(|d| d.strip_prefix(' ').unwrap_or(d))
        .collect();
    lines.reverse();
    lines.join("\n")
}
