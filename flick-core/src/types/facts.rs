//! What a plugin's `check` reads of its code: the type the checker found
//! for each `_` of it, as text, and the fields of a closed record.

use flick_plugin::{Fact, FactField, FactType, Span};

use super::Checker;
use super::ty::Type;

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
            shown: self.show(t),
            exact: (!unknown).then(|| self.show_qualified(&expanded)),
        }
    }

    /// `t` with every alias in it expanded.
    fn unaliased(&self, t: &Type) -> Type {
        self.whnf(t)
            .map_children(&mut |child| self.unaliased(child))
    }
}
