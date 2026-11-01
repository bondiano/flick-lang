//! The leaks of `regions.rs`, each followed by a use of what leaked: a
//! check of the machine, which must reach the use after close (`regions.md`
//! §3.3) that T4 says no accepted program reaches.

use std::rc::Rc;

use super::regions::{con, global, leak, unpack, use_};
use super::{Gen, MODULE, Program, mono, nothing, some, unit};
use crate::syntax::{Arm, Const, Name, Pat, Term};
use crate::types::{Row, Stage, Type};

/// A use of `x`, leaked by path `path` of `regions.md` §4, after its
/// region closed. `None` where the path uses what leaked only inside its
/// `scope`; a child of the outer `scope` uses it when that one closes, and
/// work of path 10 uses its service then.
fn use_after(path: usize, x: &Name) -> Option<Term> {
    let v = || Term::Var(x.clone());
    let call = |f: Term| Term::app(f, Term::unit());
    Some(match path {
        0 => use_(x),
        1 => unpack(v(), con("Wrapper", 1), |y| {
            Term::app(global(MODULE, "use"), y)
        }),
        2 => Term::Await(Rc::new(v())),
        3 => call(Term::Field(Rc::new(v()), "go".into())),
        5 => unpack(v(), con("Sealed", 1), call),
        6 => {
            let y: Name = "y".into();
            let get = Term::app(Term::konst(Const::RefGet), v());
            let arm = |pat, body| Arm {
                pat,
                guard: None,
                body,
            };
            Term::Match(
                Rc::new(get),
                Rc::from([
                    arm(Pat::Con(some(), Rc::from([Pat::Var(y.clone())])), use_(&y)),
                    arm(Pat::Con(nothing(), Rc::from([])), Term::unit()),
                ]),
            )
        }
        8 | 10.. => Term::unit(),
        9 => Term::app(global(MODULE, "use_conn"), v()),
        _ => return None,
    })
}

/// `fun u -> e` with the leak of path `path` and a use after it, of
/// `Unit ->{} Unit`; `None` for a path with no such use. Path 10 is
/// `10 + k`: kept by a child, a finalizer or `start` (`provides.rs`).
pub fn leak_used(path: usize) -> Option<Program> {
    use_after(path, &"x".into())?;
    let choice = [u32::try_from(path.saturating_sub(10)).unwrap_or(0)];
    let mut g = Gen::new(if path >= 10 { &choice } else { &[] });
    let body = if path >= 10 {
        super::provides::leak(&mut g, Term::unit())
    } else {
        leak(&mut g, path, |x| {
            use_after(path, x).expect("a path with a use")
        })
    };
    let empty = Stage::Runtime(Row::none(), Row::none());
    Some(Program {
        scheme: mono(Type::fun(unit(), empty, unit())),
        term: Term::lam("u", body),
    })
}
