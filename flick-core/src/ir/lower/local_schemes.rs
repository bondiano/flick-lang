//! Recording lowered local `let` / `let rec` schemes on `declarations`.

use super::Lower;
use crate::iface::LocalScheme;
use crate::syntax::cst::Node;
use crate::types::Type;

impl Lower<'_> {
    pub(super) fn record_local_scheme(&mut self, decl: &Node, unique: &str) {
        if self.current_item.is_empty() {
            return;
        }
        let Some(scheme) = self.typing.local_schemes.get(&decl.range()) else {
            return;
        };
        if let Some(scheme) = self.resolve_local_scheme(scheme) {
            self.local_schemes.push(LocalScheme {
                item: self.current_item.clone(),
                local: unique.to_owned(),
                scheme,
            });
        }
    }

    fn resolve_local_scheme(&self, ty: &Type) -> Option<Type> {
        fn go(lower: &Lower<'_>, ty: &Type, ok: &mut bool) -> Type {
            match ty {
                Type::Skolem(id) => {
                    if let Some(scope_range) = lower.typing.scope_skolems.get(id) {
                        if let Some(owner) = lower.owners.get(scope_range) {
                            Type::Var(owner.clone())
                        } else {
                            *ok = false;
                            Type::Error
                        }
                    } else if let Some(name) = lower.typing.skolem_names.get(id) {
                        Type::Var(name.clone())
                    } else {
                        *ok = false;
                        Type::Error
                    }
                }
                other => other.map_children(&mut |child| go(lower, child, ok)),
            }
        }
        let mut ok = true;
        let resolved = go(self, ty, &mut ok);
        ok.then_some(resolved)
    }
}
