//! Bindings and their uses: a `let` of each rule of §2.5, the polymorphic
//! values of known scheme it may bind, and calls of what `Γ` holds at an
//! instance of its scheme.

use std::collections::HashMap;
use std::rc::Rc;

use flick_core::numeric::{Arith, Num};

use super::term::{Cx, extend, plain};
use super::{Gen, int, mono, nothing, option, some, unit};
use crate::check::{Inst, substitute};
use crate::syntax::{Arm, Lit, Name, Pat, Prim, Term};
use crate::types::{Kind, Row, Scheme, Stage, Type, Var};

impl Gen<'_> {
    /// (Let-Gen), (Let-Mono), (Let-Sig) and (Let-Rec), with the
    /// polymorphic forms the rules of §2.5 tell apart.
    pub(super) fn let_(&mut self, cx: &Cx, t: &Type, budget: &Stage, s: u32) -> Term {
        let x = self.fresh("v");
        let (sig, rhs, scheme, rec) = match self.pick(4) {
            // without a signature: data, generalized if the right side is pure
            0 => {
                let d = self.data(1);
                (None, self.term(cx, &d, budget, s, true), mono(d), false)
            }
            // with a monomorphic signature, any type
            1 => {
                let d = self.ty(2);
                let rhs = self.term(cx, &d, budget, s, true);
                (Some(mono(d.clone())), rhs, mono(d), false)
            }
            // a polymorphic value of known scheme
            2 => {
                let (scheme, rhs, rec, signed) = template(self.pick(TEMPLATES), &x);
                (signed.then(|| scheme.clone()), rhs, scheme, rec)
            }
            // a recursive function of the counter; its base does not call
            // it, so the program ends
            _ => {
                let d = self.ty(1);
                let stage = self.within_budget(budget);
                let fun = Type::fun(int(), stage.clone(), d.clone());
                let n = self.fresh("n");
                let base = self.term(&extend(cx, &n, mono(int())), &d, &stage, s, true);
                (
                    Some(mono(fun.clone())),
                    countdown(&x, &n, base),
                    mono(fun),
                    true,
                )
            }
        };
        let inner = extend(cx, &x, scheme);
        let rest = self.term(&inner, t, budget, s, true);
        let (rhs, rest) = (Rc::new(rhs), Rc::new(rest));
        let sig = sig.map(Rc::new);
        if rec {
            Term::LetRec(x, sig, rhs, rest)
        } else {
            Term::Let(x, sig, rhs, rest)
        }
    }

    /// A stage that fits `budget`.
    pub(super) fn within_budget(&mut self, budget: &Stage) -> Stage {
        match budget {
            Stage::Runtime(r, e) if self.pick(2) == 1 => {
                Stage::Runtime(self.some_of(r), self.some_of(e))
            }
            _ => Stage::Pure,
        }
    }

    /// A call of a function of `Γ` whose result after some arguments is an
    /// instance of `t`, with the variables that leaves open chosen.
    pub(super) fn call(&mut self, cx: &Cx, t: &Type, budget: &Stage, s: u32) -> Term {
        let mut found = Vec::new();
        for (f, scheme) in cx {
            let mut params = Vec::new();
            let mut result = &scheme.body;
            while let Type::Fun(p, stage, r) = result {
                params.push(((**p).clone(), stage.clone()));
                result = r;
                let mut map = HashMap::new();
                // a variable stands for data: a function there would be
                // checked against a meta, which takes it at its least stage
                if fits(result, t, &mut map)
                    && map
                        .values()
                        .all(|i| !matches!(i, Inst::Type(u) if !plain(u)))
                {
                    found.push((f.clone(), scheme.vars.clone(), params.clone(), map));
                }
            }
        }
        if found.is_empty() {
            return self.leaf(cx, t);
        }
        let (f, vars, params, mut map) = found.swap_remove(self.pick(found.len()));
        let (services, errors) = match budget {
            Stage::Pure => (Row::none(), Row::none()),
            Stage::Runtime(r, e) => (r.clone(), e.clone()),
        };
        for (v, k) in &vars {
            if map.contains_key(v) {
                continue;
            }
            let inst = match k {
                Kind::Services => Inst::Row(self.some_of(&services)),
                Kind::Errors => Inst::Row(self.some_of(&errors)),
                _ => Inst::Type(self.data(1)),
            };
            map.insert(v.clone(), inst);
        }
        let mut args = Vec::new();
        for (p, stage) in params {
            let stage = match substitute(&Type::fun(p.clone(), stage, unit()), &map) {
                Type::Fun(_, stage, _) => stage,
                _ => Stage::Pure,
            };
            if !stage.within(budget) {
                return self.leaf(cx, t);
            }
            let p = substitute(&p, &map);
            args.push(self.term(cx, &p, budget, s, false));
        }
        Term::apps(Term::Var(f), args)
    }
}

