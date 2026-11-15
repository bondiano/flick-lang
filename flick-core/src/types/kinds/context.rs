// Canonical kind roots share witness authority, never the type-root namespace.

use super::module_contract::context::{Context, Embedding};
use super::ty::ScopeId;

#[derive(Debug, Clone, Default)]
struct KindContext {
    canonical: Context,
    permitted: Context,
    lexical: LexicalRoot,
}

#[derive(Debug)]
enum KindUndo {
    Binding(u32, Option<Kind>),
    Context(u32, KindContext),
}

impl KindStore {
    pub(in crate::types) fn for_witnesses(authority: ScopeId) -> Self {
        Self {
            witness_authority: Some(authority),
            ..Self::default()
        }
    }

    pub(in crate::types) fn valid_occurrence(&self, kind: &Kind) -> bool {
        if let Kind::Meta(meta) = kind
            && meta.lexical().is_some()
        {
            return self.lexical_mappings(meta).is_ok();
        }
        self.occurrence(kind).is_ok()
    }

    /// Joint checker validation keeps an aborted canonical registration from
    /// being hidden by permission thinning or a fresh ambient target.
    pub(in crate::types) fn valid_occurrence_in(
        &self,
        kind: &Kind,
        valid_context: impl Fn(&Context) -> bool,
    ) -> bool {
        let Kind::Meta(meta) = kind else { return false };
        let root = meta.root();
        if meta.lexical().is_some() {
            return self
                .contexts
                .get(root as usize)
                .is_some_and(|context| valid_context(&context.canonical))
                && self.lexical_mappings(meta).is_ok();
        }
        if let Some(occurrence) = meta.contextual()
            && (!occurrence.is_kind()
                || Some(occurrence.owner()) != self.witness_authority
                || !valid_context(occurrence.renaming().source())
                || !valid_context(occurrence.renaming().target()))
        {
            return false;
        }
        self.contexts.get(root as usize).is_some_and(|context| {
            valid_context(&context.canonical) && self.occurrence(kind).is_ok()
        })
    }

    /// Joint read-only observation: validate every occurrence before a solved
    /// root or a projection can erase its source/target registration lease.
    #[allow(dead_code)] // Phase M: used by the unused ZonkRead path until wired
    pub(in crate::types) fn resolve_deep_in(
        &self,
        kind: &Kind,
        valid_context: &impl Fn(&Context) -> bool,
    ) -> Result<Kind, super::module_contract::context::Invalid> {
        use super::module_contract::context::Invalid;
        Ok(match kind {
            Kind::Meta(meta) => {
                if !self.valid_occurrence_in(kind, valid_context) {
                    return Err(Invalid::Authority);
                }
                if meta.lexical().is_some() {
                    let mappings = self
                        .lexical_mappings(meta)
                        .map_err(|()| Invalid::Authority)?;
                    let Some(solution) = &self.metas[meta.root() as usize] else {
                        return Ok(kind.clone());
                    };
                    let projected = lexical::transport_kind(solution, &mappings);
                    return self.resolve_deep_in(&projected, valid_context);
                }
                let (root, view) = self.occurrence(kind).map_err(|()| Invalid::Authority)?;
                let Some(solution) = &self.metas[root as usize] else {
                    return Ok(kind.clone());
                };
                let solution = self.resolve_deep_in(solution, valid_context)?;
                let projected = view.transport_kind_checked(&solution)?;
                self.resolve_deep_in(&projected, valid_context)?
            }
            Kind::Arrow(a, b) => Kind::arrow(
                self.resolve_deep_in(a, valid_context)?,
                self.resolve_deep_in(b, valid_context)?,
            ),
            Kind::Row(inner) => Kind::Row(Box::new(self.resolve_deep_in(inner, valid_context)?)),
            other => other.clone(),
        })
    }

