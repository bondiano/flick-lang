//! Original declaration facts retained until ordinary inference has finished.

use std::{fmt, mem};

use super::{Checker, ForeignInfo, ForeignMode, Node, Retention, Span, Type, codes};
use crate::diagnostics::Diagnostic;

pub(in crate::types) struct PendingForeign {
    pub(super) decl: Node,
    pub(super) scheme: Type,
    pub(super) info: ForeignInfo,
    pub(super) retention: Option<Retention>,
    pub(super) arity: Vec<(Node, Option<usize>)>,
    pub(super) cancel: Vec<Node>,
    pub(super) catch: Vec<(Node, Node, Option<Type>)>,
    pub(super) span: Span,
    pub(super) name: String,
    pub(super) unknown: Option<Diagnostic>,
}

impl fmt::Debug for PendingForeign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingForeign")
            .field("name", &self.name)
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

impl PendingForeign {
    pub(in crate::types) fn diagnostic(&self) -> Diagnostic {
        self.unknown.clone().unwrap_or_else(|| {
            Diagnostic::error(codes::T013, format!(
                "the type of `{}` is not fully known (`_`) and cannot be generalized; give it a signature",
                self.name,
            ), self.span)
        })
    }
}

impl Checker<'_> {
    pub(in crate::types) fn finish_foreigns(&mut self) {
        for pending in mem::take(&mut self.typing.pending_foreigns) {
            let scheme = self.resolved_witness_type(&pending.scheme);
            if self.owner_type_is_pending(&scheme) {
                self.defer_foreign(pending, &scheme);
                continue;
            }
            self.finalized_foreign_info(pending, &scheme);
        }
    }

    pub(super) fn defer_foreign(&mut self, mut pending: PendingForeign, scheme: &Type) {
        let shown = self.show(scheme);
        let diagnostic = Diagnostic::error(
            codes::T013,
            format!(
                "the type of `{}` is not fully known (`{shown}`) and cannot be generalized; give it a signature",
                pending.name,
            ),
            pending.span,
        );
        let diagnostic = self
            .diagnostics
            .iter()
            .find(|existing| existing.code == codes::T013 && existing.span == Some(pending.span))
            .cloned()
            .unwrap_or_else(|| {
                self.diagnostics.push(diagnostic.clone());
                diagnostic
            });
        pending.unknown = Some(diagnostic);
        self.typing.pending_foreigns.push(pending);
    }

    pub(super) fn validate_prepared_meta(
        &mut self,
        pending: &mut PendingForeign,
        mode: ForeignMode,
        body: &Type,
        stages: usize,
    ) {
        pending.info.arity = None;
        for (node, arity) in &pending.arity {
            if arity.is_some_and(|n| n <= stages) {
                pending.info.arity = *arity;
            } else {
                self.bad_meta(node, &format!(
                    "`arity` takes the number of JS arguments, at most the {stages} parameters of the signature"
                ));
            }
        }
        if mode != ForeignMode::Async {
            pending.info.adapter.cancel = false;
            for node in &pending.cancel {
                self.bad_meta(node, "`cancel signal` is for a call whose row has `Async`: only a promise can be aborted");
            }
        }
        for (item, mapping, error) in &pending.catch {
            if mode == ForeignMode::Pure {
                pending.info.catch = None;
                self.bad_meta(
                    item,
                    "a `foreign pure` call cannot throw: `catch` needs another mode",
                );
            } else if let Some(error) = error {
                self.check_catch_mapping(mapping, body, error);
            }
        }
    }
}
