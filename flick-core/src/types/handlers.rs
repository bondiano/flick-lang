//! Constructs that move code to another stage or change what its calls
//! need: `task` defers its body, `try … with` removes the errors its handlers
//! cover completely, and `provide` removes the services a layer builds while
//! adding what building it needs.

use std::mem;

use super::Checker;
use super::effects::{NO_EFFECTS, Site};
use super::env::Form;
use super::infer::eff;
use super::ty::{Row, Type};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// `task { body }`: the body runs when the `Eff` is awaited, so its calls
    /// count against the rows of the `Eff`, not against the enclosing stage.
    pub fn task(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let Some(body) = node.nodes().next() else {
            return Type::Error;
        };
        if !self.options.effects {
            let span = self.keyword_span(node, K::TaskKw);
            let message = format!("`task` makes an effect, but {NO_EFFECTS}");
            self.error(codes::E008, span, message);
        }
        let saved = mem::take(&mut self.runtime);
        let (uses, locals) = (self.uses_mark(), self.locals.len());
        let parts = expected.and_then(|t| Some((t.clone(), self.eff_parts(t)?)));
        let (stage, result) = match parts {
            Some((expected, (services, errors, result))) => (
                self.bounded_stage(services, errors, Site::Expected(expected)),
                Some(result),
            ),
            None => (self.open_stage(), None),
        };
        let outer = mem::replace(&mut self.stage, stage);
        let result = match result {
            Some(result) => {
                self.check(body, &result);
                result
            }
            None => self.infer(body),
        };
        let inner = mem::replace(&mut self.stage, outer);
        self.runtime = saved;
        let held = self.held_since(uses, locals);
        let ty = eff(inner.services, inner.errors, result);
        let span = self.span(node);
        self.close_over(ty, held, expected.cloned(), "task", span)
    }

    /// `try body with | E p -> handler …`. An error leaves the row only when
    /// an arm without a guard matches every value of it; a variable or `_`
    /// matches every error, as a value of `Errors e`. The handlers run at the
    /// enclosing stage, so what they need is added there.
    pub fn try_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let ty = expected.cloned().unwrap_or_else(|| self.fresh());
        let mut nodes = node.nodes();
        let raised = self.fresh();
        let mut stage = self.stage.clone();
        stage.errors = raised.clone();
        stage.returns = None;
        let outer = mem::replace(&mut self.stage, stage);
        if let Some(body) = nodes.next() {
            self.check(body, &ty);
        }
        let inner = mem::replace(&mut self.stage, outer);
        self.stage.runtime |= inner.runtime;
        self.stage.calls = inner.calls;
        let mut handled: Vec<(String, Type)> = Vec::new();
        let mut all = false;
        // the handlers run in a JS function of their own
        let returns = self.stage.returns.take();
        for arm in nodes.filter(|n| n.kind == K::MatchArm) {
            let error = self.fresh();
            let (pat, guarded, span) = self.arm(arm, &error, &ty);
            let covers = !guarded && self.covers_all(&pat);
            let error = self.whnf(&error);
            if let Type::Meta(_) = error {
                let _ = self.unify(&error, &Type::app(Type::prim("Errors"), raised.clone()));
                all |= covers;
                continue;
            }
            match self.error_label(&error) {
                Some(label) => {
                    if covers && !handled.iter().any(|(l, _)| *l == label) {
                        handled.push((label, error));
                    }
                }
                None if error.spine().0.is_prim("Errors") => {
                    all |= covers;
                }
                None if error == Type::Error => {}
                None => {
                    let shown = self.show(&error);
                    self.error(
                        codes::E007,
                        span,
                        format!(
                            "a handler matches errors declared with `type error`, but this pattern has type `{shown}`"
                        ),
                    );
                }
            }
        }
        self.stage.returns = returns;
        if !all && self.stage.pure.is_none() {
            handled.sort_by(|a, b| a.0.cmp(&b.0));
            let rest = Type::Row(Row {
                entries: handled,
                tail: Some(Box::new(self.stage.errors.clone())),
            });
            let span = self.keyword_span(node, K::TryKw);
            let site = self.stage.site;
            self.sub_row(&raised, &rest, site, span, true);
        }
        ty
    }

    /// `provide layer { body }`: the layer is built before the body runs;
    /// the body may need the services it builds, and building it may need
    /// other services and fail with its own errors.
    pub fn provide(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let children: Vec<&Node> = node.nodes().collect();
        let Some(&layer) = children.first() else {
            return Type::Error;
        };
        let actual = self.infer(layer);
        let (inputs, errors, outputs) = (self.fresh(), self.fresh(), self.fresh());
        let shape = Type::apps(
            Type::prim("Layer"),
            [inputs.clone(), errors.clone(), outputs.clone()],
        );
        if self.unify(&self.whnf(&actual), &shape).is_err() {
            let shown = self.show(&actual);
            self.error(
                codes::T003,
                self.span(layer),
                format!("`provide` takes a `Layer`, but this has type `{shown}`"),
            );
        }
        let span = self.keyword_span(node, K::ProvideKw);
        if self.runtime_allowed(span) {
            self.perform(&inputs, &errors, span);
        }
        let provided = self.store.normalize(&outputs).entries;
        let needed = self.fresh();
        let mut stage = self.stage.clone();
        stage.services = needed.clone();
        stage.returns = None;
        let outer = mem::replace(&mut self.stage, stage);
        let ty = match (children.get(1), expected) {
            (Some(body), Some(expected)) => {
                self.check(body, expected);
                expected.clone()
            }
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        let inner = mem::replace(&mut self.stage, outer);
        self.stage.calls = inner.calls;
        if self.stage.pure.is_none() {
            let rest = Type::Row(Row {
                entries: provided,
                tail: Some(Box::new(self.stage.services.clone())),
            });
            let site = self.stage.site;
            self.sub_row(&needed, &rest, site, span, false);
        }
        ty
    }

    /// The row label of a type declared with `type error`.
    pub fn error_label(&self, ty: &Type) -> Option<String> {
        match ty.spine().0 {
            Type::Con { module, name }
                if matches!(
                    self.env.type_def(module, name).map(|d| &d.form),
                    Some(Form::Error)
                ) =>
            {
                Some(format!("{module}.{name}"))
            }
            _ => None,
        }
    }

    fn keyword_span(&self, node: &Node, keyword: K) -> Span {
        node.child_tokens()
            .find(|t| t.kind == keyword)
            .map_or_else(|| self.span(node), |t| self.token_span(t))
    }
}
