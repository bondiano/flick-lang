//! Ordered lexical witness contexts. No embedding can exchange ancestors.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[cfg(test)]
use super::witness_scope_occurs;
use super::{Type, WitnessScope};
#[cfg(test)]
use crate::iface::{Interface, support};
use crate::types::{Scope, ScopeId};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Coordinate {
    owner: ScopeId,
    id: ScopeId,
    witness: String,
    contract: Type,
    dependencies: BTreeSet<ScopeId>,
    modules: BTreeSet<String>,
    paired: paired::Paired,
}

/// A well-formed ordered telescope. Coordinates are never reordered or edited.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(in crate::types) struct Context(Vec<Arc<Coordinate>>);

#[derive(Debug, Clone)]
pub(in crate::types) struct Scopes {
    owner: ScopeId,
    registered: BTreeMap<ScopeId, Arc<Coordinate>>,
    #[cfg(test)]
    contexts: BTreeMap<ScopeId, Context>,
}

impl Default for Scopes {
    fn default() -> Self {
        Self {
            owner: Scope::default().identity(),
            registered: BTreeMap::new(),
            #[cfg(test)]
            contexts: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::types) enum Invalid {
    Domain,
    Authority,
    Order,
    Contract,
    Dependency,
}

impl Scopes {
    pub(in crate::types) fn valid_context(&self, context: &Context) -> bool {
        context.belongs_to(self.owner)
            && context.0.iter().all(|coordinate| {
                match coordinate.paired.registration() {
                    Some(id) => self.registered.get(&id).is_some_and(|registered| {
                        registered.paired.same_registration(&coordinate.paired)
                    }),
                    None => cfg!(test), // Unissued coordinates exist only in algebra fixtures.
                }
            })
    }

    #[cfg(test)]
    pub(in crate::types) fn register_issued(
        &mut self,
        registration: super::WitnessRegistration,
    ) -> Result<Context, Invalid> {
        let (scope, base, witness, dependencies) = registration.into_parts();
        if witness.issuer_owner() != self.owner
            || !self.valid_context(&base)
            || self.registered.contains_key(&scope.identity())
            || base
                .0
                .iter()
                .any(|coordinate| coordinate.paired.origin().is_none())
        {
            return Err(Invalid::Authority);
        }
        let modules = paired::owned_modules(witness.view());
        if self
            .registered
            .values()
            .any(|existing| !existing.modules.is_disjoint(&modules))
        {
            return Err(Invalid::Authority);
        }
        let coordinate = Arc::new(Coordinate {
            owner: self.owner,
            id: scope.identity(),
            witness: witness.view().module.clone(),
            contract: witness.canonical_contract(),
            dependencies,
            modules,
            paired: paired::Paired::issued(witness, scope.identity()),
        });
        let mut context = base;
        context.0.push(coordinate.clone());
        self.registered.insert(coordinate.id, coordinate.clone());
        self.contexts.insert(coordinate.id, context.clone());
        Ok(context)
    }

    /// Algebra fixture only; this does not issue materialization authority.
    #[cfg(test)]
    pub fn append(&mut self, base: &Context, contract: Type, witness: String) -> Context {
        self.append_at(base, Scope::default().identity(), contract, witness)
    }

    #[cfg(test)]
    fn append_at(
        &mut self,
        base: &Context,
        id: ScopeId,
        contract: Type,
        witness: String,
    ) -> Context {
        assert!(base.0.iter().all(|c| c.witness != witness && c.id != id));
        assert!(
            base.0.iter().all(|c| c.owner == self.owner),
            "checker-owned telescope"
        );
        self.append_known(
            base,
            Coordinate {
                owner: self.owner,
                id,
                contract,
                witness: witness.clone(),
                modules: BTreeSet::from([witness]),
                dependencies: BTreeSet::new(),
                paired: paired::Paired::default(),
            },
        )
    }

    #[cfg(test)]
    fn append_known(&mut self, base: &Context, mut coordinate: Coordinate) -> Context {
        assert!(
            !self.registered.contains_key(&coordinate.id),
            "new lexical scope registration"
        );
        coordinate.dependencies.extend(
            base.0
                .iter()
                .filter(|c| witness_scope_occurs(&coordinate.contract, &c.witness))
                .map(|c| c.id),
        );
        let mut context = base.clone();
        let coordinate = Arc::new(coordinate);
        self.registered.insert(coordinate.id, coordinate.clone());
        let id = coordinate.id;
        context.0.push(coordinate);
        self.contexts.insert(id, context.clone());
        context
    }

    pub fn owner(&self) -> ScopeId {
        self.owner
    }

    /// Raw algebra fixture registration. Issued-witness tests instead consume
    /// the exact checker-issued existential pair through `register_issued`.
    #[cfg(test)]
    pub fn register_view(
        &mut self,
        base: &Context,
        scope: Scope,
        contract: Type,
        view: &Interface,
    ) -> Result<Context, Invalid> {
        if !base.belongs_to(self.owner) || self.registered.contains_key(&scope.identity()) {
            return Err(Invalid::Authority);
        }
        let mut modules = BTreeSet::new();
        let mut pending = vec![view];
        while let Some(current) = pending.pop() {
            modules.insert(current.module.clone());
            pending.extend(current.modules.iter().filter_map(|(name, child)| {
                (child.module == format!("{}.{name}", current.module)).then_some(child)
            }));
        }
        let references = support::dependencies(view);
        let dependencies = base
            .0
            .iter()
            .filter(|coordinate| {
                references
                    .iter()
                    .any(|entity| coordinate.modules.contains(&entity.module))
            })
            .map(|coordinate| coordinate.id)
            .collect();
        if self
            .registered
            .values()
            .any(|existing| !existing.modules.is_disjoint(&modules))
        {
            return Err(Invalid::Authority);
        }
        Ok(self.append_known(
            base,
            Coordinate {
                owner: self.owner,
                id: scope.identity(),
                contract,
                witness: view.module.clone(),
                modules,
                dependencies,
                paired: paired::Paired::default(),
            },
        ))
    }

    #[cfg(test)]
    pub fn registered_context(&self, scope: ScopeId) -> Option<Context> {
        self.contexts.get(&scope).cloned()
    }

    #[cfg(test)]
    pub fn scope_of_module(&self, module: &str) -> Option<ScopeId> {
        self.registered
            .values()
            .find(|coordinate| coordinate.modules.contains(module))
            .map(|coordinate| coordinate.id)
    }
}

impl Context {
    pub fn belongs_to(&self, owner: ScopeId) -> bool {
        self.0.iter().all(|coordinate| coordinate.owner == owner)
    }

    pub fn contains(&self, id: ScopeId) -> bool {
        self.0.iter().any(|coordinate| coordinate.id == id)
    }

    #[cfg(test)]
    pub fn common(&self, other: &Self) -> Result<Self, Invalid> {
        if self
            .0
            .first()
            .zip(other.0.first())
            .is_some_and(|(a, b)| a.owner != b.owner)
        {
            return Err(Invalid::Authority);
        }
        let keep = self.ids().filter(|id| other.contains(*id)).collect();
        Ok(self.restrict(&keep))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn ids(&self) -> impl Iterator<Item = ScopeId> + '_ {
        self.0.iter().map(|c| c.id)
    }

    /// Greatest dependency-closed subset of `keep`; excluded coordinates are
    /// never reintroduced to satisfy a later coordinate's dependencies.
    pub fn restrict(&self, keep: &BTreeSet<ScopeId>) -> Self {
        let mut retained = BTreeSet::new();
        Self(
            self.0
                .iter()
                .filter(|c| {
                    if keep.contains(&c.id) && c.dependencies.is_subset(&retained) {
                        retained.insert(c.id);
                        true
                    } else {
                        false
                    }
                })
                .cloned()
                .collect(),
        )
    }
}

/// Total, injective, contract-preserving order embedding. Private fields and
/// constructor prevent raw pair lists (in particular swaps) from becoming views.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::types) struct Embedding {
    source: Context,
    target: Context,
    image: Vec<usize>,
}

impl Embedding {
    pub(in crate::types) fn same_carrier(&self, other: &Self) -> bool {
        self.image == other.image
            && self.source.0.len() == other.source.0.len()
            && self.target.0.len() == other.target.0.len()
            && self
                .source
                .0
                .iter()
                .zip(&other.source.0)
                .all(|(a, b)| Arc::ptr_eq(a, b))
            && self
                .target
                .0
                .iter()
                .zip(&other.target.0)
                .all(|(a, b)| Arc::ptr_eq(a, b))
    }

