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
    lexical: LexicalRoot,
}

impl Store {
    pub(in crate::types) fn valid_witness_context(&self, context: &Context) -> bool {
        self.scopes.valid_context(context)
    }

    #[cfg(test)]
    pub(in crate::types) fn witness_context(&self) -> Context {
        self.active_context.clone()
    }

    #[cfg(test)]
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
            lexical: super::ty::LexicalRoot::new(&self.active_lexical),
        });
        self.at(root, view)
    }

    fn at(&self, root: u32, view: Embedding) -> Type {
        if !self.meta_contexts[root as usize]
            .lexical
            .canonical
            .is_empty()
        {
            return Type::Meta(MetaOccurrence::lexical_type(
                root,
                self.scopes.owner(),
                &self.meta_contexts[root as usize].lexical.canonical,
            ));
        }
        if self.meta_contexts[root as usize].canonical.len() == 0 {
            Type::meta(root)
        } else {
            Type::Meta(MetaOccurrence::new(root, self.scopes.owner(), view))
        }
    }

    pub(in crate::types) fn valid_occurrence(&self, ty: &Type) -> bool {
        if let Type::Meta(meta) = ty
            && meta.lexical().is_some()
        {
            return self.lexical_mappings(meta).is_ok();
        }
        self.occurrence(ty).is_ok()
    }

    fn occurrence(&self, ty: &Type) -> Result<(u32, Embedding), ()> {
        let Type::Meta(meta) = ty else { return Err(()) };
        let root = meta.root();
        let context = self.meta_contexts.get(root as usize).ok_or(())?;
        if meta.legacy_root().is_some() {
            if context.canonical.len() != 0 || !context.lexical.canonical.is_empty() {
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

    #[cfg(test)]
    pub(in crate::types) fn enter_context(&mut self, context: Context) {
        assert!(
            context.belongs_to(self.scopes.owner()),
            "checker-owned context"
        );
        self.active_context = context;
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
