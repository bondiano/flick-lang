//! Positional projection using current, read-only kind evidence.

use super::{Embedding, Invalid};
use crate::iface::Interface;
use crate::types::module_contract::WitnessScope;
use crate::types::module_contract::kinded::{self, Bound, Identities, Incomplete, Kinding};
use crate::types::ty::Binder;
use crate::types::{Elem, Kind, ScopeId, Type};

#[derive(Debug, PartialEq, Eq)]
pub(in crate::types) enum ProjectionError {
    Incomplete(Incomplete),
    Invalid(Invalid),
}

impl Embedding {
    pub(in crate::types) fn project_actual(
        &self,
        actual: &Interface,
        kinding: &impl Kinding,
    ) -> Result<Interface, ProjectionError> {
        let mut projection = Projection {
            view: self,
            failure: None,
        };
        let actual = super::super::actual::map(actual, kinding, &mut projection)
            .map_err(ProjectionError::Incomplete)?;
        if let Some(error) = projection.failure {
            Err(ProjectionError::Invalid(error))
        } else {
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
            let mut names = WitnessScope {
                from: "",
                to: "",
                mappings: &mappings,
                shadow: None,
                changed: false,
            };
            Ok(rebase_actual_metadata(actual, &mut names))
        }
    }

    pub(in crate::types) fn project_type(
        &self,
        ty: &Type,
        expected: &Kind,
        kinding: &impl Kinding,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Type, ProjectionError> {
        let mut projection = Projection {
            view: self,
            failure: None,
        };
        let mapped = kinded::map_in(ty, expected, kinding, &mut projection, binders, bound)
            .map_err(ProjectionError::Incomplete)?;
        match projection.failure {
            Some(error) => Err(ProjectionError::Invalid(error)),
            None => Ok(mapped),
        }
    }
}

struct Projection<'a> {
    view: &'a Embedding,
    failure: Option<Invalid>,
}

impl Projection<'_> {
    fn names(&self, bound: &[Bound]) -> Vec<(&str, &str)> {
        self.view
            .source
            .0
            .iter()
            .enumerate()
            .filter(|(_, coordinate)| {
                !bound.iter().any(|local| {
                    coordinate.id == local.scope.identity()
                        || coordinate.witness == format!("$exists:{}", local.binder)
                })
            })
            .map(|(index, coordinate)| {
                (
                    coordinate.witness.as_str(),
                    self.view.target.0[self.view.image[index]].witness.as_str(),
                )
            })
            .collect()
    }

    fn ids(bound: &[Bound]) -> Vec<ScopeId> {
        bound.iter().map(|local| local.scope.identity()).collect()
    }

    fn observe<T>(&mut self, result: Result<T, Invalid>, original: T) -> T {
        match result {
            Ok(mapped) => mapped,
            Err(error) => {
                self.failure = Some(error);
                original
            }
        }
    }
}

impl Identities for Projection<'_> {
    fn ty(&mut self, ty: &Type, bound: &[Bound]) -> Type {
        if let Type::MetaView(meta) = ty {
            let mapped = self.view.transport_meta(meta, &Self::ids(bound));
            return Type::MetaView(Box::new(self.observe(mapped, (**meta).clone())));
        }
        let mappings = self.names(bound);
        let mut names = WitnessScope {
            from: "",
            to: "",
            mappings: &mappings,
            shadow: None,
            changed: false,
        };
        match ty {
            Type::Con { module, name } => Type::con(&names.module(module), name),
            _ => ty.clone(),
        }
    }

    fn kind(&mut self, kind: &Kind, bound: &[Bound]) -> Kind {
        if let Kind::MetaView(meta) = kind {
            let mapped = self.view.transport_meta(meta, &Self::ids(bound));
            return Kind::MetaView(Box::new(self.observe(mapped, (**meta).clone())));
        }
        let mappings = self.names(bound);
        WitnessScope {
            from: "",
            to: "",
            mappings: &mappings,
            shadow: None,
            changed: false,
        }
        .kind(kind)
    }

    fn finish(&mut self, ty: Type, _: &Kind, _: &[Bound]) -> Type {
        kinded::sort_identities(ty)
    }

    fn semantic_label(&mut self, label: &Elem, bound: &[Bound]) -> Elem {
        let mappings = self.names(bound);
        let mut label = WitnessScope {
            from: "",
            to: "",
            mappings: &mappings,
            shadow: None,
            changed: false,
        }
        .elem(label);
        let result = self
            .view
            .transport_label_occurrences(&mut label, &Self::ids(bound));
        self.observe(result, ());
        label
    }
}

fn rebase_actual_metadata(mut actual: Interface, names: &mut WitnessScope<'_>) -> Interface {
    actual.module = names.module(&actual.module);
    for export in &mut actual.exports {
        export.entity.module = names.module(&export.entity.module);
    }
    for support in &mut actual.support_types {
        support.entity.module = names.module(&support.entity.module);
    }
    for instance in &mut actual.instances {
        instance.module = names.module(&instance.module);
    }
    for child in actual.modules.values_mut() {
        *child = rebase_actual_metadata(child.clone(), names);
    }
    actual
}
