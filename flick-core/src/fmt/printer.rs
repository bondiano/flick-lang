//! The state of a layout, the dispatch on node kinds, and the generic layout
//! of the nodes no rule covers: their tokens on one line, spaced by the
//! table in [`space`].

use flick_pretty::{Document, DocumentArena, docvec};

use super::comments::{Comment, Comments};
use super::plugins::Layouts;
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;

pub(super) type Doc<'a> = Document<'a>;

/// Columns per level of indentation.
pub(super) const INDENT: usize = 2;

pub(super) struct Printer<'a> {
    pub a: &'a DocumentArena<'a>,
    pub text: &'a str,
    pub comments: Comments<'a>,
    /// What the plugins of the file's blocks lay them out as.
    pub layouts: &'a Layouts,
    /// The column the declarations being laid out start at: 0 at the top
    /// level, one level in for the members of a class or a `rec` group.
    pub column: usize,
    /// The node about to be laid out is followed by a `,` on its line: the
    /// parser takes no `,` after a block on lines of its own, so its last
    /// branch, arm or function body stays on its line.
    pub tail_inline: bool,
    /// The next [`body`](Printer::body) stays on the line of its `->`.
    pub inline_body: bool,
}

/// An item of a block, a list or a record, with the comments around it.
pub(super) struct Item<'a> {
    /// Own-line comments before it.
    pub comments: Vec<Comment<'a>>,
    /// A blank line before it, or before its comments, in the source.
    pub blank: bool,
    /// A blank line between its comments and it.
    pub gap: bool,
    pub doc: Doc<'a>,
    /// The comment ending its line.
    pub trailing: Option<&'a str>,
    /// Its separator from the next item: `(broken, flat)`.
    pub sep: (&'static str, &'static str),
    pub node: &'a Node,
}

impl<'a> Printer<'a> {
    pub fn new(
        a: &'a DocumentArena<'a>,
        text: &'a str,
        comments: Comments<'a>,
        layouts: &'a Layouts,
    ) -> Self {
        Printer {
            a,
            text,
            comments,
            layouts,
            column: 0,
            tail_inline: false,
            inline_body: false,
        }
    }

