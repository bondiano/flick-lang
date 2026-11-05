//! `todo`, `echo` and `let assert`: calls of `Flick.Debug`, whose
//! `CallSite` the checker gave at the keyword.

use super::Lower;
use super::expr::with_dicts;
use crate::ir::{Arm, Expr, ExprKind, Lit, Pat, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::lang::Lang;
use crate::types::{Range, let_parts};

impl Lower<'_> {
    /// `todo ["message"]` fails with its place and message.
    pub(super) fn todo(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let message = node
            .child_tokens()
            .find(|t| t.kind == K::String)
            .map_or_else(
                || "`todo` reached".to_owned(),
                |t| literal::string_value(t.text(self.text)),
            );
        let todo = self.debug(Lang::TodoAt, node.range(), span);
        apply(todo, string(message, span), span)
    }

    /// `echo e`, or a bare `echo` as a function.
    pub(super) fn echo(&mut self, node: &Node, operand: Option<&Node>) -> Expr {
        let span = self.span(node);
        let keyword = node.child_tokens().find(|t| t.kind == K::EchoKw);
        let keyword = keyword.map_or(node.range(), |t| (t.start, t.end));
        let echo = self.debug(Lang::EchoAt, keyword, span);
        match operand {
            Some(operand) => apply(echo, self.expr(operand), span),
            None => echo,
        }
    }

    /// `let assert p = e [as "message"]` and the statements after it:
    /// `match e with | p -> rest | value -> fail with the place and value`.
    pub(super) fn let_assert(
        &mut self,
        decl: &Node,
        span: Span,
        rest: impl FnOnce(&mut Self) -> Expr,
    ) -> Expr {
        let value = self.definition(decl);
        let pat = let_parts(decl, self.text)
            .pattern
            .map_or(Pat::Wild, |p| self.pattern(p));
        let body = rest(self);
        let message = decl
            .child_tokens()
            .find(|t| t.kind == K::String)
            .map_or_else(
                || "`let assert` did not match".to_owned(),
                |t| literal::string_value(t.text(self.text)),
            );
        let keyword = decl.child_tokens().find(|t| t.kind == K::AssertKw);
        let keyword = keyword.map_or(decl.range(), |t| (t.start, t.end));
        let missed = self.hidden("value");
        let fail = apply(
            apply(
                self.debug(Lang::AssertAt, keyword, span),
                string(message, span),
                span,
            ),
            Expr::new(ExprKind::Local(missed.clone()), span),
            span,
        );
        let arms = vec![
            Arm {
                pat,
                guard: None,
                body,
            },
            Arm {
                pat: Pat::Bind(missed),
                guard: None,
                body: fail,
            },
        ];
        Expr::new(ExprKind::Match(Box::new(value), arms), span)
    }

    /// The function of `Flick.Debug` the lang item `function` names, given
    /// the `CallSite` the checker found at `keyword`.
    fn debug(&self, function: Lang, keyword: Range, span: Span) -> Expr {
        let dicts = self.typing.dicts.get(&keyword).cloned().unwrap_or_default();
        with_dicts(Expr::new(ExprKind::lang(function), span), dicts)
    }
}

fn string(s: String, span: Span) -> Expr {
    Expr::new(ExprKind::Lit(Lit::String(s)), span)
}

fn apply(func: Expr, arg: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            stage: Stage::Pure,
            arg_first: false,
        },
        span,
    )
}
