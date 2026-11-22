//! Scope-owned `defer { … }` and `errdefer { … }` cleanup statements.
//!
//! Each statement binds lexically to the nearest enclosing explicit
//! `scope [owner] { … }`, traversing immediate blocks, conditionals, matches,
//! `loop`/`while`/`for … do`, `try` bodies/error-arms/`return`-arms, `provide`
//! and `unpack`, and stopping at function/lambda/`task`/`lazy`/comprehension/
//! operation-handler boundaries as well as inside cleanup bodies themselves.

use std::mem;

use super::scope;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::Elem;
use crate::types::Elided;
use crate::types::effects::Site;
use crate::types::kinds::Kind;
use crate::types::lang::{self, Lang};
use crate::types::mutation::Apart;
use crate::types::ty::{Row, Type};
use crate::types::typing::Owner;
#[cfg(feature = "ide")]
use crate::types::typing::{DeferInfo, range};

/// Execution or cleanup boundary that stops lexical `defer`/`errdefer`
/// owner resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferBarrier {
    Function,
    Task,
    Lazy,
    Sequence,
    HandlerArm,
    Cleanup,
}

/// Entry on the lexical owner stack for `defer`/`errdefer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferOwner {
    /// An explicit lexical `scope [owner] { … }`, indexed into `State::scopes`.
    Scope(usize),
    /// A boundary that stops lookup of an outer explicit `scope`.
    Barrier(DeferBarrier),
}

