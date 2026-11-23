//! Logical binder handles are inference bookkeeping, never wire authority.

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::types::{Atom, Elem, Kind};

static NEXT_SCOPE: AtomicUsize = AtomicUsize::new(0);

/// An opaque lexical identity. Observing an identity grants no witness authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(usize);

/// A clone retains one logical binder. Semantic equality and hashing deliberately
/// ignore its handle, as does the skipped field in `Type::Exists`.
#[derive(Debug, Clone, Copy)]
pub struct Scope {
    identity: ScopeId,
}

impl Scope {
    pub fn identity(self) -> ScopeId {
        self.identity
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            identity: allocate(&NEXT_SCOPE),
        }
    }
}

fn allocate(counter: &AtomicUsize) -> ScopeId {
    let identity = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .expect("lexical scope identity space exhausted");
    ScopeId(identity)
}

impl PartialEq for Scope {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Scope {}

impl Hash for Scope {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl super::Type {
    /// Copy a binder-bearing template for insertion, rather than cloning one
    /// logical term. Free caller binders are outside this subtree and retain
    /// their identities; only handles actually bound here are freshened.
    pub(super) fn copy_template(&self) -> Self {
        self.substitute_inner(&[])
    }

    /// Alpha-copy contextual occurrences owned by one binder. Canonical roots
    /// and free caller scopes do not change. This includes positions that are
    /// not child types (binder kinds and semantic row atoms).
    pub(in crate::types) fn copy_bound_handle(&self, from: ScopeId, to: ScopeId) -> Self {
        let mut copied = self.map_children(&mut |child| child.copy_bound_handle(from, to));
        match &mut copied {
            Self::Meta(meta) => *meta = meta.copy_bound(from, to),
            Self::Forall { vars, .. } => {
                for binder in vars {
                    binder.kind = copy_kind(&binder.kind, from, to);
                }
            }
            Self::Row(row) | Self::Record(row) => {
                for (label, _) in &mut row.entries {
                    copy_label(label, from, to);
                }
                row.sort();
            }
            Self::Without { gone, .. } => {
                for label in gone.iter_mut() {
                    copy_label(label, from, to);
                }
                gone.sort();
            }
            _ => {}
        }
        copied
    }
}

pub(in crate::types) fn copy_kind(kind: &Kind, from: ScopeId, to: ScopeId) -> Kind {
    match kind {
        Kind::Meta(meta) => Kind::Meta(meta.copy_bound(from, to)),
        Kind::Arrow(a, b) => Kind::arrow(copy_kind(a, from, to), copy_kind(b, from, to)),
        Kind::Row(inner) => Kind::Row(Box::new(copy_kind(inner, from, to))),
        _ => kind.clone(),
    }
}

pub(in crate::types) fn copy_label(label: &mut Elem, from: ScopeId, to: ScopeId) {
    match label {
        Elem::Capture(Atom::Meta(meta))
        | Elem::Error(Atom::Meta(meta))
        | Elem::Service(Atom::Meta(meta)) => *meta = meta.copy_bound(from, to),
        Elem::Provided(inner) => copy_label(inner, from, to),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, hash_map::DefaultHasher};
    use std::hash::{Hash, Hasher};
    use std::thread;

    use super::Scope;
    use crate::types::Type;

    fn packed() -> Type {
        Type::Exists {
            scope: Scope::default(),
            binder: "K".into(),
            contract: Box::new(Type::con("Provider", "S")),
            body: Box::new(Type::con("$exists:K", "Token")),
        }
    }

    fn hash(ty: &Type) -> u64 {
        let mut state = DefaultHasher::new();
        ty.hash(&mut state);
        state.finish()
    }

    fn identity(ty: &Type) -> super::ScopeId {
        let Type::Exists { scope, .. } = ty else {
            panic!("existential")
        };
        scope.identity()
    }

    #[test]
    fn logical_clones_retain_handles_but_independent_binders_do_not() {
        let first = packed();
        assert_eq!(identity(&first), identity(&first.clone()));
        let second = packed();
        assert_ne!(identity(&first), identity(&second));
        assert_eq!(first, second);
        assert_eq!(hash(&first), hash(&second));
        let mapped = first.map_children(&mut Clone::clone);
        assert_eq!(identity(&first), identity(&mapped));
    }

    #[test]
    fn json_is_unchanged_and_each_decode_internalizes_distinct_handles() {
        let first = packed();
        let json = serde_json::to_string(&first).expect("closed type");
        assert_eq!(
            json,
            r#"{"exists":{"binder":"K","contract":{"con":{"module":"Provider","name":"S"}},"body":{"con":{"module":"$exists:K","name":"Token"}}}}"#
        );
        let left: Type = serde_json::from_str(&json).expect("closed type");
        let right: Type = serde_json::from_str(&json).expect("closed type");
        assert_ne!(identity(&left), identity(&right));
        assert_ne!(identity(&left), identity(&first));
        assert_eq!(left, right);
        assert_eq!(hash(&left), hash(&right));
        assert_eq!(serde_json::to_string(&left).expect("closed type"), json);
    }

    #[test]
    fn interface_fingerprint_and_wire_ignore_independent_scope_handles() {
        use crate::iface::{self, Decl, Entity, Export, Interface, Namespace};

        let interface = Interface {
            module: "Consumer".into(),
            exports: vec![Export {
                entity: Entity {
                    module: "Consumer".into(),
                    namespace: Namespace::Value,
                    name: "packed".into(),
                },
                decl: Decl::Let,
                parent: None,
                scheme: Some(packed()),
                definition: None,
                blocked: None,
                at: None,
                via: Vec::new(),
            }],
            ..Interface::default()
        };
        let json = iface::write(&interface).expect("publishable interface");
        let left = iface::read(json.as_bytes(), "Consumer").expect("closed interface");
        let right = iface::read(json.as_bytes(), "Consumer").expect("closed interface");
        assert_eq!(left, right);
        assert_eq!(
            iface::fingerprint(&interface).expect("publishable fingerprint"),
            iface::fingerprint(&left).expect("publishable fingerprint")
        );
        assert_eq!(
            iface::fingerprint(&left).expect("publishable fingerprint"),
            iface::fingerprint(&right).expect("publishable fingerprint")
        );
        let left = left.exports[0].scheme.as_ref().expect("packed scheme");
        let right = right.exports[0].scheme.as_ref().expect("packed scheme");
        assert_ne!(identity(left), identity(right));
    }

    #[test]
    fn independently_created_same_named_shadow_binders_have_distinct_handles() {
        let inner = packed();
        let outer = Type::Exists {
            scope: Scope::default(),
            binder: "K".into(),
            contract: Box::new(Type::con("Provider", "S")),
            body: Box::new(inner.clone()),
        };
        assert_ne!(identity(&outer), identity(&inner));
        let Type::Exists { body, .. } = outer.clone() else {
            panic!("existential")
        };
        assert_eq!(identity(&body), identity(&inner));
    }

    #[test]
    fn concurrent_allocation_never_reuses_a_handle() {
        let handles: Vec<_> = (0..16)
            .map(|_| thread::spawn(|| Scope::default().identity()))
            .collect();
        let ids: BTreeSet<_> = handles
            .into_iter()
            .map(|handle| handle.join().expect("allocation thread"))
            .collect();
        assert_eq!(ids.len(), 16);
    }

    #[test]
    fn substituting_a_binder_bearing_template_copies_each_insertion_hygienically() {
        let template = packed();
        let target = Type::Tuple(vec![Type::Var("a".into()), Type::Var("a".into())]);
        let Type::Tuple(inserted) = target.substitute(&[("a".into(), template.clone())]) else {
            panic!("tuple")
        };
        assert_ne!(identity(&inserted[0]), identity(&inserted[1]));
        assert_ne!(identity(&inserted[0]), identity(&template));
        assert_eq!(inserted[0], template);
        assert_eq!(inserted[1], template);
        let first = template.substitute(&[]);
        let second = template.substitute(&[]);
        assert_ne!(identity(&first), identity(&second));
    }

    #[test]
    fn template_copy_transports_bound_occurrences_in_types_kinds_and_labels() {
        use crate::iface::Interface;
        use crate::types::module_contract::context::{Context, Embedding, Scopes};
        use crate::types::ty::{Binder, MetaOccurrence};
        use crate::types::{Atom, Elem, Kind, Row};

        let mut scopes = Scopes::default();
        let caller = Scope::default();
        let bound = Scope::default();
        let register = |scopes: &mut Scopes, base: &Context, scope, module: &str| {
            scopes
                .register_view(
                    base,
                    scope,
                    Type::con("Provider", "S"),
                    &Interface {
                        module: module.into(),
                        ..Interface::default()
                    },
                )
                .expect("owned scope")
        };
        let caller_context = register(&mut scopes, &Context::default(), caller, "Caller");
        let context = register(&mut scopes, &caller_context, bound, "Bound");
        let occurrence = MetaOccurrence::new(77, scopes.owner(), Embedding::identity(&context));
        let template = Type::Exists {
            scope: bound,
            binder: "K".into(),
            contract: Box::new(Type::con("Provider", "S")),
            body: Box::new(Type::Forall {
                vars: vec![Binder {
                    name: "a".into(),
                    kind: Kind::Meta(occurrence.clone()),
                }],
                constraints: vec![Type::Row(Row::closed(vec![(
                    Elem::Provided(Box::new(Elem::Capture(Atom::Meta(occurrence.clone())))),
                    Type::Meta(occurrence.clone()),
                )]))],
                body: Box::new(Type::Meta(occurrence)),
            }),
        };
        let copied = template.substitute(&[]);
        let copied_scope = identity(&copied);
        let Type::Exists { body, .. } = copied else {
            panic!("exists")
        };
        let Type::Forall {
            vars,
            constraints,
            body,
        } = *body
        else {
            panic!("forall")
        };
        let Kind::Meta(kind) = &vars[0].kind else {
            panic!("kind occurrence")
        };
        let Type::Row(row) = &constraints[0] else {
            panic!("row")
        };
        let Elem::Provided(label) = &row.entries[0].0 else {
            panic!("provided")
        };
        let Elem::Capture(Atom::Meta(label)) = &**label else {
            panic!("label occurrence")
        };
        let Type::Meta(value) = &*body else {
            panic!("type occurrence")
        };
        for occurrence in [kind, label, value] {
            assert_eq!(occurrence.root(), 77, "one canonical root");
            assert_eq!(
                occurrence
                    .contextual()
                    .expect("contextual test occurrence")
                    .renaming()
                    .mapped(bound.identity()),
                Some(copied_scope)
            );
            assert_eq!(
                occurrence
                    .contextual()
                    .expect("contextual test occurrence")
                    .renaming()
                    .mapped(caller.identity()),
                Some(caller.identity())
            );
        }
    }

    #[test]
    fn exhausting_the_nonce_space_fails_without_wrapping_or_reusing_zero() {
        use super::{AtomicUsize, Ordering, ScopeId, allocate};
        use std::panic;

        let counter = AtomicUsize::new(usize::MAX - 1);
        assert_eq!(allocate(&counter), ScopeId(usize::MAX - 1));
        assert!(panic::catch_unwind(|| allocate(&counter)).is_err());
        assert_eq!(counter.load(Ordering::Relaxed), usize::MAX);
        assert!(panic::catch_unwind(|| allocate(&counter)).is_err());
        assert_eq!(counter.load(Ordering::Relaxed), usize::MAX);
    }
}
