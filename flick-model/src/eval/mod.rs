//! The dynamics of λFlick (`docs/formal/core.md` §3): a machine whose
//! configuration is `⟨μ, E, e⟩`, with the context `E` kept as a stack of
//! one-frame contexts `F` (§3.2). Each transition is one rule of §3.3–3.5;
//! keeping `E` as a stack only spares re-finding the redex at every step.
//! `Λ` and the steps of regions (`docs/formal/regions.md` §3) are in
//! `regions.rs`.

mod frames;
mod prim;
pub mod regions;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub use frames::Frame;
pub use prim::{atom, equal};
pub use regions::Lambda;

use crate::native::structural_dict;
use crate::subst::{matches, subst};
use crate::syntax::{Arm, Const, Env, Head, Lit, Path, Term};

/// The top-level values of a program, by name.
#[derive(Default)]
pub struct Program {
    pub globals: HashMap<Path, Term>,
    /// The derived instances, which the model writes itself: the checker
    /// trusts them as it trusts natives.
    pub derived: HashSet<Path>,
}

/// How a run ends (§3.1), or why the model could not take a step.
#[derive(Debug)]
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
enum Control {
    Eval(Term),
    Ret(Term),
    Raise(Term),
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
    /// `Σ₀`, the services the runner provides.
    roots: Env,
    /// What `Platform.stdout_line` wrote.
    pub stdout: String,
    fuel: u64,
}

fn stuck(what: impl Into<String>) -> Outcome {
    Outcome::Stuck(what.into())
}

impl<'p> Machine<'p> {
    pub fn new(program: &'p Program, fuel: u64) -> Self {
        Machine {
            program,
            mem: Vec::new(),
            lambda: Lambda::default(),
            frames: Vec::new(),
            roots: Env::new(),
            stdout: String::new(),
            fuel,
        }
    }

    /// Runs `e` to its outcome.
    pub fn run(&mut self, e: Term) -> Outcome {
        self.drive(e, None)
    }

    /// Runs `e`, showing `watch` each state the machine passes through as
    /// the configuration `⟨μ, E[e]⟩` of §3.1: what Preservation is checked on.
    pub fn run_watched(&mut self, e: Term, watch: &mut Watch<'_>) -> Outcome {
        self.drive(e, Some(watch))
    }

    fn drive(&mut self, e: Term, mut watch: Option<&mut Watch<'_>>) -> Outcome {
        let mut control = Control::Eval(e);
        loop {
            if let Some(w) = watch.as_mut() {
                w(&self.mem, &self.lambda, &self.configuration(&control));
            }
            if self.fuel == 0 {
                return Outcome::OutOfFuel;
            }
            self.fuel -= 1;
            let next = match control {
                Control::Eval(e) => self.eval(e),
                Control::Ret(v) => match self.frames.pop() {
                    None => return Outcome::Value(v),
                    Some(f) => self.ret(f, v),
                },
                Control::Raise(v) => self.unwind(v),
            };
            control = match next {
                Ok(c) => c,
                Err(outcome) => return outcome,
            };
        }
    }

    /// `E[e]`: the frames around what the machine looks at.
    fn configuration(&self, control: &Control) -> Term {
        let e = match control {
            Control::Eval(e) | Control::Ret(e) => e.clone(),
            Control::Raise(v) => Term::Raise(Rc::new(v.clone())),
        };
        self.frames.iter().rev().fold(e, |e, f| f.plug(e))
    }

    fn push(&mut self, f: Frame) {
        self.frames.push(f);
    }