impl Checker<'_> {
    /// Type-checks `defer { cleanup }` or `errdefer { cleanup }`.
    pub fn defer_stmt(&mut self, node: &Node) -> Type {
        let is_errdefer = node.has_token(K::ErrdeferKw);
        let kw_kind = if is_errdefer {
            K::ErrdeferKw
        } else {
            K::DeferKw
        };
        let kw_span = node
            .child_tokens()
            .find(|t| t.kind == kw_kind)
            .map_or_else(|| self.span(node), |t| self.token_span(t));
        let stmt_span = self.span(node);

        let owner_idx = match self.cx.regions.defer_owners.last().copied() {
            Some(DeferOwner::Scope(index)) => Some(index),
            other => {
                self.missing_defer_owner(is_errdefer, kw_span, other);
                None
            }
        };

        let owner_info = owner_idx.map(|index| {
            let scope = self.cx.regions.scopes[index].clone();
            self.typing
                .owners
                .insert(node.range(), Owner::Scope(scope.node));
            #[cfg(feature = "ide")]
            self.typing.defers.insert(
                range(kw_span),
                DeferInfo {
                    owner_span: scope.span,
                    owner_node: scope.node,
                    owner_name: scope.name.clone(),
                    is_errdefer,
                },
            );
            scope
        });

        let saved_runtime = mem::take(&mut self.cx.runtime);
        let uses = self.uses_mark();
        let locals = self.cx.locals.len();

        let cleanup_services = self.store.fresh_of(&Kind::ServiceRow);
        let cleanup_site = Site::DeferCleanup {
            is_errdefer,
            span: kw_span,
        };
        let mut cleanup_stage =
            self.bounded_stage(cleanup_services.clone(), Type::empty_row(), cleanup_site);
        cleanup_stage.returns = None;
        let outer_stage = mem::replace(&mut self.cx.stage, cleanup_stage);
        self.enter_apart(Apart::Defer);
        self.cx
            .regions
            .defer_owners
            .push(DeferOwner::Barrier(DeferBarrier::Cleanup));

        if let Some(body) = node.nodes().next() {
            self.check(body, &Type::lang(Lang::Unit));
        }

        self.cx.regions.defer_owners.pop();
        self.leave_apart();
        let _inner_stage = mem::replace(&mut self.cx.stage, outer_stage);
        self.cx.runtime = saved_runtime;

        let Some(owner) = owner_info else {
            return Type::lang(Lang::Unit);
        };

        let owner_region = Type::Skolem(owner.region);
        let held = self.held_since(uses, locals);
        let captures = self.store.fresh_capture_row();
        let callback_expected = Type::Captured {
            ty: Box::new(Type::lang(Lang::Unit)),
            captures: Box::new(captures.clone()),
            value: false,
            elided: Elided::default(),
        };
        let what = if is_errdefer {
            "`errdefer` cleanup"
        } else {
            "`defer` cleanup"
        };
        let _ = self.close_over(
            Type::lang(Lang::Unit),
            held,
            Some(callback_expected),
            what,
            stmt_span,
        );

        if lang::module_of(Lang::Detached).is_some() {
            self.want(
                Type::app(Type::lang(Lang::Detached), cleanup_services.clone()),
                stmt_span,
            );
        }
        if lang::module_of(Lang::Outlives).is_some() {
            self.cx.regions.outlives.push(super::Outlives {
                captures,
                owner: owner_region.clone(),
                givens: self.classes.givens.clone(),
                span: stmt_span,
                owner_related: Some(owner.span),
            });
        }

        let provided_mark = self.cx.regions.provided.len();
        let provided_late_mark = self.cx.regions.provided_late.len();
        if lang::module_of(Lang::Provided).is_some() {
            self.want_provided_owned(
                cleanup_services.clone(),
                owner_region.clone(),
                stmt_span,
                Some(owner.span),
            );
        }

        let registration_services = Type::Row(Row {
            entries: vec![(
                Elem::named(scope()),
                Type::app(Type::lang(Lang::Scope), owner_region),
            )],
            tail: Some(Box::new(cleanup_services)),
            ..Row::default()
        });
        if self.runtime_allowed(kw_span) {
            self.perform(&registration_services, &Type::empty_row(), stmt_span);
        }
        self.settle_provided_since(provided_mark, provided_late_mark);

        Type::lang(Lang::Unit)
    }

    fn missing_defer_owner(&mut self, is_errdefer: bool, kw_span: Span, top: Option<DeferOwner>) {
        let kw = if is_errdefer { "errdefer" } else { "defer" };
        let outer_scope = self
            .cx
            .regions
            .defer_owners
            .iter()
            .rev()
            .find_map(|entry| match entry {
                DeferOwner::Scope(index) => Some(self.cx.regions.scopes[*index].span),
                DeferOwner::Barrier(_) => None,
            });
        let has_scope_service = self
            .store
            .normalize(&self.cx.stage.services)
            .get(scope())
            .is_some();

        let (message, advice, related) = match top {
            Some(DeferOwner::Barrier(DeferBarrier::Cleanup)) => (
                format!(
                    "`{kw}` cannot appear directly inside another `defer` or `errdefer` cleanup block without its own explicit `scope`"
                ),
                "open an inner `scope { … }` inside the cleanup block if it needs scoped cleanup",
                None,
            ),
            Some(DeferOwner::Barrier(DeferBarrier::Function)) if has_scope_service => (
                format!(
                    "`{kw}` requires a lexically enclosing explicit `scope {{ … }}`, not a generic `Scope` service requirement"
                ),
                "use `Scope.defer (fun _ -> …)` to register cleanup on a caller's `Scope`, or open an explicit `scope { … }`",
                None,
            ),
            Some(DeferOwner::Barrier(DeferBarrier::Function)) if outer_scope.is_some() => (
                format!(
                    "`{kw}` cannot bind across a function or lambda boundary to an outer `scope`"
                ),
                "open an explicit `scope { … }` inside this function, or pass `Scope s` and call `Scope.defer`",
                outer_scope.map(|span| {
                    (
                        span,
                        "this outer `scope` does not own code inside a nested function",
                    )
                }),
            ),
            Some(DeferOwner::Barrier(DeferBarrier::Task)) => (
                format!(
                    "`{kw}` cannot bind across a `task {{ … }}` boundary without an explicit `scope` inside it"
                ),
                "open an explicit `scope { … }` inside the `task`, or use `Scope.defer`",
                outer_scope
                    .map(|span| (span, "this outer `scope` does not own code inside `task`")),
            ),
            Some(DeferOwner::Barrier(DeferBarrier::Lazy)) => (
                format!(
                    "`{kw}` cannot bind across a `lazy` boundary without an explicit `scope` inside it"
                ),
                "open an explicit `scope { … }` inside the `lazy` block, or use `Scope.defer`",
                outer_scope
                    .map(|span| (span, "this outer `scope` does not own code inside `lazy`")),
            ),
            Some(DeferOwner::Barrier(DeferBarrier::Sequence)) => (
                format!(
                    "`{kw}` cannot appear in a `for … yield` comprehension without an explicit `scope`"
                ),
                "use `for … do` inside a `scope { … }`, or open an explicit `scope { … }`",
                outer_scope.map(|span| {
                    (
                        span,
                        "this outer `scope` does not own lazy comprehension steps",
                    )
                }),
            ),
            Some(DeferOwner::Barrier(DeferBarrier::HandlerArm)) => (
                format!(
                    "`{kw}` cannot bind across an effect handler arm boundary without an explicit `scope` inside the arm"
                ),
                "open an explicit `scope { … }` inside the handler arm",
                outer_scope.map(|span| {
                    (
                        span,
                        "this outer `scope` does not own code inside a handler arm",
                    )
                }),
            ),
            _ => (
                format!(
                    "`{kw}` requires a lexically enclosing explicit `scope {{ … }}`: an ambient runtime owner cannot be used"
                ),
                "wrap the resource lifetime in `scope { … }`, or pass `Scope s` and call `Scope.defer`",
                None,
            ),
        };

        let mut diagnostic = Diagnostic::error(codes::G004, message, kw_span)
            .with_fix(|| (advice.to_owned(), Vec::new()));
        if let Some((span, label)) = related {
            diagnostic = diagnostic.with_related(span, label);
        }
        self.diagnostics.push(diagnostic);
    }
}
