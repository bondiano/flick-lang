//! Values passed beside the SQL text as placeholders: literals, and the
//! captures `$name`, `$name.field` and `$(e)` of values of the program,
//! typed where they stand and never parameters of the template. A capture
//! of an `Option t` is a nullable placeholder of `t`: `None` is `NULL`.

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
        nullable: bool,
    ) -> usize {
        self.params.push(Param {
            name: name.clone(),
            ty: ty.clone(),
            nullable,
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
            nullable,
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
        // an `Option t` stands for `t` or `NULL`
        let resolved = self.whnf(&self.zonk(&captured));
        let (head, args) = resolved.spine();
        let nullable = *head == Type::prim("Option") && args.len() == 1;
        let ty = if nullable {
            args[0].clone()
        } else {
            self.store.fresh()
        };
        let at = cx.capture(
            self.text_of(node),
            range(self.span(name)),
            captured,
            span,
            ty,
            nullable,
        );
        let placeholder = format!("${}", cx.params[at].placeholder + 1);
        // PostgreSQL cannot type `$1 IS NULL` alone: a nullable capture says its type
        let sql = match nullable.then(|| cast(&self.whnf(&self.zonk(&cx.params[at].ty)))) {
            Some(Some(sql_type)) => format!("CAST({placeholder} AS {sql_type})"),
            _ => placeholder,
        };
        let mut v = Val::plain(sql, cx.params[at].ty.clone());
        v.null = nullable;
        v.param = Some(at);
        v
    }
}

/// The SQL type a nullable capture of `ty` is cast to.
// shortcut: only base types; a type with a `via` codec gets no cast, so it
// cannot stand alone as in `is_null $x`; derive its column's type when one needs it.
fn cast(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Con { module, name } => match (module.as_str(), name.as_str()) {
            ("Prelude", "String") => Some("text"),
            ("Prelude", "Bool") => Some("boolean"),
            ("Prelude", "Int") => Some("integer"),
            ("Prelude", "Float") => Some("double precision"),
            ("Prelude", "BigInt") => Some("bigint"),
            ("Prelude", "Json") => Some("json"),
            ("BigDecimal", "BigDecimal") => Some("numeric"),
            _ => None,
        },
        _ => None,
    }
}
