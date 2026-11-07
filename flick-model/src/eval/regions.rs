//! The dynamics of regions (`docs/formal/regions.md` §3): `Λ`, the step of
//! `scope`, the steps of the foreign bindings of regions (§3.2) and the
//! closing of a region (§3.3). The children of a region run when it
//! closes: one order of steps the rules allow, the latest a child can run.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use super::{Control, Frame, Machine, Outcome, Step, stuck};
use crate::handler::Handler;
use crate::subst::{any, subst};
use crate::syntax::{Const, Env, Name, Path, RegionOp, Term};

/// What `Λ` keeps of a region `π`.
#[derive(Clone, Debug)]
pub struct Region {
    pub live: bool,
    /// The region it was made in, which closes after it (L5).
    pub parent: Option<usize>,
    /// `F(π)`, run last first.
    pub finalizers: Vec<Finalizer>,
    /// `K(π)`: each child's supervisor and task, run first to last.
    pub children: Vec<(Term, Term, Kept)>,
}

/// The environments `θ` of the `provide`s around the place that
/// registered work: it runs with them (`runtime.md` §3.5), outermost first.
pub type Kept = Rc<[Keep]>;

/// What a child or a finalizer keeps of what is around its start.
#[derive(Clone, Debug)]
pub enum Keep {
    Env(Rc<Env>),
    /// A handler of effects: its operations stay in reach, but its `return`
    /// is its body's, not the child's.
    Handler(usize, Rc<Handler>),
}

#[derive(Clone, Debug)]
pub enum Finalizer {
    /// `close_n`: releases object `n`.
    Release(usize),
    /// A task, awaited with what it kept.
    Run(Term, Kept),
}

/// `Λ`, the objects released so far, and the environments of the
/// `provide`s that ended (`regions.md` §2.7).
#[derive(Clone, Debug, Default)]
pub struct Lambda {
    pub regions: Vec<Region>,
    pub released: HashSet<usize>,
    /// Kept alive, so no new environment takes the address of one.
    pub ended: Vec<Rc<Env>>,
    /// Handlers that ended: a child that kept one uses it after its end.
    pub ended_handlers: HashSet<usize>,
    objects: usize,
}

impl Lambda {
    /// Region `π` and every region it was made in: what outlives it.
    pub fn outlived_by(&self, pi: usize) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        let mut at = Some(pi);
        while let Some(p) = at {
            out.insert(p);
            at = self.regions[p].parent;
        }
        out
    }
}

/// The name a type gives region `π`.
pub fn region_name(pi: usize) -> Name {
    format!("π{pi}").into()
}

/// `reach_μ(e)` of `regions.md` §5.2: the regions `e` names, and those of
/// the cells it reaches.
pub fn reach(e: &Term, mem: &[Term]) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    let mut cells = Vec::new();
    let mut todo = vec![e.clone()];
    while let Some(t) = todo.pop() {
        let found = RefCell::new(Vec::new());
        any(&t, &|x| {
            match x {
                Term::Own(p) | Term::Cap(_, Some(p), _) | Term::Inside(p, ..) => {
                    found.borrow_mut().push(Ok(*p));
                }
                Term::Loc(l) => found.borrow_mut().push(Err(*l)),
                _ => {}
            }
            false
        });
        for f in found.into_inner() {
            match f {
                Ok(p) => {
                    out.insert(p);
                }
                Err(l) if !cells.contains(&l) => {
                    cells.push(l);
                    todo.push(mem[l].clone());
                }
                Err(_) => {}
            }
        }
    }
    out
}

/// How the body of a closing region ended: a value, or `raise v`.
pub type Ended = Result<Term, Term>;

