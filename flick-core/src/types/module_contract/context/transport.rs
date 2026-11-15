//! Transport contextual occurrences along the same checked witness embedding.

use std::sync::Arc;

use super::{Embedding, Invalid};
use crate::types::ty::MetaOccurrence;
use crate::types::{Atom, Elem, Kind, ScopeId, Type};

impl Embedding {
    pub fn transport_kind_checked(&self, kind: &Kind) -> Result<Kind, Invalid> {
        let mappings: Vec<_> = self
            .source
            .0
            .iter()
            .enumerate()
            .map(|(index, from)| {
                (
                    from.witness.as_str(),
                    self.target.0[self.image[index]].witness.as_str(),
                )
            })
            .collect();
        let renamed = super::super::WitnessScope {
            from: "",
            to: "",
            mappings: &mappings,
            shadow: None,
            changed: false,
        }
        .kind(kind);
        self.transport_kind_occurrences(&renamed, &[])
    }

    pub fn transport_checked(&self, ty: &Type) -> Result<Type, Invalid> {
        self.transport_under(ty, &[])
    }

    fn transport_under(&self, ty: &Type, bound: &[ScopeId]) -> Result<Type, Invalid> {
        let renamed = self.transport_names(ty, self.source.len());
        self.transport_occurrences(&renamed, bound)
    }

    fn transport_occurrences(&self, ty: &Type, bound: &[ScopeId]) -> Result<Type, Invalid> {
        if let Type::Exists {
            scope,
            binder,
            contract,
            body,
        } = ty
        {
            let contract = self.transport_occurrences(contract, bound)?;
            let mut inner = bound.to_vec();
            inner.push(scope.identity());
            return Ok(Type::Exists {
                scope: *scope,
                binder: binder.clone(),
                contract: Box::new(contract),
                body: Box::new(self.transport_occurrences(body, &inner)?),
            });
        }
        let mut failure = None;
        let mut mapped = ty.map_children(
            &mut |child| match self.transport_occurrences(child, bound) {
                Ok(mapped) => mapped,
                Err(error) => {
                    failure = Some(error);
                    child.clone()
                }
            },
        );
        if let Some(error) = failure {
            return Err(error);
        }
        match &mut mapped {
            Type::Meta(meta) => *meta = self.transport_meta(meta, bound)?,
            Type::Forall { vars, .. } => {
                for binder in vars {
                    binder.kind = self.transport_kind_occurrences(&binder.kind, bound)?;
                }
            }
            Type::Row(row) | Type::Record(row) => {
                for (label, _) in &mut row.entries {
                    self.transport_label_occurrences(label, bound)?;
                }
                row.sort();
            }
            Type::Without { gone, .. } => {
                for label in gone.iter_mut() {
                    self.transport_label_occurrences(label, bound)?;
                }
                gone.sort();
            }
            _ => {}
        }
        Ok(mapped)
    }

    fn transport_kind_occurrences(&self, kind: &Kind, bound: &[ScopeId]) -> Result<Kind, Invalid> {
        Ok(match kind {
            Kind::Meta(meta) => Kind::Meta(self.transport_meta(meta, bound)?),
            Kind::Arrow(a, b) => Kind::arrow(
                self.transport_kind_occurrences(a, bound)?,
                self.transport_kind_occurrences(b, bound)?,
            ),
            Kind::Row(inner) => Kind::Row(Box::new(self.transport_kind_occurrences(inner, bound)?)),
            _ => kind.clone(),
        })
    }

    pub(super) fn transport_label_occurrences(
        &self,
        label: &mut Elem,
        bound: &[ScopeId],
    ) -> Result<(), Invalid> {
        match label {
            Elem::Capture(Atom::Meta(meta))
            | Elem::Error(Atom::Meta(meta))
            | Elem::Service(Atom::Meta(meta)) => *meta = self.transport_meta(meta, bound)?,
            Elem::Provided(inner) => self.transport_label_occurrences(inner, bound)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn transport_meta(
        &self,
        meta: &MetaOccurrence,
        bound: &[ScopeId],
    ) -> Result<MetaOccurrence, Invalid> {
        let Some(context) = meta.contextual() else {
            return Ok(meta.clone());
        };
        if !self.source.belongs_to(context.owner()) || !self.target.belongs_to(context.owner()) {
            return Err(Invalid::Authority);
        }
        let mut target = self.target.clone();
        // Locally bound coordinates remain in place; their contracts are
        // outside that binder and are transported through prior coordinates.
        for (index, id) in bound.iter().enumerate() {
            let Some(coordinate) = context.renaming().target.0.iter().find(|c| c.id == *id) else {
                continue;
            };
            if target.contains(*id) {
                continue;
            }
            let mut coordinate = (**coordinate).clone();
            coordinate.contract = self.transport_under(&coordinate.contract, &bound[..index])?;
            #[cfg(test)]
            coordinate.paired.project(self);
            coordinate.dependencies = coordinate
                .dependencies
                .iter()
                .map(|id| {
                    self.mapped(*id)
                        .or_else(|| bound[..index].contains(id).then_some(*id))
                        .ok_or(Invalid::Dependency)
                })
                .collect::<Result<_, _>>()?;
            target.0.push(Arc::new(coordinate));
        }
        let image = context
            .renaming()
            .mapping()
            .map(|(_, actual)| {
                let mapped = self
                    .mapped(actual)
                    .or_else(|| bound.contains(&actual).then_some(actual))
                    .ok_or(Invalid::Domain)?;
                target
                    .0
                    .iter()
                    .position(|coordinate| coordinate.id == mapped)
                    .ok_or(Invalid::Domain)
            })
            .collect::<Result<_, _>>()?;
        let renaming = Self::checked(context.renaming().source.clone(), target, image)?;
        Ok(meta.with_renaming(renaming))
    }
}