    fn occurrence(&self, kind: &Kind) -> Result<(u32, Embedding), ()> {
        let Kind::Meta(meta) = kind else {
            return Err(());
        };
        let root = meta.root();
        let context = self.contexts.get(root as usize).ok_or(())?;
        if meta.legacy_root().is_some() {
            if context.canonical.len() != 0 || !context.lexical.canonical.is_empty() {
                return Err(());
            }
            return Ok((root, Embedding::identity(&context.canonical)));
        }
        let occurrence = meta.contextual().ok_or(())?;
        let authority = self.witness_authority.ok_or(())?;
        let source = occurrence.renaming().source();
        if !occurrence.is_kind()
            || occurrence.owner() != authority
            || !occurrence.renaming().target().belongs_to(authority)
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

    fn write_context(&mut self, root: u32, context: KindContext) {
        if !self.transactions.is_empty() {
            self.undo.push(KindUndo::Context(
                root,
                self.contexts[root as usize].clone(),
            ));
        }
        self.contexts[root as usize] = context;
    }

    #[cfg(test)]
    pub(in crate::types) fn enter_context(&mut self, context: Context) {
        let authority = self.witness_authority.expect("checker kind authority");
        assert!(context.belongs_to(authority), "checker-owned kind context");
        self.active_context = context;
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use crate::types::Type;
    use crate::types::module_contract::context::Scopes;

    #[test]
    fn kind_roots_project_named_solutions_and_reject_type_root_carriers() {
        let mut scopes = Scopes::default();
        let source = scopes.append(&Context::default(), Type::con("Provider", "S"), "A".into());
        let target = scopes.append(&Context::default(), Type::con("Provider", "S"), "X".into());
        let mut kinds = KindStore::for_witnesses(scopes.owner());
        kinds.enter_context(source.clone());
        let original = kinds.fresh();
        let view = Embedding::align(&source, &target).expect("alpha view");
        let projected = view
            .transport_kind_checked(&original)
            .expect("kind occurrence transport");
        let (root, _) = kinds.occurrence(&original).expect("canonical kind root");
        kinds.assign(
            root,
            Kind::Named {
                module: "A".into(),
                name: "Mode".into(),
            },
        );
        assert_eq!(
            kinds.resolve(&projected),
            Kind::Named {
                module: "X".into(),
                name: "Mode".into()
            }
        );
        let type_carrier = Kind::Meta(MetaOccurrence::new(
            root,
            scopes.owner(),
            Embedding::identity(&source),
        ));
        assert!(kinds.occurrence(&type_carrier).is_err());
        assert_eq!(
            kinds.resolve(&type_carrier),
            type_carrier,
            "wrong root sort never resolves by numeric ID"
        );
        assert_eq!(
            kinds.resolve(&Kind::meta(root)),
            Kind::meta(root),
            "dropping a view grants no authority"
        );
    }

    #[test]
    fn rollback_restores_kind_permissions_and_context_without_reusing_ids() {
        let mut scopes = Scopes::default();
        let context = scopes.append(&Context::default(), Type::con("Provider", "S"), "A".into());
        let mut kinds = KindStore::for_witnesses(scopes.owner());
        kinds.enter_context(context.clone());
        let original = kinds.fresh();
        let (root, _) = kinds.occurrence(&original).expect("kind root");
        let trial = kinds.checkpoint();
        kinds.write_context(
            root,
            KindContext {
                canonical: context.clone(),
                permitted: Context::default(),
                lexical: LexicalRoot::default(),
            },
        );
        kinds.enter_context(Context::default());
        let allocated = kinds.fresh();
        kinds.assign(root, Kind::Region);
        kinds.rollback(trial);
        assert_eq!(kinds.contexts[root as usize].permitted, context);
        assert_eq!(kinds.resolve(&original), original);
        assert_eq!(kinds.resolve(&allocated), allocated);
        assert_eq!(kinds.active_context.len(), 1);
        let Kind::Meta(next) = kinds.fresh() else {
            panic!("restored context")
        };
        assert!(next.contextual().is_some(), "restored contextual root");
        assert_eq!(next.root(), 2);
    }
}