impl Machine<'_> {
    /// `scope s { e } → inside π e[s := own π]`, `π` fresh and live.
    pub(super) fn open_scope(&mut self, s: Name, body: &Term) -> Control {
        let pi = self.lambda.regions.len();
        self.lambda.regions.push(Region {
            live: true,
            parent: self.owner(),
            finalizers: Vec::new(),
            children: Vec::new(),
        });
        let body = subst(body, &[(s.clone(), Term::Own(pi))]);
        self.push(Frame::Inside(pi, s));
        Control::Eval(body)
    }

    /// The region a step that needs `Scope ϱ` acquires into: the
    /// innermost `inside` around it. G6 makes the owner lexical; the
    /// generator never runs a step of an outer `Scope` in an inner `scope`.
    fn owner(&self) -> Option<usize> {
        self.frames.iter().rev().find_map(|f| match f {
            Frame::Inside(p, _) | Frame::Close(p, ..) => Some(*p),
            _ => None,
        })
    }

    /// One step of closing `π` (§3.3): the next child, else the last
    /// finalizer, else `π` is closed and its body's end goes on.
    pub(super) fn close(&mut self, pi: usize, s: Name, ended: Ended) -> Control {
        let r = &mut self.lambda.regions[pi];
        let work = if r.children.is_empty() {
            loop {
                match r.finalizers.pop() {
                    Some(Finalizer::Release(n)) => {
                        self.lambda.released.insert(n);
                    }
                    Some(Finalizer::Run(t, kept)) => break Some((t, kept)),
                    None => break None,
                }
            }
        } else {
            let (_, t, kept) = r.children.remove(0);
            Some((t, kept))
        };
        if let Some((t, kept)) = work {
            self.push(Frame::Close(pi, s, ended));
            for keep in kept.iter() {
                self.push(match keep {
                    Keep::Env(env) => Frame::Kept(env.clone()),
                    Keep::Handler(id, h) => Frame::KeptHandler(*id, h.clone()),
                });
            }
            return Control::Eval(Term::Await(Rc::new(t)));
        }
        self.lambda.regions[pi].live = false;
        match ended {
            Ok(v) => Control::Ret(v),
            Err(v) => Control::Raise(v),
        }
    }

    /// The environments of the `provide`s around the step.
    fn kept(&self) -> Kept {
        self.frames
            .iter()
            .filter_map(|f| match f {
                Frame::With(env) | Frame::Kept(env) => Some(Keep::Env(env.clone())),
                Frame::Handler(id, h) | Frame::KeptHandler(id, h) => {
                    Some(Keep::Handler(*id, h.clone()))
                }
                _ => None,
            })
            .collect()
    }

    fn object(&mut self, p: &Path, region: Option<usize>) -> Term {
        self.lambda.objects += 1;
        Term::Cap(p.clone(), region, self.lambda.objects)
    }

    fn live(&self, pi: usize) -> Result<usize, Outcome> {
        if self.lambda.regions[pi].live {
            Ok(pi)
        } else {
            Err(stuck(format!("a registration with closed region π{pi}")))
        }
    }

    /// The steps of §3.2.
    pub(super) fn region_step(&mut self, p: &Path, op: RegionOp, args: &[Term]) -> Step {
        let owner = || {
            self.owner()
                .ok_or_else(|| stuck(format!("`{p}` outside every region")))
        };
        Ok(match (op, args) {
            (RegionOp::Acquire, [_]) => {
                let owner = owner()?;
                let cap = self.object(p, Some(owner));
                let Term::Cap(_, _, n) = cap else {
                    unreachable!("an object")
                };
                self.lambda.regions[owner]
                    .finalizers
                    .push(Finalizer::Release(n));
                Control::Ret(cap)
            }
            (RegionOp::Use, [Term::Cap(_, region, n)]) => {
                let closed = region.is_some_and(|r| !self.lambda.regions[r].live);
                if closed || self.lambda.released.contains(n) {
                    return Err(stuck(format!("use after close of object {n}")));
                }
                Control::Ret(Term::unit())
            }
            (RegionOp::Make, [_]) => Control::Ret(self.object(p, None)),
            (RegionOp::Release, [Term::Cap(_, _, n)]) => {
                self.lambda.released.insert(*n);
                Control::Ret(Term::unit())
            }
            (RegionOp::Spawn, [Term::Own(pi), f, t]) => {
                let pi = self.live(*pi)?;
                let kept = self.kept();
                self.lambda.regions[pi]
                    .children
                    .push((f.clone(), t.clone(), kept));
                Control::Ret(self.object(p, None))
            }
            (RegionOp::AddFinalizer, [t]) => {
                let pi = self.live(owner()?)?;
                let kept = self.kept();
                self.lambda.regions[pi]
                    .finalizers
                    .push(Finalizer::Run(t.clone(), kept));
                Control::Ret(Term::unit())
            }
            // `let x = await t in Scope.add_finalizer (task (release x)); x`
            (RegionOp::AcquireRelease, [t, release]) => {
                let x: Name = "$acquired".into();
                let x_var = || Term::Var(x.clone());
                let add = Term::konst(Const::Region(
                    Path::new("Scope", "add_finalizer"),
                    RegionOp::AddFinalizer,
                ));
                let fin = Term::Task(Rc::new(Term::app(release.clone(), x_var())));
                let rest = Term::Let(
                    "_".into(),
                    None,
                    Rc::new(Term::app(add, fin)),
                    Rc::new(x_var()),
                );
                Control::Eval(Term::Let(
                    x.clone(),
                    None,
                    Rc::new(Term::Await(Rc::new(t.clone()))),
                    Rc::new(rest),
                ))
            }
            (op, args) => return Err(stuck(format!("{op:?} of {args:?}"))),
        })
    }
}
