//! One change that may make a generated program ill-typed: what
//! `checkers_agree` judges with both checkers.

use std::rc::Rc;

use super::holes::{cast, opaque_field, reshape, widen};
use super::regions::leak;
use super::{Gen, Program, mono, unit};
use crate::syntax::{Arm, Const, Head, Lit, Term};
use crate::types::{Row, Scheme, Stage, Type};

/// `p` changed as `choices` say: a subterm replaced by a closed term of
/// some type and stage, a runtime step put at the head of a block, an arm
/// dropped, an element taken off a row of a signature, a leak of a region
/// put at the head of a block, a qualifier or `Outlives` taken off a local
/// signature (`regions.md` §4); or what would open a hole of TypeScript
/// (`ts-holes.md` §2): a stage widened in a parameter or a cell, a record
/// with a field more or fewer, a field of an opaque value, a cast.
pub fn mutate(p: &Program, choices: &[u32]) -> Program {
    let mut g = Gen::new(choices);
    // the names a change makes are not the program's: a `scope` would shadow one
    g.names = 100_000;
    let kind = g.pick(10);
    let mut at = g.pick(size(&p.term) + 1);
    let mut p = p.clone();
    if kind == 3 && at == 0 {
        p.scheme = narrow(&p.scheme, g.pick(64));
        return p;
    }
    let mut change = |e: &Term, block: bool, g: &mut Gen| -> Option<Term> {
        match (kind, e) {
            (0, _) => {
                let t = g.ty(2);
                let stage = if g.pick(2) == 0 {
                    Stage::Pure
                } else {
                    Stage::Runtime(g.services(), g.errors())
                };
                Some(g.term(&[], &t, &stage, 3, block))
            }
            (1, _) if block => Some(Term::Let(
                "_".into(),
                Some(Rc::new(mono(unit()))),
                Rc::new(runtime_step()),
                Rc::new(e.clone()),
            )),
            (2, Term::Match(s, arms)) if arms.len() > 1 => {
                Some(Term::Match(s.clone(), drop_arm(arms, g)))
            }
            (2, Term::Try(s, arms)) if arms.len() > 1 => {
                Some(Term::Try(s.clone(), drop_arm(arms, g)))
            }
            (3, Term::Let(x, Some(s), rhs, rest)) => Some(Term::Let(
                x.clone(),
                Some(Rc::new(narrow(s, g.pick(64)))),
                rhs.clone(),
                rest.clone(),
            )),
            (4, _) if block => {
                let path = g.pick(10);
                Some(leak(g, path, |_| e.clone()))
            }
            (5, Term::Let(x, Some(s), rhs, rest)) => Some(Term::Let(
                x.clone(),
                Some(Rc::new(unqualify(s))),
                rhs.clone(),
                rest.clone(),
            )),
            (6, Term::Let(x, Some(s), rhs, rest)) => Some(Term::Let(
                x.clone(),
                Some(Rc::new(widen(s)?)),
                rhs.clone(),
                rest.clone(),
            )),
            (7, Term::Record(fs)) => Some(reshape(g, fs)),
            (8, _) => Some(opaque_field(g)),
            // a constant and an operation are written applied: a cast of a
            // call could take its head apart, so neither is cast
            (9, Term::Apply(Head::Const(_), _) | Term::Field(..) | Term::App(..)) => None,
            (9, _) => Some(cast(g, e)),
            _ => None,
        }
    };
    p.term = rewrite(&p.term, &mut at, true, &mut g, &mut change);
    p
}

type Change<'a> = dyn FnMut(&Term, bool, &mut Gen) -> Option<Term> + 'a;

/// `Ref.set (Ref.make 0) 1`: a closed step of `->{}`.
fn runtime_step() -> Term {
    let cell = Term::app(Term::konst(Const::RefMake), Term::Lit(Lit::Int(0)));
    Term::apps(Term::konst(Const::RefSet), [cell, Term::Lit(Lit::Int(1))])
}