/// How many polymorphic templates there are.
const TEMPLATES: usize = 8;

/// A polymorphic value of known scheme: its scheme, its λ, whether it is
/// recursive and whether it is bound with its signature (Let-Sig) or
/// without (Let-Gen; only values that are no function, `FLICK-T004`).
fn template(i: usize, name: &str) -> (Scheme, Term, bool, bool) {
    let var = |n: &str| Type::Var(Var::Named(n.into()));
    let rows = |n: &str| Row::var(Var::Named(n.into()));
    let (a, b) = (var("a"), var("b"));
    let ty_vars = |vs: &[&str]| {
        vs.iter()
            .map(|v| (Name::from(*v), Kind::Type))
            .collect::<Vec<_>>()
    };
    let x = || Term::Var("x".into());
    let scheme = |vars, body| Scheme { vars, body, outlives: Vec::new() };
    match i {
        0 => (
            scheme(ty_vars(&["a"]), Type::fun(a.clone(), Stage::Pure, a)),
            Term::lam("x", x()),
            false,
            true,
        ),
        1 => (
            scheme(
                ty_vars(&["a"]),
                Type::fun(
                    a.clone(),
                    Stage::Pure,
                    Type::Tuple(Rc::from([a.clone(), a])),
                ),
            ),
            Term::lam("x", Term::Tuple(Rc::from([x(), x()]))),
            false,
            true,
        ),
        2 => (
            scheme(
                ty_vars(&["a"]),
                Type::fun(a.clone(), Stage::Pure, option(a)),
            ),
            Term::lam("x", Term::app(Term::konst_con(some()), x())),
            false,
            true,
        ),
        3 => (
            scheme(
                ty_vars(&["a", "b"]),
                Type::fun(a.clone(), Stage::Pure, Type::fun(b, Stage::Pure, a)),
            ),
            Term::lam("x", Term::lam("y", x())),
            false,
            true,
        ),
        // a higher-order function polymorphic in its rows
        4 => {
            let stage = Stage::Runtime(rows("r"), rows("e"));
            let f = Type::fun(a.clone(), stage.clone(), b.clone());
            let mut vars = ty_vars(&["a", "b"]);
            vars.extend([("r".into(), Kind::Services), ("e".into(), Kind::Errors)]);
            (
                scheme(vars, Type::fun(f, Stage::Pure, Type::fun(a, stage, b))),
                Term::lam("g", Term::lam("x", Term::app(Term::Var("g".into()), x()))),
                false,
                true,
            )
        }
        // a computation polymorphic in its rows
        5 => {
            let mut vars = ty_vars(&["a"]);
            vars.extend([("r".into(), Kind::Services), ("e".into(), Kind::Errors)]);
            (
                scheme(
                    vars,
                    Type::fun(
                        a.clone(),
                        Stage::Pure,
                        Type::Eff(rows("r"), rows("e"), Rc::new(a)),
                    ),
                ),
                Term::lam("x", Term::Task(Rc::new(x()))),
                false,
                true,
            )
        }
        // polymorphic recursion is allowed under an explicit `forall`
        6 => (
            scheme(
                ty_vars(&["a"]),
                Type::fun(int(), Stage::Pure, Type::fun(a.clone(), Stage::Pure, a)),
            ),
            Term::lam("n", Term::lam("x", countdown_body(name, x()))),
            true,
            true,
        ),
        // (Let-Gen): `None` generalized
        _ => (
            scheme(ty_vars(&["a"]), option(a)),
            Term::konst_con(nothing()),
            false,
            false,
        ),
    }
}

