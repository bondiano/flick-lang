//! `F`, the one-frame contexts of `docs/formal/core.md` §3.2, and the
//! frame a guard is evaluated in. The machine never looks inside `λ`,
//! `task`, arms or the body of `provide`: those wait in a frame as terms.

use std::rc::Rc;

use super::regions::Ended;
use crate::handler::{Cont, Handler};
use crate::syntax::{Arm, Env, Lit, Name, Pat, Path, Term};
use crate::types::Scheme;

#[derive(Debug)]
pub enum Frame {
    /// `□ e`.
    AppL(Rc<Term>),
    /// `v □`.
    AppR(Term),
    /// `let x = □ in e`, or `let x : σ = □ in e`.
    Let(Name, Option<Rc<Scheme>>, Rc<Term>),
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
    /// `with θ □` again around work a region runs as it closes: the `θ`
    /// of a `provide` around the place that registered it, which may have
    /// ended (`regions.md` §2.7).
    Kept(Rc<Env>),
    /// `inside π □` (`regions.md` §3.2), its types naming the region `Name`.
    Inside(usize, Name),
    /// `inside π □` while `π` closes (§3.3): `□` is a child or a
    /// finalizer, and the body ended with what it keeps.
    Close(usize, Name, Ended),
    /// A handler of effects around its body, `id` while it runs
    /// (`handler.rs`).
    Handler(usize, Rc<Handler>),
    /// The rest of a body under its handler, the handler's frame first,
    /// for the value of a `tail` arm that runs below it.
    Resume(Vec<Frame>),
    /// The arm of a `control` operation of the handler `id`, and the rest
    /// of the body it may resume.
    Discontinue(usize, Cont),
}

impl Frame {
    /// `F[e]`: the term this frame stands for around `e`, so a machine
    /// state can be read back as the configuration `E[e]` of §3.1.
    pub fn plug(&self, e: Term) -> Term {
        let e = Rc::new(e);
        match self {
            Frame::AppL(a) => Term::App(e, a.clone()),
            Frame::AppR(f) => Term::App(Rc::new(f.clone()), e),
            Frame::Let(x, sig, body) => Term::Let(x.clone(), sig.clone(), e, body.clone()),
            Frame::Match(arms) => Term::Match(e, arms.clone()),
            // match g with True → e[δ] | False → match v with ā (§3.3, §3.5)
            Frame::Guard {
                scrutinee,
                arms,
                at,
                body,
                handler,
            } => {
                let v = Rc::new(scrutinee.clone());
                let rest: Rc<[Arm]> = arms[at + 1..].into();
                let otherwise = if *handler {
                    Term::Handle(v, rest)
                } else {
                    Term::Match(v, rest)
                };
                let arm = |b: bool, body: Term| Arm {
                    pat: Pat::Lit(Lit::Bool(b)),
                    guard: None,
                    body,
                };
                Term::Match(
                    e,
                    Rc::from([arm(true, body.clone()), arm(false, otherwise)]),
                )
            }
            Frame::Items(tuple, done, rest) => {
                let items: Rc<[Term]> = done
                    .iter()
                    .cloned()
                    .chain([(*e).clone()])
                    .chain(rest.iter().cloned())
                    .collect();
                if *tuple {
                    Term::Tuple(items)
                } else {
                    Term::List(items)
                }
            }
            Frame::Record(done, label, rest) => Term::Record(
                done.iter()
                    .cloned()
                    .chain([(label.clone(), (*e).clone())])
                    .chain(rest.iter().cloned())
                    .collect(),
            ),
            Frame::Field(l) => Term::Field(e, l.clone()),
            Frame::UpdateRecord(l, v) => Term::Update(e, l.clone(), v.clone()),
            Frame::UpdateValue(r, l) => Term::Update(Rc::new(r.clone()), l.clone(), e),
            Frame::Await => Term::Await(e),
            Frame::Try(arms) => Term::Try(e, arms.clone()),
            Frame::ProvideLayer(body) | Frame::ProvideEnv(body) => Term::Provide(e, body.clone()),
            Frame::Bind(s) => Term::Bind(s.clone(), e),
            Frame::JoinLeft(b) => Term::Join(e, b.clone()),
            Frame::JoinRight(a) => Term::Join(Rc::new(a.clone()), e),
            Frame::With(env) | Frame::Kept(env) => Term::With(env.clone(), e),
            Frame::Inside(pi, s) => Term::Inside(*pi, s.clone(), e),
            // inside π (let _ = □ in w)
            Frame::Close(pi, s, ended) => {
                let w = match ended {
                    Ok(v) => v.clone(),
                    Err(v) => Term::Raise(Rc::new(v.clone())),
                };
                let body = Term::Let("_".into(), None, e, Rc::new(w));
                Term::Inside(*pi, s.clone(), Rc::new(body))
            }
            Frame::Handler(_, h) => Term::Handler(Rc::new(Handler {
                body: (*e).clone(),
                ..(**h).clone()
            })),
            // K[e]: the arm's value goes into the rest of the body
            Frame::Resume(k) => k.iter().rev().fold((*e).clone(), |e, f| f.plug(e)),
            Frame::Discontinue(..) => (*e).clone(),
        }
    }
}
