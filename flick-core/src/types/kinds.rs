//! Kinds: `Type`, the three effect-row kinds,
//! `Row k` for record fields, labels, regions, constraints, finite kinds a
//! `kind` declaration introduces, and arrows between them. Kinds of type
//! variables are inferred from their positions; what stays open becomes `Type`.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::ty::MetaOccurrence;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Type,
    /// A `type module` contract, used by parameterized modules and `Module Sig`.
    Module,
    ServiceRow,
    ErrorRow,
    CaptureRow,
    Symbol,
    Region,
    Constraint,
    /// A service identity: what a `ServiceRow` holds.
    Service,
    /// `Row k`; `Row Type` is the kind of record fields.
    Row(Box<Kind>),
    /// A finite kind declared with `kind K = A | B`.
    Named {
        module: String,
        name: String,
    },
    Arrow(Box<Kind>, Box<Kind>),
    /// An unknown kind while kinds are inferred.
    #[serde(skip)]
    Meta(MetaOccurrence),
}

impl Kind {
    pub fn meta(root: u32) -> Self {
        Self::Meta(MetaOccurrence::legacy(root))
    }

    pub(in crate::types) fn legacy_meta_root(&self) -> Option<u32> {
        match self {
            Self::Meta(meta) => meta.legacy_root(),
            _ => None,
        }
    }

    pub fn has_meta_view(&self) -> bool {
        match self {
            Kind::Meta(meta) => meta.contextual().is_some(),
            Kind::Row(inner) => inner.has_meta_view(),
            Kind::Arrow(a, b) => a.has_meta_view() || b.has_meta_view(),
            _ => false,
        }
    }

    pub fn arrow(param: Kind, result: Kind) -> Kind {
        Kind::Arrow(Box::new(param), Box::new(result))
    }

    /// `k1 -> … -> kn -> result`.
    pub fn arrows(params: impl DoubleEndedIterator<Item = Kind>, result: Kind) -> Kind {
        params.rev().fold(result, |acc, p| Kind::arrow(p, acc))
    }

    pub fn is_row(&self) -> bool {
        matches!(
            self,
            Kind::ServiceRow | Kind::ErrorRow | Kind::CaptureRow | Kind::Row(_)
        )
    }

    /// The built-in kind a capitalized name in kind position denotes.
    pub fn builtin(name: &str) -> Option<Kind> {
        Some(match name {
            "Type" => Kind::Type,
            "Module" => Kind::Module,
            "ServiceRow" => Kind::ServiceRow,
            "ErrorRow" => Kind::ErrorRow,
            "CaptureRow" => Kind::CaptureRow,
            "FieldRow" => Kind::Row(Box::new(Kind::Type)),
            "Symbol" => Kind::Symbol,
            "Region" => Kind::Region,
            "Constraint" => Kind::Constraint,
            _ => return None,
        })
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Type => f.write_str("Type"),
            Kind::Module => f.write_str("Module"),
            Kind::ServiceRow => f.write_str("ServiceRow"),
            Kind::ErrorRow => f.write_str("ErrorRow"),
            Kind::CaptureRow => f.write_str("CaptureRow"),
            Kind::Symbol => f.write_str("Symbol"),
            Kind::Region => f.write_str("Region"),
            Kind::Constraint => f.write_str("Constraint"),
            Kind::Service => f.write_str("Service"),
            Kind::Row(k) if matches!(**k, Kind::Arrow(..)) => write!(f, "Row ({k})"),
            Kind::Row(k) => write!(f, "Row {k}"),
            Kind::Named { name, .. } => f.write_str(name),
            Kind::Arrow(a, b) if matches!(**a, Kind::Arrow(..)) => write!(f, "({a}) -> {b}"),
            Kind::Arrow(a, b) => write!(f, "{a} -> {b}"),
            Kind::Meta(_) => f.write_str("_"),
        }
    }
}

/// Unknown kinds of one inference: kinds of a module's declarations or of
/// one signature.
#[derive(Debug, Default)]
pub struct KindStore {
    metas: Vec<Option<Kind>>,
    contexts: Vec<KindContext>,
    active_context: Context,
    witness_authority: Option<ScopeId>,
    undo: Vec<KindUndo>,
    transactions: Vec<u64>,
    next_transaction: u64,
}

