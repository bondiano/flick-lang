//! Collection and top-level function checking for neutral `@export "name"`
//! attributes. Backend-specific name validation, package-wide uniqueness, and
//! exportability rules (rejecting generic or dictionary-taking functions) live
//! in the target backend.

use super::Checker;
use super::bindings::let_parts;
use super::toplevel::next_non_attr;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::export_attr;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal::string_value;

impl<'a> Checker<'a> {
    /// Collects `@export "name"` attributes attached to top-level `let`
    /// functions (including members of `rec { … }` groups) into
    /// `self.typing.host_exports`, reporting `FLICK-S014` for non-function
    /// `let`s, destructurings, and `foreign` declarations.
    pub(super) fn check_host_exports(&mut self, decls: &[&Node]) {
        let text = self.text;
        let mut pending: Vec<((u32, u32), &'a str)> = Vec::new();
        for (i, decl) in decls.iter().enumerate() {
            match decl.kind {
                K::Attribute => {
                    if let Some(Ok(raw)) = export_attr::read_raw(decl, text) {
                        pending.push((decl.range(), raw));
                    }
                }
                K::Signature => {
                    if !next_non_attr(decls, i).is_some_and(|next| self.pairs(decl, next)) {
                        pending.clear();
                    }
                }
                K::LetDecl => {
                    if pending.is_empty() {
                        continue;
                    }
                    let Some(name) = let_parts(decl, text).name else {
                        self.reject_exports(
                            &mut pending,
                            "`@export` belongs before a named top-level function",
                        );
                        continue;
                    };
                    let entity = self.env.entity(Namespace::Value, &name);
                    let ok = self
                        .env
                        .value(&entity)
                        .is_some_and(|t| self.is_function_or_error(t));
                    if ok {
                        #[cfg(not(target_arch = "wasm32"))]
                        for (range, raw) in pending.drain(..) {
                            self.typing.host_exports.push(flick_ir::HostExport {
                                name: name.clone(),
                                export_name: string_value(raw),
                                span: Span::new(self.file, range.0, range.1),
                            });
                        }
                        #[cfg(target_arch = "wasm32")]
                        pending.clear();
                    } else {
                        for (range, raw) in pending.drain(..) {
                            let export_name = string_value(raw);
                            self.error(
                                codes::S014,
                                Span::new(self.file, range.0, range.1),
                                format!(
                                    "`@export \"{export_name}\"` on `{name}` requires a top-level function"
                                ),
                            );
                        }
                    }
                }
                K::ForeignDecl => {
                    self.reject_exports(
                        &mut pending,
                        "`@export` belongs before a top-level `let` function, not a `foreign` declaration",
                    );
                }
                K::RecGroup => {
                    pending.clear();
                    let members: Vec<&Node> = decl
                        .nodes()
                        .filter(|n| n.kind == K::Members)
                        .flat_map(Node::nodes)
                        .collect();
                    self.check_host_exports(&members);
                }
                _ => pending.clear(),
            }
        }
    }

    fn reject_exports(&mut self, pending: &mut Vec<((u32, u32), &'a str)>, message: &str) {
        for (range, _) in pending.drain(..) {
            self.error(
                codes::S014,
                Span::new(self.file, range.0, range.1),
                message.to_owned(),
            );
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
