//! Programs where a value holds a capability its region closes before the
//! value is used: a closure, a task or a record whose body names `h` but
//! never uses it (`let _ = h`), or, with `used`, the same program that
//! uses it (`use h`). For the discipline of `check/regions/live.rs`: what
//! only holds a region needs it alive nowhere, so the first runs without a
//! use after close and the second reaches one. The discipline of captures
//! rejects both (`FLICK-G002`).

use std::rc::Rc;

use super::regions::{con, let_, open, stmt, task, use_, var};
use super::{Gen, Program, SIZE, mono, unit};
use crate::syntax::{Name, Term};
use crate::types::{Row, Scheme, Stage, Type};

/// `->{}`.
fn empty() -> Stage {
    Stage::Runtime(Row::none(), Row::none())
}

/// `Unit ->{} Unit`.
fn thunk() -> Type {
    Type::fun(unit(), empty(), unit())
}

impl Gen<'_> {
    /// `h` held, or used: then `rest`.
    fn touch(&mut self, h: &Name, used: bool, rest: Term) -> Term {
        if used {
            return stmt(use_(h), rest);
        }
        let held = match self.pick(3) {
            0 => var(h),
            // through a pure function that drops it
            1 => Term::app(Term::lam("z", Term::unit()), var(h)),
            _ => Term::app(Term::konst_con(con("Wrapper", 1)), var(h)),
        };
        Term::Let("_".into(), None, Rc::new(held), Rc::new(rest))
    }

    /// A body of `Unit` that touches `h`, with a generated part that does
    /// not see it.
    fn touching(&mut self, h: &Name, used: bool) -> Term {
        let before = self.term(&[], &unit(), &empty(), SIZE / 2, false);
        let after = self.term(&[], &unit(), &empty(), SIZE / 2, true);
        stmt(before, self.touch(h, used, after))
    }

    /// The value made in the `scope` and how it is used after: its form.
    fn holder(&mut self, h: &Name, used: bool) -> (Term, Box<dyn Fn(Term) -> Term>) {
        let call = |x: Term| Term::app(x, Term::unit());
        match self.pick(5) {
            0 => (Term::lam("y", self.touching(h, used)), Box::new(call)),
            1 => (
                task(self.touching(h, used)),
                Box::new(|x| Term::Await(Rc::new(x))),
            ),
            2 => {
                let go = Term::lam("y", self.touching(h, used));
                (
                    Term::Record(Rc::from([(Name::from("go"), go)])),
                    Box::new(move |x| call(Term::Field(Rc::new(x), "go".into()))),
                )
            }
            3 => {
                let inner = Term::lam("z", self.touching(h, used));
                (Term::lam("y", inner), Box::new(move |x| call(call(x))))
            }
            // a local function whose signature names no region: its own
            // qualifier is inferred
            _ => {
                let k = self.fresh("k");
                let f = Term::lam("y", self.touching(h, used));
                (let_(&k, Some(mono(thunk())), f, var(&k)), Box::new(call))
            }
        }
    }

    /// `let x = scope s { let h = open; value }; use_after x`, inside up to
    /// two more `scope`s.
    fn holding(&mut self, used: bool) -> Term {
        let (s, h, x) = (self.fresh("s"), self.fresh("h"), self.fresh("v"));
        let (value, after) = self.holder(&h, used);
        let scoped = Term::Scope(s, Rc::new(let_(&h, None, open(), value)));
        let mut e = let_(&x, None, scoped, after(var(&x)));
        for _ in 0..self.pick(3) {
            let o = self.fresh("o");
            let h = self.fresh("h");
            let pre = self.term(&[], &unit(), &empty(), SIZE / 2, false);
            e = Term::Scope(o, Rc::new(let_(&h, None, open(), stmt(pre, e))));
        }
        e
    }
}

/// The program of `holding`, of `Unit ->{} Unit`.
pub fn holding(choices: &[u32], used: bool) -> Program {
    let mut g = Gen::new(choices);
    let body = g.holding(used);
    Program {
        scheme: Scheme::new(Vec::new(), thunk()),
        term: Term::lam("u", body),
    }
}