    pub fn str(&self, text: &str) -> Doc<'a> {
        self.a.text(self.a.alloc_str(text))
    }

    /// A token of the source, with the block comments written just before
    /// it on its line.
    pub fn tok(&mut self, t: Token) -> Doc<'a> {
        let inline = self.comments.inline(t.start);
        self.comments.cursor = t.end;
        let text = self.a.text(t.text(self.text));
        if inline.is_empty() {
            return text;
        }
        let comments = inline
            .iter()
            .flat_map(|c| [self.a.text(c), self.a.text(" ")]);
        self.a.concat(comments.chain([text]))
    }

    /// The comments on their own lines, each followed by a line break; a
    /// blank line before one is kept unless it is the first.
    pub fn comment_lines(&self, comments: &[Comment<'a>]) -> Doc<'a> {
        docvec![self.a, self.comment_block(comments), self.a.line()].force_break(self.a)
    }

    /// The comments on their own lines, a blank line kept between two.
    pub fn comment_block(&self, comments: &[Comment<'a>]) -> Doc<'a> {
        let a = self.a;
        let lines = comments.iter().enumerate().flat_map(|(i, c)| {
            let separator = if i == 0 { 0 } else { 1 + usize::from(c.blank) };
            [a.lines(separator), a.text(c.text)]
        });
        a.concat(lines).force_break(a)
    }

    /// The own-line comments of an item, and the blank line after them.
    pub fn leading(&self, item: &Item<'a>) -> Doc<'a> {
        if item.comments.is_empty() {
            return self.a.empty();
        }
        let gap = self.a.lines(usize::from(item.gap));
        docvec![self.a, self.comment_lines(&item.comments), gap]
    }

    /// The layout of any node.
    pub fn node(&mut self, n: &'a Node) -> Doc<'a> {
        match n.kind {
            K::AppExpr | K::AppType => self.app(n, &[]),
            K::BinExpr => self.binary(n, false, &[]),
            K::ParenExpr => self.paren(n, &[]),
            K::FunExpr => self.fun(n, &[]),
            K::IfExpr => self.if_(n),
            K::MatchExpr | K::TryExpr => self.match_(n),
            K::LetDecl => self.let_(n),
            K::BindStmt => self.bind(n),
            K::BindGroup => self.bind_group(n),
            K::Block => self.block(n),
            K::TaskExpr
            | K::UiExpr
            | K::ScopeExpr
            | K::ProvideExpr
            | K::DoExpr
            | K::SqlExpr
            | K::RecGroup => self.braced(n, &[]),
            K::ForExpr => self.for_(n),
            K::ListExpr | K::TupleExpr | K::ListPat | K::TuplePat | K::TupleType | K::Exposing => {
                self.bracket(n, &[])
            }
            K::RecordExpr | K::RecordUpdate | K::RecordType | K::RecordPat => self.record(n, &[]),
            K::RecordField => self.field(n),
            K::ArrowType => self.arrow(n),
            K::Signature => self.signature(n),
            K::TypeDecl | K::KindDecl | K::ErrorDecl => self.type_decl(n),
            K::ClassDecl | K::InstanceDecl | K::ServiceDecl => self.class(n),
            K::ForeignDecl => self.foreign(n),
            // the arguments of a derive are its plugin's words
            K::DerivingArgs => self.as_written(n),
            K::PluginExpr | K::PluginDecl => self.plugin_block(n),
            _ => self.generic(n),
        }
    }

    /// `n` as written, without whitespace at line ends: SQL keeps its own
    /// layout.
    fn as_written(&mut self, n: &'a Node) -> Doc<'a> {
        let (start, end) = n.range();
        // its comments are in the text
        self.comments.skip(end);
        let text = &self.text[start as usize..end as usize];
        let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
        self.a.text(self.a.alloc_str(&lines.join("\n")))
    }

    /// The children of `n` on one line.
    pub fn generic(&mut self, n: &'a Node) -> Doc<'a> {
        self.elements(n, &n.children)
    }

    /// Children of `n`, spaced by [`space`].
    pub fn elements(&mut self, n: &'a Node, children: &'a [Element]) -> Doc<'a> {
        let mut docs = Vec::with_capacity(children.len() * 2);
        let mut previous = None;
        for child in children {
            let Some((first, last)) = edges(n, child) else {
                continue;
            };
            if previous.is_some_and(|p| space(p, first)) {
                docs.push(self.a.text(" "));
            }
            docs.push(match child {
                Element::Node(c) => self.node(c),
                Element::Token(t) => self.tok(*t),
            });
            previous = Some(last);
        }
        self.a.concat(docs)
    }

    /// The items of a block or a list with the comments around them.
    pub fn items(
        &mut self,
        nodes: &[&'a Node],
        end: u32,
        sep: Option<char>,
        build: &mut dyn FnMut(&mut Self, &'a Node) -> Doc<'a>,
    ) -> Vec<Item<'a>> {
        let mut items = Vec::with_capacity(nodes.len());
        for (i, node) in nodes.iter().enumerate() {
            let start = node.range().0;
            let comments = self.comments.leading(start);
            let first = comments.first().map_or(start, |c| c.start);
            let blank = i > 0 && self.comments.blank_before(first);
            let gap = !comments.is_empty() && self.comments.blank_before(start);
            let doc = build(self, node);
            let next = nodes.get(i + 1).map_or(end, |n| n.range().0);
            let trailing = self.comments.trailing(next, sep);
            items.push(Item {
                comments,
                blank,
                gap,
                doc,
                trailing,
                sep: ("", ""),
                node,
            });
        }
        items
    }

    /// Items one per line, with their comments and the blank lines between
    /// them (at most one). The result holds line breaks and so is broken.
    pub fn join_lines(&self, items: &[Item<'a>]) -> Doc<'a> {
        let a = self.a;
        let mut docs = Vec::with_capacity(items.len() * 4);
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                docs.push(a.lines(if item.blank { 2 } else { 1 }));
            }
            docs.push(self.leading(item));
            docs.push(item.doc);
            if let Some(comment) = item.trailing {
                docs.push(docvec![a, " ", comment]);
            }
        }
        a.concat(docs).force_break(a)
    }
}

/// The first and last tokens of a child of `parent` with their parents.
fn edges(parent: &Node, child: &Element) -> Option<((K, K), (K, K))> {
    match child {
        Element::Token(t) if t.kind.is_trivia() => None,
        Element::Token(t) => Some(((t.kind, parent.kind), (t.kind, parent.kind))),
        Element::Node(n) => Some((edge(n, false)?, edge(n, true)?)),
    }
}

fn edge(n: &Node, last: bool) -> Option<(K, K)> {
    let pick = |c: &Element| match c {
        Element::Token(t) if t.kind.is_trivia() => None,
        Element::Token(t) => Some((t.kind, n.kind)),
        Element::Node(c) => edge(c, last),
    };
    if last {
        n.children.iter().rev().find_map(pick)
    } else {
        n.children.iter().find_map(pick)
    }
}

/// Whether two tokens on one line are separated by a space; each comes
/// with the kind of the node it belongs to.
fn space((a, a_parent): (K, K), (b, b_parent): (K, K)) -> bool {
    let tight_braces = |parent: K| matches!(parent, K::EffectAnnot | K::RowType);
    match (a, b) {
        (K::LParen | K::LBracket | K::Caret | K::Dollar | K::At | K::DotDotDot, _)
        | (_, K::RParen | K::RBracket | K::Comma | K::Semi | K::Caret | K::Dot | K::Question)
        | (K::LBrace, K::RBrace) => false,
        (K::Dot, _) => a_parent == K::ForallType,
        (K::DotDot | K::DotDotLt, _) if a_parent == K::RangeExpr => false,
        (_, K::DotDot | K::DotDotLt) if b_parent == K::RangeExpr => false,
        (K::Arrow, K::LBrace) if b_parent == K::EffectAnnot => false,
        (K::LBrace, _) => !tight_braces(a_parent),
        (_, K::RBrace) => !tight_braces(b_parent),
        (K::Minus, _) => !matches!(a_parent, K::PrefixExpr | K::LitPat),
        (K::Con, K::LParen) => b_parent != K::ExposeItem,
        _ => true,
    }
}

/// The child nodes of `n`.
pub(super) fn nodes(n: &Node) -> Vec<&Node> {
    n.nodes().collect()
}

/// The first child token of kind `kind`.
pub(super) fn token(n: &Node, kind: K) -> Option<Token> {
    n.child_tokens().find(|t| t.kind == kind)
}

/// The index of the first child element that is a token of kind `kind`.
pub(super) fn position(n: &Node, kind: K) -> Option<usize> {
    n.children
        .iter()
        .position(|c| matches!(c, Element::Token(t) if t.kind == kind))
}

/// The index of the first child element that is a node of kind `kind`.
pub(super) fn node_position(n: &Node, kind: K) -> Option<usize> {
    n.children
        .iter()
        .position(|c| matches!(c, Element::Node(c) if c.kind == kind))
}
