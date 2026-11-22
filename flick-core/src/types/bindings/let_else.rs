//! Type checking for `let pattern = initializer else fallback`: checked as
//! `match initializer with | pattern -> remainder | _ -> fallback`, using
//! existing GADT/row/class branch frames and monomorphic match bindings.

use std::{mem, slice};

use super::super::gadt::columns::Coverage;
use super::super::patterns::Pat;
use super::{Checker, Tail, Type, let_parts};
use crate::diagnostics::{Related, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::patterns;

impl Checker<'_> {
    /// `let pattern = initializer else fallback`: checked as
    /// `match initializer with | pattern -> remainder | _ -> fallback`, using
    /// the existing GADT/row/class branch frames and monomorphic match pattern
    /// bindings.
    #[allow(clippy::too_many_lines)]
    pub(in crate::types) fn let_else(
        &mut self,
        decl: &Node,
        rest: &[&Node],
        block_end: u32,
        expected: Option<&Type>,
        tail: Option<Tail<'_, Self>>,
    ) -> Type {
        let parts = let_parts(decl, self.text);
        let pattern_binders: Vec<(String, Span)> = parts
            .pattern
            .map(|p| {
                let mut tokens = Vec::new();
                patterns::binders(p, &mut tokens);
                tokens
                    .into_iter()
                    .map(|t| (t.text(self.text).to_owned(), self.token_span(t)))
                    .collect()
            })
            .unwrap_or_default();

        let since_init = self.diagnostics.len();
        let scrutinee = parts.body.map_or(Type::Error, |b| self.infer(b));
        self.explain_let_else_scope(
            since_init,
            &pattern_binders,
            "is bound by this `let ... else` pattern and is not in scope in its initializer",
            "bound here, available only after `else`",
        );

        let result = expected.cloned().unwrap_or_else(|| self.fresh());
        let open = expected.is_none();
        let errors = self.diagnostics.len();
        let outer_rows = self.error_rows.replace(Vec::new());

        let arm_mark = self.cx.locals.len();
        let frame = self.enter_arm();
        let mut rows = self.error_rows.take();
        let (pat, pat_span) = match parts.pattern {
            Some(pattern) => {
                self.error_rows = rows.take();
                let pat = self.bind_arm_pattern(pattern, &scrutinee);
                rows = self.error_rows.take();
                (pat, self.span(pattern))
            }
            None => (Pat::Wild, self.span(decl)),
        };

        let empty_remainder = rest.is_empty() && tail.is_none();
        if empty_remainder {
            self.error(
                codes::T019,
                Span::new(self.file, block_end, block_end),
                "a block must end with an expression: its value is the value of the block",
            );
        } else if !self.gadt.dead {
            if rest.is_empty() {
                if let Some(tail) = tail {
                    let ty = tail(self, Some(&result));
                    self.expect(&ty, &result, Span::new(self.file, block_end, block_end));
                }
            } else {
                self.check_statements(rest, block_end, Some(&result), tail);
            }
        }
        self.error_rows = rows;
        let dead = self.leave_arm(frame);
        if dead {
            self.typing.dead_arms.insert(decl.range());
        }
        self.cx.locals.truncate(arm_mark);

        let else_kw = decl.child_tokens().find(|t| t.kind == K::ElseKw);
        let else_span = match (else_kw, parts.fallback) {
            (Some(kw), Some(fb)) => Span::new(self.file, kw.start, fb.range().1),
            (Some(kw), None) => self.token_span(kw),
            _ => self.span(decl),
        };
        let fallback_rows = self.error_rows.take();
        let since_fallback = self.diagnostics.len();
        if let Some(fallback) = parts.fallback {
            self.check(fallback, &result);
        }
        self.explain_let_else_scope(
            since_fallback,
            &pattern_binders,
            "is bound by the `let ... else` pattern and is only in scope after `else`",
            "bound here, available only in the success remainder",
        );
        if open
            && !dead
            && !empty_remainder
            && let Some(last) = rest.last()
        {
            let success_span = self.span(last);
            self.explain_since(since_fallback, &result, success_span, &|t| {
                format!("the success remainder of `let ... else` has type `{t}`")
            });
        }
        self.error_rows = fallback_rows;

        let arms = [(pat, false, pat_span), (Pat::Wild, false, else_span)];
        let rows = mem::replace(&mut self.error_rows, outer_rows).unwrap_or_default();
        self.close_error_rows(rows, &arms, &scrutinee);

        if !self.diagnostics[errors..]
            .iter()
            .any(|d| d.severity == Severity::Error)
            && !matches!(self.whnf(&scrutinee), Type::Error)
        {
            self.check_let_else_reachability(&arms[0].0, pat_span, else_span, &scrutinee);
        }

        self.note_value(decl.range(), &result);
        if let Some(kw) = else_kw {
            self.note_type(self.token_span(kw), &result);
        }
        if let Some(kw) = decl.child_tokens().find(|t| t.kind == K::LetKw) {
            self.note_type(self.token_span(kw), &result);
        }
        result
    }

    fn explain_let_else_scope(
        &mut self,
        since: usize,
        binders: &[(String, Span)],
        why: &str,
        note: &str,
    ) {
        if binders.is_empty() {
            return;
        }
        for d in &mut self.diagnostics[since..] {
            if d.code != codes::T001 {
                continue;
            }
            let Some(span) = d.span else {
                continue;
            };
            let used = &self.text[span.start as usize..span.end as usize];
            if let Some((name, binder_span)) = binders.iter().find(|(n, _)| n == used) {
                let prefix = format!("cannot find value `{name}`");
                if d.message.starts_with(&prefix) {
                    d.message = format!("{prefix}: `{name}` {why}");
                    d.related.push(Related {
                        span: *binder_span,
                        message: note.to_owned(),
                    });
                }
            }
        }
    }

    fn check_let_else_reachability(
        &mut self,
        pat: &Pat,
        pat_span: Span,
        else_span: Span,
        scrutinee: &Type,
    ) {
        let tys = slice::from_ref(scrutinee);
        let coverage = Coverage::new(self);
        if self
            .useful_at(&[], slice::from_ref(pat), tys, &coverage)
            .is_none()
        {
            self.warning(
                codes::T010,
                pat_span,
                "this pattern is never matched: no value of the matched type is built so",
            );
        } else if self
            .useful_at(&[vec![pat.clone()]], &[Pat::Wild], tys, &coverage)
            .is_none()
        {
            self.warning(
                codes::T010,
                else_span,
                "this `else` branch is never reached: the pattern matches every value",
            );
        }
    }
}
