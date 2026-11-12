//! Programs of regions (`docs/formal/regions.md`): `scope`, a capability
//! acquired into its region, the values that hold it and how each is used;
//! and for `mutate`, a leak of each path of §4 but the fourth, which takes
//! modules. A closure sees only what its qualifier allows (§2.3 (Lam)), a
//! child only what outlives its owner (§2.4).

use std::collections::BTreeSet;
use std::rc::Rc;

use super::sealed::is_sealed;
use super::term::{Cx, extend};
use super::{Gen, MODULE, error_con, mono, nothing, some, unit};
use crate::check::scope_service;
use crate::syntax::{Arm, Con, Const, Name, Pat, Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

pub(super) fn con(name: &str, arity: usize) -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new(MODULE, name),
        name: name.into(),
        arity,
        dicts: 0,
    })
}

pub(super) fn global(module: &str, name: &str) -> Term {
    Term::Global(Path::new(module, name))
}

pub(super) fn region(s: &Name) -> Type {
    Type::Var(Var::Named(s.clone()))
}

fn handle(s: &Name) -> Type {
    Type::app(Type::con(MODULE, "Handle"), region(s))
}

pub(super) fn owner(s: &Name) -> Type {
    Type::app(Type::con("Fiber", "Owner"), region(s))
}

fn caps(regions: &BTreeSet<Name>) -> Caps {
    Caps {
        atoms: regions.iter().cloned().map(Var::Named).collect(),
        vars: BTreeSet::new(),
    }
}

/// `->{}`.
pub(super) fn empty() -> Stage {
    Stage::Runtime(Row::none(), Row::none())
}

/// `Unit ->{} Unit`.
fn thunk() -> Type {
    Type::fun(unit(), empty(), unit())
}

/// `Eff none none Unit`.
fn job() -> Type {
    Type::Eff(Row::none(), Row::none(), Rc::new(unit()))
}

pub(super) fn var(x: &Name) -> Term {
    Term::Var(x.clone())
}

pub(super) fn stmt(s: Term, rest: Term) -> Term {
    Term::Let(
        "_".into(),
        Some(Rc::new(mono(unit()))),
        Rc::new(s),
        Rc::new(rest),
    )
}

pub(super) fn let_(x: &Name, sig: Option<Scheme>, rhs: Term, rest: Term) -> Term {
    Term::Let(x.clone(), sig.map(Rc::new), Rc::new(rhs), Rc::new(rest))
}

pub(super) fn task(body: Term) -> Term {
    Term::Task(Rc::new(body))
}

pub(super) fn open() -> Term {
    Term::app(global(MODULE, "open_scoped"), Term::string("x"))
}

pub(super) fn use_(h: &Name) -> Term {
    Term::app(global(MODULE, "use"), var(h))
}

/// `Fiber.spawn o report t`, its result dropped.
pub(super) fn spawn(o: Term, t: Term, rest: Term) -> Term {
    let call = Term::apps(global("Fiber", "spawn"), [o, global(MODULE, "report"), t]);
    Term::Let("_".into(), None, Rc::new(call), Rc::new(rest))
}

/// `match x with | K y -> body y`.
pub(super) fn unpack(x: Term, k: Rc<Con>, body: impl FnOnce(Term) -> Term) -> Term {
    let y: Name = "y".into();
    let arm = Arm {
        pat: Pat::Con(k, Rc::from([Pat::Var(y.clone())])),
        guard: None,
        body: body(var(&y)),
    };
    Term::Match(Rc::new(x), Rc::from([arm]))
}

/// The regions of the `scope`s `cx` is inside, outermost first: a `scope`
/// binds its owner, `s : Owner s`.
pub(super) fn scopes(cx: &Cx) -> Vec<Name> {
    cx.iter()
        .filter(|(x, s)| s.body == owner(x))
        .map(|(x, _)| x.clone())
        .collect()
}

