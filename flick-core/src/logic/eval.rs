//! Normalization by evaluation: a term evaluates to a value, unknown where
//! it reads a variable of the statement, and a value reads back to its
//! normal form. Case analysis refines a variable to a constructor; reading
//! one looks its refinement up. A deterministic budget bounds the work.

use std::rc::Rc;

use super::defs::Admitted;
use super::term::{Arm, Ctor, Def, Literal, Pat, Term};

#[derive(Debug, Clone)]
pub enum Value {
    /// A variable of the statement, or one a rule introduced.
    Sym(Rc<str>),
    /// A variable bound inside a normal form, by its depth.
    Bound(u32),
    Lit(Literal),
    /// A constructor applied to its first fields.
    Con(Rc<Ctor>, Rc<[Value]>),
    Closure(Env, Rc<str>, Rc<Term>),
    /// An application of an unknown function.
    App(Rc<Value>, Rc<Value>),
    /// A match on an unknown value.
    Match(Rc<Value>, Env, Rc<[Arm]>),
    /// A definition left folded.
    Def(Rc<Def>),
}

/// The values of the local variables in scope.
#[derive(Debug, Clone, Default)]
pub struct Env(Option<Rc<(Rc<str>, Value, Env)>>);

impl Env {
    pub fn with(&self, name: Rc<str>, value: Value) -> Env {
        Env(Some(Rc::new((name, value, self.clone()))))
    }

    fn get(&self, name: &str) -> Option<&Value> {
        let mut at = self;
        while let Some(cell) = &at.0 {
            if &*cell.0 == name {
                return Some(&cell.1);
            }
            at = &cell.2;
        }
        None
    }
}

/// A normal form: equal normal forms are the same up to bound names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nf {
    Sym(Rc<str>),
    Bound(u32),
    Lit(Literal),
    Con(Rc<Ctor>, Vec<Nf>),
    Lam(Box<Nf>),
    App(Box<Nf>, Box<Nf>),
    Match(Box<Nf>, Vec<(NfPat, Nf)>),
    Def(Rc<Def>),
}

/// A pattern of a normal form: its binders are numbered by depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NfPat {
    Wild,
    Bind,
    Lit(Literal),
    Con(Rc<Ctor>, Vec<NfPat>),
}

/// Why evaluation stopped short of a normal form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The budget ran out: nothing is known.
    Budget,
    /// A `match` has no arm for the value: the computation fails.
    Missed,
    /// A definition no admission let in.
    Unadmitted,
}

/// The work allowed for one proof.
pub const STEPS: u64 = 200_000;
/// The depth of the evaluation, which also bounds the native stack.
pub const DEPTH: u32 = 400;

pub struct Machine<'a> {
    /// What case analysis knows of the variables of the statement.
    pub refined: &'a [(Rc<str>, Value)],
    /// The admitted definitions, which evaluation unfolds.
    defs: &'a Admitted,
    /// Whether definitions unfold: a lemma's instance is compared with them
    /// folded, so it is about exactly the definitions it names.
    pub unfold: bool,
    pub steps: u64,
    depth: u32,
}

enum Matched {
    Yes(Vec<(Rc<str>, Value)>),
    No,
    Unknown,
}

impl<'a> Machine<'a> {
    pub fn new(refined: &'a [(Rc<str>, Value)], defs: &'a Admitted, steps: u64) -> Self {
        Machine {
            refined,
            defs,
            unfold: true,
            steps,
            depth: 0,
        }
    }

    fn enter(&mut self) -> Result<(), Stop> {
        if self.steps == 0 || self.depth >= DEPTH {
            return Err(Stop::Budget);
        }
        self.steps -= 1;
        self.depth += 1;
        Ok(())
    }

    /// `v` with each refined variable at its head replaced.
    fn whnf(&self, mut v: Value) -> Value {
        while let Value::Sym(x) = &v {
            match self.refined.iter().rev().find(|(n, _)| n == x) {
                Some((_, r)) => v = r.clone(),
                None => break,
            }
        }
        v
    }

    pub fn eval(&mut self, t: &Term, env: &Env) -> Result<Value, Stop> {
        self.enter()?;
        let v = match t {
            Term::Var(x) => env.get(x).cloned().unwrap_or_else(|| Value::Sym(x.clone())),
            Term::Lit(l) => Value::Lit(l.clone()),
            Term::Lam(x, body) => Value::Closure(env.clone(), x.clone(), body.clone()),
            Term::App(f, a) => {
                let f = self.eval(f, env)?;
                let a = self.eval(a, env)?;
                self.apply(f, a)?
            }
            Term::Con(c) => Value::Con(c.clone(), Rc::from([])),
            Term::Match(s, arms) => {
                let s = self.eval(s, env)?;
                self.case(s, env, arms)?
            }
            Term::Def(d) if !self.unfold => Value::Def(d.clone()),
            Term::Def(d) => match self.defs.body(d) {
                Some(body) => self.eval(body, &Env::default())?,
                None => return Err(Stop::Unadmitted),
            },
            Term::Let(x, value, body) => {
                let value = self.eval(value, env)?;
                self.eval(body, &env.with(x.clone(), value))?
            }
        };
        self.depth -= 1;
        Ok(v)
    }

