//! Collection and top-level function checking for neutral `@export "name"`
//! attributes. Backend-specific name validation, package-wide uniqueness, and
//! exportability rules (rejecting generic or dictionary-taking functions) live
//! in the target backend.

use std::mem;

use super::Checker;
use super::bindings::let_parts;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::export_attr::{self, ExportAttr};
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Collects `@export "name"` attributes attached to top-level `let`
    /// functions (including members of `rec { … }` groups) into
    /// `self.typing.host_exports`, reporting `FLICK-S014` for non-function
    /// `let`s, destructurings, and `foreign` declarations.
    pub(super) fn check_host_exports(&mut self, decls: &[&Node]) {
        self.collect_host_exports(decls);
    }

    fn collect_host_exports(&mut self, decls: &[&Node]) {
        let mut pending: Vec<ExportAttr> = Vec::new();
        for (i, decl) in decls.iter().enumerate() {
            match decl.kind {
                K::Attribute => {
                    if let Some(Ok(attr)) = export_attr::read(decl, self.text) {
                        pending.push(attr);
                    }
                }
                K::Signature => {
                    let paired = decls
                        .iter()
                        .skip(i + 1)
                        .find(|n| n.kind != K::Attribute)
                        .is_some_and(|next| self.pairs(decl, next));
                    if !paired {
                        pending.clear();
                    }
                }
                K::LetDecl => {
                    let attrs = mem::take(&mut pending);
                    if attrs.is_empty() {
                        continue;
                    }
                    let parts = let_parts(decl, self.text);
                    let Some(name) = parts.name else {
                        for attr in attrs {
                            self.error(
                                codes::S014,
                                Span::new(self.file, attr.span.0, attr.span.1),
                                "`@export` belongs before a named top-level function".to_owned(),
                            );
                        }
                        continue;
                    };
                    let entity = self.env.entity(Namespace::Value, &name);
                    let ty = self.env.value(&entity).cloned().map(|t| self.zonk(&t));
                    let ok = ty.as_ref().is_some_and(|t| self.is_function_or_error(t));
                    for attr in attrs {
                        let span = Span::new(self.file, attr.span.0, attr.span.1);
                        if ok {
                            self.typing.host_exports.push(flick_ir::HostExport {
                                name: name.clone(),
                                export_name: attr.name,
                                span,
                            });
                        } else {
                            self.error(
                                codes::S014,
                                span,
                                format!(
                                    "`@export \"{}\"` on `{name}` requires a top-level function",
                                    attr.name
                                ),
                            );
                        }
                    }
                }
                K::ForeignDecl => {
                    for attr in mem::take(&mut pending) {
                        self.error(
                            codes::S014,
                            Span::new(self.file, attr.span.0, attr.span.1),
                            "`@export` belongs before a top-level `let` function, not a `foreign` declaration"
                                .to_owned(),
                        );
                    }
                }
                K::RecGroup => {
                    pending.clear();
                    let members: Vec<&Node> = decl
                        .nodes()
                        .filter(|n| n.kind == K::Members)
                        .flat_map(Node::nodes)
                        .collect();
                    self.collect_host_exports(&members);
                }
                _ => pending.clear(),
            }
        }
    }
    fn is_function_or_error(&self, ty: &Type) -> bool {
        let mut current = self.whnf(ty);
        for _ in 0..64 {
            match current {
                Type::Forall { body, .. } => current = self.whnf(&body),
                Type::Fun { .. } | Type::Error | Type::Meta(_) => return true,
                _ => return false,
            }
        }
        false
    }
}
