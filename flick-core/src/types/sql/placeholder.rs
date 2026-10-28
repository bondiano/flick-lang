//! Values passed beside the SQL text as placeholders: literals, and the
//! captures `$name`, `$name.field` and `$(e)` of values of the program,
//! typed where they stand and never parameters of the template.

use super::super::Checker;
use super::super::infer::literal_type;
use super::super::ty::Type;
use super::super::typing::{SqlForm, SqlValue, range};
use super::expr::Val;
use super::query::{Cx, Param};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Cx {
    /// A placeholder for the value of `$name` at `at`, of type `captured`;
    /// its SQL type follows from where it is used, as a parameter's does.
    pub fn capture(
        &mut self,
        name: String,
        at: (u32, u32),
        captured: Type,
        span: Span,
        ty: Type,
    ) -> usize {
        self.params.push(Param {
            name: name.clone(),
            ty: ty.clone(),
            nullable: false,
            form: None,
            span,
            placeholder: self.slots.len(),
            captured: Some(captured),
        });
        self.slots.push(ty);
        self.plan.params.push(SqlValue {
            name,
            literal: Some(at),
            form: SqlForm::Kind(String::new()),
            nullable: false,
        });
        self.params.len() - 1
    }
}

impl Checker<'_> {
    /// A literal: `true` and `false` inline, anything else a placeholder.
    pub(super) fn sql_literal(&mut self, cx: &mut Cx, node: &Node) -> Val {
        let first = node.child_tokens().next();
        let token = first.map(|t| t.kind);
        let ty = match first {
            Some(t) if t.kind == K::Number => self.number(node, t.text(self.text)),
            _ => literal_type(token),
        };
        match token {
            Some(K::TrueKw) => Val::plain("TRUE".into(), ty),
            Some(K::FalseKw) => Val::plain("FALSE".into(), ty),
            _ => {
                let at = cx.literal(range(self.span(node)), ty.clone());
                Val::plain(format!("${}", at + 1), ty)
            }
        }
    }

    /// A capture: the value of its expression, passed as a placeholder.
    pub(super) fn sql_capture(&mut self, cx: &mut Cx, node: &Node) -> Val {
        let Some(name) = node.nodes().next() else {
            return Val::error();
        };
        let captured = self.infer(name);
        let span = self.span(node);
        let ty = self.store.fresh();
        let at = cx.capture(
            self.text_of(node),
            range(self.span(name)),
            captured,
            span,
            ty,
        );
        let mut v = Val::plain(
            format!("${}", cx.params[at].placeholder + 1),
            cx.params[at].ty.clone(),
        );
        v.param = Some(at);
        v
    }
}