    /// One step of `e` in the current context.
    fn eval(&mut self, e: Term) -> Step {
        if e.is_value() {
            return Ok(Control::Ret(e));
        }
        Ok(match e {
            Term::Var(x) => return Err(stuck(format!("free variable `{x}`"))),
            Term::Global(p) => match self.program.globals.get(&p) {
                Some(def) => Control::Eval(def.clone()),
                None => return Err(stuck(format!("no definition of `{p}`"))),
            },
            Term::App(f, a) => {
                self.push(Frame::AppL(a));
                Control::Eval((*f).clone())
            }
            Term::Let(x, sig, e1, e2) => {
                self.push(Frame::Let(x, sig, e2));
                Control::Eval((*e1).clone())
            }
            Term::LetRec(f, sig, lam, e2) => {
                let fix = Term::Fix(f.clone(), sig, lam);
                Control::Eval(subst(&e2, &[(f, fix)]))
            }
            Term::Match(e, arms) => {
                self.push(Frame::Match(arms));
                Control::Eval((*e).clone())
            }
            Term::Tuple(xs) => self.items(true, Vec::new(), &xs),
            Term::List(xs) => self.items(false, Vec::new(), &xs),
            Term::Record(fs) => self.fields(Vec::new(), &fs),
            Term::Field(e, l) => {
                self.push(Frame::Field(l));
                Control::Eval((*e).clone())
            }
            Term::Update(e, l, v) => {
                self.push(Frame::UpdateRecord(l, v));
                Control::Eval((*e).clone())
            }
            Term::Await(e) => {
                self.push(Frame::Await);
                Control::Eval((*e).clone())
            }
            Term::Try(e, arms) => {
                self.push(Frame::Try(arms));
                Control::Eval((*e).clone())
            }
            Term::Provide(l, e) => {
                self.push(Frame::ProvideLayer(e));
                Control::Eval((*l).clone())
            }
            Term::Apply(Head::Const(c), args) => return self.delta(&c, &args),
            Term::Bind(s, e) => {
                self.push(Frame::Bind(s));
                Control::Eval((*e).clone())
            }
            Term::Join(a, b) => {
                self.push(Frame::JoinLeft(b));
                Control::Eval((*a).clone())
            }
            Term::With(env, e) => {
                self.push(Frame::With(env));
                Control::Eval((*e).clone())
            }
            Term::Handle(v, arms) => return self.handle((*v).clone(), &arms, 0),
            Term::Raise(v) => Control::Raise((*v).clone()),
            Term::Structural(class, fields) => Control::Eval(structural_dict(&class, &fields)),
            Term::Die(why) => return Err(Outcome::Die(why.to_string())),
            Term::Stuck(why) => return Err(stuck(why.to_string())),
            Term::Scope(s, body) => self.open_scope(s, &body),
            other => return Err(stuck(format!("no rule for {other:?}"))),
        })
    }

    /// The next item of a tuple or list, after the values `done`.
    fn items(&mut self, tuple: bool, done: Vec<Term>, rest: &[Term]) -> Control {
        let mut done = done;
        let mut i = 0;
        while i < rest.len() && rest[i].is_value() {
            done.push(rest[i].clone());
            i += 1;
        }
        match rest.get(i) {
            None if tuple => Control::Ret(Term::Tuple(done.into())),
            None => Control::Ret(Term::List(done.into())),
            Some(next) => {
                let next = next.clone();
                self.push(Frame::Items(tuple, done, rest[i + 1..].into()));
                Control::Eval(next)
            }
        }
    }

    /// The next field of a record, `{ℓ₁ = v₁, …, ℓᵢ = E, …}`.
    fn fields(&mut self, done: Vec<(Rc<str>, Term)>, rest: &[(Rc<str>, Term)]) -> Control {
        let mut done = done;
        let mut i = 0;
        while i < rest.len() && rest[i].1.is_value() {
            done.push(rest[i].clone());
            i += 1;
        }
        match rest.get(i) {
            None => Control::Ret(Term::Record(done.into())),
            Some((label, next)) => {
                let next = next.clone();
                self.push(Frame::Record(done, label.clone(), rest[i + 1..].into()));
                Control::Eval(next)
            }
        }
    }