/// `match n < 1 with true → x | false → f (n - 1) x`.
fn countdown_body(f: &str, x: Term) -> Term {
    let n = || Term::Var("n".into());
    let lt = Term::apps(Term::prim(Prim::Lt), [n(), Term::Lit(Lit::Int(1))]);
    let pred = Term::apps(
        Term::prim(Prim::Arith(Arith::Sub, Num::Int)),
        [n(), Term::Lit(Lit::Int(1))],
    );
    let again = Term::apps(Term::Var(f.into()), [pred, x.clone()]);
    Term::Match(
        Rc::new(lt),
        Rc::from([
            Arm {
                pat: Pat::Lit(Lit::Bool(true)),
                guard: None,
                body: x,
            },
            Arm {
                pat: Pat::Lit(Lit::Bool(false)),
                guard: None,
                body: again,
            },
        ]),
    )
}

/// `λn. match n < 1 with true → base | false → f (n - 1)`.
fn countdown(f: &Name, n: &Name, base: Term) -> Term {
    let lt = Term::apps(
        Term::prim(Prim::Lt),
        [Term::Var(n.clone()), Term::Lit(Lit::Int(1))],
    );
    let pred = Term::apps(
        Term::prim(Prim::Arith(Arith::Sub, Num::Int)),
        [Term::Var(n.clone()), Term::Lit(Lit::Int(1))],
    );
    let again = Term::app(Term::Var(f.clone()), pred);
    let body = Term::Match(
        Rc::new(lt),
        Rc::from([
            Arm {
                pat: Pat::Lit(Lit::Bool(true)),
                guard: None,
                body: base,
            },
            Arm {
                pat: Pat::Lit(Lit::Bool(false)),
                guard: None,
                body: again,
            },
        ]),
    );
    Term::Lam(n.clone(), Rc::new(body))
}

/// Whether `t` is an instance of `pat`, the body of a scheme, extending
/// `map` of its variables; a row of a scheme is closed or one variable.
pub(super) fn fits(pat: &Type, t: &Type, map: &mut HashMap<Name, Inst>) -> bool {
    match (pat, t) {
        (Type::Var(Var::Named(a)), _) => match map.get(a) {
            Some(Inst::Type(u)) => u == t,
            Some(_) => false,
            None => {
                map.insert(a.clone(), Inst::Type(t.clone()));
                true
            }
        },
        (Type::Con(a), Type::Con(b)) => a == b,
        (Type::App(f, x), Type::App(g, y)) => fits(f, g, map) && fits(x, y, map),
        (Type::Fun(p, s, r), Type::Fun(q, s2, r2)) => {
            fits(p, q, map) && fits_stage(s, s2, map) && fits(r, r2, map)
        }
        (Type::Eff(r, e, a), Type::Eff(r2, e2, b)) => {
            fits_row(r, r2, map) && fits_row(e, e2, map) && fits(a, b, map)
        }
        (Type::Tuple(xs), Type::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(x, y)| fits(x, y, map))
        }
        (Type::Record(f), Type::Record(g)) => {
            f.fields.len() == g.fields.len()
                && f.fields
                    .iter()
                    .all(|(l, x)| g.fields.get(l).is_some_and(|y| fits(x, y, map)))
        }
        (Type::Ref(a), Type::Ref(b)) => fits(a, b, map),
        _ => false,
    }
}

fn fits_stage(s: &Stage, t: &Stage, map: &mut HashMap<Name, Inst>) -> bool {
    match (s, t) {
        (Stage::Pure, Stage::Pure) => true,
        (Stage::Runtime(r, e), Stage::Runtime(r2, e2)) => {
            fits_row(r, r2, map) && fits_row(e, e2, map)
        }
        _ => false,
    }
}

fn fits_row(pat: &Row, r: &Row, map: &mut HashMap<Name, Inst>) -> bool {
    match pat.vars.iter().next() {
        Some(Var::Named(v)) if pat.elems.is_empty() && pat.vars.len() == 1 => match map.get(v) {
            Some(Inst::Row(x)) => x == r,
            Some(_) => false,
            None => {
                map.insert(v.clone(), Inst::Row(r.clone()));
                true
            }
        },
        _ => pat == r,
    }
}