/// The regions of `open` that `t` names.
fn regions_of(t: &Type, open: &[Name], out: &mut BTreeSet<Name>) {
    let mut go = |t: &Type| regions_of(t, open, out);
    match t {
        Type::Var(Var::Named(n)) if open.contains(n) => {
            out.insert(n.clone());
        }
        Type::App(a, b) | Type::Fun(a, _, b) => {
            go(a);
            go(b);
        }
        Type::Eff(_, _, x) | Type::Ref(x) => go(x),
        Type::Record(f) => f.fields.values().for_each(go),
        Type::Tuple(ts) => ts.iter().for_each(go),
        Type::Captured(x, c) | Type::Bounded(x, c) => {
            go(x);
            for v in &c.atoms {
                if let Var::Named(n) = v
                    && open.contains(n)
                {
                    out.insert(n.clone());
                }
            }
        }
        _ => {}
    }
}

fn regions(t: &Type, open: &[Name]) -> BTreeSet<Name> {
    let mut out = BTreeSet::new();
    regions_of(t, open, &mut out);
    out
}

/// What a closure whose qualifier allows `allowed` may use of `cx`.
pub(super) fn visible(cx: &Cx, allowed: &BTreeSet<Name>) -> Vec<(Name, Scheme)> {
    let open = scopes(cx);
    cx.iter()
        .filter(|(_, s)| regions(&s.body, &open).is_subset(allowed))
        .cloned()
        .collect()
}

/// A statement that uses the value `x` of `t` that holds a region.
fn consume(x: &Name, t: &Type) -> Option<Term> {
    let is = |t: &Type, name: &str| matches!(t, Type::App(h, _) if **h == Type::con(MODULE, name));
    Some(match t {
        t if is(t, "Handle") => use_(x),
        t if is(t, "Wrapper") => unpack(var(x), con("Wrapper", 1), |y| {
            Term::app(global(MODULE, "use"), y)
        }),
        Type::Captured(inner, _) => match &**inner {
            Type::Con(p) if &*p.name == "Sealed" => {
                unpack(var(x), con("Sealed", 1), |g| Term::app(g, Term::unit()))
            }
            Type::Eff(..) => Term::Await(Rc::new(var(x))),
            Type::Fun(..) => Term::app(var(x), Term::unit()),
            _ => return None,
        },
        Type::Ref(_) => {
            let get = Term::app(Term::konst(Const::RefGet), var(x));
            let y: Name = "y".into();
            Term::Match(
                Rc::new(get),
                Rc::from([
                    Arm {
                        pat: Pat::Con(some(), Rc::from([Pat::Var(y.clone())])),
                        guard: None,
                        body: use_(&y),
                    },
                    Arm {
                        pat: Pat::Con(nothing(), Rc::from([])),
                        guard: None,
                        body: Term::unit(),
                    },
                ]),
            )
        }
        Type::Record(_) => Term::app(Term::Field(Rc::new(var(x)), "go".into()), Term::unit()),
        Type::Bounded(..) => Term::app(global(MODULE, "use_conn"), var(x)),
        _ => return None,
    })
}

/// The values of `cx` that hold a region, with how each is used.
pub(super) fn holders(cx: &Cx) -> Vec<Term> {
    let open = scopes(cx);
    cx.iter()
        .filter(|(_, s)| s.vars.is_empty() && !regions(&s.body, &open).is_empty())
        .filter_map(|(x, s)| consume(x, &s.body))
        .collect()
}

