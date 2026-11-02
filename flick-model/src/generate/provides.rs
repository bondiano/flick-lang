//! Path 10 of `docs/formal/regions.md` §4 (§2.7, `docs/formal/runtime.md#a-provide-service-outlives-the-release-of-its-layer`): a child or a
//! finalizer an owner keeps uses a service, which comes from a provider
//! that must outlive the owner. The forms the rules accept, and the leaks
//! a `mutate` makes: the owner's `scope` around the `provide` that builds
//! the service.

use super::regions::{global, let_, spawn, spawn_in, spawn_in_body, stmt, task, var};
use super::{Gen, implementation, operation, row, service};
use crate::syntax::{Const, Name, Term};

/// `provide (Layer.succeed Log {…}) { e }`.
fn provide(e: Term) -> Term {
    let layer = Term::app(
        Term::konst(Const::LayerSucceed(service("Log"))),
        implementation("Log", None),
    );
    Term::Provide(layer.into(), e.into())
}

/// `task { Log.log "kept" }`: work that uses the service.
fn work() -> Term {
    task(Term::app(operation("Log", "log"), Term::string("kept")))
}

/// `start : forall t c. (Fiber.Outlives c t, Fiber.Provided {Log} t) =>
/// Fiber.Owner t -> (Eff {Scope t | Log} none Unit)^c ->{Log;} Unit`: the
/// signature gives `Provided` to its body and wants it of its caller.
fn start(rest: Term) -> Term {
    let_(
        &"start".into(),
        Some(spawn_in(&row(&["Log"]))),
        spawn_in_body(),
        rest,
    )
}

/// Owner `o` keeps work that uses `Log`: a child, a finalizer of the
/// innermost `scope`, or a child through `start`.
fn keep(g: &mut Gen, o: &Name) -> Term {
    match g.pick(3) {
        0 => spawn(var(o), work(), Term::unit()),
        1 => Term::app(global("Scope", "add_finalizer"), work()),
        _ => start(Term::apps(var(&"start".into()), [var(o), work()])),
    }
}

/// `scope s { … }` with `s` fresh.
fn scope(g: &mut Gen, body: impl FnOnce(&mut Gen, &Name) -> Term) -> Term {
    let s = g.fresh("s");
    let body = body(g, &s);
    Term::Scope(s, body.into())
}

impl Gen<'_> {
    /// `provide L { scope s { keep } }; rest`: the owner's region is
    /// inside the `provide`, which outlives it.
    pub(super) fn kept_service(&mut self, rest: Term) -> Term {
        let kept = scope(self, keep);
        stmt(provide(kept), rest)
    }
}

/// `scope o { provide L { keep } }; e`: the `provide` ends before `o`
/// closes, and the work uses its service after (`FLICK-G003`).
pub(super) fn leak(g: &mut Gen, e: Term) -> Term {
    let leaked = scope(g, |g, o| provide(keep(g, o)));
    stmt(leaked, e)
}
