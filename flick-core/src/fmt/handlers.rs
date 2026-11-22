//! Layout of `match`, `try … with`, and postfix `catch`.

use std::{mem, ptr};

use flick_pretty::docvec;

use super::binary::operator;
use super::block::Value;
use super::expr::Part;
use super::printer::{Doc, INDENT, Printer, position, token};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

impl<'a> Printer<'a> {
    /// `match e with` or `try e with`, then one arm per line.
    pub fn match_(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let inline = mem::take(&mut self.tail_inline);
        let Some(with) = position(n, K::WithKw) else {
            return self.generic(n);
        };
        let head = if n.kind == K::TryExpr {
            self.try_head(n, with)
        } else {
            let (Some(keyword), Some(subject)) = (token(n, K::MatchKw), n.nodes().next()) else {
                return self.generic(n);
            };
            let keyword = self.tok(keyword);
            let subject = self.node(subject);
            let subject = docvec![a, a.break_("", " "), subject].nest(a, INDENT);
            let Element::Token(with) = n.children[with] else {
                unreachable!("a token position")
            };
            let with = self.tok(with);
            docvec![a, keyword, subject, a.break_("", " "), with].group(a)
        };
        let arms: Vec<&Node> = n.nodes().filter(|c| c.kind == K::MatchArm).collect();
        let last = arms.last().copied();
        let items = self.items(&arms, n.range().1, None, &mut |p, arm| {
            p.inline_body = inline && last.is_some_and(|l| ptr::eq(l, arm));
            p.arm(arm)
        });
        docvec![a, head, a.line(), self.join_lines(&items)].force_break(a)
    }

    /// `try body with`: one line when the body is one expression that fits.
    fn try_head(&mut self, n: &'a Node, with: usize) -> Doc<'a> {
        let a = self.a;
        let (Some(keyword), Some(block)) = (token(n, K::TryKw), n.nodes().next()) else {
            return self.generic(n);
        };
        let keyword = self.tok(keyword);
        let Element::Token(with) = n.children[with] else {
            unreachable!("a token position")
        };
        let (branch, broken) = self.branch(block, with.start, false);
        let with = self.tok(with);
        let separator = if broken { a.line() } else { a.break_("", " ") };
        let doc = docvec![a, keyword, branch, separator, with];
        if broken {
            doc.force_break(a)
        } else {
            doc.group(a)
        }
    }

    /// `expr catch`, then one arm per line. Mixed pipeline expressions on the
    /// left are parenthesized explicitly so the grouping stays unmistakable.
    pub fn catch_(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let inline = mem::take(&mut self.tail_inline);
        let (Some(subject), Some(catch_kw)) = (n.nodes().next(), token(n, K::CatchKw)) else {
            return self.generic(n);
        };
        let needs_parens = subject.kind == K::BinExpr
            && operator(subject).is_some_and(|op| matches!(op.kind, K::PipeGt | K::LtPipe));
        let mut lhs = if needs_parens {
            let inner = self.with_suffix(subject, &[Part::Text(")")]);
            docvec![a, "(", inner]
        } else {
            self.node(subject)
        };
        let mut forced = matches!(
            subject.kind,
            K::IfExpr | K::MatchExpr | K::TryExpr | K::ForExpr | K::WhileExpr
        );
        if let Some(c) = self.comments.trailing(catch_kw.start, None) {
            lhs = docvec![a, lhs, " ", c];
            forced = true;
        }
        let leading = self.comments.leading(catch_kw.start);
        let comments = if leading.is_empty() {
            a.empty()
        } else {
            forced = true;
            self.comment_lines(&leading)
        };
        let catch_doc = self.tok(catch_kw);
        let sep = if forced { a.line() } else { a.break_("", " ") };
        let head = docvec![a, lhs, sep, comments, catch_doc];
        let head = if forced {
            head.force_break(a)
        } else {
            head.group(a)
        };
        let arms: Vec<&Node> = n.nodes().filter(|c| c.kind == K::MatchArm).collect();
        let last = arms.last().copied();
        let items = self.items(&arms, n.range().1, None, &mut |p, arm| {
            p.inline_body = inline && last.is_some_and(|l| ptr::eq(l, arm));
            p.arm(arm)
        });
        docvec![a, head, a.line(), self.join_lines(&items)].force_break(a)
    }

    /// `| pattern when guard -> body`.
    fn arm(&mut self, n: &'a Node) -> Doc<'a> {
        let (Some(arrow), Some(block)) = (position(n, K::Arrow), n.nodes().last()) else {
            return self.generic(n);
        };
        if block.kind != K::Block {
            return self.generic(n);
        }
        let head = self.elements(n, &n.children[..=arrow]);
        self.body(head, Value::Block(block), n.range().1, &[])
    }
}
