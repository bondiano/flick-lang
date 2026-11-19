// Canonical root contexts and checked occurrence recognition.

use super::module_contract::context::{Context, Embedding, Scopes};
#[cfg(test)]
use super::ty::Scope;
use super::ty::{MetaOccurrence, ScopeId};
#[cfg(test)]
use crate::iface::Interface;

#[derive(Debug, Clone, Default)]
struct MetaContext {
    canonical: Context,
    permitted: Context,
    kind: Option<Kind>,
}

impl Store {
    pub(in crate::types) fn solver_root(&self, ty: &Type) -> Option<u32> {
        let (root, view) = self.occurrence(ty).ok()?;
        (view.source().issued() && view.target().issued()).then_some(root)
    }

    pub(in crate::types) fn variable(&self, root: u32) -> Type {
        self.at(
            root,
            Embedding::identity(&self.meta_contexts[root as usize].permitted),
        )
    }

    pub(in crate::types) fn note_kind(&mut self, ty: &Type, kind: &Kind) {
        if let Some(root) = self.solver_root(ty)
            && self.meta_contexts[root as usize].kind.is_none()
        {
            let mut context = self.meta_contexts[root as usize].clone();
            context.kind = Some(kind.clone());
            self.write_context(root, context);
        }
    }

    pub(in crate::types) fn fresh_row_beside(&mut self, root: u32) -> Type {
        let ty = self.allocate(
            self.meta_level(root).unwrap_or(self.level),
            self.active_context.clone(),
            self.meta_contexts[root as usize].kind.clone(),
        );
        self.mark_like(ty.meta_root().expect("fresh row"), root);
        ty
    }
    fn binding_view(&self, ty: &Type) -> Result<(u32, Embedding), ()> {
        let (root, view) = self.occurrence(ty)?;
        if !view.source().issued() || !view.target().issued() {
            return Err(());
        }
        let ambient = view.in_context(&self.active_context).unwrap_or(view);
        Ok((root, ambient))
    }

    pub(in crate::types) fn valid_witness_context(&self, context: &Context) -> bool {
        self.scopes.valid_context(context)
    }

    pub(in crate::types) fn witness_context(&self) -> Context {
        self.active_context.clone()
    }

    pub(in crate::types) fn register_issued_witness(
        &mut self,
        registration: super::module_contract::WitnessRegistration,
    ) -> Result<Context, super::module_contract::context::Invalid> {
        self.scopes.register_issued(registration)
    }

    fn allocate(&mut self, level: u32, context: Context, kind: Option<Kind>) -> Type {
        let root = id(self.metas.len());
        self.metas.push(MetaState::Open { level });
        let view = Embedding::identity(&context);
        self.meta_contexts.push(MetaContext {
            canonical: context.clone(),
            permitted: context,
            kind,
        });
        self.at(root, view)
    }

    fn at(&self, root: u32, view: Embedding) -> Type {
        if self.meta_contexts[root as usize].canonical.len() == 0 {
            Type::meta(root)
        } else {
            Type::Meta(MetaOccurrence::new(root, self.scopes.owner(), view))
        }
    }

    pub(in crate::types) fn valid_occurrence(&self, ty: &Type) -> bool {
        self.occurrence(ty).is_ok()
    }

    fn occurrence(&self, ty: &Type) -> Result<(u32, Embedding), ()> {
        let Type::Meta(meta) = ty else { return Err(()) };
        let root = meta.root();
        let context = self.meta_contexts.get(root as usize).ok_or(())?;
        if meta.legacy_root().is_some() {
            if context.canonical.len() != 0 {
                return Err(());
            }
            return Ok((root, Embedding::identity(&context.canonical)));
        }
        let occurrence = meta.contextual().ok_or(())?;
        let source = occurrence.renaming().source();
        if !occurrence.is_type()
            || occurrence.owner() != self.scopes.owner()
            || !self.scopes.valid_context(occurrence.renaming().target())
            || !self.scopes.valid_context(source)
            || !self.scopes.valid_context(&context.canonical)
            || context.canonical.restrict(&source.ids().collect()) != *source
            || context.permitted.ids().any(|scope| !source.contains(scope))
        {
            return Err(());
        }
        Ok((
            root,
            occurrence
                .renaming()
                .restrict(&context.permitted.ids().collect()),
        ))
    }

    fn write_context(&mut self, root: u32, context: MetaContext) {
        if !self.transactions.is_empty() {
            self.undo.push(Undo::Context(
                root,
                self.meta_contexts[root as usize].clone(),
            ));
        }
        self.meta_contexts[root as usize] = context;
    }

    pub(in crate::types) fn witness_authority(&self) -> ScopeId {
        self.scopes.owner()
    }

    #[cfg(test)]
    fn register_scope(
        &mut self,
        scope: Scope,
        contract: Type,
        view: &Interface,
    ) -> Result<Context, ()> {
        self.scopes
            .register_view(&self.active_context, scope, contract, view)
            .map_err(|_| ())
    }

    pub(in crate::types) fn enter_context(&mut self, context: Context) {
        assert!(
            context.belongs_to(self.scopes.owner()),
            "checker-owned context"
        );
        self.active_context = context;
    }
}

