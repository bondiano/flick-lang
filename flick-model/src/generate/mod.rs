//! A generator of well-typed λFlick programs for the properties of X2
//! (`flick-model/tests/props.rs`). A program is built type first from a
//! sequence of choices: `pick(n)` takes the next one modulo `n`, and a
//! choice past the end is 0, the simplest form, so proptest shrinking the
//! sequence shrinks the program with it. The programs use the declarations
//! of `Gen.fl` and the constants of Γ₀ only, and `print` writes each as
//! Flick source: flick-core and the model judge one program.

mod bind;
mod handle;
mod mutate;
mod print;
mod term;

use std::rc::Rc;

pub use mutate::mutate;
pub use print::source;

use crate::native::prelude;
use crate::syntax::{Con, Const, Lit, Name, Path, Term};
use crate::types::{Row, Scheme, Stage, Type};

/// The declarations the programs use, as a Flick module.
const PRELUDE: &str = include_str!("Gen.fl");

/// A top-level value and its signature: `program : σ`, `let program = e`.
#[derive(Clone, Debug)]
pub struct Program {
    pub scheme: Scheme,
    pub term: Term,
}

/// What a program is built from: the choices, and a counter of names.
pub struct Gen<'c> {
    choices: &'c [u32],
    at: usize,
    names: u32,
}

/// How big a program is: the depth its terms may nest to.
const SIZE: u32 = 6;

impl<'c> Gen<'c> {
    pub fn new(choices: &'c [u32]) -> Self {
        Gen {
            choices,
            at: 0,
            names: 0,
        }
    }

    /// The next choice among `n`; 0 once the choices run out.
    fn pick(&mut self, n: usize) -> usize {
        let c = self.choices.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        c as usize % n.max(1)
    }

    /// One time in `n`.
    fn one_in(&mut self, n: usize) -> bool {
        self.pick(n) == 1
    }

    fn fresh(&mut self, what: &str) -> Name {
        self.names += 1;
        format!("{what}{}", self.names).into()
    }

    /// A program: a pure value, a function of `()` with a runtime arrow,
    /// or a task, so the rows of its signature and (Module) are exercised.
    pub fn program(&mut self) -> Program {
        let t = self.ty(2);
        let (r, e) = (self.services(), self.errors());
        let budget = Stage::Runtime(r.clone(), e.clone());
        let (body, term) = match self.pick(3) {
            0 => (t.clone(), self.term(&[], &t, &Stage::Pure, SIZE, true)),
            1 => {
                let body = self.term(&[], &t, &budget, SIZE, true);
                let fun = Type::fun(unit(), budget, t);
                (fun, Term::lam("u", body))
            }
            _ => {
                let body = self.term(&[], &t, &budget, SIZE, true);
                (Type::Eff(r, e, Rc::new(t)), Term::Task(Rc::new(body)))
            }
        };
        Program {
            scheme: mono(body),
            term,
        }
    }
}

/// The program `choices` build.
pub fn program(choices: &[u32]) -> Program {
    Gen::new(choices).program()
}

/// What the runner gives a program: an implementation of each service
/// around it, so it runs in the machine with an empty `Σ₀`.
pub fn runnable(e: Term) -> Term {
    SERVICES.iter().fold(e, |e, s| {
        let layer = Term::app(
            Term::konst(Const::LayerSucceed(service(s))),
            implementation(s, None),
        );
        Term::Provide(Rc::new(layer), Rc::new(e))
    })
}

const MODULE: &str = "Gen";

/// The services of `Gen.fl`.
const SERVICES: [&str; 2] = ["Log", "Counter"];

/// The errors of `Gen.fl` with the type of their field.
const ERRORS: [&str; 2] = ["Oops", "Bad"];

fn service(name: &str) -> Path {
    Path::new(MODULE, name)
}

/// The type of the field of error `name`.
fn payload(name: &str) -> Type {
    if name == "Oops" { int() } else { string() }
}

fn error_con(name: &str) -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new(MODULE, name),
        name: name.into(),
        arity: 1,
    })
}

/// A record implementing service `s`; `body` is what `log` does.
fn implementation(s: &str, body: Option<Term>) -> Term {
    let (op, x, result) = if s == "Log" {
        ("log", "line", body.unwrap_or_else(Term::unit))
    } else {
        ("next", "u", body.unwrap_or(Term::Lit(Lit::Int(7))))
    };
    Term::Record(Rc::from([(Name::from(op), Term::lam(x, result))]))
}

/// `(ask_S ()).op`: an operation of a service, as the translation of IR
/// writes `S.op` (§1.4).
fn operation(s: &str, op: &str) -> Term {
    let ask = Term::app(Term::konst(Const::Ask(service(s))), Term::unit());
    Term::Field(Rc::new(ask), op.into())
}

fn ty(name: &str) -> Type {
    Type::con("Prelude", name)
}

fn int() -> Type {
    ty("Int")
}

fn boolean() -> Type {
    ty("Bool")
}

fn string() -> Type {
    ty("String")
}

fn unit() -> Type {
    ty("Unit")
}

fn option(t: Type) -> Type {
    Type::app(ty("Option"), t)
}

fn some() -> Rc<Con> {
    prelude("Option", "Some", 1)
}

fn nothing() -> Rc<Con> {
    prelude("Option", "None", 0)
}

fn mono(body: Type) -> Scheme {
    Scheme {
        vars: Vec::new(),
        body,
        outlives: Vec::new(),
    }
}

/// `{S₁, …}` of the names `names`.
fn row(names: &[&str]) -> Row {
    let mut r = Row::none();
    for n in names {
        r.elems.insert(service(n), Rc::from([]));
    }
    r
}
