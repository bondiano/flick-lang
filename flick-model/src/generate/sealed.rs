//! A `match` on data with hidden storage, after a value of `Sealed` is
//! bound (`regions.rs`): the certificates type its field at the row the
//! value hides (`docs/formal/regions.md` §1.4, (Pat-Adt)).

use std::rc::Rc;

use super::regions::{con, empty, let_, stmt, unpack, var};
use super::{Gen, MODULE, mono, unit};
use crate::syntax::{Name, Term};
use crate::types::Type;

/// Whether `t` is `Sealed` at the capture row its value hides.
pub(super) fn is_sealed(t: &Type) -> bool {
    matches!(t, Type::Captured(x, _) if **x == Type::con(MODULE, "Sealed"))
}

impl Gen<'_> {
    /// Uses of the `Sealed` value `x` before `rest`, each a `match`: one at
    /// its own type, one through a function whose signature writes `Sealed`
    /// bare, so its hidden row is read. No pick: the run of programs keeps
    /// its choices.
    pub(super) fn open_sealed(&mut self, x: &Name, rest: Term) -> Term {
        let call = |g: Term| Term::app(g, Term::unit());
        let f = self.fresh("open");
        let k = self.fresh("k");
        let bare = Type::fun(Type::con(MODULE, "Sealed"), empty(), unit());
        let body = Term::Lam(k.clone(), Rc::new(unpack(var(&k), con("Sealed", 1), call)));
        let through = stmt(Term::app(var(&f), var(x)), rest);
        stmt(
            unpack(var(x), con("Sealed", 1), call),
            let_(&f, Some(mono(bare)), body, through),
        )
    }
}