/// The capabilities of `cx`, with their regions.
pub(super) fn handles(cx: &Cx) -> Vec<(Name, Name)> {
    cx.iter()
        .filter_map(|(x, s)| match &s.body {
            Type::App(h, r) if **h == Type::con(MODULE, "Handle") => match &**r {
                Type::Var(Var::Named(n)) => Some((x.clone(), n.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The region of the `Scope` a budget allows to acquire into.
pub(super) fn acquires_into(budget: &Stage) -> Option<Name> {
    let Stage::Runtime(r, _) = budget else {
        return None;
    };
    match r.elems.get(&scope_service())?.first()? {
        Type::Var(Var::Named(n)) => Some(n.clone()),
        _ => None,
    }
}

impl Gen<'_> {
    /// `scope s { let h = open_scoped "x"; e }`, `e : t` within `budget`
    /// with `Scope s` for the one it had (§2.3 (Scope)).
    pub(super) fn scope(&mut self, cx: &Cx, t: &Type, budget: &Stage, size: u32) -> Term {
        let s = self.fresh("s");
        let h = self.fresh("h");
        let Stage::Runtime(r, e) = budget else {
            return self.leaf(cx, t);
        };
        let mut r = r.clone();
        r.elems.insert(scope_service(), Rc::from([region(&s)]));
        let inner = Stage::Runtime(r, e.clone());
        let cx = extend(&extend(cx, &s, mono(owner(&s))), &h, mono(handle(&s)));
        let body = self.term(&cx, t, &inner, size, true);
        Term::Scope(s, Rc::new(let_(&h, None, open(), body)))
    }

    /// An owner of `cx` and a task for its child, which holds what
    /// outlives the owner: its region and the ones around it (§2.4).
    fn child(&mut self, cx: &Cx, size: u32) -> (Name, Term) {
        let open = scopes(cx);
        let at = self.pick(open.len());
        let outer: BTreeSet<Name> = open[..=at].iter().cloned().collect();
        let body = self.term(&visible(cx, &outer), &unit(), &empty(), size, true);
        (open[at].clone(), task(body))
    }

    /// `scope s { let h = open_scoped "x"; e; value }`, `e : Unit`.
    fn nested(&mut self, cx: &Cx, budget: &Stage, size: u32, value: Term) -> Term {
        let s = self.fresh("s");
        let h = self.fresh("h");
        let Stage::Runtime(r, e) = budget else {
            return value;
        };
        let mut r = r.clone();
        r.elems.insert(scope_service(), Rc::from([region(&s)]));
        let inner = Stage::Runtime(r, e.clone());
        let cx = extend(&extend(cx, &s, mono(owner(&s))), &h, mono(handle(&s)));
        let e = self.term(&cx, &unit(), &inner, size, false);
        Term::Scope(s, Rc::new(let_(&h, None, open(), stmt(e, value))))
    }

    /// A statement that uses a value of `cx` holding a region.
    pub(super) fn consume(&mut self, cx: &Cx) -> Term {
        let mut found = holders(cx);
        found.swap_remove(self.pick(found.len()))
    }

    /// A value that holds a capability of `cx`, bound before `e : t`, or a
    /// statement that hands it to a child or a function.
    pub(super) fn hold(&mut self, cx: &Cx, t: &Type, budget: &Stage, size: u32) -> Term {
        let x = self.fresh("v");
        let rest = |g: &mut Self, held: Option<Type>| {
            let cx = match held {
                Some(ty) => extend(cx, &x, mono(ty)),
                None => cx.to_vec(),
            };
            g.term(&cx, t, budget, size, true)
        };
        match self.pick(4) {
            // a child of a `scope`, directly or through a function with
            // `Outlives` in its signature
            0 => {
                let (o, child) = self.child(cx, size);
                let rest = rest(self, None);
                match self.pick(3) {
                    0 => spawn(var(&o), child, rest),
                    1 => {
                        let call = Term::apps(var(&x), [var(&o), child]);
                        let_(
                            &x,
                            Some(spawn_in(&Row::none())),
                            spawn_in_body(),
                            stmt(call, rest),
                        )
                    }
                    // path 10: work kept with a service (`provides.rs`)
                    _ => self.kept_service(rest),
                }
            }
            // a closure handed to a parameter whose qualifier allows it
            1 => {
                let (_, rh) = self.handle(cx);
                let mine = BTreeSet::from([rh]);
                let param = Type::Captured(Rc::new(thunk()), caps(&mine));
                let sig = mono(Type::fun(param, empty(), unit()));
                let f: Name = "f".into();
                let apply = Term::Lam(f.clone(), Rc::new(Term::app(var(&f), Term::unit())));
                let body = self.term(&visible(cx, &mine), &unit(), &empty(), size, true);
                let arg = Term::Lam(self.fresh("x"), Rc::new(body));
                let call = stmt(Term::app(var(&x), arg), rest(self, None));
                let_(&x, Some(sig), apply, call)
            }
            _ => match self.held(cx, budget, size) {
                Some((sig, value, held)) if is_sealed(&held) => {
                    let rest = rest(self, Some(held));
                    let_(&x, sig, value, self.open_sealed(&x, rest))
                }
                Some((sig, value, held)) => let_(&x, sig, value, rest(self, Some(held))),
                None => rest(self, None),
            },
        }
    }

    /// A capability of `cx` and its region.
    fn handle(&mut self, cx: &Cx) -> (Name, Name) {
        let hs = handles(cx);
        hs[self.pick(hs.len())].clone()
    }

    /// A value that holds a capability of `cx`: its signature, if any, the
    /// value, and its type as the generator sees it.
    fn held(&mut self, cx: &Cx, budget: &Stage, size: u32) -> Option<(Option<Scheme>, Term, Type)> {
        let all: BTreeSet<Name> = scopes(cx).into_iter().collect();
        let (h, rh) = self.handle(cx);
        let mine = BTreeSet::from([rh.clone()]);
        let closure = |allowed: &BTreeSet<Name>| Type::Captured(Rc::new(thunk()), caps(allowed));
        // a closure's body may use what its qualifier allows
        let body = |g: &mut Self, allowed: &BTreeSet<Name>| {
            let e = g.term(&visible(cx, allowed), &unit(), &empty(), size, true);
            Term::Lam(g.fresh("x"), Rc::new(e))
        };
        let wrapper = || {
            let w = Term::app(Term::konst_con(con("Wrapper", 1)), var(&h));
            (w, Type::app(Type::con(MODULE, "Wrapper"), region(&rh)))
        };
        Some(match self.pick(8) {
            // with a qualifier written, or inferred as a local function's (`docs/formal/regions.md#local-signatures-infer-only-the-outer-qualifier`)
            0 => {
                let allowed = if self.pick(2) == 0 { &mine } else { &all };
                let sig = if self.pick(2) == 0 {
                    closure(allowed)
                } else {
                    thunk()
                };
                (Some(mono(sig)), body(self, allowed), closure(allowed))
            }
            1 => {
                let (w, held) = wrapper();
                (None, w, held)
            }
            // (Con): the field hides what the closure holds
            2 => {
                let k = Term::app(Term::konst_con(con("Sealed", 1)), body(self, &all));
                let held = Type::Captured(Rc::new(Type::con(MODULE, "Sealed")), caps(&all));
                (None, k, held)
            }
            3 => {
                let signed = self.pick(2) == 0;
                let allowed = if signed { &mine } else { &all };
                let held = Type::Captured(Rc::new(job()), caps(allowed));
                let Term::Lam(_, e) = body(self, allowed) else {
                    return None;
                };
                (signed.then(|| mono(held.clone())), task((*e).clone()), held)
            }
            4 => {
                let some_h = Term::app(Term::konst_con(some()), var(&h));
                let cell = Term::app(Term::konst(Const::RefMake), some_h);
                let option = Type::app(Type::con("Prelude", "Option"), handle(&rh));
                (None, cell, Type::Ref(Rc::new(option)))
            }
            5 => {
                let r = Term::Record(Rc::from([(Name::from("go"), body(self, &all))]));
                let held = Type::Record(Fields {
                    fields: [(Name::from("go"), closure(&all))].into(),
                    tail: None,
                });
                (None, r, held)
            }
            // what holds an outer region leaves a nested `scope`
            6 => {
                let (value, held) = if self.pick(2) == 0 {
                    wrapper()
                } else {
                    (task(use_(&h)), Type::Captured(Rc::new(job()), caps(&mine)))
                };
                (None, self.nested(cx, budget, size, value), held)
            }
            // G4: what `acquire_release` gives holds its region (`docs/formal/regions.md#acquired-values-carry-their-region`)
            _ => {
                let into = BTreeSet::from([acquires_into(budget)?]);
                let held = Type::Bounded(Rc::new(Type::con(MODULE, "Raw")), caps(&into));
                (None, acquire(), held)
            }
        })
    }
}

/// `Resource.acquire_release (task { make_conn "db" }) close_conn`.
fn acquire() -> Term {
    let make = task(Term::app(global(MODULE, "make_conn"), Term::string("db")));
    Term::apps(
        global("Resource", "acquire_release"),
        [make, global(MODULE, "close_conn")],
    )
}

/// `spawn_in : forall t c. (Fiber.Outlives c t, Fiber.Provided r t) =>
/// Fiber.Owner t -> (Eff {Scope t | r} none Unit)^c ->{r;} Unit`, `r` the
/// services; without them, no `Provided`.
pub(super) fn spawn_in(services: &Row) -> Scheme {
    let (t, c): (Name, Name) = ("t".into(), "c".into());
    let mut r = services.clone();
    r.elems.insert(scope_service(), Rc::from([region(&t)]));
    let work = Type::Captured(
        Rc::new(Type::Eff(r, Row::none(), Rc::new(unit()))),
        Caps::var(Var::Named(c.clone())),
    );
    let call = Stage::Runtime(services.clone(), Row::none());
    let mut body = Type::fun(owner(&t), Stage::Pure, Type::fun(work, call, unit()));
    if !services.is_none() {
        body = body.with_provided(services, &region(&t));
    }
    Scheme {
        vars: vec![(t.clone(), Kind::Region), (c.clone(), Kind::Captures)],
        body,
        outlives: vec![(Caps::var(Var::Named(c)), region(&t))],
    }
}

/// `fun o w -> let _ = Fiber.spawn o report w; ()`.
pub(super) fn spawn_in_body() -> Term {
    let (o, w): (Name, Name) = ("o".into(), "w".into());
    Term::Lam(
        o.clone(),
        Rc::new(Term::Lam(
            w.clone(),
            Rc::new(spawn(var(&o), var(&w), Term::unit())),
        )),
    )
}

/// `e` after a leak of path `path` of `regions.md` §4: a value that holds
/// a region leaves its `scope`, or is held past it; `e` may use it as
/// `x`. Every one is rejected.
pub(super) fn leak(g: &mut Gen, path: usize, e: impl Fn(&Name) -> Term) -> Term {
    let (s, h, x) = (g.fresh("s"), g.fresh("h"), g.fresh("v"));
    let scoped = |body: Term| Term::Scope(s.clone(), Rc::new(let_(&h, None, open(), body)));
    let leaves = |value: Term| let_(&x, None, scoped(value), e(&x));
    let lam = |body: Term| Term::lam("y", body);
    match path % 10 {
        // the result, directly or in data
        0 => leaves(var(&h)),
        1 => leaves(Term::app(Term::konst_con(con("Wrapper", 1)), var(&h))),
        // a closure or a task
        2 => leaves(task(use_(&h))),
        3 => leaves(Term::Record(Rc::from([(Name::from("go"), lam(use_(&h)))]))),
        // a closure handed to a parameter that allows no captures
        4 => {
            let f: Name = "f".into();
            let sig = mono(Type::fun(thunk(), empty(), unit()));
            let apply = Term::Lam(f.clone(), Rc::new(Term::app(var(&f), Term::unit())));
            let call = Term::app(var(&x), lam(use_(&h)));
            stmt(scoped(let_(&x, Some(sig), apply, call)), e(&x))
        }
        // opaque packaging
        5 => leaves(Term::app(Term::konst_con(con("Sealed", 1)), lam(use_(&h)))),
        // a cell made before the `scope`
        6 => {
            let cell = Term::app(Term::konst(Const::RefMake), Term::konst_con(nothing()));
            let some_h = Term::app(Term::konst_con(some()), var(&h));
            let set = Term::apps(Term::konst(Const::RefSet), [var(&x), some_h]);
            let_(&x, None, cell, stmt(scoped(set), e(&x)))
        }
        // an error that carries the capability, even one caught inside
        7 => {
            let leaked = Term::app(Term::konst_con(error_con("Leaked")), var(&h));
            let raise = Term::app(Term::konst(Const::Raise), leaked);
            let arm = Arm {
                pat: Pat::Con(error_con("Leaked"), Rc::from([Pat::Wild])),
                guard: None,
                body: Term::unit(),
            };
            stmt(scoped(Term::Try(Rc::new(raise), Rc::from([arm]))), e(&x))
        }
        // a child of the outer `scope` that holds the inner region
        8 => {
            let o = g.fresh("s");
            let child = spawn(var(&o), task(use_(&h)), Term::unit());
            stmt(Term::Scope(o, Rc::new(scoped(child))), e(&x))
        }
        // what a foreign acquisition gives (G4)
        _ => let_(&x, None, Term::Scope(s.clone(), Rc::new(acquire())), e(&x)),
    }
}
