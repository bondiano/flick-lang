//! Constructs that move code to another stage or change what its calls
//! need: `task` defers its body, `try … with` removes the errors its handlers
//! cover completely, and `provide` removes the services a layer builds while
//! adding what building it needs.

use std::mem;

use super::Checker;
use super::effects::{NO_EFFECTS, Site};
use super::env::Form;
use super::handle::{Handled, arm_pattern, operation_path};
use super::infer::eff;
use super::patterns::is_pattern;
use super::services::Operation;
use super::ty::{Row, Type};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

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
    /// enclosing stage, so what they need is added there. With arms for
    /// operations (`handle.rs`), an effect leaves the row of the body when
    /// arms without a guard cover all its operations; the body's other
    /// calls of it go on to the enclosing stage.
    pub fn try_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let ty = expected.cloned().unwrap_or_else(|| self.fresh());
        let arms: Vec<&Node> = node.nodes().filter(|n| n.kind == K::MatchArm).collect();
        let effects = self.handles_effects(&arms);
        let span = self.keyword_span(node, K::TryKw);
        if effects && self.runtime_allowed(span) {
            let empty = Type::empty_row();
            self.perform(&empty, &empty, span);
        }
        // each arm for an operation, its effect resolved before the body is checked
        let mut handled = Vec::new();
        let operations: Vec<Option<(usize, Operation)>> = arms
            .iter()
            .map(|arm| {
                let pattern = arm_pattern(arm)?;
                let (qualifier, name) = operation_path(pattern, self.text)?;
                let at = self.name_span(pattern);
                Some(self.handled_operation(&mut handled, (&qualifier, &name, at)))
            })
            .map(Option::flatten)
            .collect();
        let value = if arms.iter().any(|a| a.has_token(K::ReturnKw)) {
            self.fresh()
        } else {
            ty.clone()
        };
        let raised = self.store.fresh_error_row();
        let needed = effects.then(|| self.fresh());
        let mut stage = self.stage.clone();
        stage.errors = raised.clone();
        stage.returns = None;
        if let Some(needed) = &needed {
            stage.services = needed.clone();
        }
        let outer = mem::replace(&mut self.stage, stage);
        if effects {
            let labels = handled.iter().map(|h: &Handled| h.label.clone()).collect();
            self.enter_provide(node, span, labels, false);
        }
        if let Some(body) = node.nodes().next() {
            self.check(body, &value);
        }
        if effects {
            self.leave_provide();
        }
        let inner = mem::replace(&mut self.stage, outer);
        self.stage.runtime |= inner.runtime;
        self.stage.calls = inner.calls;
        let mut errors = Caught::default();
        // the handlers run in a JS function of their own
        let returns = self.stage.returns.take();
        let region = self.enter_resume(node, span, &operations);
        let mut returned = false;
        for (arm, operation) in arms.iter().zip(&operations) {
            if let Some((index, op)) = operation {
                let covers = self.operation_arm(arm, &handled[*index], op, &ty, region.as_ref());
                if covers {
                    handled[*index].covered.push(op.name.clone());
                }
            } else if arm.has_token(K::ReturnKw) {
                let (pat, guarded, span) = self.arm(arm, &value, &ty);
                if returned || guarded || !self.covers_all(&pat) {
                    self.error(
                        codes::E015,
                        span,
                        "a handler has one `return` arm, without a guard, and it matches every value of the body".to_owned(),
                    );
                }
                returned = true;
            } else if arm_pattern(arm).is_some_and(|p| operation_path(p, self.text).is_some()) {
                self.unknown_operation_arm(arm, &ty);
            } else {
                self.error_arm(arm, &raised, &ty, &mut errors);
            }
        }
        if region.is_some() {
            self.leave_resume(&ty, node);
        }
        self.stage.returns = returns;
        if !errors.all && self.stage.pure.is_none() {
            let mut handled = errors.handled;
            handled.sort_by(|a, b| a.0.cmp(&b.0));
            let rest = Type::Row(Row {
                entries: handled,
                tail: Some(Box::new(self.stage.errors.clone())),
            });
            let site = self.stage.site;
            self.sub_row(&raised, &rest, site, span, true);
        }
        if let Some(needed) = needed
            && self.stage.pure.is_none()
        {
            self.handled_row(&needed, &handled, span);
        }
        ty
    }

    /// The services `needed` by the body of a handler of `handled`: those
    /// it covers, besides the enclosing stage's; an effect it does not
    /// cover is also the enclosing stage's, for the operations without arms.
    fn handled_row(&mut self, needed: &Type, handled: &[Handled], span: Span) {
        let mut entries: Vec<(String, Type)> = handled
            .iter()
            .map(|h| (h.label.clone(), h.entry.clone()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let partial: Vec<(String, Type)> = handled
            .iter()
            .filter(|h| !h.complete())
            .map(|h| (h.label.clone(), h.entry.clone()))
            .collect();
        if !partial.is_empty() {
            let row = Type::Row(Row::closed(partial));
            self.perform(&row, &Type::empty_row(), span);
        }
        let rest = Type::Row(Row {
            entries,
            tail: Some(Box::new(self.stage.services.clone())),
        });
        let site = self.stage.site;
        self.sub_row(needed, &rest, site, span, false);
    }

    /// An arm for errors.
    fn error_arm(&mut self, arm: &Node, raised: &Type, ty: &Type, caught: &mut Caught) {
        if arm_pattern(arm).is_some_and(catch_all) {
            // it receives every error the body raises, whatever its uses ask for
            let every = self.store.fresh_error_row();
            let errors = Type::app(Type::lang(Lang::Errors), every.clone());
            let (_, guarded, span) = self.arm(arm, &errors, ty);
            if self.stage.pure.is_none() {
                self.sites.push(Site::Expected(errors));
                self.sub_row(raised, &every, self.sites.len() - 1, span, true);
            }
            caught.all |= !guarded;
            return;
        }
        let error = self.fresh();
        let (pat, guarded, span) = self.arm(arm, &error, ty);
        let covers = !guarded && self.covers_all(&pat);
        let error = self.whnf(&error);
        if let Type::Meta(_) = error {
            let _ = self.unify(&error, &Type::app(Type::lang(Lang::Errors), raised.clone()));
            caught.all |= covers;
            return;
        }
        match self.error_label(&error) {
            Some(label) => {
                if covers && !caught.handled.iter().any(|(l, _)| *l == label) {
                    caught.handled.push((label, error));
                }
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

    /// `provide layer { body }`: the layer is built before the body runs;
    /// the body may need the services it builds, and building it may need
    /// other services and fail with its own errors.
    pub fn provide(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let children: Vec<&Node> = node.nodes().collect();
        let Some(&layer) = children.first() else {
            return Type::Error;
        };
        let actual = self.infer(layer);
        let (inputs, errors, outputs) = (self.fresh(), self.store.fresh_error_row(), self.fresh());
        let shape = Type::apps(
            Type::lang(Lang::Layer),
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
        let outputs = self.store.normalize(&outputs);
        let labels = outputs.entries.iter().map(|(l, _)| l.clone()).collect();
        // an unknown layer may build any service
        let unknown = !outputs.tails.is_empty();
        let provided = outputs.entries;
        // outputs ending in a variable of the signature (`Layer none none o`)
        // provide all of it; an unknown tail does not grow by the body's needs
        let open: Vec<Type> = outputs
            .tails
            .into_iter()
            .filter(|t| matches!(t, Type::Skolem(_) | Type::Var(_)))
            .collect();
        let needed = self.fresh();
        let mut stage = self.stage.clone();
        stage.services = needed.clone();
        stage.returns = None;
        let outer = mem::replace(&mut self.stage, stage);
        self.enter_provide(node, span, labels, unknown);
        let ty = match (children.get(1), expected) {
            (Some(body), Some(expected)) => {
                self.check(body, expected);
                expected.clone()
            }
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        self.leave_provide();
        let inner = mem::replace(&mut self.stage, outer);
        self.stage.calls = inner.calls;
        if self.stage.pure.is_none() {
            let tail = open
                .into_iter()
                .fold(self.stage.services.clone(), |rest, t| {
                    Type::Union(Box::new(t), Box::new(rest))
                });
            let rest = Type::Row(Row {
                entries: provided,
                tail: Some(Box::new(tail)),
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

/// The errors the arms of a `try` catch: by label, or all of them.
#[derive(Default)]
struct Caught {
    handled: Vec<(String, Type)>,
    all: bool,
}

/// A handler pattern that matches every error: a variable or `_`, maybe in
/// parentheses or annotated.
fn catch_all(pattern: &Node) -> bool {
    match pattern.kind {
        K::VarPat | K::WildcardPat => true,
        K::ParenPat | K::AnnotPat => pattern
            .nodes()
            .find(|n| is_pattern(n.kind))
            .is_some_and(catch_all),
        _ => false,
    }
}
