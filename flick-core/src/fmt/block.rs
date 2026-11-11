//! Statements, blocks in braces, and the values after `=`, `->` and `<-`.

use std::mem;

use flick_pretty::{NextBreakFitsMode, docvec};

use super::expr::Part;
use super::printer::{Doc, INDENT, Printer, node_position, nodes, position, token};
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;

/// What follows `=`, `->` or `<-`: the block of a body, or one expression.
#[derive(Clone, Copy)]
pub(super) enum Value<'a> {
    Block(&'a Node),
    Expr(&'a Node),
}

/// Values that stay after `=` or `->` and break inside instead, so that
/// `let xs = [` or `fun x -> task {` keeps the opener on the line.
pub(super) fn hugs(n: &Node) -> bool {
    matches!(
        n.kind,
        K::RecordExpr
            | K::RecordUpdate
            | K::RecordType
            | K::ListExpr
            | K::TupleExpr
            | K::TaskExpr
            | K::ScopeExpr
            | K::ProvideExpr
            | K::DoExpr
            | K::FunExpr
    ) || matches!(n.kind, K::ForExpr | K::PluginExpr) && n.has_token(K::LBrace)
}

fn is_expression(n: &Node) -> bool {
    !matches!(n.kind, K::LetDecl | K::Signature)
}

impl<'a> Printer<'a> {
    /// `head`, which ends in `=`, `->` or `<-`, and its value: on the same
    /// line when it fits, else on the next lines one level in. A hugging
    /// value stays on the line and breaks inside; several statements always
    /// go on their own lines. `end` bounds the comment that may end the
    /// value's last line; `closing` starts with the `)` around a function,
    /// which takes its own line when the body breaks.
    pub fn body(&mut self, head: Doc<'a>, value: Value<'a>, end: u32, closing: &[Part]) -> Doc<'a> {
        // a body is laid out by its lines again, whatever is around it
        let outer = mem::replace(&mut self.in_bracket, false);
        let doc = self.body_of(head, value, end, closing);
        self.in_bracket = outer;
        doc
    }

    fn body_of(&mut self, head: Doc<'a>, value: Value<'a>, end: u32, closing: &[Part]) -> Doc<'a> {
        let a = self.a;
        let inline = mem::take(&mut self.inline_body);
        let statements = match value {
            Value::Block(block) => nodes(block),
            Value::Expr(expr) => vec![expr],
        };
        let end = match closing.first() {
            Some(Part::Token(t)) => t.start,
            _ => end,
        };
        let first = statements.first().map_or(end, |n| n.range().0);
        let after = self.comments.trailing(first, None);
        let single = match statements.as_slice() {
            [one] if is_expression(one) && after.is_none() && !self.comments.any_before(first) => {
                Some(*one)
            }
            _ => None,
        };
        let Some(value) = single else {
            let items = self.items(&statements, end, Some(';'), &mut |p, n| p.statement(n));
            let after = after.map_or(a.empty(), |c| docvec![a, " ", c]);
            let lines = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
            let close = self.closing(closing, a.line());
            return docvec![a, head, after, lines, close].force_break(a);
        };
        let doc = self.statement(value);
        let trailing = self.comments.trailing(end, None);
        if let Some(comment) = trailing {
            let doc = docvec![a, doc, " ", comment];
            if hugs(value) && closing.is_empty() {
                return docvec![a, head, " ", doc].force_break(a);
            }
            let lines = docvec![a, a.line(), doc].nest(a, INDENT);
            let close = self.closing(closing, a.line());
            return docvec![a, head, lines, close].force_break(a);
        }
        if hugs(value) || inline {
            let close = self.closing(closing, a.empty());
            return docvec![a, head, " ", doc, close];
        }
        let close = self.closing(closing, a.break_("", ""));
        let value = docvec![a, a.break_("", " "), doc.group(a)].nest(a, INDENT);
        // after `=` a call that does not fit breaks onto its own line rather
        // than hugging its last argument (Gleam does the same); a function
        // body may start on the next line of a hugging function
        let value = if closing.is_empty() {
            value.next_break_fits(a, NextBreakFitsMode::Disabled)
        } else {
            value
        };
        docvec![a, head, value, close].group(a)
    }

    /// The `)` after a function, preceded by `before`.
    fn closing(&mut self, closing: &[Part], before: Doc<'a>) -> Doc<'a> {
        if closing.is_empty() {
            return self.a.empty();
        }
        let closing = self.suffix(closing);
        docvec![self.a, before, closing]
    }

    /// A statement, which starts its line: a pipeline there lines its
    /// steps up under its first operand.
    pub fn statement(&mut self, n: &'a Node) -> Doc<'a> {
        if n.kind == K::BinExpr && token(n, K::PipeGt).is_some() {
            self.binary(n, true, &[])
        } else {
            self.node(n)
        }
    }

    /// The statements of a block, one per line.
    pub fn block(&mut self, n: &'a Node) -> Doc<'a> {
        let end = n.range().1;
        let outer = mem::replace(&mut self.in_bracket, false);
        let items = self.items(&nodes(n), end, Some(';'), &mut |p, n| p.statement(n));
        self.in_bracket = outer;
        self.join_lines(&items)
    }

    /// `let` and `server fn` declarations.
    pub fn let_(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let (Some(eq), Some(block)) = (position(n, K::Eq), node_position(n, K::Block)) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..=eq]);
        let Element::Node(body) = &n.children[block] else {
            unreachable!("a node position")
        };
        let doc = self.body(head, Value::Block(body), body.range().1, &[]);
        // `as "message"` of `let assert` ends the body's last line
        let rest = self.elements(n, &n.children[block + 1..]);
        if rest.is_empty() {
            doc
        } else {
            docvec![a, doc, " ", rest]
        }
    }

    /// `pattern <- value` in `do` and `for`.
    pub fn bind(&mut self, n: &'a Node) -> Doc<'a> {
        let (Some(arrow), Some(value)) = (position(n, K::LArrow), nodes(n).get(1).copied()) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..=arrow]);
        self.body(head, Value::Expr(value), n.range().1, &[])
    }

    /// `p <- a and q <- b`: `and` starts a deeper line when it breaks.
    pub fn bind_group(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let mut docs = Vec::new();
        for child in &n.children {
            match child {
                Element::Node(bind) if docs.is_empty() => docs.push(self.node(bind)),
                Element::Node(bind) => {
                    let bind = self.node(bind);
                    docs.push(docvec![a, " ", bind]);
                }
                Element::Token(t) if t.kind == K::AndKw => {
                    let and = self.tok(*t);
                    docs.push(docvec![a, a.break_("", " "), and].nest(a, INDENT));
                }
                Element::Token(_) => {}
            }
        }
        a.concat(docs).group(a)
    }

    /// A construct around a block in braces: `task { … }`, `scope s { … }`,
    /// `provide layer { … }`, `rec { … }`, `sql table … { … }`, ….
    pub fn braced(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let a = self.a;
        let Some(open) = position(n, K::LBrace) else {
            let doc = self.generic(n);
            let suffix = self.suffix(suffix);
            return docvec![a, doc, suffix];
        };
        let head = self
            .elements(n, &n.children[..open])
            .next_break_fits(a, NextBreakFitsMode::Disabled);
        let doc = self.braces(n, open, suffix);
        docvec![a, head, " ", doc]
    }

    /// The block in braces starting at the child `open` of `n`.
    pub fn braces(&mut self, n: &'a Node, open: usize, suffix: &[Part]) -> Doc<'a> {
        let Element::Token(lbrace) = n.children[open] else {
            unreachable!("a token position")
        };
        let inner = n.children[open + 1..].iter().find_map(|c| match c {
            Element::Node(c) if matches!(c.kind, K::Block | K::Members) => Some(c),
            _ => None,
        });
        let rbrace = n.children[open + 1..].iter().find_map(|c| match c {
            Element::Token(t) if t.kind == K::RBrace => Some(*t),
            _ => None,
        });
        let Some(rbrace) = rbrace else {
            return self.generic(n);
        };
        let statements = inner.map(nodes).unwrap_or_default();
        let members = inner.is_some_and(|i| i.kind == K::Members);
        let always = n.kind == K::RecGroup;
        self.braces_of((lbrace, rbrace), &statements, (members, always), suffix)
    }

    /// `{ statements }` between the braces `around`: on one line when it is
    /// one expression that fits, else a line for each. `members` are one
    /// level further in, and `always` breaks whatever fits.
    pub fn braces_of(
        &mut self,
        (lbrace, rbrace): (Token, Token),
        statements: &[&'a Node],
        (members, always): (bool, bool),
        suffix: &[Part],
    ) -> Doc<'a> {
        let a = self.a;
        let lbrace = self.tok(lbrace);
        let first = statements.first().map_or(rbrace.start, |s| s.range().0);
        let after = self.comments.trailing(first, None);
        self.column += if members { INDENT } else { 0 };
        let outer = mem::replace(&mut self.in_bracket, false);
        let items = self.items(statements, rbrace.start, Some(';'), &mut |p, s| {
            p.statement(s)
        });
        self.in_bracket = outer;
        self.column -= if members { INDENT } else { 0 };
        let dangling = self.comments.leading(rbrace.start);
        let close = self.tok(rbrace);
        let suffix = self.suffix(suffix);
        let close = docvec![a, close, suffix];
        let plain = after.is_none()
            && dangling.is_empty()
            && items
                .iter()
                .all(|i| i.comments.is_empty() && i.trailing.is_none());
        match items.as_slice() {
            [] if plain => return docvec![a, lbrace, close],
            [one] if plain && !always && is_expression(one.node) => {
                let inner = docvec![a, a.break_("", " "), one.doc].nest(a, INDENT);
                return docvec![a, lbrace, inner, a.break_("", " "), close].group(a);
            }
            _ => {}
        }
        let after = after.map_or(a.empty(), |c| docvec![a, " ", c]);
        let mut inner = vec![a.line(), self.join_lines(&items)];
        if !dangling.is_empty() {
            let blank = dangling.first().is_some_and(|c| c.blank) && !items.is_empty();
            let separator = if items.is_empty() {
                0
            } else {
                1 + usize::from(blank)
            };
            inner.push(a.lines(separator));
            inner.push(self.comment_block(&dangling));
        }
        let inner = a.concat(inner).nest(a, INDENT);
        docvec![a, lbrace, after, inner, a.line(), close].force_break(a)
    }

    /// `for { … } yield e`, `for x <- xs yield e`, and `do` instead of `yield`.
    pub fn for_(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let children = &n.children;
        let mut docs = Vec::new();
        let mut at = 0;
        while at < children.len() {
            match &children[at] {
                Element::Token(token) if token.kind == K::LBrace => {
                    docs.push(a.text(" "));
                    docs.push(self.braces(n, at, &[]));
                    at = (at + 1..children.len())
                        .find(|&next| {
                            matches!(&children[next], Element::Token(close) if close.kind == K::RBrace)
                        })
                        .unwrap_or(children.len());
                }
                Element::Token(token) if token.kind == K::DoKw => {
                    let rest = &children[at + 1..];
                    let braced = rest
                        .iter()
                        .any(|e| matches!(e, Element::Token(open) if open.kind == K::LBrace));
                    let keyword = self.tok(*token);
                    let body = rest.iter().find_map(|e| match e {
                        Element::Node(body) if !braced => Some(body),
                        _ => None,
                    });
                    let Some(body) = body else {
                        docs.push(docvec![a, " ", keyword]);
                        at += 1;
                        continue;
                    };
                    let head = docvec![a, a.concat(docs.drain(..)), " ", keyword];
                    return self.body(head, Value::Block(body), body.range().1, &[]);
                }
                Element::Token(token) if token.kind.is_trivia() => {}
                Element::Token(token) => {
                    let space = if docs.is_empty() {
                        a.empty()
                    } else {
                        a.text(" ")
                    };
                    let token = self.tok(*token);
                    docs.push(docvec![a, space, token]);
                }
                Element::Node(clauses) if clauses.kind == K::Block => {
                    let clauses = self.elements(clauses, &clauses.children);
                    docs.push(docvec![a, " ", clauses]);
                }
                Element::Node(child) => {
                    let child = self.node(child);
                    docs.push(docvec![a, " ", child]);
                }
            }
            at += 1;
        }
        a.concat(docs)
    }
}
