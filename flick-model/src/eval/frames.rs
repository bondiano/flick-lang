//! `F`, the one-frame contexts of `docs/formal/core.md` §3.2, and the
//! frame a guard is evaluated in. The machine never looks inside `λ`,
//! `task`, arms or the body of `provide`: those wait in a frame as terms.

use std::rc::Rc;

use crate::syntax::{Arm, Env, Name, Path, Term};

#[derive(Debug)]
pub enum Frame {
    /// `□ e`.
    AppL(Rc<Term>),
    /// `v □`.
    AppR(Term),
    /// `let x = □ in e`.
    Let(Name, Rc<Term>),
    /// `match □ with ā`.
    Match(Rc<[Arm]>),
    /// `match g[δ] with True → e[δ] | False → match v with ā` of §3.3,
    /// and its `handle` form of §3.5: arm `at` of `arms` matched
    /// `scrutinee`, and its guard is running.
    Guard {
        scrutinee: Term,
        arms: Rc<[Arm]>,
        at: usize,
        body: Term,
        handler: bool,
    },
    /// The items of a tuple (`true`) or a list: those done, then the rest.
    Items(bool, Vec<Term>, Rc<[Term]>),
    /// `{ℓ₁ = v₁, …, ℓᵢ = □, ℓᵢ₊₁ = eᵢ₊₁, …}`.
    Record(Vec<(Name, Term)>, Name, Rc<[(Name, Term)]>),
    /// `□.ℓ`.
    Field(Name),
    /// `{□ with ℓ = e}`.
    UpdateRecord(Name, Rc<Term>),
    /// `{v with ℓ = □}`.
    UpdateValue(Term, Name),
    /// `await □`.
    Await,
    /// `try □ with ā`.
    Try(Rc<[Arm]>),
    /// `provide □ e`.
    ProvideLayer(Rc<Term>),
    /// `provide □ e` once the layer is a value: `□` builds its `env`.
    ProvideEnv(Rc<Term>),
    /// `bind_S □`.
    Bind(Path),
    /// `join □ e`.
    JoinLeft(Rc<Term>),
    /// `join v □`.
    JoinRight(Term),
    /// `with θ □`.
    With(Rc<Env>),
}
