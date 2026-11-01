//! `e[δ]` and `match(p, v) = δ` of `docs/formal/core.md` §3.3. Substituted
//! values are closed, so substitution only has to stop at binders that
//! shadow a name; it never renames.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::syntax::{Arm, Head, Lit, Name, Pat, Term};

pub type Subst = [(Name, Term)];

/// The bindings without `x`, which a binder of `x` shadows.
fn without<'a>(s: &'a Subst, x: &str) -> Cow<'a, Subst> {
    if s.iter().any(|(n, _)| &**n == x) {
        Cow::Owned(s.iter().filter(|(n, _)| &**n != x).cloned().collect())
    } else {
        Cow::Borrowed(s)
    }
}

/// `e[δ]`.
pub fn subst(e: &Term, s: &Subst) -> Term {
    if s.is_empty() {
        return e.clone();
    }
    let go = |t: &Rc<Term>| Rc::new(subst(t, s));
    match e {
        Term::Var(x) => s
            .iter()
            .rev()
            .find(|(n, _)| n == x)
            .map_or_else(|| e.clone(), |(_, v)| v.clone()),
        // closed: built from values, or without variables
        Term::Global(_)
        | Term::Lit(_)
        | Term::Apply(..)
        | Term::Tag(_)
        | Term::Stuck(_)
        | Term::Loc(_)
        | Term::Env(_)
        | Term::Die(_) => e.clone(),
        Term::Lam(x, b) => Term::Lam(x.clone(), Rc::new(subst(b, &without(s, x)))),
        Term::App(f, a) => Term::App(go(f), go(a)),
        Term::Let(x, sig, e1, e2) => Term::Let(
            x.clone(),
            sig.clone(),
            go(e1),
            Rc::new(subst(e2, &without(s, x))),
        ),
        Term::LetRec(f, sig, e1, e2) => {
            let inner = without(s, f);
            Term::LetRec(
                f.clone(),
                sig.clone(),
                Rc::new(subst(e1, &inner)),
                Rc::new(subst(e2, &inner)),
            )
        }
        Term::Match(e, arms) => Term::Match(go(e), arms_subst(arms, s)),
        Term::Tuple(xs) => Term::Tuple(xs.iter().map(|x| subst(x, s)).collect()),
        Term::Structural(c, xs) => {
            Term::Structural(c.clone(), xs.iter().map(|x| subst(x, s)).collect())
        }
        Term::List(xs) => Term::List(xs.iter().map(|x| subst(x, s)).collect()),
        Term::Record(fs) => {
            Term::Record(fs.iter().map(|(l, x)| (l.clone(), subst(x, s))).collect())
        }
        Term::Field(e, l) => Term::Field(go(e), l.clone()),
        Term::Update(e, l, v) => Term::Update(go(e), l.clone(), go(v)),
        Term::Task(e) => Term::Task(go(e)),
        Term::Await(e) => Term::Await(go(e)),
        Term::Try(e, arms) => Term::Try(go(e), arms_subst(arms, s)),
        Term::Provide(l, e) => Term::Provide(go(l), go(e)),
        Term::Scope(r, e) => Term::Scope(r.clone(), Rc::new(subst(e, &without(s, r)))),
        Term::Fix(f, sig, lam) => {
            Term::Fix(f.clone(), sig.clone(), Rc::new(subst(lam, &without(s, f))))
        }
        Term::Bind(svc, e) => Term::Bind(svc.clone(), go(e)),
        Term::Join(a, b) => Term::Join(go(a), go(b)),
        Term::With(env, e) => Term::With(env.clone(), go(e)),
        Term::Handle(v, arms) => Term::Handle(go(v), arms_subst(arms, s)),
        Term::Raise(v) => Term::Raise(go(v)),
    }
}

fn arms_subst(arms: &Rc<[Arm]>, s: &Subst) -> Rc<[Arm]> {
    arms.iter()
        .map(|arm| {
            let mut bound = Vec::new();
            binders(&arm.pat, &mut bound);
            let mut inner = Cow::Borrowed(s);
            for b in &bound {
                inner = Cow::Owned(without(&inner, b).into_owned());
            }
            Arm {
                pat: arm.pat.clone(),
                guard: arm.guard.as_ref().map(|g| subst(g, &inner)),
                body: subst(&arm.body, &inner),
            }
        })
        .collect()
}

/// The names `p` binds.
pub fn binders(p: &Pat, out: &mut Vec<Name>) {
    match p {
        Pat::Wild | Pat::Lit(_) => {}
        Pat::Var(x) => out.push(x.clone()),
        Pat::Con(_, ps) | Pat::Tuple(ps) => ps.iter().for_each(|p| binders(p, out)),
        Pat::List(ps, rest) => {
            ps.iter().for_each(|p| binders(p, out));
            if let Some(r) = rest {
                binders(r, out);
            }
        }
        Pat::Record(fs) => fs.iter().for_each(|(_, p)| binders(p, out)),
        // the alternatives bind the same names (§2.4)
        Pat::Or(ps) => {
            if let Some(p) = ps.first() {
                binders(p, out);
            }
        }
    }
}