impl Checker<'_> {
    /// Deferred constraints can be solved after leaving their lexical block.
    /// The empty ambient context alone therefore does not make a write closed.
    pub(super) fn witness_free_binding(&self, ty: &Type) -> bool {
        let ambient = self.store.witness_context();
        if ambient.len() != 0 && ambient.issued() {
            return false;
        }
        let scoped = self.named_witnesses.scoped_modules();
        if scoped.is_empty() {
            return true;
        }
        !ty.contains(&mut |ty| matches!(ty, Type::Meta(meta) if meta.contextual().is_some() && self.store.solver_root(ty).is_some()))
            && !scoped.iter().any(|witness| super::module_contract::witness_scope_occurs(ty, witness))
    }

    pub(in crate::types) fn enter_witness_context(&mut self, context: Context) {
        self.store.enter_context(context);
    }

    /// Transport retained inference occurrences when a closed binder is
    /// reopened. Nominal names and inference views follow the same issued pair.
    pub(in crate::types) fn open_witness_body(
        &mut self,
        scope: super::ty::Scope,
        binder: &str,
        body: &Type,
        target: &Context,
    ) -> Result<Type, ()> {
        let mut contexts = Vec::new();
        body.visit(&mut |ty| {
            if let Type::Meta(meta) = ty
                && let Some(meta) = meta.contextual()
                && meta.renaming().target().contains(scope.identity())
                && !contexts.contains(meta.renaming().target())
            {
                contexts.push(meta.renaming().target().clone());
            }
        });
        let mut body = self
            .read_types()
            .head(body, &Kind::Type)
            .map_err(|_| ())?
            .into_owned();
        for source in contexts {
            let view = self
                .align_witness_contexts(&source, target)
                .map_err(|_| ())?;
            body = view
                .project_type(&body, &Kind::Type, &self.read_types(), &[], &[])
                .map_err(|_| ())?;
        }
        let witness = target
            .ids()
            .last()
            .and_then(|id| target.witness(id))
            .ok_or(())?;
        Ok(super::module_contract::open_existential_type(
            &body, binder, witness,
        ))
    }

    pub(in crate::types) fn bind_occurrence(&mut self, occurrence: &Type, ty: &Type) -> Unified {
        fn effective(store: &Store, ty: &Type) -> Type {
            if let Type::Meta(_) = ty
                && let Ok((root, view)) = store.binding_view(ty)
            {
                return store.at(root, view);
            }
            ty.map_children(&mut |child| effective(store, child))
        }
        self.inference_trial(|this| {
            if let Some(root) = occurrence.legacy_meta_root()
                && this.witness_free_binding(ty)
            {
                return this.bind_inner(root, ty);
            }
            let fail = || Mismatch::Types(occurrence.clone(), ty.clone());
            let (root, view) = this.store.binding_view(occurrence).map_err(|()| fail())?;
            let expected = this.store.meta_contexts[root as usize]
                .kind
                .clone()
                .unwrap_or(Kind::Type);
            let mut ty = this.read_types().zonk(ty, &expected).map_err(|_| fail())?;
            if let Type::Meta(_) = ty
                && let Ok((other, other_view)) = this.store.binding_view(&ty)
                && root == other
            {
                let common = view.equalizer(&other_view).map_err(|_| fail())?;
                let mut context = this.store.meta_contexts[root as usize].clone();
                context.permitted = common.source().clone();
                this.store.write_context(root, context);
                return Ok(());
            }
            let allowed = view.image_context();
            // Prune younger open roots before transporting the candidate into
            // the canonical context. Later solutions must respect this meet.
            let mut occurrences = Vec::new();
            ty.visit(&mut |ty| {
                if matches!(ty, Type::Meta(_)) {
                    occurrences.push(ty.clone());
                }
            });
            for occurrence in occurrences {
                let (other, other_view) =
                    this.store.binding_view(&occurrence).map_err(|()| fail())?;
                if root == other {
                    return Err(Mismatch::Occurs(ty));
                }
                let common = allowed
                    .common(&other_view.image_context())
                    .map_err(|_| fail())?;
                let keep = other_view
                    .mapping()
                    .filter_map(|(from, to)| common.contains(to).then_some(from))
                    .collect();
                let mut context = this.store.meta_contexts[other as usize].clone();
                context.permitted = context.permitted.restrict(&keep);
                this.store.write_context(other, context);
            }
            ty = effective(&this.store, &ty);
            allowed
                .type_dependencies(
                    &ty,
                    &expected,
                    &this.read_types().observing(),
                    &this.named_witnesses.scoped_modules(),
                )
                .map_err(|_| fail())?;
            let canonical = view
                .inverse_on_image()
                .project_type(&ty, &expected, &this.read_types(), &[], &[])
                .map_err(|_| fail())?;
            this.bind_inner(root, &canonical)
        })
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use crate::iface::Interface;

    #[test]
    fn contextual_root_solutions_are_lazily_projected_through_head_and_zonk() {
        let mut store = Store::default();
        let register = |store: &mut Store, module: &str| {
            store
                .register_scope(
                    Scope::default(),
                    Type::con("Provider", "S"),
                    &Interface {
                        module: module.into(),
                        ..Interface::default()
                    },
                )
                .expect("owned witness")
        };
        let source = register(&mut store, "A");
        store.enter_context(source.clone());
        let root = store.fresh();
        let inner = store.fresh();
        store.enter_context(Context::default());
        let target = register(&mut store, "X");
        let view = Embedding::align(&source, &target).expect("alpha view");
        let occurrence = view.transport(&root);
        store.set(
            root.meta_root().expect("root"),
            Type::app(inner.clone(), Type::con("A", "Token")),
        );
        store.set(inner.meta_root().expect("inner"), Type::con("A", "Wrapper"));
        let expected = Type::app(Type::con("X", "Wrapper"), Type::con("X", "Token"));
        assert_eq!(store.zonk(&occurrence), expected);
        let (head, args) = store.spine(&occurrence);
        assert_eq!(*head, Type::con("X", "Wrapper"));
        assert_eq!(*args[0], Type::con("X", "Token"));
        assert!(
            matches!(store.head(&expected), Cow::Borrowed(_)),
            "closed heads stay borrowed"
        );
    }
}
