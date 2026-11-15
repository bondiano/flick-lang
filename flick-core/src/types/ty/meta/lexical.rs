//! Lightweight inference views for lexically opened existential witnesses.

use super::{MetaOccurrence, MetaView, ScopeId, Sort};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::types) struct LexicalScope {
    pub id: ScopeId,
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::types) struct LexicalCoordinate {
    pub canonical: ScopeId,
    pub target: LexicalScope,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::types) struct LexicalMeta {
    pub owner: ScopeId,
    sort: Sort,
    pub view: Vec<LexicalCoordinate>,
}

impl LexicalMeta {
    pub fn is_type(&self) -> bool {
        self.sort == Sort::Type
    }

    pub fn is_kind(&self) -> bool {
        self.sort == Sort::Kind
    }
}

impl MetaOccurrence {
    pub(in crate::types) fn lexical(&self) -> Option<&LexicalMeta> {
        match self.context.as_deref()? {
            MetaView::Lexical(lexical) => Some(lexical),
            MetaView::Contextual(_) => None,
        }
    }

    pub(in crate::types) fn lexical_type(
        root: u32,
        owner: ScopeId,
        scopes: &[LexicalScope],
    ) -> Self {
        Self::with_lexical(root, owner, Sort::Type, scopes)
    }

    pub(in crate::types) fn lexical_kind(
        root: u32,
        owner: ScopeId,
        scopes: &[LexicalScope],
    ) -> Self {
        Self::with_lexical(root, owner, Sort::Kind, scopes)
    }

    fn with_lexical(root: u32, owner: ScopeId, sort: Sort, scopes: &[LexicalScope]) -> Self {
        Self {
            root,
            context: Some(Box::new(MetaView::Lexical(LexicalMeta {
                owner,
                sort,
                view: scopes
                    .iter()
                    .map(|scope| LexicalCoordinate {
                        canonical: scope.id,
                        target: scope.clone(),
                    })
                    .collect(),
            }))),
        }
    }

    pub(in crate::types) fn map_lexical(
        &self,
        mut map: impl FnMut(&LexicalCoordinate) -> Option<LexicalCoordinate>,
    ) -> Self {
        let Some(lexical) = self.lexical() else {
            return self.clone();
        };
        let mut lexical = lexical.clone();
        lexical.view = lexical.view.iter().filter_map(&mut map).collect();
        Self {
            root: self.root,
            context: Some(Box::new(MetaView::Lexical(lexical))),
        }
    }

    pub(in crate::types) fn inference_type_root(&self) -> Option<u32> {
        (self.legacy_root().is_some() || self.lexical().is_some_and(LexicalMeta::is_type))
            .then_some(self.root)
    }

    pub(in crate::types) fn inference_kind_root(&self) -> Option<u32> {
        (self.legacy_root().is_some() || self.lexical().is_some_and(LexicalMeta::is_kind))
            .then_some(self.root)
    }
}

/// Canonical dependencies never change; only the permitted subset can shrink.
#[derive(Debug, Clone, Default)]
pub(in crate::types) struct LexicalRoot {
    pub canonical: Vec<LexicalScope>,
    pub permitted: Vec<ScopeId>,
}

impl LexicalRoot {
    pub fn new(scopes: &[LexicalScope]) -> Self {
        Self {
            canonical: scopes.to_vec(),
            permitted: scopes.iter().map(|scope| scope.id).collect(),
        }
    }

    pub fn mappings(
        &self,
        meta: &MetaOccurrence,
        owner: ScopeId,
        kind: bool,
    ) -> Result<Vec<(LexicalScope, LexicalScope)>, ()> {
        if meta.legacy_root().is_some() {
            return self.canonical.is_empty().then(Vec::new).ok_or(());
        }
        let lexical = meta.lexical().ok_or(())?;
        if lexical.owner != owner
            || lexical.is_kind() != kind
            || lexical.view.iter().any(|coordinate| {
                !self
                    .canonical
                    .iter()
                    .any(|scope| scope.id == coordinate.canonical)
            })
        {
            return Err(());
        }
        self.permitted
            .iter()
            .map(|id| {
                let canonical = self
                    .canonical
                    .iter()
                    .find(|scope| scope.id == *id)
                    .ok_or(())?;
                let target = &lexical
                    .view
                    .iter()
                    .find(|coordinate| coordinate.canonical == *id)
                    .ok_or(())?
                    .target;
                Ok((canonical.clone(), target.clone()))
            })
            .collect()
    }
}

pub(in crate::types) fn reference_allowed(
    path: &str,
    allowed: &[LexicalScope],
    bound: &[LexicalScope],
    inventory: &[LexicalScope],
) -> bool {
    let within = |scope: &LexicalScope| {
        path == scope.namespace
            || path
                .strip_prefix(&scope.namespace)
                .is_some_and(|suffix| suffix.starts_with('.'))
    };
    allowed.iter().chain(bound).any(within)
        || (!path.starts_with("$exists:") && !inventory.iter().any(within))
}
