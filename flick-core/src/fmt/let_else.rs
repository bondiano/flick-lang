//! Formatting for `let pattern = initializer else fallback`.

use flick_pretty::{NextBreakFitsMode, docvec};

use super::block::{Value, hugs, is_expression};
use super::printer::{Doc, INDENT, Printer, nodes, position};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

/// Whether `n` contains an unparenthesized `if`, `match`, `try` or `fun`
/// whose own branches/arms could break to the enclosing line's column.
fn has_layout_branch(n: &Node) -> bool {
    match n.kind {
        K::IfExpr | K::MatchExpr | K::TryExpr | K::FunExpr => true,
        K::ParenExpr
        | K::TupleExpr
        | K::ListExpr
        | K::RecordExpr
        | K::RecordUpdate
        | K::TaskExpr
        | K::LazyExpr
        | K::ScopeExpr
        | K::ProvideExpr
        | K::DoExpr
        | K::LoopExpr
        | K::PluginExpr => false,
        _ => n.nodes().any(has_layout_branch),
    }
}

impl<'a> Printer<'a> {
    /// `let pattern = initializer else fallback`: keeps simple single-line
    /// forms on one line, and breaks initializers or fallbacks with layout
    /// branches (`if`, `match`, `try`, `fun`) or multiple statements into
    /// indented blocks so nearest-`else` ownership and block boundaries are
    /// preserved on round-trip.
    pub(super) fn let_else(&mut self, n: &'a Node) -> Doc<'a> {
        let a = self.a;
        let (Some(eq), Some(else_pos)) = (position(n, K::Eq), position(n, K::ElseKw)) else {
            return self.generic(n);
        };
        let init = n.children[eq + 1..else_pos].iter().find_map(|c| match c {
            Element::Node(c) if c.kind == K::Block => Some(c),
            _ => None,
        });
        let fallback = n.children[else_pos + 1..].iter().find_map(|c| match c {
            Element::Node(c) if c.kind == K::Block => Some(c),
            _ => None,
        });
        let Element::Token(else_kw) = n.children[else_pos] else {
            unreachable!("a token position")
        };
        let (Some(init), Some(fallback)) = (init, fallback) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..=eq]);
        let outer = self.lines_again();
        let init_stmts = nodes(init);
        let init_first = init_stmts.first().map_or(else_kw.start, |s| s.range().0);
        let after_eq = self.comments.trailing(init_first, None);
        let init_single = match init_stmts.as_slice() {
            [one]
                if is_expression(one)
                    && !has_layout_branch(one)
                    && after_eq.is_none()
                    && !self.comments.any_before(init_first) =>
            {
                Some(*one)
            }
            _ => None,
        };
        let else_head = if let Some(expr) = init_single {
            let expr_doc = self.statement(expr);
            let trailing = self.comments.trailing(else_kw.start, None);
            let before_else = self.comments.leading(else_kw.start);
            let else_tok = self.tok(else_kw);
            if trailing.is_none() && before_else.is_empty() {
                if hugs(expr) {
                    docvec![a, head, " ", expr_doc, " ", else_tok]
                } else {
                    let value = docvec![a, a.break_("", " "), expr_doc.group(a)]
                        .nest(a, INDENT)
                        .next_break_fits(a, NextBreakFitsMode::Disabled);
                    docvec![a, head, value, a.break_("", " "), else_tok].group(a)
                }
            } else {
                let expr_doc = match trailing {
                    Some(c) => docvec![a, expr_doc, " ", c],
                    None => expr_doc,
                };
                let before_else = if before_else.is_empty() {
                    a.empty()
                } else {
                    self.comment_lines(&before_else)
                };
                let lines = docvec![a, a.line(), expr_doc].nest(a, INDENT);
                docvec![a, head, lines, a.line(), before_else, else_tok].force_break(a)
            }
        } else {
            let items = self.items(&init_stmts, else_kw.start, Some(';'), &mut |p, s| {
                p.statement(s)
            });
            let after_eq = after_eq.map_or(a.empty(), |c| docvec![a, " ", c]);
            let before_else = self.comments.leading(else_kw.start);
            let before_else = if before_else.is_empty() {
                a.empty()
            } else {
                self.comment_lines(&before_else)
            };
            let else_tok = self.tok(else_kw);
            let lines = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
            docvec![a, head, after_eq, lines, a.line(), before_else, else_tok].force_break(a)
        };
        let fallback_stmts = nodes(fallback);
        let fallback_first = fallback_stmts
            .first()
            .map_or(fallback.range().1, |s| s.range().0);
        let fallback_layout = matches!(fallback_stmts.as_slice(), [one] if has_layout_branch(one));
        let doc = if fallback_layout {
            let after_else = self.comments.trailing(fallback_first, None);
            let items = self.items(
                &fallback_stmts,
                fallback.range().1,
                Some(';'),
                &mut |p, s| p.statement(s),
            );
            let after_else = after_else.map_or(a.empty(), |c| docvec![a, " ", c]);
            let lines = docvec![a, a.line(), self.join_lines(&items)].nest(a, INDENT);
            docvec![a, else_head, after_else, lines].force_break(a)
        } else {
            self.body_of(else_head, Value::Block(fallback), fallback.range().1, &[])
        };
        self.calls = outer;
        doc
    }
}