    /// The value `v` returns to the frame `f`.
    fn ret(&mut self, f: Frame, v: Term) -> Step {
        Ok(match f {
            Frame::AppL(a) => {
                self.push(Frame::AppR(v));
                Control::Eval((*a).clone())
            }
            Frame::AppR(func) => return Self::apply(func, v),
            Frame::Let(x, _, e2) => Control::Eval(subst(&e2, &[(x, v)])),
            Frame::Match(arms) => return self.select(v, &arms, 0),
            Frame::Guard {
                scrutinee,
                arms,
                at,
                body,
                handler,
            } => match v {
                Term::Lit(Lit::Bool(true)) => Control::Eval(body),
                Term::Lit(Lit::Bool(false)) if handler => {
                    return self.handle(scrutinee, &arms, at + 1);
                }
                Term::Lit(Lit::Bool(false)) => return self.select(scrutinee, &arms, at + 1),
                other => return Err(stuck(format!("a guard gave {other:?}"))),
            },
            Frame::Items(tuple, mut done, rest) => {
                done.push(v);
                self.items(tuple, done, &rest)
            }
            Frame::Record(mut done, label, rest) => {
                done.push((label, v));
                self.fields(done, &rest)
            }
            Frame::Field(l) => match &v {
                Term::Record(fs) => match fs.iter().find(|(m, _)| *m == l) {
                    Some((_, x)) => Control::Ret(x.clone()),
                    None => return Err(stuck(format!("no field `{l}`"))),
                },
                _ => return Err(stuck(format!("`.{l}` of {v:?}"))),
            },
            Frame::UpdateRecord(l, e) => {
                self.push(Frame::UpdateValue(v, l));
                Control::Eval((*e).clone())
            }
            Frame::UpdateValue(record, l) => match record {
                Term::Record(fs) => Control::Ret(Term::Record(
                    fs.iter()
                        .map(|(m, x)| (m.clone(), if *m == l { v.clone() } else { x.clone() }))
                        .collect(),
                )),
                other => return Err(stuck(format!("`with` on {other:?}"))),
            },
            // `await` runs the body again each time: a task is not a cache
            Frame::Await => match v {
                Term::Task(e) => Control::Eval((*e).clone()),
                other => return Err(stuck(format!("`await` of {other:?}"))),
            },
            // try v with ā → v
            Frame::Try(_) | Frame::With(_) => Control::Ret(v),
            Frame::ProvideLayer(e) => {
                self.push(Frame::ProvideEnv(e));
                Control::Eval(build(&v)?)
            }
            Frame::ProvideEnv(e) => match v {
                Term::Env(env) => {
                    self.push(Frame::With(env));
                    Control::Eval((*e).clone())
                }
                other => return Err(stuck(format!("`provide` of {other:?}"))),
            },
            Frame::Bind(s) => Control::Ret(Term::Env(Rc::new(Env::from([(s, v)])))),
            Frame::JoinLeft(b) => {
                self.push(Frame::JoinRight(v));
                Control::Eval((*b).clone())
            }
            Frame::JoinRight(left) => match (left, v) {
                (Term::Env(a), Term::Env(b)) => {
                    let mut env = (*a).clone();
                    env.extend(b.iter().map(|(k, x)| (k.clone(), x.clone())));
                    Control::Ret(Term::Env(Rc::new(env)))
                }
                (a, b) => return Err(stuck(format!("join of {a:?} and {b:?}"))),
            },
            Frame::Inside(pi, s) => self.close(pi, s, Ok(v)),
            // what a child or a finalizer gives is dropped
            Frame::Close(pi, s, ended) => self.close(pi, s, ended),
        })
    }

    /// `f v`.
    fn apply(f: Term, v: Term) -> Step {
        match f {
            Term::Lam(x, body) => Ok(Control::Eval(subst(&body, &[(x, v)]))),
            Term::Fix(name, sig, lam) => match &*lam {
                Term::Lam(x, body) => {
                    let fix = Term::Fix(name.clone(), sig, lam.clone());
                    Ok(Control::Eval(subst(body, &[(name, fix), (x.clone(), v)])))
                }
                other => Err(stuck(format!("`fix` of {other:?}"))),
            },
            Term::Apply(head, args) if args.len() < head.arity() => {
                let mut args = args.to_vec();
                args.push(v);
                Ok(Control::Eval(Term::Apply(head, args.into())))
            }
            other => Err(stuck(format!("applied {other:?}, which is not a function"))),
        }
    }

