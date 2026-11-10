//! `retention owner`: what a foreign function keeps until the owner of a region
//! closes may hold only what outlives that owner. A signature with one region
//! gets the constraints that say so; any other has to write them.

use std::slice;

use super::mentions;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::classes::args_of;
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::{Binder, Type};

impl Checker<'_> {
    /// What a foreign function keeps until an owner closes may hold only what
    /// outlives that owner: a capture row of a parameter that nothing after
    /// its last parameter shows needs `Fiber.Outlives`.
    pub(super) fn owner_retention(&mut self, scheme: &Type, body: &Type, span: Span) {
        let Type::Forall {
            vars, constraints, ..
        } = scheme
        else {
            return;
        };
        let outlived = constrained(constraints, Lang::Outlives);
        let kept = self.kept_rows(vars, &outlived, body);
        let unprovided = self.unprovided_rows(vars, constraints, &outlived, body);
        if let Some(row) = unprovided.first() {
            self.error(
                codes::F002,
                span,
                format!(
                    "with `retention owner` the foreign function keeps work that uses the services `{0}` until its owner closes, so its signature needs `Fiber.Provided {0} s` for the owner's region `s`",
                    row.name
                ),
            );
        }
        if let Some(row) = kept.first() {
            self.error(
                codes::F002,
                span,
                format!(
                    "with `retention owner` the foreign function keeps what `{0}` holds until its owner closes, so its signature needs `Fiber.Outlives {0} s` for the owner's region `s`",
                    row.name
                ),
            );
        }
    }

    /// `scheme` of the foreign declaration `decl` with what `retention owner`
    /// says of its one region: what a capture row keeps outlives it, and what
    /// kept work uses is provided and detached. With other than one region the
    /// signature has to say which.
    pub(in crate::types) fn with_owner_constraints(&self, decl: &Node, scheme: Type) -> Type {
        let Type::Forall {
            vars,
            mut constraints,
            body,
        } = scheme
        else {
            return scheme;
        };
        let mut regions = vars.iter().filter(|v| matches!(v.kind, Kind::Region));
        if let (Some(region), None, true) = (
            regions.next(),
            regions.next(),
            keeps_for_owner(decl, self.text),
        ) {
            let region = Type::Var(region.name.clone());
            let needs = |class, row: &str| {
                Type::apps(
                    Type::lang(class),
                    [Type::Var(row.to_owned()), region.clone()],
                )
            };
            let mut outlived = constrained(&constraints, Lang::Outlives);
            for row in self.kept_rows(&vars, &outlived, &body) {
                constraints.push(needs(Lang::Outlives, &row.name));
                outlived.push(row.name.clone());
            }
            for row in self.unprovided_rows(&vars, &constraints, &outlived, &body) {
                constraints.push(needs(Lang::Provided, &row.name));
            }
            let detached = constrained(&constraints, Lang::Detached);
            for row in constrained(&constraints, Lang::Provided) {
                if !detached.contains(&row) {
                    constraints.push(Type::app(Type::lang(Lang::Detached), Type::Var(row)));
                }
            }
        }
        Type::Forall {
            vars,
            constraints,
            body,
        }
    }

    /// The capture rows `body` keeps past its result and no constraint says
    /// outlive the owner: what follows the last parameter that holds such a
    /// row does not show it.
    fn kept_rows<'b>(
        &self,
        vars: &'b [Binder],
        outlived: &[String],
        body: &Type,
    ) -> Vec<&'b Binder> {
        // a later stage holds what the result shows, though a signature
        // leaves it out
        let body = &self.held_stages(body);
        (vars.iter())
            .filter(|v| {
                let name = slice::from_ref(&v.name);
                if !matches!(v.kind, Kind::CaptureRow) || outlived.contains(&v.name) {
                    return false;
                }
                let (mut rest, mut after) = (body.clone(), None);
                while let Type::Fun { param, result, .. } = self.whnf_captured(&rest).0 {
                    if mentions(&param, name) {
                        after = Some((*result).clone());
                    }
                    rest = *result;
                }
                after.is_some_and(|after| !mentions(&after, name))
            })
            .collect()
    }

    /// The service rows of kept work no constraint says are provided: work
    /// kept until the owner closes runs with the caller's services, which must
    /// outlive the owner too (`docs/formal/runtime.md#a-provide-service-outlives-the-release-of-its-layer`).
    fn unprovided_rows<'b>(
        &self,
        vars: &'b [Binder],
        constraints: &[Type],
        outlived: &[String],
        body: &Type,
    ) -> Vec<&'b Binder> {
        let provided = constrained(constraints, Lang::Provided);
        let held = self.params_of(body);
        (vars.iter())
            .filter(|v| {
                matches!(v.kind, Kind::ServiceRow)
                    && !provided.contains(&v.name)
                    && held
                        .iter()
                        .any(|p| mentions(p, outlived) && mentions(p, slice::from_ref(&v.name)))
            })
            .collect()
    }
}

/// Whether `decl` is a foreign declaration that says `retention owner`.
pub fn keeps_for_owner(decl: &Node, text: &str) -> bool {
    (decl.kind == K::ForeignDecl)
        && (decl.nodes().filter(|n| n.kind == K::ForeignMeta))
            .flat_map(Node::nodes)
            .any(|item| {
                let name = item.child_tokens().find(|t| t.kind == K::Ident);
                let value = item.nodes().next().map(|n| n.text(text));
                name.is_some_and(|t| t.text(text) == "retention")
                    && value.is_some_and(|v| v.trim() == "owner")
            })
}

/// The variables `class` constrains first in `constraints`.
fn constrained(constraints: &[Type], class: Lang) -> Vec<String> {
    (constraints.iter())
        .filter_map(|c| match args_of(c, class)?.first()? {
            Type::Var(v) => Some(v.clone()),
            _ => None,
        })
        .collect()
}
