//! A retained issued origin, with checked lazy transforms of its actual carrier.
//! The sidecar is not part of structural telescope equality or hashing.

#[cfg(test)]
use std::collections::BTreeSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[cfg(test)]
use super::{Context, Embedding, ProjectionError};
#[cfg(test)]
use crate::iface::Interface;
use crate::types::ScopeId;
#[cfg(test)]
use crate::types::module_contract::kinded::{self, Bound, Identities, Incomplete, Kinding};
#[cfg(test)]
use crate::types::module_contract::named::ExistentialWitness;
#[cfg(test)]
use crate::types::ty::scope::{copy_kind, copy_label};
#[cfg(test)]
use crate::types::{Elem, Kind, Type};

#[derive(Clone, Default)]
pub(super) struct Paired(Option<Origin>);

#[derive(Clone)]
struct Origin {
    #[cfg(test)]
    issued: ExistentialWitness,
    registration: ScopeId,
    lease: Arc<()>,
    #[cfg(test)]
    transforms: Vec<Transform>,
}

#[cfg(test)]
#[derive(Clone)]
enum Transform {
    Copy(ScopeId, ScopeId),
    Project(Embedding),
}

impl fmt::Debug for Paired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() {
            "IssuedWitness"
        } else {
            "Fixture"
        })
    }
}
impl PartialEq for Paired {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Paired {}
impl Hash for Paired {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl Paired {
    #[cfg(test)]
    pub(super) fn issued(issued: ExistentialWitness, registration: ScopeId) -> Self {
        Self(Some(Origin {
            issued,
            registration,
            lease: Arc::new(()),
            transforms: Vec::new(),
        }))
    }

    pub(super) fn same_registration(&self, other: &Self) -> bool {
        self.0.as_ref().zip(other.0.as_ref()).is_some_and(|(a, b)| {
            a.registration == b.registration && Arc::ptr_eq(&a.lease, &b.lease)
        })
    }

    pub(super) fn registration(&self) -> Option<ScopeId> {
        self.0.as_ref().map(|origin| origin.registration)
    }

    #[cfg(test)]
    pub(super) fn origin(&self) -> Option<&ExistentialWitness> {
        self.0.as_ref().map(|origin| &origin.issued)
    }

    #[cfg(test)]
    pub(super) fn copy_bound(&mut self, from: ScopeId, to: ScopeId) {
        if let Some(origin) = &mut self.0 {
            origin.transforms.push(Transform::Copy(from, to));
        }
    }

    #[cfg(test)]
    pub(super) fn project(&mut self, view: &Embedding) {
        if let Some(origin) = &mut self.0 {
            origin.transforms.push(Transform::Project(view.clone()));
        }
    }

    /// `current` must be re-paired with this checker's live declaration before
    /// this call; an old raw revision tag alone never validates the origin.
    #[cfg(test)]
    pub(super) fn materialize(
        &self,
        current: &ExistentialWitness,
        kinding: &impl Kinding,
    ) -> Result<(Type, Interface), ProjectionError> {
        let mut contract = current.canonical_contract();
        let mut actual = current.view().clone();
        if let Some(origin) = &self.0 {
            for transform in &origin.transforms {
                match transform {
                    Transform::Copy(from, to) => {
                        contract = contract.copy_bound_handle(*from, *to);
                        actual = super::super::actual::map(&actual, kinding, &mut Copy(*from, *to))
                            .map_err(ProjectionError::Incomplete)?;
                    }
                    Transform::Project(view) => {
                        contract =
                            view.project_type(&contract, &Kind::Module, kinding, &[], &[])?;
                        actual = view.project_actual(&actual, kinding)?;
                    }
                }
            }
        }
        let actual =
            super::super::actual::observe(&actual, kinding).map_err(ProjectionError::Incomplete)?;
        Ok((contract, actual))
    }
}

#[cfg(test)]
struct Copy(ScopeId, ScopeId);
#[cfg(test)]
impl Identities for Copy {
    fn ty(&mut self, ty: &Type, _: &[Bound]) -> Type {
        ty.copy_bound_handle(self.0, self.1)
    }
    fn kind(&mut self, kind: &Kind, _: &[Bound]) -> Kind {
        copy_kind(kind, self.0, self.1)
    }
    fn semantic_label(&mut self, label: &Elem, _: &[Bound]) -> Elem {
        let mut label = label.clone();
        copy_label(&mut label, self.0, self.1);
        label
    }
}

#[cfg(test)]
impl Context {
    pub(in crate::types) fn issued_origins(&self) -> impl Iterator<Item = &ExistentialWitness> {
        self.0
            .iter()
            .filter_map(|coordinate| coordinate.paired.origin())
    }