    pub fn apply(&mut self, f: Value, a: Value) -> Result<Value, Stop> {
        Ok(match self.whnf(f) {
            Value::Closure(env, x, body) => self.eval(&body, &env.with(x, a))?,
            Value::Con(c, args) if args.len() < c.arity => {
                let mut args = args.to_vec();
                args.push(a);
                Value::Con(c, args.into())
            }
            f => Value::App(Rc::new(f), Rc::new(a)),
        })
    }

    fn case(&mut self, s: Value, env: &Env, arms: &Rc<[Arm]>) -> Result<Value, Stop> {
        for arm in arms.iter() {
            match self.matches(&arm.pat, &s) {
                Matched::Yes(binds) => {
                    let env = binds
                        .into_iter()
                        .fold(env.clone(), |e, (x, v)| e.with(x, v));
                    return self.eval(&arm.body, &env);
                }
                Matched::No => {}
                Matched::Unknown => return Ok(Value::Match(Rc::new(s), env.clone(), arms.clone())),
            }
        }
        Err(Stop::Missed)
    }

    fn matches(&self, pat: &Pat, v: &Value) -> Matched {
        match pat {
            Pat::Wild => Matched::Yes(Vec::new()),
            Pat::Bind(x) => Matched::Yes(vec![(x.clone(), v.clone())]),
            Pat::Lit(l) => match self.whnf(v.clone()) {
                Value::Lit(m) if m == *l => Matched::Yes(Vec::new()),
                Value::Lit(_) => Matched::No,
                _ => Matched::Unknown,
            },
            Pat::Con(c, fields) => match self.whnf(v.clone()) {
                Value::Con(d, args) if args.len() == d.arity => {
                    if *c != d {
                        return Matched::No;
                    }
                    let mut binds = Vec::new();
                    let mut unknown = false;
                    for (p, a) in fields.iter().zip(args.iter()) {
                        match self.matches(p, a) {
                            Matched::Yes(b) => binds.extend(b),
                            Matched::No => return Matched::No,
                            Matched::Unknown => unknown = true,
                        }
                    }
                    if unknown {
                        Matched::Unknown
                    } else {
                        Matched::Yes(binds)
                    }
                }
                _ => Matched::Unknown,
            },
        }
    }

    /// The normal form of `v`, with `depth` variables bound around it.
    pub fn read(&mut self, v: Value, depth: u32) -> Result<Nf, Stop> {
        self.enter()?;
        let nf = match self.whnf(v) {
            Value::Sym(x) => Nf::Sym(x),
            Value::Def(d) => Nf::Def(d),
            Value::Bound(n) => Nf::Bound(n),
            Value::Lit(l) => Nf::Lit(l),
            Value::Con(c, args) => Nf::Con(
                c,
                args.iter()
                    .map(|a| self.read(a.clone(), depth))
                    .collect::<Result<_, _>>()?,
            ),
            f @ Value::Closure(..) => {
                let body = self.apply(f, Value::Bound(depth))?;
                Nf::Lam(Box::new(self.read(body, depth + 1)?))
            }
            Value::App(f, a) => Nf::App(
                Box::new(self.read((*f).clone(), depth)?),
                Box::new(self.read((*a).clone(), depth)?),
            ),
            Value::Match(s, env, arms) => {
                let s = self.read((*s).clone(), depth)?;
                let mut out = Vec::new();
                for arm in arms.iter() {
                    let mut binds = Vec::new();
                    let shape = shape(&arm.pat, &mut binds);
                    let mut at = depth;
                    let env = binds.into_iter().fold(env.clone(), |e, x| {
                        at += 1;
                        e.with(x, Value::Bound(at - 1))
                    });
                    let body = self.eval(&arm.body, &env)?;
                    out.push((shape, self.read(body, at)?));
                }
                Nf::Match(Box::new(s), out)
            }
        };
        self.depth -= 1;
        Ok(nf)
    }
}

fn shape(pat: &Pat, binds: &mut Vec<Rc<str>>) -> NfPat {
    match pat {
        Pat::Wild => NfPat::Wild,
        Pat::Bind(x) => {
            binds.push(x.clone());
            NfPat::Bind
        }
        Pat::Lit(l) => NfPat::Lit(l.clone()),
        Pat::Con(c, fields) => {
            NfPat::Con(c.clone(), fields.iter().map(|f| shape(f, binds)).collect())
        }
    }
}

/// Whether two normal forms are values that differ in every model: different
/// constructors or literals where nothing unknown stands above them.
pub fn distinct(a: &Nf, b: &Nf) -> bool {
    match (a, b) {
        (Nf::Con(c, xs), Nf::Con(d, ys)) => {
            c != d || xs.len() != ys.len() || xs.iter().zip(ys).any(|(x, y)| distinct(x, y))
        }
        (Nf::Lit(l), Nf::Lit(m)) => l != m,
        (Nf::Con(..), Nf::Lit(_)) | (Nf::Lit(_), Nf::Con(..)) => true,
        _ => false,
    }
}
