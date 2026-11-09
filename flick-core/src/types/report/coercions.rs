//! One-step conversions chosen only from settled nominal types.

use crate::source::Span;
use crate::types::Checker;
use crate::types::classes::{Dict, split};
use crate::types::lang::Lang;
use crate::types::ty::Type;
use crate::types::typing::{Coercion, range};

impl Checker<'_> {
    pub(super) fn coercion_types(&self, actual: &Type, expected: &Type) -> Option<(Type, Type)> {
        let (actual, expected) = (self.conversion_type(actual), self.conversion_type(expected));
        (nominal(&actual) && nominal(&expected)).then_some((actual, expected))
    }

    pub(super) fn insert_coercion(&mut self, actual: &Type, expected: &Type, span: Span) -> bool {
        let mut candidates = self.env.instances().filter_map(|instance| {
            if !instance.implicit {
                return None;
            }
            let head = self.canon(&instance.head);
            let ((module, name), args) = split(&head)?;
            if args.as_slice() != [actual, expected] {
                return None;
            }
            let method = self.conversion_method(module, name)?;
            Some(Coercion {
                method,
                dict: Dict::Instance {
                    module: instance.module.clone(),
                    name: instance.name.clone(),
                    args: Vec::new(),
                },
            })
        });
        let Some(coercion) = candidates.next() else {
            return false;
        };
        if candidates.next().is_some() {
            return false;
        }
        self.typing.coercions.insert(range(span), coercion);
        true
    }
}

fn nominal(ty: &Type) -> bool {
    match ty {
        Type::Con { .. } => !ty.is_lang(Lang::Eff) && !ty.is_lang(Lang::Layer),
        Type::App { head, arg } => nominal(head) && nominal(arg),
        _ => false,
    }
}