fn drop_arm(arms: &Rc<[Arm]>, g: &mut Gen) -> Rc<[Arm]> {
    let i = g.pick(arms.len());
    arms.iter()
        .enumerate()
        .filter(|(j, _)| *j != i)
        .map(|(_, a)| a.clone())
        .collect()
}

/// `σ` with element `k` of its rows, counted over them all, taken off.
fn narrow(s: &Scheme, k: usize) -> Scheme {
    let mut rows = Vec::new();
    collect_rows(&s.body, &mut rows);
    let total: usize = rows.iter().map(|r| r.elems.len()).sum();
    if total == 0 {
        return s.clone();
    }
    let mut k = k % total;
    Scheme {
        vars: s.vars.clone(),
        body: narrow_type(&s.body, &mut k),
        outlives: s.outlives.clone(),
    }
}

/// `σ` without its facts `Θ`, or else without its first qualifier.
fn unqualify(s: &Scheme) -> Scheme {
    if !s.outlives.is_empty() {
        return Scheme {
            outlives: Vec::new(),
            ..s.clone()
        };
    }
    Scheme {
        body: strip(&s.body).unwrap_or_else(|| s.body.clone()),
        ..s.clone()
    }
}

/// `t` with its first qualifier, outermost first, taken off.
fn strip(t: &Type) -> Option<Type> {
    match t {
        Type::Captured(x, _) | Type::Bounded(x, _) => Some((**x).clone()),
        Type::Fun(p, s, r) => match strip(p) {
            Some(p) => Some(Type::Fun(Rc::new(p), s.clone(), r.clone())),
            None => Some(Type::Fun(p.clone(), s.clone(), Rc::new(strip(r)?))),
        },
        _ => None,
    }
}

fn collect_rows<'t>(t: &'t Type, out: &mut Vec<&'t Row>) {
    match t {
        Type::Fun(p, s, r) => {
            collect_rows(p, out);
            if let Stage::Runtime(a, b) = s {
                out.extend([a, b]);
            }
            collect_rows(r, out);
        }
        Type::Eff(a, b, x) => {
            out.extend([a, b]);
            collect_rows(x, out);
        }
        Type::App(f, x) => {
            collect_rows(f, out);
            collect_rows(x, out);
        }
        Type::Tuple(ts) => ts.iter().for_each(|t| collect_rows(t, out)),
        Type::Record(f) => f.fields.values().for_each(|t| collect_rows(t, out)),
        _ => {}
    }
}

fn narrow_type(t: &Type, k: &mut usize) -> Type {
    let row = |r: &Row, k: &mut usize| {
        let mut r = r.clone();
        if *k < r.elems.len() {
            let name = r.elems.keys().nth(*k).cloned();
            if let Some(n) = name {
                r.elems.remove(&n);
            }
            *k = usize::MAX;
        } else if *k != usize::MAX {
            *k -= r.elems.len();
        }
        r
    };
    match t {
        Type::Fun(p, s, r) => {
            let p = narrow_type(p, k);
            let s = match s {
                Stage::Runtime(a, b) => {
                    let a = row(a, k);
                    Stage::Runtime(a, row(b, k))
                }
                Stage::Pure => Stage::Pure,
            };
            Type::fun(p, s, narrow_type(r, k))
        }
        Type::Eff(a, b, x) => {
            let a = row(a, k);
            let b = row(b, k);
            Type::Eff(a, b, Rc::new(narrow_type(x, k)))
        }
        Type::App(f, x) => {
            let f = narrow_type(f, k);
            Type::app(f, narrow_type(x, k))
        }
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| narrow_type(t, k)).collect()),
        Type::Record(f) => {
            let mut f = f.clone();
            for t in f.fields.values_mut() {
                *t = narrow_type(t, k);
            }
            Type::Record(f)
        }
        t => t.clone(),
    }
}