/// `match(p, v)`: pushes the bindings and answers whether `v` matches.
pub fn matches(p: &Pat, v: &Term, out: &mut Vec<(Name, Term)>) -> bool {
    match (p, v) {
        (Pat::Wild, _) => true,
        (Pat::Var(x), _) => {
            out.push((x.clone(), v.clone()));
            true
        }
        (Pat::Lit(a), Term::Lit(b)) => lit_eq(a, b),
        (Pat::Con(c, ps), Term::Apply(Head::Con(k), args)) => {
            c.ty == k.ty && c.name == k.name && all(ps, args, out)
        }
        (Pat::Tuple(ps), Term::Tuple(xs)) => ps.len() == xs.len() && all(ps, xs, out),
        (Pat::List(ps, rest), Term::List(xs)) => {
            let fits = match rest {
                None => ps.len() == xs.len(),
                Some(_) => ps.len() <= xs.len(),
            };
            fits && all(ps, &xs[..ps.len()], out)
                && rest.as_ref().is_none_or(|r| {
                    matches(
                        r,
                        &Term::List(xs[ps.len()..].iter().cloned().collect()),
                        out,
                    )
                })
        }
        (Pat::Record(fs), Term::Record(vs)) => fs.iter().all(|(l, p)| {
            vs.iter()
                .find(|(m, _)| m == l)
                .is_some_and(|(_, x)| matches(p, x, out))
        }),
        (Pat::Or(ps), _) => ps.iter().any(|p| {
            let mark = out.len();
            let ok = matches(p, v, out);
            if !ok {
                out.truncate(mark);
            }
            ok
        }),
        _ => false,
    }
}

fn all(ps: &[Pat], xs: &[Term], out: &mut Vec<(Name, Term)>) -> bool {
    ps.iter().zip(xs).all(|(p, x)| matches(p, x, out))
}

/// A literal pattern matches as JS `===` compares: `NaN` matches nothing.
pub fn lit_eq(a: &Lit, b: &Lit) -> bool {
    match (a, b) {
        (Lit::Float(x), Lit::Float(y)) => x == y,
        _ => a == b,
    }
}

/// The variables `e` uses that no binder in it binds: `fv(e)`.
pub fn free_vars(e: &Term) -> BTreeSet<Name> {
    let mut out = BTreeSet::new();
    free(e, &mut Vec::new(), &mut out);
    out
}

fn free(e: &Term, bound: &mut Vec<Name>, out: &mut BTreeSet<Name>) {
    let under = |x: &Name, t: &Term, bound: &mut Vec<Name>, out: &mut BTreeSet<Name>| {
        bound.push(x.clone());
        free(t, bound, out);
        bound.pop();
    };
    match e {
        Term::Var(x) => {
            if !bound.contains(x) {
                out.insert(x.clone());
            }
        }
        Term::Lam(x, b) | Term::Fix(x, _, b) | Term::Scope(x, b) => under(x, b, bound, out),
        Term::Let(x, _, e1, e2) => {
            free(e1, bound, out);
            under(x, e2, bound, out);
        }
        Term::LetRec(f, _, e1, e2) => {
            under(f, e1, bound, out);
            under(f, e2, bound, out);
        }
        Term::Match(x, arms) | Term::Try(x, arms) | Term::Handle(x, arms) => {
            free(x, bound, out);
            for arm in arms.iter() {
                let mark = bound.len();
                binders(&arm.pat, bound);
                if let Some(g) = &arm.guard {
                    free(g, bound, out);
                }
                free(&arm.body, bound, out);
                bound.truncate(mark);
            }
        }
        Term::Tuple(xs) | Term::List(xs) | Term::Structural(_, xs) | Term::Apply(_, xs) => {
            xs.iter().for_each(|x| free(x, bound, out));
        }
        Term::Record(fs) => fs.iter().for_each(|(_, x)| free(x, bound, out)),
        Term::App(a, b) | Term::Update(a, _, b) | Term::Provide(a, b) | Term::Join(a, b) => {
            free(a, bound, out);
            free(b, bound, out);
        }
        Term::Field(x, _)
        | Term::Task(x)
        | Term::Await(x)
        | Term::Bind(_, x)
        | Term::With(_, x)
        | Term::Raise(x) => free(x, bound, out),
        Term::Global(_)
        | Term::Lit(_)
        | Term::Tag(_)
        | Term::Stuck(_)
        | Term::Loc(_)
        | Term::Env(_)
        | Term::Die(_) => {}
    }
}
