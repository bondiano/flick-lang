//! What a check typed each subterm at, for the Lean certificate of a
//! program (`crate::cert`, `docs/formal/certificates.md`): a node by the
//! address of its term, its types once the metas are solved.

use std::collections::HashMap;
use std::ptr;

use super::Checker;
use super::generalize::Inst;
use crate::syntax::Term;
use crate::types::{Scheme, Type};

#[derive(Clone, Debug, Default)]
pub struct Node {
    /// The type its rule gave it.
    pub(crate) inferred: Option<Type>,
    /// What it was checked against.
    pub(crate) expected: Option<Type>,
    /// The scheme of a variable or a global, and the instance it took.
    pub(crate) instance: Option<(Scheme, Vec<Inst>)>,
    /// The scheme a `let` binds, on its right side.
    pub(crate) scheme: Option<Scheme>,
}

/// The nodes of a check, by [`key`].
pub type Trace = HashMap<usize, Node>;

/// The node of `e`: the address of the term the check visited.
pub fn key(e: &Term) -> usize {
    ptr::from_ref(e) as usize
}

impl Checker<'_> {
    pub(super) fn note(&mut self, e: &Term, f: impl FnOnce(&mut Node)) {
        if let Some(t) = &mut self.trace {
            f(t.entry(key(e)).or_default());
        }
    }

    /// The trace with every meta solved.
    pub(super) fn zonk_trace(&self, trace: &Trace) -> Trace {
        let inst = |i: &Inst| match i {
            Inst::Type(t) => Inst::Type(self.zonk(t)),
            Inst::Row(r) => Inst::Row(self.zonk_row(r)),
            Inst::Caps(c) => Inst::Caps(self.zonk_caps(c)),
            Inst::Tail(v) => Inst::Tail(v.clone()),
        };
        let scheme = |s: &Scheme| Scheme {
            body: self.zonk(&s.body),
            ..s.clone()
        };
        trace
            .iter()
            .map(|(k, n)| {
                let node = Node {
                    inferred: n.inferred.as_ref().map(|t| self.zonk(t)),
                    expected: n.expected.as_ref().map(|t| self.zonk(t)),
                    instance: n
                        .instance
                        .as_ref()
                        .map(|(s, is)| (scheme(s), is.iter().map(inst).collect())),
                    scheme: n.scheme.as_ref().map(scheme),
                };
                (*k, node)
            })
            .collect()
    }
}