/// LIFO inference checkpoint. Allocations are append-only even after rollback.
#[derive(Debug)]
pub(in crate::types) struct KindCheckpoint {
    undo: usize,
    allocated: usize,
    transaction: u64,
    context: Context,
}

impl KindStore {
    pub fn checkpoint(&mut self) -> KindCheckpoint {
        let transaction = self.next_transaction;
        self.next_transaction = transaction.checked_add(1).expect("few kind transactions");
        self.transactions.push(transaction);
        KindCheckpoint {
            undo: self.undo.len(),
            allocated: self.metas.len(),
            transaction,
            context: self.active_context.clone(),
        }
    }

    pub fn commit(&mut self, checkpoint: KindCheckpoint) {
        assert_eq!(
            self.transactions.last(),
            Some(&checkpoint.transaction),
            "LIFO kind transaction"
        );
        self.transactions.pop();
        if self.transactions.is_empty() {
            self.undo.clear();
        }
        drop(checkpoint.context);
    }

    pub fn rollback(&mut self, checkpoint: KindCheckpoint) {
        assert_eq!(
            self.transactions.last(),
            Some(&checkpoint.transaction),
            "LIFO kind transaction"
        );
        for change in self.undo.drain(checkpoint.undo..).rev() {
            match change {
                KindUndo::Binding(root, previous) => self.metas[root as usize] = previous,
                #[cfg(test)]
                KindUndo::Context(root, previous) => self.contexts[root as usize] = previous,
            }
        }
        // Diagnostic and Typing data can still refer to newly allocated IDs.
        // Keep the slots, but discard their aborted solutions.
        self.metas[checkpoint.allocated..].fill(None);
        for context in &mut self.contexts[checkpoint.allocated..] {
            context.permitted = Context::default();
        }
        self.active_context = checkpoint.context;
        self.transactions.pop();
    }

    fn assign(&mut self, m: u32, kind: Kind) {
        let slot = &mut self.metas[m as usize];
        if !self.transactions.is_empty() {
            self.undo.push(KindUndo::Binding(m, slot.clone()));
        }
        *slot = Some(kind);
    }

    pub fn fresh(&mut self) -> Kind {
        let root = u32::try_from(self.metas.len()).expect("few kind variables");
        self.metas.push(None);
        self.contexts.push(KindContext {
            canonical: self.active_context.clone(),
            permitted: self.active_context.clone(),
        });
        if self.active_context.len() == 0 {
            Kind::meta(root)
        } else {
            Kind::Meta(MetaOccurrence::kind(
                root,
                self.witness_authority.expect("checker-owned kind context"),
                Embedding::identity(&self.active_context),
            ))
        }
    }

    /// Follows solved unknowns, lazily projecting contextual solutions.
    pub fn resolve(&self, kind: &Kind) -> Kind {
        let mut kind = kind.clone();
        loop {
            match &kind {
                Kind::Meta(meta) => {
                    if let Some(root) = meta.legacy_root() {
                        let Some(context) = self.contexts.get(root as usize) else {
                            return kind;
                        };
                        if context.canonical.len() != 0 {
                            return kind;
                        }
                        let Some(next) = &self.metas[root as usize] else {
                            return kind;
                        };
                        kind = next.clone();
                    } else {
                        let Ok((root, view)) = self.occurrence(&kind) else {
                            return kind;
                        };
                        let Some(solution) = &self.metas[root as usize] else {
                            return kind;
                        };
                        let Ok(projected) = view.transport_kind_checked(solution) else {
                            return kind;
                        };
                        kind = projected;
                    }
                }
                _ => return kind,
            }
        }
    }

    /// Recursively observe solved kinds without introducing defaults.
    pub fn resolve_deep(&self, kind: &Kind) -> Kind {
        match self.resolve(kind) {
            Kind::Row(inner) => Kind::Row(Box::new(self.resolve_deep(&inner))),
            Kind::Arrow(a, b) => Kind::arrow(self.resolve_deep(&a), self.resolve_deep(&b)),
            other => other,
        }
    }

