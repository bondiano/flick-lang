//! Opaque occurrences of canonical solver roots, with explicit legacy provenance.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use super::ScopeId;
use crate::types::module_contract::context::Embedding;

mod lexical;
pub(in crate::types) use lexical::reference_allowed as lexical_reference_allowed;
pub(in crate::types) use lexical::{LexicalCoordinate, LexicalMeta, LexicalRoot, LexicalScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Sort {
    Type,
    Kind,
}

#[derive(Debug, Clone)]
pub struct MetaOccurrence {
    root: u32,
    context: Option<Box<MetaView>>,
}

#[derive(Debug, Clone)]
enum MetaView {
    Contextual(ContextualMeta),
    Lexical(LexicalMeta),
}

#[derive(Debug, Clone)]
pub(in crate::types) struct ContextualMeta {
    owner: ScopeId,
    sort: Sort,
    renaming: Embedding,
}

impl MetaOccurrence {
    /// An ordinary root; observing this ID claims no store authority.
    pub fn legacy(root: u32) -> Self {
        Self {
            root,
            context: None,
        }
    }

    pub fn root(&self) -> u32 {
        self.root
    }

    /// The former raw-root mode, never inferred from an identity embedding.
    pub fn legacy_root(&self) -> Option<u32> {
        self.context.is_none().then_some(self.root)
    }

    pub(in crate::types) fn contextual(&self) -> Option<&ContextualMeta> {
        match self.context.as_deref()? {
            MetaView::Contextual(context) => Some(context),
            MetaView::Lexical(_) => None,
        }
    }

    /// Private issued-footprint integrity, not public semantic equality.
    pub(in crate::types) fn same_carrier(&self, other: &Self) -> bool {
        self == other
            && match (self.contextual(), other.contextual()) {
                (Some(a), Some(b)) => a.renaming.same_carrier(&b.renaming),
                (None, None) => true,
                _ => false,
            }
    }

    pub(in crate::types) fn new(root: u32, owner: ScopeId, renaming: Embedding) -> Self {
        Self {
            root,
            context: Some(Box::new(MetaView::Contextual(ContextualMeta {
                owner,
                sort: Sort::Type,
                renaming,
            }))),
        }
    }

    pub(in crate::types) fn kind(root: u32, owner: ScopeId, renaming: Embedding) -> Self {
        Self {
            root,
            context: Some(Box::new(MetaView::Contextual(ContextualMeta {
                owner,
                sort: Sort::Kind,
                renaming,
            }))),
        }
    }

    /// Legacy roots have no lexical view to replace; contextual provenance stays contextual.
    pub(in crate::types) fn with_renaming(&self, renaming: Embedding) -> Self {
        if self.lexical().is_some() {
            return self.clone();
        }
        Self {
            root: self.root,
            context: self.contextual().map(|context| {
                Box::new(MetaView::Contextual(ContextualMeta {
                    owner: context.owner,
                    sort: context.sort,
                    renaming,
                }))
            }),
        }
    }

    pub(in crate::types) fn copy_bound(&self, from: ScopeId, to: ScopeId) -> Self {
        if self.lexical().is_some() {
            return self.map_lexical(|coordinate| {
                let mut coordinate = coordinate.clone();
                if coordinate.target.id == from {
                    coordinate.target.id = to;
                }
                Some(coordinate)
            });
        }
        self.contextual().map_or_else(
            || self.clone(),
            |context| self.with_renaming(context.renaming.copy_bound(from, to)),
        )
    }
}

impl ContextualMeta {
    pub(in crate::types) fn owner(&self) -> ScopeId {
        self.owner
    }
    pub(in crate::types) fn is_type(&self) -> bool {
        self.sort == Sort::Type
    }
    pub(in crate::types) fn is_kind(&self) -> bool {
        self.sort == Sort::Kind
    }
    pub(in crate::types) fn renaming(&self) -> &Embedding {
        &self.renaming
    }
}

impl PartialEq for MetaOccurrence {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && match (self.context.as_deref(), other.context.as_deref()) {
                (None, None) => true,
                (Some(MetaView::Contextual(a)), Some(MetaView::Contextual(b))) => {
                    a.owner == b.owner
                        && a.sort == b.sort
                        && a.renaming.mapping().eq(b.renaming.mapping())
                }
                (Some(MetaView::Lexical(a)), Some(MetaView::Lexical(b))) => a == b,
                _ => false,
            }
    }
}
impl Eq for MetaOccurrence {}

