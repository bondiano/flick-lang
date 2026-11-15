// Immutable kind evidence, separate from contextual type projection.

use super::convert::TyVar;
use super::env::Env;
use super::kinds::KindStore;
use super::module_contract::context::{Invalid, ProjectionError};
use super::module_contract::kinded::{self, Bound, Identities, Incomplete, Kinding};
use super::regions;
use super::ty::Binder;

// Phase M: retained for the next ownership/witness checkpoint.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(in crate::types) struct Read<'a, 'b> {
    store: &'a Store,
    kinds: &'a KindStore,
    env: &'a Env<'b>,
    variables: &'a [TyVar],
    rigid_kinds: &'a [(u32, Kind)],
    regions: &'a [regions::Scope],
}

impl Store {
    /// Raw canonical observation. This never projects a contextual occurrence or queries a
    /// constructor kind, so kind lookup cannot recurse through its projector.
    fn raw_solution(&self, root: u32) -> Option<&Type> {
        match self.metas.get(root as usize)? {
            MetaState::Bound(ty) => Some(ty),
            MetaState::Open { .. } => None,
        }
    }
}

#[allow(dead_code)] // Phase M: retained for the next ownership/witness checkpoint
impl<'s, 'e> Read<'s, 'e> {
    /// Deep positional observation for live retained carriers, not a snapshot
    /// of the solutions that happened to exist at witness registration.
    pub(in crate::types) fn observing(&self) -> ZonkRead<'s, 'e> {
        ZonkRead(*self)
    }