/// How many subterms `e` has, `e` itself included: how far `rewrite`
/// counts down a position it never reaches.
fn size(e: &Term) -> usize {
    let start = usize::MAX - 1;
    let mut at = start;
    rewrite(e, &mut at, true, &mut Gen::new(&[]), &mut |_, _, _| None);
    start - at
}

/// `e` with the subterm at preorder position `at` changed by `change`,
/// which learns whether a block is there. Only the subterms the generator
/// makes are visited: the λ of a `let rec` stays one.
fn rewrite(e: &Term, at: &mut usize, block: bool, g: &mut Gen, change: &mut Change) -> Term {
    if *at == 0 {
        *at = usize::MAX;
        if let Some(new) = change(e, block, g) {
            return new;
        }
    } else if *at != usize::MAX {
        *at -= 1;
    }
    let mut go = |t: &Term, block: bool| Rc::new(rewrite(t, at, block, g, change));
    match e {
        Term::Lam(x, b) => Term::Lam(x.clone(), go(b, true)),
        Term::App(f, a) => {
            let f = go(f, false);
            Term::App(f, go(a, false))
        }
        // `(e : σ)`: its `e` is no block, its variable no subterm
        Term::Let(x, sig @ Some(_), rhs, rest) if matches!(&**rest, Term::Var(y) if y == x) => {
            Term::Let(x.clone(), sig.clone(), go(rhs, false), rest.clone())
        }
        Term::Let(x, sig, rhs, rest) => {
            // a statement `s` of a block is a line, no block of its own
            let rhs = go(rhs, &**x != "_");
            Term::Let(x.clone(), sig.clone(), rhs, go(rest, true))
        }
        Term::LetRec(f, sig, lam, rest) => {
            let lam = match &**lam {
                Term::Lam(x, b) => Rc::new(Term::Lam(x.clone(), go(b, true))),
                other => Rc::new(other.clone()),
            };
            Term::LetRec(f.clone(), sig.clone(), lam, go(rest, true))
        }
        Term::Match(s, arms) => {
            let s = go(s, false);
            Term::Match(s, arms_of(arms, &mut go))
        }
        Term::Try(body, arms) => {
            let body = go(body, true);
            Term::Try(body, arms_of(arms, &mut go))
        }
        Term::Tuple(xs) => Term::Tuple(xs.iter().map(|x| (*go(x, false)).clone()).collect()),
        Term::Record(fs) => Term::Record(
            fs.iter()
                .map(|(l, x)| (l.clone(), (*go(x, false)).clone()))
                .collect(),
        ),
        // the `ask_S ()` of an operation `S.op` is no subterm of the source
        Term::Field(r, _) if matches!(&**r, Term::App(f, _) if matches!(&**f, Term::Apply(Head::Const(Const::Ask(_)), _))) => {
            e.clone()
        }
        Term::Field(r, l) => Term::Field(go(r, false), l.clone()),
        // the record of `with` stays a leaf (`holes.rs` `update`)
        Term::Update(r, l, v) => Term::Update(r.clone(), l.clone(), go(v, false)),
        Term::List(xs) => Term::List(xs.iter().map(|x| (*go(x, false)).clone()).collect()),
        Term::Task(b) => Term::Task(go(b, true)),
        Term::Scope(s, b) => Term::Scope(s.clone(), go(b, true)),
        Term::Await(x) => Term::Await(go(x, false)),
        Term::Provide(l, b) => {
            let l = go(l, false);
            Term::Provide(l, go(b, true))
        }
        other => other.clone(),
    }
}

fn arms_of(arms: &[Arm], go: &mut dyn FnMut(&Term, bool) -> Rc<Term>) -> Rc<[Arm]> {
    arms.iter()
        .map(|a| Arm {
            pat: a.pat.clone(),
            guard: a.guard.as_ref().map(|g| (*go(g, false)).clone()),
            body: (*go(&a.body, true)).clone(),
        })
        .collect()
}
