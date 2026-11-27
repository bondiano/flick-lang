//! The dynamics of λFlick (`docs/formal/core.md` §3): a machine whose
//! configuration is `⟨μ, E, e⟩`, with the context `E` kept as a stack of
//! one-frame contexts `F` (§3.2). Each transition is one rule of §3.3–3.5;
//! keeping `E` as a stack only spares re-finding the redex at every step.
//! `Λ` and the steps of regions (`docs/formal/regions.md` §3) are in
//! `regions.rs`.

mod frames;
mod handler;
mod prim;
pub mod regions;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub use frames::Frame;
pub use prim::{atom, equal};
pub use regions::Lambda;

use crate::native::{seq, settled, structural_dict};
use crate::subst::{matches, subst};
use crate::syntax::{Arm, Const, Env, GETTER, Head, Lit, Name, Path, Term};

/// The top-level values of a program, by name.
#[derive(Clone, Default)]
pub struct Program {
    pub globals: HashMap<Path, Term>,
    /// The derived instances, which the model writes itself: the checker
    /// trusts them as it trusts natives.
    pub derived: HashSet<Path>,
}

/// How a run ends (§3.1), or why the model could not take a step.
#[derive(Clone, Debug)]
pub enum Outcome {
    Value(Term),
    /// An expected error that no `try` took.
    Raise(Term),
    Die(String),
    /// No rule applies: a gap of the translation, or a term the checker
    /// would reject.
    Stuck(String),
    OutOfFuel,
}

/// What the machine does next.
#[derive(Clone, Debug)]
pub(crate) enum Control {
    Eval(Term),
    Ret(Term),
    Raise(Term),
}

/// Owned machine execution state detached from `&Program` so a live session
/// can inspect, type-check, hot-swap globals, or migrate state cells between
/// individual small steps.
#[derive(Clone, Debug, Default)]
pub(crate) struct MachineState {
    pub(crate) mem: Vec<Term>,
    pub(crate) lambda: Lambda,
    pub(crate) frames: Vec<Frame>,
    pub(crate) roots: Env,
    pub(crate) stdout: String,
    pub(crate) fuel: u64,
    pub(crate) handlers: usize,
}

type Step = Result<Control, Outcome>;

/// What sees each configuration `⟨Λ, μ, E[e]⟩`: `μ`, `Λ`, then `E[e]`.
pub type Watch<'a> = dyn FnMut(&[Term], &Lambda, &Term) + 'a;

pub struct Machine<'p> {
    program: &'p Program,
    /// `μ`.
    mem: Vec<Term>,
    lambda: Lambda,
    frames: Vec<Frame>,
    /// `Σ₀`, the services the runner provides: none until
    /// [`Machine::provide_roots`].
    roots: Env,
    /// What `Platform.stdout_line` wrote.
    pub stdout: String,
    fuel: u64,
    /// Handlers installed so far: the `id` of the next is one more.
    handlers: usize,
}

fn stuck(what: impl Into<String>) -> Outcome {
    Outcome::Stuck(what.into())
}

include!("evaluation.rs");
include!("continuations.rs");

/// `build(v)` of a layer value (§3.4).
fn build(v: &Term) -> Result<Term, Outcome> {
    match v {
        Term::Apply(Head::Const(Const::LayerSucceed(s)), args) => Ok(Term::Env(Rc::new(
            Env::from([(s.clone(), args[0].clone())]),
        ))),
        Term::Apply(Head::Const(Const::LayerEffect(s)), args) => Ok(Term::Bind(
            s.clone(),
            Rc::new(Term::Await(Rc::new(args[0].clone()))),
        )),
        Term::Apply(Head::Const(Const::LayerMerge), args) => Ok(Term::Join(
            Rc::new(build(&args[0])?),
            Rc::new(build(&args[1])?),
        )),
        other => Err(stuck(format!("{other:?} is not a layer"))),
    }
}
