//! What a plugin's `check` reads of its code: the type the checker found
//! for each `_` of it, as text, and the fields of a closed record.

use flick_plugin::{Fact, FactField, FactType, Span};

use super::Checker;
use super::env::{Abi, EnumLit, Form};
use super::ty::Type;
use crate::types::lang::Lang;

impl Checker<'_> {
    /// The fact of a `_` of generated code the checker found `t` for; the
    /// span is the block's, which the expansion gives it.
    pub(super) fn fact(&self, t: &Type) -> Fact {
        let t = self.zonk(t);
        let fields = match self.whnf(&t) {
            Type::Record(row) if row.tail.is_none() => Some(
                (row.entries.iter())
                    .map(|(label, ty)| FactField {
                        label: label.clone(),
                        ty: self.fact_type(ty),
                    })
                    .collect(),
            ),
            _ => None,
        };
        Fact {
            span: Span::default(),
            ty: self.fact_type(&t),
            fields,
        }
    }

    fn fact_type(&self, t: &Type) -> FactType {
        let expanded = self.unaliased(t);
        let unknown = expanded.contains(&mut |x| matches!(x, Type::Meta(_) | Type::Error));
        FactType {
            shown: self.show_exact(t),
            exact: (!unknown).then(|| self.show_qualified(&expanded)),
            literal_of: self.literal_of(&expanded),
        }
    }

    /// The exact base type of `t` when it is an enum, opaque or not.
    fn literal_of(&self, t: &Type) -> Option<String> {
        let Type::Con { module, name } = t else {
            return None;
        };
        let first = match &self.env.type_def(module, name)?.form {
            Form::Adt {
                abi: Abi::Literal(literals),
                ..
            }
            | Form::Nominal { literals } => literals.first()?,
            _ => return None,
        };
        let base = match first {
            EnumLit::Int(_) => Lang::Int,
            EnumLit::String(_) => Lang::String,
        };
        Some(self.show_qualified(&Type::lang(base)))
    }

    /// `t` with every alias in it expanded.
    fn unaliased(&self, t: &Type) -> Type {
        self.whnf(t)
            .map_children(&mut |child| self.unaliased(child))
    }
}
