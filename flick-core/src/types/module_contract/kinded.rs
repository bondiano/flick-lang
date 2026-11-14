//! One positional interpretation of identities in checked types and kinds.
//! Named row labels are not self-describing: their expected kind is evidence.

use std::borrow::Cow;

use super::{Elem, Kind, Scope, Type};
use crate::types::Effects;
use crate::types::ty::{Binder, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::types) enum Incomplete {
    Kind(Kind),
    Head(Type),
    NotFunction(Kind),
    NotRow(Kind),
    InvalidOccurrence(super::context::Invalid),
}

/// Read-only observations. Implementations must neither project the same type
/// recursively to discover its kind nor default an unresolved kind.
pub(in crate::types) trait Kinding {
    fn observe<'a>(
        &'a self,
        ty: &'a Type,
        _expected: &Kind,
        _binders: &[Binder],
        _bound: &[Bound],
    ) -> Result<Cow<'a, Type>, Incomplete> {
        Ok(Cow::Borrowed(ty))
    }
    fn observe_label<'a>(
        &'a self,
        label: &'a Elem,
        _binders: &[Binder],
        _bound: &[Bound],
    ) -> Result<Cow<'a, Elem>, Incomplete> {
        Ok(Cow::Borrowed(label))
    }
    fn scoped_head_kind(
        &self,
        ty: &Type,
        binders: &[Binder],
        _bound: &[Bound],
    ) -> Result<Kind, Incomplete> {
        self.head_kind(ty, binders)
    }
    fn resolve_kind(&self, kind: &Kind) -> Kind;
    fn checked_kind(&self, kind: &Kind) -> Result<Kind, Incomplete> {
        Ok(self.resolve_kind(kind))
    }
    fn head_kind(&self, ty: &Type, binders: &[Binder]) -> Result<Kind, Incomplete>;
}

#[derive(Clone)]
pub(in crate::types) struct Bound {
    pub scope: Scope,
    pub binder: String,
    pub contract: Type,
}

/// Both renaming and support collection use these same identity positions.
/// Literal row keys never enter `semantic_label`.
pub(in crate::types) trait Identities {
    fn ty(&mut self, ty: &Type, bound: &[Bound]) -> Type;
    fn kind(&mut self, kind: &Kind, bound: &[Bound]) -> Kind;
    fn semantic_label(&mut self, label: &Elem, bound: &[Bound]) -> Elem;
    fn finish(&mut self, ty: Type, _expected: &Kind, _bound: &[Bound]) -> Type {
        ty
    }
}

pub(in crate::types) fn map(
    ty: &Type,
    expected: &Kind,
    kinding: &impl Kinding,
    identities: &mut impl Identities,
) -> Result<Type, Incomplete> {
    map_in(ty, expected, kinding, identities, &[], &[])
}

pub(in crate::types) fn map_in(
    ty: &Type,
    expected: &Kind,
    kinding: &impl Kinding,
    identities: &mut impl Identities,
    binders: &[Binder],
    bound: &[Bound],
) -> Result<Type, Incomplete> {
    Walk {
        kinding,
        identities,
        binders: binders.to_vec(),
        bound: bound.to_vec(),
    }
    .ty(ty, expected)
}

struct Walk<'a, 'b, K, I> {
    kinding: &'a K,
    identities: &'b mut I,
    binders: Vec<Binder>,
    bound: Vec<Bound>,
}

impl<K: Kinding, I: Identities> Walk<'_, '_, K, I> {
    fn kind(&mut self, kind: &Kind) -> Result<Kind, Incomplete> {
        let kind = self.kinding.checked_kind(kind)?;
        Ok(match kind {
            Kind::Arrow(a, b) => Kind::arrow(self.kind(&a)?, self.kind(&b)?),
            Kind::Row(inner) => Kind::Row(Box::new(self.kind(&inner)?)),
            _ => self.identities.kind(&kind, &self.bound),
        })
    }