impl Hash for MetaOccurrence {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.context.as_deref() {
            None => self.root.hash(state),
            Some(MetaView::Contextual(context)) => {
                context.owner.hash(state);
                self.root.hash(state);
                context.sort.hash(state);
                context.renaming.source().len().hash(state);
                for pair in context.renaming.mapping() {
                    pair.hash(state);
                }
            }
            Some(MetaView::Lexical(lexical)) => {
                "lexical".hash(state);
                self.root.hash(state);
                lexical.hash(state);
            }
        }
    }
}

impl Ord for MetaOccurrence {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.context.as_deref(), other.context.as_deref()) {
            (None, None) => self.root.cmp(&other.root),
            (None, Some(_)) | (Some(MetaView::Contextual(_)), Some(MetaView::Lexical(_))) => {
                Ordering::Less
            }
            (Some(_), None) | (Some(MetaView::Lexical(_)), Some(MetaView::Contextual(_))) => {
                Ordering::Greater
            }
            (Some(MetaView::Contextual(a)), Some(MetaView::Contextual(b))) => {
                (a.owner, a.sort, self.root)
                    .cmp(&(b.owner, b.sort, other.root))
                    .then_with(|| a.renaming.mapping().cmp(b.renaming.mapping()))
            }
            (Some(MetaView::Lexical(a)), Some(MetaView::Lexical(b))) => {
                a.cmp(b).then_with(|| self.root.cmp(&other.root))
            }
        }
    }
}
impl PartialOrd for MetaOccurrence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl super::Type {
    pub fn meta(root: u32) -> Self {
        Self::Meta(MetaOccurrence::legacy(root))
    }

    #[cfg(test)]
    pub(in crate::types) fn legacy_meta_root(&self) -> Option<u32> {
        match self {
            Self::Meta(meta) => meta.legacy_root(),
            _ => None,
        }
    }

    pub(in crate::types) fn inference_meta_root(&self) -> Option<u32> {
        match self {
            Self::Meta(meta) => meta.inference_type_root(),
            _ => None,
        }
    }

