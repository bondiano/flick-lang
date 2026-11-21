//! Constructs that move code to another stage or change what its calls
//! need: `task` defers its body, `try … with` removes the errors its handlers
//! cover completely, and `provide` removes the services a layer builds while
//! adding what building it needs.

use crate::types::Elem;
use std::mem;
use std::slice;

use super::Checker;
use super::effects::Site;
use super::env::{Form, Found};
use super::handle::{Handled, arm_pattern, failed_pattern, operation_path};
use super::infer::eff;
use super::patterns::is_pattern;
use super::services::Operation;
use super::ty::{Row, Type, Written, without};
use super::typing::Ref;
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

impl Checker<'_> {
    /// `task { body }`: the body runs when the `Eff` is awaited, so its calls
    /// count against the rows of the `Eff`, not against the enclosing stage.
    pub fn task(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        match node.nodes().next() {
            Some(body) => self.deferred(node, body, expected),
            None => Type::Error,
        }
    }

    /// The `Eff` of `body`, checked as the body of a task that `node` stands
    /// for: `task { body }`, or an argument passed unstarted.
    pub fn deferred(&mut self, node: &Node, body: &Node, expected: Option<&Type>) -> Type {
        let saved = mem::take(&mut self.cx.runtime);
        let (uses, locals) = (self.uses_mark(), self.cx.locals.len());
        let parts = expected.and_then(|t| Some((t.clone(), self.eff_parts(t)?)));
        let (mut stage, result) = match parts {
            Some((expected, (services, errors, result))) => {
                // the services of an `Eff` written as a type: a fix adds to them there
                let written = self.local_place_at(&services, self.span(node));
                let site = Site::Expected(expected, written);
                (self.bounded_stage(services, errors, site), Some(result))
            }
            None => (self.open_stage(), None),
        };
        self.live_bound(&mut stage, expected);
        let owner_mark = self.typing.owner_stack.len();
        self.enter_owner_stage(
            super::StageKey::Task { node: node.range() },
            stage.services.clone(),
        );
        let outer = mem::replace(&mut self.cx.stage, stage);
        self.enter_apart(super::mutation::Apart::Task);
        let result = match result {
            Some(result) => {
                self.check(body, &result);
                result
            }
            None => self.infer(body),
        };
        self.leave_apart();
        self.typing.owner_stack.truncate(owner_mark);
        let inner = mem::replace(&mut self.cx.stage, outer);
        self.cx.runtime = saved;
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
        let mut stage = self.cx.stage.clone();
        (
            stage.errors,
            stage.error_site,
            stage.in_result,
            stage.returns,
        ) = (raised.clone(), None, false, None);
        if let Some(needed) = &needed {
            stage.services = needed.clone();
        }
        // what this handler catches stays below the `tail` arm around it
        if let Some(allowed) = &mut stage.tail_arm {
            allowed.extend(handled.iter().map(Handled::row));
        }
        let outer = mem::replace(&mut self.cx.stage, stage);
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
        let inner = mem::replace(&mut self.cx.stage, outer);
        self.cx.stage.runtime |= inner.runtime;
        self.cx.stage.calls = inner.calls;
        let mut errors = Caught::default();
        // the handlers run in a JS function of their own
        let returns = self.cx.stage.returns.take();
        let region = self.enter_resume(node, span, &operations);
        let mut returned = false;
        for (arm, operation) in arms.iter().zip(&operations) {
            if let Some((index, op)) = operation {
                self.enter_apart(super::mutation::Apart::HandlerArm);
                let covers = self.operation_arm(arm, &handled[*index], op, &ty, region.as_ref());
                self.leave_apart();
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
        self.cx.stage.returns = returns;
        if !errors.all && self.cx.stage.pure.is_none() {
            self.unhandled_errors(&raised, errors.handled, span);
        }
        if let Some(needed) = needed
            && self.cx.stage.pure.is_none()
        {
            self.live_install(handled.iter().map(|h| &h.entry), span);
            self.handled_row(&needed, &handled, span);
        }
        ty
    }

    fn unhandled_errors(&mut self, raised: &Type, mut handled: Vec<(Elem, Type)>, span: Span) {
        Row::sort_entries(&mut handled);
        let rest = Type::Row(Row {
            entries: handled,
            tail: Some(Box::new(self.cx.stage.errors.clone())),
            ..Row::default()
        });
        let site = self.cx.stage.error_site.unwrap_or(self.cx.stage.site);
        self.sub_row(raised, &rest, site, span, true);
    }

    /// The services `needed` by the body of a handler of `handled`: those
    /// it covers, besides the enclosing stage's; an effect it does not
    /// cover is also the enclosing stage's, for the operations without arms.
    fn handled_row(&mut self, needed: &Type, handled: &[Handled], span: Span) {
        let mut entries: Vec<(Elem, Type)> = handled
            .iter()
            .map(|h| (h.label.clone(), h.entry.clone()))
            .collect();
        Row::sort_entries(&mut entries);
        let partial: Vec<(Elem, Type)> = handled
            .iter()
            .filter(|h| !h.complete())
            .map(|h| (h.label.clone(), h.entry.clone()))
            .collect();
        if !partial.is_empty() {
            let row = Type::Row(Row::closed(partial));
            self.perform(&row, &Type::empty_row(), span);
        }
        // an effect with arguments is answered by its name: the body may need
        // no other instance of it, which only the outside could give
        let instances: Vec<Elem> = (handled.iter())
            .filter(|h| h.label.is_instance(&h.entry))
            .map(|h| h.label.clone())
            .collect();
        let tail = self.cx.stage.services.clone();
        let tail = if instances.is_empty() {
            tail
        } else {
            without(tail, &instances)
        };
        let rest = Type::Row(Row {
            entries,
            tail: Some(Box::new(tail)),
            ..Row::default()
        });
        let site = self.cx.stage.site;
        self.sub_row(needed, &rest, site, span, false);
    }

    /// An arm for errors.
    fn error_arm(&mut self, arm: &Node, raised: &Type, ty: &Type, caught: &mut Caught) {
        let pattern = arm_pattern(arm).map(|p| failed_pattern(p, self.text).unwrap_or(p));
        if pattern.is_some_and(catch_all) {
            // it receives every error the body raises, whatever its uses ask for
            let every = self.store.fresh_error_row();
            let errors = Type::app(Type::lang(Lang::Errors), every.clone());
            let (_, guarded, span) = self.arm(arm, &errors, ty);
            if self.cx.stage.pure.is_none() {
                self.sites.push(Site::Expected(errors, Written::default()));
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
}

include!("handlers/providing.rs");
include!("handlers/result.rs");

impl Checker<'_> {
    /// The row label of a type declared with `type error`.
    pub fn error_label(&self, ty: &Type) -> Option<Elem> {
        match ty.spine().0 {
            Type::Con { module, name }
                if matches!(
                    self.env.type_def(module, name).map(|d| &d.form),
                    Some(Form::Error)
                ) =>
            {
                Some(Elem::Named(format!("{module}.{name}")))
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
    handled: Vec<(Elem, Type)>,
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
