//! `-` in a row: `{...A, -B; -E}` takes elements out of what the row names.

use super::Free;
use super::sets::rebuilt;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::Elem;
use crate::types::kinds::Kind;
use crate::types::ty::{Norm, Type};

/// An element `-` takes out.
pub(super) struct Gone {
    label: Elem,
    ty: Type,
    span: Span,
}

impl Checker<'_> {
    /// What `-` takes out of a side of `kind`: services, then errors.
    pub(super) fn taken(
        &mut self,
        omitted: &[&Node],
        kind: &Kind,
        free: Free,
    ) -> (Vec<Gone>, Vec<Gone>) {
        let (mut services, mut errors) = (Vec::new(), Vec::new());
        for &node in omitted {
            let Some(item) = node.nodes().next() else {
                continue;
            };
            let span = self.span(node);
            let is_set = self.set_kind(item).is_some();
            if is_set && !node.has_token(K::DotDotDot) {
                self.set_needs_dots(item, None);
            }
            if is_set || node.has_token(K::DotDotDot) {
                let Some(set) = self.set_of(node, item, free) else {
                    continue;
                };
                if *kind == Kind::ErrorRow && !set.services.is_empty() {
                    self.services_after_semi(item);
                    continue;
                }
                services.extend((set.services.into_iter()).map(|(label, ty)| Gone {
                    label,
                    ty,
                    span,
                }));
                for failed in set.fails {
                    let norm = self.store.normalize(&failed);
                    errors.extend((norm.entries.into_iter()).map(|(label, ty)| Gone {
                        label,
                        ty,
                        span,
                    }));
                }
                continue;
            }
            if *kind == Kind::ServiceRow && self.row_side(item) == Some(Kind::ErrorRow) {
                let text = self.text_of(item);
                self.error(
                    codes::T002,
                    span,
                    format!("`{text}` is an error, which is taken out after `;`: `{{…; -{text}}}`"),
                );
                continue;
            }
            let element = if *kind == Kind::ServiceRow {
                Kind::Service
            } else {
                Kind::Type
            };
            let reported = self.diagnostics.len();
            let ty = self.ty(item, &element, free);
            if self.diagnostics.len() > reported {
                continue;
            }
            let ty = match self.whnf(&ty) {
                Type::Error => ty,
                expanded => expanded,
            };
            let Some(label) = self.element_label(&ty, kind, item) else {
                continue;
            };
            let gone = Gone { label, ty, span };
            if *kind == Kind::ServiceRow {
                services.push(gone);
            } else {
                errors.push(gone);
            }
        }
        (services, errors)
    }

    /// `row` without what is `gone`, which it must hold; its tail stays as it is.
    pub(super) fn without(&mut self, row: Type, gone: Vec<Gone>, named: &[Elem]) -> Type {
        if gone.is_empty() {
            return row;
        }
        let Norm { mut entries, tails } = self.store.normalize(&row);
        for Gone { label, ty, span } in gone {
            let shown = self.show(&ty);
            if named.contains(&label) {
                let message = format!("`{shown}` is both written and taken out of this row");
                self.error(codes::T002, span, message);
            } else if let Some(at) = (entries.iter()).position(|(l, t)| *l == label && *t == ty) {
                entries.remove(at);
            } else if tails.is_empty() {
                let message = format!("this row holds no `{shown}`, so `-` has nothing to take");
                self.error(codes::T002, span, message);
            } else {
                let message = format!(
                    "`-` takes `{shown}` out of what the row names, and a row variable names nothing: list what it holds instead"
                );
                self.error(codes::T002, span, message);
            }
        }
        rebuilt(entries, tails)
    }
}