    /// `match v with ā`, from arm `at`.
    fn select(&mut self, v: Term, arms: &Rc<[Arm]>, at: usize) -> Step {
        self.arms(v, arms, at, false)
    }

    /// `handle v with ā` (§3.5): like `match`, but no arm rethrows.
    fn handle(&mut self, v: Term, arms: &Rc<[Arm]>, at: usize) -> Step {
        self.arms(v, arms, at, true)
    }

    fn arms(&mut self, v: Term, arms: &Rc<[Arm]>, at: usize, handler: bool) -> Step {
        for (i, arm) in arms.iter().enumerate().skip(at) {
            let mut delta = Vec::new();
            if !matches(&arm.pat, &v, &mut delta) {
                continue;
            }
            let body = subst(&arm.body, &delta);
            return Ok(match &arm.guard {
                None => Control::Eval(body),
                Some(g) => {
                    self.push(Frame::Guard {
                        scrutinee: v,
                        arms: arms.clone(),
                        at: i,
                        body,
                        handler,
                    });
                    Control::Eval(subst(g, &delta))
                }
            });
        }
        if handler {
            Ok(Control::Raise(v))
        } else {
            Err(stuck(format!("no arm matches {v:?}")))
        }
    }

    /// `E[F[raise v]] → E[raise v]` up to the nearest `try` (§3.5): a
    /// `with` frame goes with the others, which restores the services. A
    /// region closes on the way (`regions.md` §3.3).
    fn unwind(&mut self, v: Term) -> Step {
        while let Some(f) = self.frames.pop() {
            match f {
                Frame::Try(arms) => return self.handle(v, &arms, 0),
                Frame::Inside(pi, s) => return Ok(self.close(pi, s, Err(v))),
                // shortcut: a child or a finalizer that fails stops the machine, its supervisor and the order of failures are λRT's (`runtime.md`); run the supervisor once a generated child can fail
                Frame::Close(pi, ..) => {
                    return Err(stuck(format!("a child or finalizer of π{pi} raised {v:?}")));
                }
                _ => {}
            }
        }
        Err(Outcome::Raise(v))
    }

    /// `lookup(E, S)`: the innermost `with` that has `S`, else `Σ₀`.
    fn lookup(&self, s: &Path) -> Option<Term> {
        self.frames
            .iter()
            .rev()
            .find_map(|f| match f {
                Frame::With(env) => env.get(s).cloned(),
                _ => None,
            })
            .or_else(|| self.roots.get(s).cloned())
    }

    /// The steps of the constants (§3.4), and of the natives that stand
    /// for foreign bindings under hypothesis Г1.
    fn delta(&mut self, c: &Const, args: &[Term]) -> Step {
        Ok(match (c, args) {
            (Const::RefMake, [v]) => {
                self.mem.push(v.clone());
                Control::Ret(Term::Loc(self.mem.len() - 1))
            }
            (Const::RefGet, [Term::Loc(l)]) => Control::Ret(self.mem[*l].clone()),
            (Const::RefSet, [Term::Loc(l), v]) => {
                self.mem[*l] = v.clone();
                Control::Ret(Term::unit())
            }
            (Const::Raise, [v]) => Control::Raise(v.clone()),
            (Const::Ask(s), [_]) => match self.lookup(s) {
                Some(v) => Control::Ret(v),
                None => return Err(stuck(format!("no implementation of service `{s}`"))),
            },
            (Const::Prim(p), args) => match prim::run(*p, args) {
                Ok(v) => Control::Eval(v),
                Err(why) => return Err(stuck(why)),
            },
            (Const::Region(p, op), args) => return self.region_step(p, *op, args),
            (Const::Native(n), args) => match (n.run)(args, &mut self.stdout) {
                Ok(t) => Control::Eval(t),
                Err(why) => return Err(stuck(format!("{}: {why}", n.export))),
            },
            (c, args) => return Err(stuck(format!("{c:?} of {args:?}"))),
        })
    }
}

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