    fn ty(&mut self, ty: &Type, expected: &Kind) -> Result<Type, Incomplete> {
        // Validate kind evidence before any observation can erase its views.
        let _observed_kind = self.kind(expected)?;
        let observed = self
            .kinding
            .observe(ty, expected, &self.binders, &self.bound)?;
        let ty = observed.as_ref();
        let mapped: Result<Type, Incomplete> = match ty {
            Type::App { head, arg } => self.application(head, arg),
            Type::Fun {
                param,
                effects,
                result,
            } => Ok(Type::Fun {
                param: Box::new(self.ty(param, &Kind::Type)?),
                effects: effects
                    .as_ref()
                    .map(|effects| {
                        Ok(Box::new(Effects {
                            services: self.ty(&effects.services, &Kind::ServiceRow)?,
                            errors: self.ty(&effects.errors, &Kind::ErrorRow)?,
                            ..(**effects).clone()
                        }))
                    })
                    .transpose()?,
                result: Box::new(self.ty(result, &Kind::Type)?),
            }),
            Type::Tuple(items) => Ok(Type::Tuple(
                items
                    .iter()
                    .map(|item| self.ty(item, &Kind::Type))
                    .collect::<Result<_, _>>()?,
            )),
            Type::Record(row) => Ok(Type::Record(self.field_row(row, &Kind::Type)?)),
            Type::Row(row) => self.row(row, expected).map(Type::Row),
            Type::Union(a, b) => Ok(Type::Union(
                Box::new(self.ty(a, expected)?),
                Box::new(self.ty(b, expected)?),
            )),
            Type::Without { row, gone } => {
                let literal = self.literal_row(expected)?;
                let gone = gone
                    .iter()
                    .map(|label| {
                        if literal {
                            Ok(label.clone())
                        } else {
                            self.label(label)
                        }
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Type::Without {
                    row: Box::new(self.ty(row, expected)?),
                    gone,
                })
            }
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => Ok(Type::Captured {
                ty: Box::new(self.ty(ty, &Kind::Type)?),
                captures: Box::new(self.ty(captures, &Kind::CaptureRow)?),
                value: *value,
                elided: *elided,
            }),
            Type::Bounded { ty, captures } => Ok(Type::Bounded {
                ty: Box::new(self.ty(ty, &Kind::Type)?),
                captures: Box::new(self.ty(captures, &Kind::CaptureRow)?),
            }),
            Type::Forall {
                vars,
                constraints,
                body,
            } => self.forall(vars, constraints, body),
            Type::Exists {
                scope,
                binder,
                contract,
                body,
            } => self.exists(*scope, binder, contract, body),
            Type::Con { .. }
            | Type::Var(_)
            | Type::Symbol(_)
            | Type::Meta(_)
            | Type::Skolem(_)
            | Type::Error => Ok(self.identities.ty(ty, &self.bound)),
        };
        let mapped = mapped?;
        Ok(self
            .identities
            .finish(mapped, &self.kinding.checked_kind(expected)?, &self.bound))
    }

    fn application(&mut self, head: &Type, arg: &Type) -> Result<Type, Incomplete> {
        let head_kind = self
            .kinding
            .scoped_head_kind(head, &self.binders, &self.bound)?;
        let head_kind = self.kinding.checked_kind(&head_kind)?;
        let Kind::Arrow(parameter, _) = &head_kind else {
            return Err(match head_kind {
                Kind::Meta(_) => Incomplete::Kind(head_kind),
                _ => Incomplete::NotFunction(head_kind),
            });
        };
        Ok(Type::app(
            self.ty(head, &head_kind)?,
            self.ty(arg, parameter)?,
        ))
    }

    fn literal_row(&self, kind: &Kind) -> Result<bool, Incomplete> {
        match self.kinding.checked_kind(kind)? {
            Kind::Row(_) => Ok(true),
            Kind::ServiceRow | Kind::ErrorRow | Kind::CaptureRow => Ok(false),
            kind @ Kind::Meta(_) => Err(Incomplete::Kind(kind)),
            kind => Err(Incomplete::NotRow(kind)),
        }
    }

    fn label(&mut self, label: &Elem) -> Result<Elem, Incomplete> {
        let observed = self
            .kinding
            .observe_label(label, &self.binders, &self.bound)?;
        Ok(self.identities.semantic_label(&observed, &self.bound))
    }

    fn field_row(&mut self, row: &Row, value_kind: &Kind) -> Result<Row, Incomplete> {
        Ok(Row {
            entries: row
                .entries
                .iter()
                .map(|(label, value)| Ok((label.clone(), self.ty(value, value_kind)?)))
                .collect::<Result<_, _>>()?,
            tail: row
                .tail
                .as_ref()
                .map(|tail| {
                    self.ty(tail, &Kind::Row(Box::new(value_kind.clone())))
                        .map(Box::new)
                })
                .transpose()?,
            written: row.written,
        })
    }

    fn row(&mut self, row: &Row, expected: &Kind) -> Result<Row, Incomplete> {
        let expected = self.kinding.checked_kind(expected)?;
        if let Kind::Row(value_kind) = &expected {
            return self.field_row(row, value_kind);
        }
        self.literal_row(&expected)?;
        Ok(Row {
            entries: row
                .entries
                .iter()
                .map(|(label, value)| {
                    // Live/Provided payloads have their real declared kind, not
                    // necessarily Service: classify the label from the ROW kind.
                    let kind = if expected == Kind::ServiceRow && matches!(label, Elem::Live) {
                        // This is the explicit representation of Live C, not
                        // an inference from its label text or payload shape.
                        Kind::CaptureRow
                    } else {
                        self.kinding
                            .scoped_head_kind(value, &self.binders, &self.bound)?
                    };
                    let label = self.label(label)?;
                    Ok((label, self.ty(value, &kind)?))
                })
                .collect::<Result<_, _>>()?,
            tail: row
                .tail
                .as_ref()
                .map(|tail| self.ty(tail, &expected).map(Box::new))
                .transpose()?,
            written: row.written,
        })
    }

    fn forall(
        &mut self,
        vars: &[Binder],
        constraints: &[Type],
        body: &Type,
    ) -> Result<Type, Incomplete> {
        let mark = self.binders.len();
        let mapped = vars
            .iter()
            .map(|binder| {
                Ok(Binder {
                    name: binder.name.clone(),
                    kind: self.kind(&binder.kind)?,
                })
            })
            .collect::<Result<Vec<_>, Incomplete>>()?;
        self.binders.extend_from_slice(vars);
        let result = (|| {
            Ok(Type::Forall {
                vars: mapped,
                constraints: constraints
                    .iter()
                    .map(|constraint| self.ty(constraint, &Kind::Constraint))
                    .collect::<Result<_, _>>()?,
                body: Box::new(self.ty(body, &Kind::Type)?),
            })
        })();
        self.binders.truncate(mark);
        result
    }

    fn exists(
        &mut self,
        scope: Scope,
        binder: &str,
        contract: &Type,
        body: &Type,
    ) -> Result<Type, Incomplete> {
        let original_contract = contract.clone();
        let contract = self.ty(contract, &Kind::Module)?;
        // The binder scopes only its body, never its own contract.
        let mark = self.bound.len();
        self.bound.push(Bound {
            scope,
            binder: binder.into(),
            contract: original_contract,
        });
        let result = self.ty(body, &Kind::Type);
        self.bound.truncate(mark);
        let body = result?;
        Ok(Type::Exists {
            scope,
            binder: binder.into(),
            contract: Box::new(contract),
            body: Box::new(body),
        })
    }
}

#[cfg(test)]
mod tests;

/// Reordering follows identity transport without detaching labels from values.
pub(super) fn sort_identities(mut ty: Type) -> Type {
    match &mut ty {
        Type::Row(row) | Type::Record(row) => row.sort(),
        Type::Without { gone, .. } => gone.sort(),
        _ => {}
    }
    ty
}