    /// Solves every unknown it can; with `default`, open unknowns become `Type`.
    pub fn zonk(&mut self, kind: &Kind, default: bool) -> Kind {
        if !default {
            return self.resolve_deep(kind);
        }
        match self.resolve(kind) {
            Kind::Meta(meta) if meta.legacy_root().is_some() => {
                self.assign(meta.legacy_root().expect("legacy kind default"), Kind::Type);
                Kind::Type
            }
            Kind::Row(k) => Kind::Row(Box::new(self.zonk(&k, default))),
            Kind::Arrow(a, b) => Kind::arrow(self.zonk(&a, default), self.zonk(&b, default)),
            other => other,
        }
    }

    /// `Err` with both kinds as far as they are known.
    pub fn unify(&mut self, a: &Kind, b: &Kind) -> Result<(), (Kind, Kind)> {
        let checkpoint = self.checkpoint();
        let result = self.unify_inner(a, b);
        if result.is_ok() {
            self.commit(checkpoint);
        } else {
            self.rollback(checkpoint);
        }
        result
    }

    fn unify_inner(&mut self, a: &Kind, b: &Kind) -> Result<(), (Kind, Kind)> {
        let (a, b) = (self.resolve(a), self.resolve(b));
        let legacy = a
            .legacy_meta_root()
            .map(|m| (m, &b))
            .or_else(|| b.legacy_meta_root().map(|m| (m, &a)));
        if let Some((m, other)) = legacy {
            if a == b {
                return Ok(());
            }
            if self.occurs(m, other) {
                return Err((self.zonk(&a, false), self.zonk(&b, false)));
            }
            self.assign(m, other.clone());
            return Ok(());
        }
        match (&a, &b) {
            (Kind::Row(x), Kind::Row(y)) => self.unify_inner(x, y),
            (Kind::Arrow(a1, b1), Kind::Arrow(a2, b2)) => {
                self.unify_inner(a1, a2)?;
                self.unify_inner(b1, b2)
            }
            _ if a == b => Ok(()),
            _ => Err((self.zonk(&a, false), self.zonk(&b, false))),
        }
    }

    fn occurs(&self, m: u32, kind: &Kind) -> bool {
        match self.resolve(kind) {
            Kind::Meta(meta) => meta.legacy_root() == Some(m),
            Kind::Row(k) => self.occurs(m, &k),
            Kind::Arrow(a, b) => self.occurs(m, &a) || self.occurs(m, &b),
            _ => false,
        }
    }
}

include!("kinds/context.rs");

#[cfg(test)]
mod transaction_tests {
    use super::{Kind, KindStore};

    #[test]
    fn a_later_arrow_mismatch_restores_earlier_kind_bindings() {
        let mut kinds = KindStore::default();
        let variable = kinds.fresh();
        let left = Kind::arrow(variable.clone(), Kind::Type);
        let right = Kind::arrow(Kind::Symbol, Kind::Region);
        assert!(kinds.unify(&left, &right).is_err());
        assert_eq!(kinds.resolve(&variable), variable);
    }

    #[test]
    fn nested_commits_still_rollback_with_the_outer_attempt() {
        let mut kinds = KindStore::default();
        let original = kinds.fresh();
        let outer = kinds.checkpoint();
        assert!(kinds.unify(&original, &Kind::Region).is_ok());
        let inner = kinds.checkpoint();
        let allocated = kinds.fresh();
        assert!(kinds.unify(&allocated, &Kind::Symbol).is_ok());
        kinds.commit(inner);
        kinds.rollback(outer);
        assert_eq!(kinds.resolve(&original), original);
        assert_eq!(kinds.resolve(&allocated), allocated);
        assert_eq!(kinds.fresh(), Kind::meta(2));
        assert!(kinds.unify(&allocated, &Kind::Type).is_ok());
    }

    #[test]
    fn rollback_restores_defaulting_and_retains_preexisting_solutions() {
        let mut kinds = KindStore::default();
        let solved = kinds.fresh();
        let open = kinds.fresh();
        assert!(kinds.unify(&solved, &Kind::Region).is_ok());
        let checkpoint = kinds.checkpoint();
        assert_eq!(kinds.zonk(&open, true), Kind::Type);
        kinds.rollback(checkpoint);
        assert_eq!(kinds.resolve(&open), open);
        assert_eq!(kinds.resolve(&solved), Kind::Region);
    }
}
