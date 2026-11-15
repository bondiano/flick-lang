use super::module_contract::lexical;
use super::ty::{LexicalRoot, LexicalScope};
use std::mem;

impl KindStore {
    pub(in crate::types) fn lexical_context(&self) -> Vec<LexicalScope> {
        self.active_lexical.clone()
    }

    pub(in crate::types) fn enter_lexical(&mut self, scopes: Vec<LexicalScope>) {
        self.active_lexical = scopes;
    }

    pub(in crate::types) fn register_lexical(&mut self, scope: LexicalScope) {
        self.lexical_scopes.push(scope);
    }

    pub(in crate::types) fn fresh_in(&mut self, scopes: &[LexicalScope]) -> Kind {
        let old = mem::replace(&mut self.active_lexical, scopes.to_vec());
        let fresh = self.fresh();
        self.active_lexical = old;
        fresh
    }

    fn lexical_mappings(
        &self,
        meta: &MetaOccurrence,
    ) -> Result<Vec<(LexicalScope, LexicalScope)>, ()> {
        let entry = self.contexts.get(meta.root() as usize).ok_or(())?;
        if meta.legacy_root().is_some() && entry.canonical.len() != 0 {
            return Err(());
        }
        let context = &entry.lexical;
        if meta.legacy_root().is_some() && context.canonical.is_empty() {
            return Ok(Vec::new());
        }
        context.mappings(meta, self.witness_authority.ok_or(())?, true)
    }
}

impl KindStore {
    pub(in crate::types) fn restrict_lexical(
        &mut self,
        kind: &Kind,
        allowed: &[LexicalScope],
        bound: &[LexicalScope],
    ) -> Result<Kind, ()> {
        let kind = self.resolve_deep(kind);
        match &kind {
            Kind::Named { module, .. } => {
                if super::ty::lexical_reference_allowed(
                    module,
                    allowed,
                    bound,
                    &self.lexical_scopes,
                ) {
                    Ok(kind)
                } else {
                    Err(())
                }
            }
            Kind::Meta(meta) => {
                if meta.contextual().is_some() {
                    return self.valid_occurrence(&kind).then(|| kind.clone()).ok_or(());
                }
                let mappings = self.lexical_mappings(meta)?;
                let permitted = mappings
                    .iter()
                    .filter(|(_, target)| {
                        allowed
                            .iter()
                            .chain(bound)
                            .any(|scope| scope.id == target.id)
                    })
                    .map(|(source, _)| source.id)
                    .collect::<Vec<_>>();
                let mut context = self.contexts[meta.root() as usize].clone();
                if context.lexical.permitted != permitted {
                    context.lexical.permitted = permitted;
                    self.write_context(meta.root(), context);
                }
                Ok(Kind::Meta(meta.map_lexical(|coordinate| {
                    allowed
                        .iter()
                        .chain(bound)
                        .any(|scope| scope.id == coordinate.target.id)
                        .then(|| coordinate.clone())
                })))
            }
            Kind::Arrow(a, b) => Ok(Kind::arrow(
                self.restrict_lexical(a, allowed, bound)?,
                self.restrict_lexical(b, allowed, bound)?,
            )),
            Kind::Row(inner) => Ok(Kind::Row(Box::new(
                self.restrict_lexical(inner, allowed, bound)?,
            ))),
            _ => Ok(kind),
        }
    }
}
