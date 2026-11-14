//! Opaque occurrence of a canonical solver root in an ordered witness view.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use super::ScopeId;
use crate::types::module_contract::context::Embedding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Sort {
    Type,
    Kind,
}

#[derive(Debug, Clone)]
pub struct MetaOccurrence {
    root: u32,
    owner: ScopeId,
    sort: Sort,
    renaming: Embedding,
}

impl MetaOccurrence {
    /// Private issued-footprint integrity, not public semantic equality.
    pub(in crate::types) fn same_carrier(&self, other: &Self) -> bool {
        self == other && self.renaming.same_carrier(&other.renaming)
    }

    pub fn root(&self) -> u32 {
        self.root
    }

    pub(in crate::types) fn new(root: u32, owner: ScopeId, renaming: Embedding) -> Self {
        Self {
            root,
            owner,
            sort: Sort::Type,
            renaming,
        }
    }

    pub(in crate::types) fn kind(root: u32, owner: ScopeId, renaming: Embedding) -> Self {
        Self {
            root,
            owner,
            sort: Sort::Kind,
            renaming,
        }
    }

    pub(in crate::types) fn is_type(&self) -> bool {
        self.sort == Sort::Type
    }

    pub(in crate::types) fn is_kind(&self) -> bool {
        self.sort == Sort::Kind
    }

    pub(in crate::types) fn with_renaming(&self, renaming: Embedding) -> Self {
        Self {
            renaming,
            ..self.clone()
        }
    }

    pub(in crate::types) fn copy_bound(&self, from: ScopeId, to: ScopeId) -> Self {
        self.with_renaming(self.renaming.copy_bound(from, to))
    }

    pub(in crate::types) fn owner(&self) -> ScopeId {
        self.owner
    }

    pub(in crate::types) fn renaming(&self) -> &Embedding {
        &self.renaming
    }
}

impl PartialEq for MetaOccurrence {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.sort == other.sort
            && self.root == other.root
            && self.renaming.mapping().eq(other.renaming.mapping())
    }
}

impl Eq for MetaOccurrence {}

impl Hash for MetaOccurrence {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.owner.hash(state);
        self.root.hash(state);
        self.sort.hash(state);
        self.renaming.source().len().hash(state);
        for pair in self.renaming.mapping() {
            pair.hash(state);
        }
    }
}

impl Ord for MetaOccurrence {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.owner, self.sort, self.root)
            .cmp(&(other.owner, other.sort, other.root))
            .then_with(|| self.renaming.mapping().cmp(other.renaming.mapping()))
    }
}

impl PartialOrd for MetaOccurrence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl super::Type {
    pub fn meta_root(&self) -> Option<u32> {
        match self {
            Self::Meta(root) => Some(*root),
            Self::MetaView(occurrence) if occurrence.is_type() => Some(occurrence.root()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::{MetaOccurrence, Ordering};
    use crate::types::module_contract::context::{Context, Embedding, Scopes};
    use crate::types::{Atom, Elem, Kind, Type};

    fn hash(value: &MetaOccurrence) -> u64 {
        let mut state = DefaultHasher::new();
        value.hash(&mut state);
        state.finish()
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
        let occurrence = Box::new(MetaOccurrence::new(
            7,
            scopes.owner(),
            Embedding::identity(&context),
        ));
        assert!(serde_json::to_string(&Type::MetaView(occurrence.clone())).is_err());
        assert!(serde_json::to_string(&Kind::MetaView(occurrence.clone())).is_err());
        for label in [
            Elem::Capture(Atom::MetaView(occurrence.clone())),
            Elem::Error(Atom::MetaView(occurrence.clone())),
            Elem::Service(Atom::MetaView(occurrence)),
        ] {
            assert!(serde_json::to_string(&label).is_err());
            assert!(serde_json::to_string(&Elem::Provided(Box::new(label))).is_err());
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
        let kind_carrier = Type::MetaView(Box::new(MetaOccurrence::kind(
            5,
            scopes.owner(),
            Embedding::identity(&a),
        )));
        assert_eq!(
            kind_carrier.meta_root(),
            None,
            "classification cannot reinterpret a kind-root ID"
        );
        let left = Elem::Capture(Atom::MetaView(Box::new(original)));
        let right = Elem::Capture(Atom::MetaView(Box::new(shifted)));
        assert_eq!(left.to_string(), "^?5");
        assert_eq!(right.to_string(), "^?5");
        assert_ne!(left, right);
        assert_ne!(left.cmp(&right), Ordering::Equal);
        assert_ne!(left.cmp(&Elem::Capture(Atom::Meta(5))), Ordering::Equal);
        let mut labels = vec![left, right, Elem::Capture(Atom::Meta(5))];
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), 3);
    }
}