    pub(in crate::types) fn actuals(
        &self,
        current: &[ExistentialWitness],
        kinding: &impl Kinding,
    ) -> Result<Vec<(ScopeId, Type, Interface)>, ProjectionError> {
        self.0
            .iter()
            .map(|coordinate| {
                let origin = coordinate
                    .paired
                    .origin()
                    .ok_or(ProjectionError::Invalid(super::Invalid::Authority))?;
                let current = current
                    .iter()
                    .find(|pair| {
                        pair.issuer_owner() == origin.issuer_owner()
                            && pair.view().module == origin.view().module
                    })
                    .ok_or(ProjectionError::Invalid(super::Invalid::Authority))?;
                let (contract, actual) = coordinate.paired.materialize(current, kinding)?;
                Ok((coordinate.id, contract, actual))
            })
            .collect()
    }
}

#[cfg(test)]
impl Context {
    pub(in crate::types) fn actual_dependencies(
        &self,
        actual: &Interface,
        kinding: &impl Kinding,
        scoped: &BTreeSet<String>,
    ) -> Result<BTreeSet<ScopeId>, Incomplete> {
        let mut collect = Dependencies {
            context: self,
            scoped,
            own: owned_modules(actual),
            missing: false,
            found: BTreeSet::default(),
        };
        super::super::actual::map(actual, kinding, &mut collect)?;
        collect.finish()
    }
}

#[cfg(test)]
struct Dependencies<'a> {
    context: &'a Context,
    scoped: &'a BTreeSet<String>,
    own: BTreeSet<String>,
    missing: bool,
    found: BTreeSet<ScopeId>,
}

#[cfg(test)]
impl Dependencies<'_> {
    fn finish(self) -> Result<BTreeSet<ScopeId>, Incomplete> {
        if self.missing {
            Err(Incomplete::InvalidOccurrence(super::Invalid::Dependency))
        } else {
            Ok(self.found)
        }
    }

    fn module(&mut self, module: &str, bound: &[Bound]) {
        let mut available = self.own.contains(module);
        for coordinate in &self.context.0 {
            if coordinate.modules.contains(module) {
                available = true;
                if !bound
                    .iter()
                    .any(|local| local.scope.identity() == coordinate.id)
                {
                    self.found.insert(coordinate.id);
                }
            }
        }
        if self.scoped.contains(module) && !available {
            self.missing = true;
        }
    }
}

#[cfg(test)]
impl Identities for Dependencies<'_> {
    fn ty(&mut self, ty: &Type, bound: &[Bound]) -> Type {
        if let Type::Con { module, .. } = ty {
            self.module(module, bound);
        }
        ty.clone()
    }
    fn kind(&mut self, kind: &Kind, bound: &[Bound]) -> Kind {
        if let Kind::Named { module, .. } = kind {
            self.module(module, bound);
        }
        kind.clone()
    }
    fn semantic_label(&mut self, label: &Elem, bound: &[Bound]) -> Elem {
        match label {
            Elem::Named(name) => {
                // A generated atomic name may itself contain dots. Preserve the
                // existing qualified-label interpretation, but only against
                // exact module identities supplied by issued/retained views.
                let modules: BTreeSet<_> = self
                    .scoped
                    .iter()
                    .chain(
                        self.context
                            .0
                            .iter()
                            .flat_map(|coordinate| &coordinate.modules),
                    )
                    .filter(|module| {
                        name.strip_prefix(module.as_str())
                            .is_some_and(|suffix| suffix.starts_with('.'))
                    })
                    .cloned()
                    .collect();
                for module in modules {
                    self.module(&module, bound);
                }
            }
            Elem::Provided(inner) => {
                self.semantic_label(inner, bound);
            }
            _ => {}
        }
        label.clone()
    }
}

#[cfg(test)]
impl Context {
    pub(in crate::types) fn refreshed_dependencies(
        &self,
        actuals: &[(ScopeId, Type, Interface)],
        kinding: &impl Kinding,
        scoped: &BTreeSet<String>,
    ) -> Result<Self, ProjectionError> {
        let mut refreshed = Self::default();
        for coordinate in &self.0 {
            let (_, head, actual) = actuals
                .iter()
                .find(|(id, _, _)| *id == coordinate.id)
                .ok_or(ProjectionError::Invalid(super::Invalid::Authority))?;
            if *head != coordinate.contract {
                return Err(ProjectionError::Invalid(super::Invalid::Contract));
            }
            let mut collect = Dependencies {
                context: &refreshed,
                scoped,
                own: owned_modules(actual),
                missing: false,
                found: BTreeSet::default(),
            };
            kinded::map(head, &Kind::Module, kinding, &mut collect)
                .map_err(ProjectionError::Incomplete)?;
            super::super::actual::map(actual, kinding, &mut collect)
                .map_err(ProjectionError::Incomplete)?;
            let mut coordinate = (**coordinate).clone();
            coordinate.dependencies = collect.finish().map_err(ProjectionError::Incomplete)?;
            coordinate.modules = owned_modules(actual);
            refreshed.0.push(Arc::new(coordinate));
        }
        Ok(refreshed)
    }
}

#[cfg(test)]
pub(in crate::types::module_contract) fn owned_modules(actual: &Interface) -> BTreeSet<String> {
    let mut modules = BTreeSet::new();
    let mut pending = vec![actual];
    while let Some(current) = pending.pop() {
        modules.insert(current.module.clone());
        pending.extend(current.modules.iter().filter_map(|(name, child)| {
            (child.module == format!("{}.{name}", current.module)).then_some(child)
        }));
    }
    modules
}