    fn checked(source: Context, target: Context, image: Vec<usize>) -> Result<Self, Invalid> {
        if image.len() != source.len() || image.iter().any(|i| *i >= target.len()) {
            return Err(Invalid::Domain);
        }
        if let Some(owner) = source
            .0
            .first()
            .or_else(|| target.0.first())
            .map(|c| c.owner)
            && (!source.belongs_to(owner) || !target.belongs_to(owner))
        {
            return Err(Invalid::Authority);
        }
        if image.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(Invalid::Order);
        }
        let view = Self {
            source,
            target,
            image,
        };
        for (index, from) in view.source.0.iter().enumerate() {
            let to = &view.target.0[view.image[index]];
            let prefix = Self {
                source: Context(view.source.0[..index].to_vec()),
                target: view.target.clone(),
                image: view.image[..index].to_vec(),
            };
            if prefix.transport_checked(&from.contract)? != to.contract {
                return Err(Invalid::Contract);
            }
            // `extend`, not `collect`: collecting compiles a sort of `ScopeId` into the wasm compiler.
            let mut mapped = BTreeSet::new();
            mapped.extend(
                view.source.0[..index]
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| from.dependencies.contains(&c.id))
                    .map(|(i, _)| view.target.0[view.image[i]].id),
            );
            if mapped != to.dependencies {
                return Err(Invalid::Dependency);
            }
        }
        Ok(view)
    }

    pub fn source(&self) -> &Context {
        &self.source
    }

    pub fn target(&self) -> &Context {
        &self.target
    }

    pub fn mapped(&self, id: ScopeId) -> Option<ScopeId> {
        self.source
            .0
            .iter()
            .position(|coordinate| coordinate.id == id)
            .map(|i| self.target.0[self.image[i]].id)
    }

    #[cfg(test)]
    pub fn image_context(&self) -> Context {
        self.target
            .restrict(&self.image.iter().map(|i| self.target.0[*i].id).collect())
    }

    /// Change only the ambient context, retaining the same actual image.
    #[cfg(test)]
    pub fn in_context(&self, target: &Context) -> Result<Self, Invalid> {
        let image = self
            .image
            .iter()
            .map(|i| {
                target
                    .0
                    .iter()
                    .position(|c| c.id == self.target.0[*i].id)
                    .ok_or(Invalid::Domain)
            })
            .collect::<Result<_, _>>()?;
        Self::checked(self.source.clone(), target.clone(), image)
    }

    /// Hygienic copying replaces bound handles in place. It does not reorder
    /// ancestors or install any new nominal/type authority.
    pub fn copy_bound(&self, from: ScopeId, to: ScopeId) -> Self {
        if !self.target.contains(from) {
            return self.clone();
        }
        assert!(!self.target.contains(to), "fresh bound handle");
        let mut target = self.target.clone();
        for coordinate in &mut target.0 {
            let coordinate = Arc::make_mut(coordinate);
            if coordinate.id == from {
                coordinate.id = to;
            }
            if coordinate.dependencies.remove(&from) {
                coordinate.dependencies.insert(to);
            }
            coordinate.contract = coordinate.contract.copy_bound_handle(from, to);
            #[cfg(test)]
            coordinate.paired.copy_bound(from, to);
        }
        // A fresh in-place alpha copy preserves totality, injectivity and
        // lexical order. Contracts (including their contextual occurrences)
        // are transported along the same bound-handle renaming above.
        Self {
            source: self.source.clone(),
            target,
            image: self.image.clone(),
        }
    }

    pub fn mapping(&self) -> impl Iterator<Item = (ScopeId, ScopeId)> + '_ {
        self.source
            .0
            .iter()
            .zip(&self.image)
            .map(|(from, index)| (from.id, self.target.0[*index].id))
    }

    pub fn identity(context: &Context) -> Self {
        Self {
            source: context.clone(),
            target: context.clone(),
            image: (0..context.len()).collect(),
        }
    }

    /// Weakening retains the identities of every source coordinate.
    #[cfg(test)]
    pub fn weaken(source: &Context, target: &Context) -> Result<Self, Invalid> {
        let image = source
            .ids()
            .map(|id| {
                target
                    .0
                    .iter()
                    .position(|c| c.id == id)
                    .ok_or(Invalid::Domain)
            })
            .collect::<Result<_, _>>()?;
        Self::checked(source.clone(), target.clone(), image)
    }

    /// Replace coordinates in place by a contract-equivalent telescope. This
    /// is alpha transport, not an arbitrary permutation constructor.
    #[cfg(test)]
    pub fn align(source: &Context, target: &Context) -> Result<Self, Invalid> {
        Self::checked(source.clone(), target.clone(), (0..source.len()).collect())
    }

    #[cfg(test)]
    pub fn compose(&self, next: &Self) -> Result<Self, Invalid> {
        if self.target != next.source {
            return Err(Invalid::Domain);
        }
        Self::checked(
            self.source.clone(),
            next.target.clone(),
            self.image.iter().map(|i| next.image[*i]).collect(),
        )
    }

    pub fn restrict(&self, keep: &BTreeSet<ScopeId>) -> Self {
        let source = self.source.restrict(keep);
        // `extend`, not `collect`: collecting compiles a sort of `ScopeId` into the wasm compiler.
        let mut ids = BTreeSet::new();
        ids.extend(source.ids());
        let image = self
            .source
            .0
            .iter()
            .zip(&self.image)
            .filter(|(c, _)| ids.contains(&c.id))
            .map(|(_, i)| *i)
            .collect();
        Self::checked(source, self.target.clone(), image).expect("closed restriction")
    }

    /// Pointwise equalizer for same-root equations. Ordered embeddings require
    /// no residual equations, even if the eventual type is an unordered row.
    #[cfg(test)]
    pub fn equalizer(&self, other: &Self) -> Result<Self, Invalid> {
        if self.source != other.source || self.target != other.target {
            return Err(Invalid::Domain);
        }
        let keep = self
            .source
            .0
            .iter()
            .zip(self.image.iter().zip(&other.image))
            .filter(|(_, (a, b))| a == b)
            .map(|(c, _)| c.id)
            .collect();
        Ok(self.restrict(&keep))
    }

    /// The dependency-closed common image in one ambient telescope.
    #[cfg(test)]
    pub fn meet(&self, other: &Self) -> Result<(Self, Self), Invalid> {
        if self.target != other.target {
            return Err(Invalid::Domain);
        }
        let left: BTreeSet<_> = self.image.iter().map(|i| self.target.0[*i].id).collect();
        let right: BTreeSet<_> = other.image.iter().map(|i| self.target.0[*i].id).collect();
        let common = self
            .target
            .restrict(&left.intersection(&right).copied().collect());
        let ambient = Self::weaken(&common, &self.target)?;
        Ok((
            ambient.inverse_image_in(self)?,
            ambient.inverse_image_in(other)?,
        ))
    }

    #[cfg(test)]
    fn inverse_image_in(&self, other: &Self) -> Result<Self, Invalid> {
        if self.target != other.target {
            return Err(Invalid::Domain);
        }
        let image = self
            .image
            .iter()
            .map(|i| {
                other
                    .image
                    .iter()
                    .position(|j| i == j)
                    .ok_or(Invalid::Domain)
            })
            .collect::<Result<_, _>>()?;
        Self::checked(self.source.clone(), other.source.clone(), image)
    }

    /// Inversion is defined only on the dependency-closed image, never on
    /// arbitrary ambient coordinates. The solver checks support before using it.
    #[cfg(test)]
    pub fn inverse_on_image(&self) -> Self {
        let keep = self.image.iter().map(|i| self.target.0[*i].id).collect();
        let image = self.target.restrict(&keep);
        Self::checked(image, self.source.clone(), (0..self.source.len()).collect())
            .expect("inverse of a checked order embedding")
    }

    #[cfg(test)]
    pub fn transport(&self, ty: &Type) -> Type {
        self.transport_checked(ty)
            .expect("checked witness transport")
    }

    fn transport_names(&self, ty: &Type, count: usize) -> Type {
        let mappings: Vec<_> = self.source.0[..count]
            .iter()
            .enumerate()
            .map(|(i, from)| {
                (
                    from.witness.as_str(),
                    self.target.0[self.image[i]].witness.as_str(),
                )
            })
            .collect();
        WitnessScope {
            from: "",
            to: "",
            mappings: &mappings,
            shadow: None,
            changed: false,
        }
        .ty(ty)
    }
}

pub(super) mod paired;
#[cfg(test)]
mod projection;
mod transport;
#[cfg(test)]
pub(in crate::types) use projection::ProjectionError;

#[cfg(test)]
mod tests;