    pub fn meta_root(&self) -> Option<u32> {
        match self {
            Self::Meta(meta)
                if meta.inference_type_root().is_some()
                    || meta.contextual().is_some_and(ContextualMeta::is_type) =>
            {
                Some(meta.root())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::{MetaOccurrence, Ordering};
    use crate::iface::{Decl, Entity, Export, Namespace, closed};
    use crate::types::module_contract::context::{Context, Embedding, Scopes};
    use crate::types::{Atom, Elem, Kind, Row, Type};

    fn hash(value: &MetaOccurrence) -> u64 {
        let mut state = DefaultHasher::new();
        value.hash(&mut state);
        state.finish()
    }

    #[test]
    fn legacy_identity_and_clone_have_no_context_allocation() {
        let legacy = MetaOccurrence::legacy(7);
        let cloned = legacy.clone();
        assert!(legacy.contextual().is_none());
        assert!(cloned.contextual().is_none());
        assert_eq!(legacy.legacy_root(), Some(7));
        assert_eq!(legacy, cloned);
        assert_eq!(hash(&legacy), hash(&cloned));
        assert!(legacy.same_carrier(&cloned));
        assert_eq!(legacy.cmp(&MetaOccurrence::legacy(8)), Ordering::Less);
        assert_eq!(Type::meta(7).meta_root(), Some(7));
        let label = Elem::Capture(Atom::meta(7));
        let encoded = serde_json::to_string(&label).expect("legacy label encoding");
        assert_eq!(encoded, "\"^?7\"");
        assert_eq!(
            serde_json::from_str::<Elem>(&encoded).expect("legacy label"),
            label
        );
        let changed = legacy.with_renaming(Embedding::identity(&Context::default()));
        assert!(changed.contextual().is_none());
        assert_eq!(changed, legacy);
    }

    #[test]
    fn contextual_identity_never_becomes_legacy_even_with_an_empty_embedding() {
        let scopes = Scopes::default();
        let legacy = MetaOccurrence::legacy(7);
        let contextual =
            MetaOccurrence::new(7, scopes.owner(), Embedding::identity(&Context::default()));
        assert_eq!(legacy.root(), contextual.root());
        assert_eq!(contextual.legacy_root(), None);
        assert!(contextual.contextual().is_some());
        assert_ne!(legacy, contextual);
        assert_eq!(legacy.cmp(&contextual), Ordering::Less);
        assert_eq!(
            MetaOccurrence::legacy(u32::MAX).cmp(&contextual),
            Ordering::Less
        );
        assert!(!legacy.same_carrier(&contextual));
        assert_eq!(contextual.clone().legacy_root(), None);
        assert_eq!(
            contextual
                .with_renaming(Embedding::identity(&Context::default()))
                .legacy_root(),
            None
        );
        let label = Elem::Provided(Box::new(Elem::Capture(Atom::Meta(contextual.clone()))));
        assert!(
            serde_json::to_string(&label).is_err(),
            "empty identity still carries contextual provenance"
        );
        assert_eq!(Type::Meta(contextual).meta_root(), Some(7));
    }

    #[test]
    fn occurrence_identity_ignores_unused_ambient_weakening() {
        let mut scopes = Scopes::default();
        let root = scopes.append(
            &Context::default(),
            Type::con("Provider", "S"),
            "Outer".into(),
        );
        let ambient = scopes.append(&root, Type::con("Provider", "S"), "Unused".into());
        let before = MetaOccurrence::new(5, scopes.owner(), Embedding::identity(&root));
        let after = MetaOccurrence::new(
            5,
            scopes.owner(),
            Embedding::weaken(&root, &ambient).expect("weakening"),
        );
        assert_eq!(before, after, "same root and same coordinate map");
        assert_eq!(before.cmp(&after), Ordering::Equal);
        assert_eq!(hash(&before), hash(&after));
        assert!(
            !before.same_carrier(&after),
            "semantic equality does not erase the issued ambient carrier"
        );
        let shifted = MetaOccurrence::new(
            5,
            scopes.owner(),
            Embedding::align(&root, &ambient.restrict(&ambient.ids().skip(1).collect()))
                .expect("independent contract-equivalent coordinate"),
        );
        assert_ne!(
            before, shifted,
            "different actual coordinate is not weakening"
        );
    }

    #[test]
    fn contextual_occurrences_cannot_be_serialized_through_any_carrier() {
        let mut scopes = Scopes::default();
        let context = scopes.append(
            &Context::default(),
            Type::con("Provider", "S"),
            "Witness".into(),
        );
        let occurrence = MetaOccurrence::new(7, scopes.owner(), Embedding::identity(&context));
        assert!(serde_json::to_string(&Type::Meta(occurrence.clone())).is_err());
        assert!(serde_json::to_string(&Kind::Meta(occurrence.clone())).is_err());
        for label in [
            Elem::Capture(Atom::Meta(occurrence.clone())),
            Elem::Error(Atom::Meta(occurrence.clone())),
            Elem::Service(Atom::Meta(occurrence)),
        ] {
            assert!(serde_json::to_string(&label).is_err());
            assert!(serde_json::to_string(&Elem::Provided(Box::new(label.clone()))).is_err());
            let mut export = Export {
                entity: Entity {
                    module: "M".into(),
                    namespace: Namespace::Value,
                    name: "x".into(),
                },
                decl: Decl::Let,
                parent: None,
                scheme: Some(Type::Without {
                    row: Box::new(Type::Var("r".into())),
                    gone: vec![Elem::Provided(Box::new(label))],
                }),
                definition: None,
                blocked: None,
                at: None,
                via: Vec::new(),
            };
            assert!(!closed::export_is_closed(&export));
            export.scheme = Some(Type::Record(Row::closed(vec![(
                Elem::Capture(Atom::Meta(MetaOccurrence::legacy(1))),
                Type::con("Core", "Int"),
            )])));
            assert!(closed::export_is_closed(&export));
        }
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    use crate::types::module_contract::context::{Context, Scopes};
    use crate::types::{Atom, Elem, Type};

    #[test]
    fn row_label_display_hides_private_views_without_collapsing_their_order() {
        let mut scopes = Scopes::default();
        let a = scopes.append(&Context::default(), Type::con("Provider", "S"), "A".into());
        let x = scopes.append(&Context::default(), Type::con("Provider", "S"), "X".into());
        let original = MetaOccurrence::new(5, scopes.owner(), Embedding::identity(&a));
        let shifted = original.with_renaming(Embedding::align(&a, &x).expect("alpha view"));
        let kind_carrier = Type::Meta(MetaOccurrence::kind(
            5,
            scopes.owner(),
            Embedding::identity(&a),
        ));
        assert_eq!(
            kind_carrier.meta_root(),
            None,
            "classification cannot reinterpret a kind-root ID"
        );
        let left = Elem::Capture(Atom::Meta(original));
        let right = Elem::Capture(Atom::Meta(shifted));
        assert_eq!(left.to_string(), "^?5");
        assert_eq!(right.to_string(), "^?5");
        assert_ne!(left, right);
        assert_ne!(left.cmp(&right), Ordering::Equal);
        assert_ne!(left.cmp(&Elem::Capture(Atom::meta(5))), Ordering::Equal);
        let mut labels = vec![left, right, Elem::Capture(Atom::meta(5))];
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), 3);
    }
}