    /// Unlike the legacy Store-only reader this requires the positional kind
    /// of the root and reports missing evidence instead of returning a bound
    /// contextual occurrence as though it had been normalized.
    pub(in crate::types) fn head<'a>(
        &self,
        ty: &'a Type,
        expected: &Kind,
    ) -> Result<Cow<'a, Type>, ProjectionError>
    where
        's: 'a,
    {
        self.head_in(ty, expected, &[], &[])
    }

    fn head_in<'a>(
        &self,
        ty: &'a Type,
        expected: &Kind,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Cow<'a, Type>, ProjectionError>
    where
        's: 'a,
    {
        let store: &'s Store = self.store;
        let mut head = Cow::Borrowed(ty);
        loop {
            match head.as_ref() {
                Type::Meta(meta) if meta.lexical().is_some() => {
                    let mappings = store
                        .lexical_mappings(meta)
                        .map_err(|()| ProjectionError::Invalid(Invalid::Authority))?;
                    let Some(solution) = store.raw_solution(meta.root()) else {
                        return Ok(head);
                    };
                    head = Cow::Owned(lexical::transport_type(solution, &mappings));
                }
                Type::Meta(_) => {
                    let (root, view) = self
                        .store
                        .occurrence(&head)
                        .map_err(|()| ProjectionError::Invalid(Invalid::Authority))?;
                    let Some(solution) = store.raw_solution(root) else {
                        return Ok(head);
                    };
                    if view.source().len() == 0 {
                        head = Cow::Borrowed(solution);
                    } else {
                        let canonical_kind = store.meta_contexts[root as usize]
                            .kind
                            .as_ref()
                            .unwrap_or(expected);
                        head = Cow::Owned(view.project_type(
                            solution,
                            canonical_kind,
                            self,
                            binders,
                            bound,
                        )?);
                    }
                }
                Type::Skolem(rigid) => match store.equal(*rigid) {
                    Some(solution) => head = Cow::Borrowed(solution),
                    None => return Ok(head),
                },
                _ => return Ok(head),
            }
        }
    }

    pub(in crate::types) fn zonk(&self, ty: &Type, expected: &Kind) -> Result<Type, Incomplete> {
        let mut normalize = Normalize {
            read: *self,
            failure: None,
        };
        let mapped = kinded::map(ty, expected, &ZonkRead(*self), &mut normalize)?;
        match normalize.failure {
            Some(error) => Err(error),
            None => Ok(mapped),
        }
    }

    fn resolved_label(&self, label: &Elem) -> Result<Elem, Incomplete> {
        self.resolved_label_in(label, &[], &[])
    }

    fn resolved_label_in(
        &self,
        label: &Elem,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Elem, Incomplete> {
        use super::elem::Atom;
        let atom = match label {
            Elem::Provided(inner) => {
                return self
                    .resolved_label_in(inner, binders, bound)
                    .map(|label| Elem::Provided(Box::new(label)));
            }
            Elem::Capture(atom) | Elem::Error(atom) | Elem::Service(atom) => atom,
            Elem::Named(_) | Elem::Live => return Ok(label.clone()),
        };
        let ty = match atom {
            Atom::Meta(meta) => Type::Meta(meta.clone()),
            Atom::Var(_) | Atom::Skolem(_) => return Ok(label.clone()),
        };
        let kind = self.scoped_head_kind(&ty, binders, bound)?;
        let head = self
            .head_in(&ty, &kind, binders, bound)
            .map_err(observation_error)?;
        Ok(relabel(label, &head))
    }

    fn variable_kind(&self, name: &str, binders: &[Binder]) -> Option<Kind> {
        binders
            .iter()
            .rev()
            .find(|binder| binder.name == name)
            .map(|binder| binder.kind.clone())
            .or_else(|| {
                self.variables
                    .iter()
                    .rev()
                    .find(|variable| variable.name == name)
                    .map(|variable| variable.kind.clone())
            })
    }

    fn rigid_kind(&self, rigid: u32) -> Option<Kind> {
        self.rigid_kinds
            .iter()
            .rev()
            .find(|(id, _)| *id == rigid)
            .map(|(_, kind)| kind.clone())
            .or_else(|| {
                self.regions
                    .iter()
                    .any(|scope| scope.region == rigid)
                    .then_some(Kind::Region)
            })
    }

    fn root_kind(
        &self,
        ty: &Type,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Kind, Incomplete> {
        if let Type::Meta(meta) = ty
            && meta.lexical().is_some()
        {
            let mappings = self
                .store
                .lexical_mappings(meta)
                .map_err(|()| Incomplete::InvalidOccurrence(Invalid::Authority))?;
            let context = &self.store.meta_contexts[meta.root() as usize];
            let canonical = if let Some(kind) = &context.kind {
                kind.clone()
            } else if let Some(solution) = self.store.raw_solution(meta.root()) {
                self.scoped_head_kind(solution, binders, bound)?
            } else {
                return Err(Incomplete::Head(ty.clone()));
            };
            return self.checked_kind(&lexical::transport_kind(&canonical, &mappings));
        }
        let (root, view) = self
            .store
            .occurrence(ty)
            .map_err(|()| Incomplete::InvalidOccurrence(Invalid::Authority))?;
        let context = &self.store.meta_contexts[root as usize];
        let canonical = if let Some(kind) = &context.kind {
            kind.clone()
        } else if let Some(solution) = self.store.raw_solution(root) {
            // Raw shape observation is not a projection. A bare Row remains
            // unknown here: its payload does not tell us its row kind.
            self.scoped_head_kind(solution, binders, bound)?
        } else {
            return Err(Incomplete::Head(ty.clone()));
        };
        view.transport_kind_checked(&self.checked_kind(&canonical)?)
            .map_err(Incomplete::InvalidOccurrence)
    }
}

struct Normalize<'s, 'e> {
    read: Read<'s, 'e>,
    failure: Option<Incomplete>,
}

impl Identities for Normalize<'_, '_> {
    fn ty(&mut self, ty: &Type, _: &[Bound]) -> Type {
        ty.clone()
    }
    fn kind(&mut self, kind: &Kind, _: &[Bound]) -> Kind {
        kind.clone()
    }
    fn semantic_label(&mut self, label: &Elem, _: &[Bound]) -> Elem {
        match self.read.resolved_label(label) {
            Ok(mapped) => mapped,
            Err(error) => {
                self.failure = Some(error);
                label.clone()
            }
        }
    }
    fn finish(&mut self, ty: Type, expected: &Kind, _: &[Bound]) -> Type {
        match ty {
            Type::Record(row) => Type::Record(flatten_row(row, false)),
            Type::Row(row) => {
                let row = flatten_row(row, !matches!(expected, Kind::Row(_)));
                if row.entries.is_empty()
                    && let Some(tail) = &row.tail
                {
                    return (**tail).clone();
                }
                Type::Row(row)
            }
            ty @ (Type::Union(..) | Type::Without { .. }) => {
                let mut normalized = Norm::default();
                Store::collect_row_with(
                    &ty,
                    &mut normalized,
                    &Clone::clone,
                    !matches!(expected, Kind::Row(_)),
                );
                normalized.sort();
                normalized.entries.dedup();
                normalized.into_type()
            }
            ty => ty,
        }
    }
}

fn flatten_row(mut row: Row, semantic: bool) -> Row {
    if semantic {
        for (label, value) in &mut row.entries {
            *label = relabel(label, value);
        }
    }
    while let Some(Type::Row(inner)) = row.tail.as_deref() {
        row.entries.extend(inner.entries.iter().cloned());
        row.tail = inner.tail.clone();
    }
    row.sort();
    row.entries.dedup();
    row
}

fn observation_error(error: ProjectionError) -> Incomplete {
    match error {
        ProjectionError::Incomplete(incomplete) => incomplete,
        ProjectionError::Invalid(error) => Incomplete::InvalidOccurrence(error),
    }
}

#[allow(dead_code)] // Phase M: retained for the next ownership/witness checkpoint
pub(in crate::types) struct ZonkRead<'s, 'e>(Read<'s, 'e>);

impl Kinding for ZonkRead<'_, '_> {
    fn observe_label<'a>(
        &'a self,
        label: &'a Elem,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Cow<'a, Elem>, Incomplete> {
        self.0
            .resolved_label_in(label, binders, bound)
            .map(Cow::Owned)
    }

    fn scoped_head_kind(
        &self,
        ty: &Type,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Kind, Incomplete> {
        self.0.scoped_head_kind(ty, binders, bound)
    }

    fn observe<'a>(
        &'a self,
        ty: &'a Type,
        expected: &Kind,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Cow<'a, Type>, Incomplete> {
        self.0
            .head_in(ty, expected, binders, bound)
            .map_err(observation_error)
    }

    fn resolve_kind(&self, kind: &Kind) -> Kind {
        self.0.resolve_kind(kind)
    }
    fn checked_kind(&self, kind: &Kind) -> Result<Kind, Incomplete> {
        self.0.checked_kind(kind)
    }
    fn head_kind(&self, ty: &Type, binders: &[Binder]) -> Result<Kind, Incomplete> {
        self.0.head_kind(ty, binders)
    }
}

impl Kinding for Read<'_, '_> {
    fn scoped_head_kind(
        &self,
        ty: &Type,
        binders: &[Binder],
        bound: &[Bound],
    ) -> Result<Kind, Incomplete> {
        match ty {
            Type::Con { module, name } => {
                for frame in bound.iter().rev() {
                    let path = format!("$exists:{}", frame.binder);
                    if module != &path
                        && !module
                            .strip_prefix(&path)
                            .is_some_and(|suffix| suffix.starts_with('.'))
                    {
                        continue;
                    }
                    let Type::Con {
                        module: contract_module,
                        name: contract_name,
                    } = &frame.contract
                    else {
                        return Err(Incomplete::Head(frame.contract.clone()));
                    };
                    let definition = self
                        .env
                        .type_def(contract_module, contract_name)
                        .ok_or_else(|| Incomplete::Head(frame.contract.clone()))?;
                    let signature;
                    let signature = match &definition.form {
                        Form::ModuleContract { requirements } => {
                            signature = super::env::ModuleSignature::Structure {
                                requirements: requirements.clone(),
                            };
                            &signature
                        }
                        Form::ModuleSignature { signature } => signature,
                        _ => return Err(Incomplete::Head(frame.contract.clone())),
                    };
                    return signature
                        .header_kind(&path, module, name)
                        .ok_or_else(|| Incomplete::Head(ty.clone()))
                        .and_then(|kind| self.checked_kind(&kind));
                }
                self.head_kind(ty, binders)
            }
            Type::App { head, .. } => {
                match self.checked_kind(&self.scoped_head_kind(head, binders, bound)?)? {
                    Kind::Arrow(_, result) => Ok(*result),
                    kind @ Kind::Meta(_) => Err(Incomplete::Kind(kind)),
                    kind => Err(Incomplete::NotFunction(kind)),
                }
            }
            Type::Meta(_) => self.root_kind(ty, binders, bound),
            Type::Without { row, .. } => self.scoped_head_kind(row, binders, bound),
            _ => self.head_kind(ty, binders),
        }
    }

    fn resolve_kind(&self, kind: &Kind) -> Kind {
        self.kinds.resolve_deep(kind)
    }

    fn checked_kind(&self, kind: &Kind) -> Result<Kind, Incomplete> {
        self.kinds
            .resolve_deep_in(kind, &|context| self.store.valid_witness_context(context))
            .map_err(Incomplete::InvalidOccurrence)
    }

    fn head_kind(&self, ty: &Type, binders: &[Binder]) -> Result<Kind, Incomplete> {
        let kind = match ty {
            Type::Con { module, name } => self.env.constructor_kind(module, name),
            Type::Var(name) => self.variable_kind(name, binders),
            Type::Skolem(rigid) => self.rigid_kind(*rigid),
            Type::Meta(_) => return self.root_kind(ty, binders, &[]),
            Type::App { head, .. } => match self.checked_kind(&self.head_kind(head, binders)?)? {
                Kind::Arrow(_, result) => Some(*result),
                kind @ Kind::Meta(_) => return Err(Incomplete::Kind(kind)),
                kind => return Err(Incomplete::NotFunction(kind)),
            },
            Type::Fun { .. }
            | Type::Tuple(_)
            | Type::Record(_)
            | Type::Forall { .. }
            | Type::Exists { .. }
            | Type::Captured { .. }
            | Type::Bounded { .. } => Some(Kind::Type),
            Type::Symbol(_) => Some(Kind::Symbol),
            Type::Row(_) | Type::Error => None,
            Type::Union(a, b) => {
                let a = self.head_kind(a, binders)?;
                let b = self.head_kind(b, binders)?;
                (self.checked_kind(&a)? == self.checked_kind(&b)?).then_some(a)
            }
            Type::Without { row, .. } => return self.head_kind(row, binders),
        };
        kind.ok_or_else(|| Incomplete::Head(ty.clone()))
            .and_then(|kind| self.checked_kind(&kind))
    }
}

impl<'b> Checker<'b> {
    #[allow(dead_code)] // Phase M: retained for the next ownership/witness checkpoint
    pub(in crate::types) fn read_types(&self) -> Read<'_, 'b> {
        Read {
            store: &self.store,
            kinds: &self.kinds,
            env: &self.env,
            variables: &self.cx.tyvars,
            rigid_kinds: &self.cx.regions.rigid_kinds,
            regions: &self.cx.regions.scopes,
        }
    }
}

include!("read/evidence_tests.rs");
