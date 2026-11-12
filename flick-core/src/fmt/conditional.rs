//! `if c then a else b`: on one line, each branch on its own lines, or a
//! branch in braces, which hold its statements.

use std::mem;

use flick_pretty::docvec;

use super::printer::{Doc, INDENT, Printer, nodes, position, token};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

impl<'a> Printer<'a> {
    /// `if c then a else b` on one line, or each branch on its own lines.
    pub fn if_(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let inline = mem::take(&mut self.tail_inline);
        let mut docs = Vec::new();
        let mut forced = false;
        let mut current = n;
        loop {
            let parts = nodes(current);
            // the condition is the first child node, the `then` branch the second
            let (Some(then), Some(&block)) = (position(current, K::ThenKw), parts.get(1)) else {
                return self.generic(n);
            };
            let head = self.elements(current, &current.children[..=then]);
            let otherwise = token(current, K::ElseKw);
            let end = otherwise.map_or(current.range().1, |t| t.start);
            // braces hold the statements of a branch, as after `do`
            let braced = brace_after(current, then);
            let (branch, broken) = match braced {
                Some(open) => (docvec![a, " ", self.braces(current, open, &[])], false),
                None => self.branch(block, end, inline && otherwise.is_none()),
            };
            forced |= broken;
            docs.push(head);
            docs.push(branch);
            let Some(otherwise) = otherwise else {
                break;
            };
            let comments = self.comments.leading(otherwise.start);
            if comments.is_empty() && braced.is_some() {
                // `} else` on the line of the closing brace
                docs.push(a.text(" "));
            } else if comments.is_empty() {
                docs.push(a.break_("", " "));
            } else {
                forced = true;
                docs.push(a.line());
                docs.push(self.comment_lines(&comments));
            }
            docs.push(self.tok(otherwise));
            match parts.get(2) {
                Some(next) if next.kind == K::IfExpr => {
                    docs.push(a.text(" "));
                    current = next;
                }
                Some(block) => {
                    let braced =
                        position(current, K::ElseKw).and_then(|at| brace_after(current, at));
                    if let Some(open) = braced {
                        docs.push(docvec![a, " ", self.braces(current, open, &[])]);
                    } else {
                        let (branch, broken) = self.branch(block, current.range().1, inline);
                        forced |= broken;
                        docs.push(branch);
                    }
                    break;
                }
                None => return self.generic(n),
            }
        }
        let doc = a.concat(docs);
        if forced {
            doc.force_break(a).group(a)
        } else {
            doc.group(a)
        }
    }

    /// A branch of an `if`, one level in; whether it must break.
    /// `inline` keeps it on the line of its `then` or `else`.
    pub(super) fn branch(&mut self, block: &'a Node, end: u32, inline: bool) -> (Doc<'a>, bool) {
        let a = self.a;
        let statements = nodes(block);
        let first = statements.first().map_or(end, |s| s.range().0);
        let after = self.comments.trailing(first, None);
        // `else` and an `if` on the next line differ from `else if`
        let nested_if = |one: &Node| one.kind == K::IfExpr && !inline;
        if let ([one], None) = (statements.as_slice(), after)
            && one.kind != K::LetDecl
            && one.kind != K::Signature
            && !nested_if(one)
            && !self.comments.any_before(first)
        {
            // what ends the line of an inline branch ends the branch's value too
            self.tail_inline =
                inline && matches!(one.kind, K::IfExpr | K::MatchExpr | K::TryExpr | K::FunExpr);
            let mut doc = self.statement(one);
            self.tail_inline = false;
            let trailing = self.comments.trailing(end, None);
            if let Some(c) = trailing {
                doc = docvec![a, doc, " ", c];
            }
            if inline {
                // the lines of the value go deeper than its `else`
                return (docvec![a, " ", doc].nest(a, INDENT), trailing.is_some());
            }
            let doc = docvec![a, a.break_("", " "), doc].nest(a, INDENT);
            return (doc, trailing.is_some());
        }
        let items = self.items(&statements, end, Some(';'), &mut |p, s| p.statement(s));
        let after = after.map_or(a.empty(), |c| docvec![a, " ", c]);
        let lines = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
        (docvec![a, after, lines], true)
    }
}

/// The child of `n` that is the `{` right after its child `at`, if braces
/// follow it.
fn brace_after(n: &Node, at: usize) -> Option<usize> {
    n.children
        .iter()
        .enumerate()
        .skip(at + 1)
        .find_map(|(i, child)| match child {
            Element::Token(t) if t.kind.is_trivia() => None,
            Element::Token(t) => Some((t.kind == K::LBrace).then_some(i)),
            Element::Node(_) => Some(None),
        })?
}
