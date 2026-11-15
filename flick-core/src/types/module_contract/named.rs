//! Named existential authority. Static/anonymous signature witnesses stay separate.

#[cfg(test)]
use std::collections::BTreeSet;
use std::collections::HashMap;
#[cfg(test)]
use std::fmt;
use std::sync::Arc;

#[cfg(test)]
use super::context::{Context, Embedding, ProjectionError};
#[cfg(test)]
use super::kinded::Incomplete;
use super::{Form, Interface, Kind, Namespace, Type, TypeDef};
use crate::iface::Entity;
#[cfg(test)]
use crate::types::Scope;
use crate::types::ty::carrier::{self, Stamp};
use crate::types::{Checker, ModuleSignature, ScopeId};

#[cfg(test)]
type Actuals = Vec<(ScopeId, Type, Interface)>;

#[derive(Default)]
pub(in crate::types) struct NamedWitnesses {
    templates: HashMap<Entity, Template>,
    issued: HashMap<String, Vec<Arc<Issued>>>,
}

struct Template {
    snapshot: TypeDef,
    revision: Arc<()>,
}

#[derive(Clone)]
struct Issued {
    owner: ScopeId,
    contract: Entity,
    revision: Arc<()>,
    signature: ModuleSignature,
    actual: Interface,
}

#[derive(Clone)]
pub(in crate::types) struct ExistentialWitness(Arc<Issued>);

#[cfg(test)]
#[derive(Debug)]
pub(in crate::types) enum WitnessFailure {
    Invalid(String),
    Projection(ProjectionError),
}

#[cfg(test)]
impl WitnessFailure {
    pub(in crate::types) fn is_pending(&self) -> bool {
        matches!(
            self,
            Self::Projection(ProjectionError::Incomplete(
                Incomplete::Kind(_) | Incomplete::Head(_)
            ))
        )
    }
}

#[cfg(test)]
impl From<String> for WitnessFailure {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}

#[cfg(test)]
impl From<ProjectionError> for WitnessFailure {
    fn from(error: ProjectionError) -> Self {
        Self::Projection(error)
    }
}

#[cfg(test)]
impl From<Incomplete> for WitnessFailure {
    fn from(error: Incomplete) -> Self {
        Self::Projection(ProjectionError::Incomplete(error))
    }
}

#[cfg(test)]
impl fmt::Display for WitnessFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Projection(_) if self.is_pending() => {
                f.write_str("witness view needs more inference evidence")
            }
            Self::Projection(_) => f.write_str("invalid contextual witness projection"),
        }
    }
}

#[cfg(test)]
pub(in crate::types) struct WitnessRegistration {
    scope: Scope,
    base: Context,
    witness: ExistentialWitness,
    dependencies: BTreeSet<ScopeId>,
}

#[cfg(test)]
impl WitnessRegistration {
    pub(in crate::types) fn into_parts(
        self,
    ) -> (Scope, Context, ExistentialWitness, BTreeSet<ScopeId>) {
        (self.scope, self.base, self.witness, self.dependencies)
    }
}

pub(in crate::types) struct WitnessRefresh<'a> {
    old: &'a Interface,
    new: &'a Interface,
}

impl WitnessRefresh<'_> {
    pub(in crate::types) fn views(&self) -> (&Interface, &Interface) {
        (self.old, self.new)
    }
}

impl ExistentialWitness {
    /// Metadata only: use still requires this checker's current-pair validation.
    #[cfg(test)]
    pub(in crate::types) fn issuer_owner(&self) -> ScopeId {
        self.0.owner
    }

    #[cfg(test)]
    pub(in crate::types) fn canonical_contract(&self) -> Type {
        Type::con(&self.0.contract.module, &self.0.contract.name)
    }

    pub(in crate::types) fn view(&self) -> &Interface {
        &self.0.actual
    }
    pub(in crate::types) fn signature(&self) -> &ModuleSignature {
        &self.0.signature
    }

    #[cfg(test)]
    pub(in crate::types) fn replace_actual_for_test(&self, actual: Interface) -> Self {
        Self(Arc::new(Issued {
            actual,
            ..(*self.0).clone()
        }))
    }
}

impl NamedWitnesses {
    fn revision(&self, entity: &Entity, definition: &TypeDef) -> Arc<()> {
        self.templates
            .get(entity)
            .filter(|template| template.snapshot == *definition)
            .map_or_else(|| Arc::new(()), |template| template.revision.clone())
    }

    fn remember(&mut self, entity: Entity, definition: TypeDef, revision: Arc<()>) {
        self.templates.insert(
            entity,
            Template {
                snapshot: definition,
                revision,
            },
        );
    }

    /// Reservation only, not an inference of ownership from a spelling. An
    /// independently issued view cannot write inside another issued root (or
    /// reserve an ancestor that would absorb its existing exact footprint).
    fn overlaps_namespace(&self, witness: &str) -> bool {
        let within = |path: &str, root: &str| {
            path == root
                || path
                    .strip_prefix(root)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        };
        self.issued
            .keys()
            .any(|root| within(witness, root) || within(root, witness))
    }

