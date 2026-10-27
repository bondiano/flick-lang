//! `todo`, `echo` and `let assert`: calls of `Flick.Debug` told where they are.

use super::Lower;
use crate::ir::{Arm, Expr, ExprKind, Lit, Pat, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::let_parts;

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
        let message = format!("{}: {message}", self.location(node));
        debug_call("todo_at", string(message, span), span)
    }

    /// `echo e`, or a bare `echo` as a function.
    pub(super) fn echo(&mut self, node: &Node, operand: Option<&Node>) -> Expr {
        let span = self.span(node);
        let echo = debug_call("echo_at", string(self.location(node), span), span);
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
        let message = format!("{}: {message}", self.location(decl));
        let missed = self.hidden("value");
        let fail = apply(
            debug_call("assert_at", string(message, span), span),
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

    /// `Module:line` of a node.
    pub(super) fn location(&self, node: &Node) -> String {
        let start = node.range().0 as usize;
        let line = self.text[..start].matches('\n').count() + 1;
        format!("{}:{line}", self.module)
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

fn debug_call(name: &str, arg: Expr, span: Span) -> Expr {
    let func = Expr::new(
        ExprKind::Global {
            module: "Flick.Debug".into(),
            name: name.into(),
        },
        span,
    );
    apply(func, arg, span)
}