    /// Identity inventory only; membership does not issue a scope or lease.
    #[cfg(test)]
    fn scoped_modules(&self) -> BTreeSet<String> {
        self.issued
            .values()
            .flatten()
            .flat_map(|issued| super::context::paired::owned_modules(&issued.actual))
            .collect()
    }

    fn issued(&self, witness: &ExistentialWitness) -> bool {
        self.issued
            .get(&witness.view().module)
            .is_some_and(|records| records.iter().any(|record| Arc::ptr_eq(record, &witness.0)))
    }
}

impl Checker<'_> {
    /// Registration for issued-witness tests: refresh the exact issued pair,
    /// then derive support from its live kind-indexed actual carrier.
    #[cfg(test)]
    pub(in crate::types) fn register_existential_scope(
        &mut self,
        scope: Scope,
        witness: &ExistentialWitness,
    ) -> Result<Context, WitnessFailure> {
        let current = self.validate_existential_witness(witness)?;
        let base = self.current_witness_context(&self.store.witness_context())?;
        let read = self.read_types();
        let observing = read.observing();
        let dependencies = base.actual_dependencies(
            current.view(),
            &observing,
            &self.named_witnesses.scoped_modules(),
        )?;
        self.store
            .register_issued_witness(WitnessRegistration {
                scope,
                base,
                witness: current,
                dependencies,
            })
            .map_err(|error| WitnessFailure::Projection(ProjectionError::Invalid(error)))
    }

    #[cfg(test)]
    pub(in crate::types) fn current_witness_context(
        &mut self,
        context: &Context,
    ) -> Result<Context, WitnessFailure> {
        self.prepare_witness_context(context)
            .map(|(context, _)| context)
    }

    #[cfg(test)]
    fn prepare_witness_context(
        &mut self,
        context: &Context,
    ) -> Result<(Context, Actuals), WitnessFailure> {
        if !self.store.valid_witness_context(context) {
            return Err(WitnessFailure::Invalid(
                "witness telescope is no longer registered by this checker".into(),
            ));
        }
        let origins: Vec<_> = context.issued_origins().cloned().collect();
        let mut refreshed = Vec::new();
        for origin in &origins {
            refreshed.push(self.validate_existential_witness(origin)?);
        }
        let read = self.read_types();
        let observing = read.observing();
        // Re-materialize copied/projected actuals through current Store/KindStore
        // evidence before accepting the enclosing telescope.
        let actuals = context.actuals(&refreshed, &observing)?;
        for (_, _, actual) in &actuals {
            self.validate_interface_carrier(actual)?;
        }
        let context = context.refreshed_dependencies(
            &actuals,
            &observing,
            &self.named_witnesses.scoped_modules(),
        )?;
        Ok((context, actuals))
    }

    /// Mathematical embedding equality never substitutes for validating both
    /// live issued carriers and their current materialized actual layouts.
    #[cfg(test)]
    pub(in crate::types) fn align_witness_contexts(
        &mut self,
        source: &Context,
        target: &Context,
    ) -> Result<Embedding, WitnessFailure> {
        let (source, from_actuals) = self.prepare_witness_context(source)?;
        let (target, to_actuals) = self.prepare_witness_context(target)?;
        let view = Embedding::align(&source, &target)
            .map_err(|error| WitnessFailure::Projection(ProjectionError::Invalid(error)))?;
        let read = self.read_types();
        let observing = read.observing();
        for (from, to) in view.mapping() {
            let (_, _, actual) = from_actuals
                .iter()
                .find(|(id, _, _)| *id == from)
                .ok_or_else(|| WitnessFailure::Invalid("missing source actual carrier".into()))?;
            let (_, _, expected) = to_actuals
                .iter()
                .find(|(id, _, _)| *id == to)
                .ok_or_else(|| WitnessFailure::Invalid("missing target actual carrier".into()))?;
            let projected = view.project_actual(actual, &observing)?;
            self.validate_interface_carrier(&projected)?;
            self.validate_interface_carrier(expected)?;
            if projected != *expected {
                return Err(WitnessFailure::Invalid(
                    "witness actual layouts do not align".into(),
                ));
            }
        }
        Ok(view)
    }

    pub(in crate::types) fn materialize_existential_witness(
        &mut self,
        contract: &Type,
        witness: &str,
    ) -> Result<ExistentialWitness, String> {
        let (entity, definition, signature) = self.existential_template(contract)?;
        if let Some(existing) = self
            .named_witnesses
            .issued
            .get(witness)
            .and_then(|records| records.last())
            .cloned()
        {
            if existing.contract != entity {
                return Err("witness namespace belongs to another contract".into());
            }
            return self.validate_existential_witness(&ExistentialWitness(existing));
        }
        if self.named_witnesses.overlaps_namespace(witness)
            || self.env.has_witness_namespace(witness)
        {
            return Err("witness namespace is not fresh".into());
        }
        let revision = self.named_witnesses.revision(&entity, &definition);
        let actual = signature.interface(witness);
        self.validate_interface_carrier(&actual)?;
        self.env.register_witness_types(witness, &actual)?;
        self.named_witnesses
            .remember(entity.clone(), definition, revision.clone());
        let issued = Arc::new(Issued {
            owner: self.store.witness_authority(),
            contract: entity,
            revision,
            signature,
            actual,
        });
        self.named_witnesses
            .issued
            .entry(witness.into())
            .or_default()
            .push(issued.clone());
        Ok(ExistentialWitness(issued))
    }

    fn existential_template(
        &self,
        contract: &Type,
    ) -> Result<(Entity, TypeDef, ModuleSignature), String> {
        let Type::Con { module, name } = contract else {
            return Err("existential contract must be a named type module".into());
        };
        let definition = self
            .env
            .type_def(module, name)
            .cloned()
            .ok_or_else(|| format!("module contract {module}.{name} is unavailable"))?;
        if self.kinds.resolve_deep(&definition.kind) != Kind::Module {
            return Err("existential contract does not have Module kind".into());
        }
        let Form::ModuleContract { requirements } = &definition.form else {
            return Err("existential contract is not a type module contract".into());
        };
        self.validate_definition_carrier(&definition)?;
        let signature = ModuleSignature::Structure {
            requirements: requirements.clone(),
        };
        Ok((
            Entity {
                module: module.clone(),
                namespace: Namespace::Type,
                name: name.clone(),
            },
            definition,
            signature,
        ))
    }

    pub(in crate::types) fn validate_existential_witness(
        &mut self,
        witness: &ExistentialWitness,
    ) -> Result<ExistentialWitness, String> {
        if witness.0.owner != self.store.witness_authority()
            || !self.named_witnesses.issued(witness)
        {
            return Err("existential witness was not issued by this checker".into());
        }
        let current = self.named_witnesses.issued[&witness.view().module]
            .last()
            .expect("issued witness")
            .clone();
        let contract = Type::con(&current.contract.module, &current.contract.name);
        let (entity, definition, signature) = self.existential_template(&contract)?;
        let revision = self.named_witnesses.revision(&entity, &definition);
        let same_carrier = signature == current.signature
            && carrier::signature_stamps(&signature)
                == carrier::signature_stamps(&current.signature);
        self.env.validate_witness_footprint(&current.actual)?;
        self.validate_interface_carrier(&current.actual)?;
        if Arc::ptr_eq(&revision, &current.revision) && same_carrier {
            self.named_witnesses.remember(entity, definition, revision);
            return Ok(ExistentialWitness(current));
        }
        let actual = signature.interface(&current.actual.module);
        self.validate_interface_carrier(&actual)?;
        self.env.refresh_witness_footprint(&WitnessRefresh {
            old: &current.actual,
            new: &actual,
        })?;
        // Publish neither the revision nor its exact issued pair until the
        // complete owned footprint has passed preflight and committed.
        self.named_witnesses
            .remember(entity.clone(), definition, revision.clone());
        let updated = Arc::new(Issued {
            owner: current.owner,
            contract: entity,
            revision,
            signature,
            actual,
        });
        self.named_witnesses
            .issued
            .entry(updated.actual.module.clone())
            .or_default()
            .push(updated.clone());
        Ok(ExistentialWitness(updated))
    }

    fn validate_definition_carrier(&self, definition: &TypeDef) -> Result<(), String> {
        self.validate_stamps(carrier::definition_stamps(definition))
    }

    fn validate_stamps(&self, stamps: Vec<Stamp>) -> Result<(), String> {
        for stamp in stamps {
            let valid = match stamp {
                Stamp::Scope(_) => true, // A lexical handle alone grants no authority.
                Stamp::TypeRoot(root) => self.store.valid_occurrence(&Type::meta(root)),
                Stamp::KindRoot(root) => self.kinds.valid_occurrence(&Kind::meta(root)),
                Stamp::TypeView(meta) => self.store.valid_occurrence(&Type::Meta(meta)),
                Stamp::KindView(meta) => self
                    .kinds
                    .valid_occurrence_in(&Kind::Meta(meta), |context| {
                        self.store.valid_witness_context(context)
                    }),
            };
            if !valid {
                return Err("invalid inference carrier in existential contract".into());
            }
        }
        Ok(())
    }

    fn validate_interface_carrier(&self, interface: &Interface) -> Result<(), String> {
        for export in &interface.exports {
            if let Some(definition) = &export.definition {
                self.validate_definition_carrier(definition)?;
            }
            if let Some(scheme) = &export.scheme {
                self.validate_stamps(carrier::stamps_of_type(scheme))?;
            }
        }
        for support in &interface.support_types {
            self.validate_definition_carrier(&support.definition)?;
        }
        if let Some(signature) = &interface.parameterized {
            self.validate_stamps(carrier::signature_stamps(signature))?;
        }
        for child in interface.modules.values() {
            self.validate_interface_carrier(child)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
