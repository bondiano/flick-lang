//! Terms of λFlick (`docs/formal/core.md` §1.3) and the forms that only
//! exist at run time (§3.1). Values are terms: substitution puts them in
//! place of variables, and every value the machine builds is closed.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use flick_core::numeric::{Arith, Num};

use crate::handler::{Cont, Handler, OpClass, Signal};
use crate::native::Native;
use crate::types::Scheme;

pub type Name = Rc<str>;

/// A top-level entity, `Module.name`: a value, a type, a service, an error.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Path {
    pub module: Name,
    pub name: Name,
}

impl Path {
    pub fn new(module: &str, name: &str) -> Path {
        Path {
            module: module.into(),
            name: name.into(),
        }
    }

    /// `Module.Name` as one string, the way flick-core names a service.
    pub fn parse(qualified: &str) -> Path {
        match qualified.rsplit_once('.') {
            Some((module, name)) => Path::new(module, name),
            None => Path::new("", qualified),
        }
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.module, self.name)
    }
}

/// `n`: a literal. `Bool` and `Unit` are literals too: their patterns are.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i32),
    Float(f64),
    BigInt(num_bigint::BigInt),
    String(Rc<str>),
    Bool(bool),
    Unit,
}

/// `K`: a constructor of a declared ADT or error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Con {
    /// The type it builds.
    pub ty: Path,
    pub name: Name,
    /// Its arguments: the dictionaries a value of a GADT holds, then its fields.
    pub arity: usize,
    /// How many of the arguments are dictionaries, which `==` leaves out.
    pub dicts: usize,
}

/// The operators the surface writes infix, and helpers the translation
/// of derived instances needs: constants of Γ₀ with a fixed arity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    Arith(Arith, Num),
    Neg(Num),
    /// `==`: structural, `SameValueZero` on `Float`.
    Eq,
    NotEq,
    /// `===`: on values whose identity is their equality.
    Same,
    NotSame,
    Lt,
    Le,
    Gt,
    Ge,
    Not,
    Concat,
    ListConcat,
    /// The name of the constructor of a value, which `==` on types without
    /// fields compares (`flick-core/src/ir/lower/expr.rs`).
    TagOf,
    /// `atom` of `runtime/src/core/data.ts`: parentheses around a shown
    /// argument with a space.
    Atom,
}

/// `c`: a constant of Γ₀ (§1.5).
#[derive(Clone, Debug)]
pub enum Const {
    RefMake,
    RefGet,
    RefSet,
    Raise,
    Ask(Path),
    LayerSucceed(Path),
    LayerEffect(Path),
    LayerMerge,
    Prim(Prim),
    Native(&'static Native),
    /// A foreign binding of regions (`docs/formal/regions.md` §1.5): the
    /// machine takes its step (§3.2), its type is the scheme of the global
    /// `Path` it stands for.
    Region(Path, RegionOp),
    /// An operation of the effect `Path` the handler `id` handles, given
    /// its dictionaries and arguments (only at run time).
    Perform {
        id: usize,
        effect: Path,
        op: Name,
        class: OpClass,
        arity: usize,
    },
}

/// The steps of the foreign bindings of regions (`regions.md` §3.2), and
/// of the resource of its probes, which marks a closed object (§6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionOp {
    /// `open_H`: a capability of the owner's region, released when it closes.
    Acquire,
    /// `op_H`: stuck on a capability of a closed region, or a released object.
    Use,
    /// An object without a region.
    Make,
    /// Releases an object.
    Release,
    /// `Fiber.spawn`: a child of the region of an owner.
    Spawn,
    /// `Scope.add_finalizer`.
    AddFinalizer,
    /// `Resource.acquire_release`: the value, its release a finalizer.
    AcquireRelease,
}

impl RegionOp {
    pub fn arity(self) -> usize {
        match self {
            RegionOp::Spawn => 3,
            RegionOp::AcquireRelease => 2,
            _ => 1,
        }
    }
}

impl Const {
    pub fn arity(&self) -> usize {
        match self {
            Const::RefMake
            | Const::RefGet
            | Const::Raise
            | Const::Ask(_)
            | Const::LayerSucceed(_)
            | Const::LayerEffect(_) => 1,
            Const::RefSet | Const::LayerMerge => 2,
            Const::Prim(p) => match p {
                Prim::Neg(_) | Prim::Not | Prim::TagOf | Prim::Atom => 1,
                _ => 2,
            },
            Const::Native(n) => n.arity,
            Const::Region(_, op) => op.arity(),
            Const::Perform { arity, .. } => *arity,
        }
    }

    /// Whether a full application is a value, as `Layer.*` is (L1).
    pub fn builds_value(&self) -> bool {
        matches!(
            self,
            Const::LayerSucceed(_) | Const::LayerEffect(_) | Const::LayerMerge
        )
    }
}

/// What an application `c v̄` or `K v̄` stands on.
#[derive(Clone, Debug)]
pub enum Head {
    Con(Rc<Con>),
    Const(Const),
}

impl Head {
    pub fn arity(&self) -> usize {
        match self {
            Head::Con(c) => c.arity,
            Head::Const(c) => c.arity(),
        }
    }
}

/// `θ`: implementations of services by name.
pub type Env = BTreeMap<Path, Term>;

#[derive(Clone, Debug)]
pub enum Term {
    Var(Name),
    /// A top-level value of a module: the nested `let`s of §2.6, unfolded
    /// when the machine reaches it.
    Global(Path),
    Lit(Lit),
    Lam(Name, Rc<Term>),
    App(Rc<Term>, Rc<Term>),
    Let(Name, Option<Rc<Scheme>>, Rc<Term>, Rc<Term>),
    /// `let rec f : σ = λx. e₁ in e₂`; the right side is a λ.
    LetRec(Name, Option<Rc<Scheme>>, Rc<Term>, Rc<Term>),
    Match(Rc<Term>, Rc<[Arm]>),
    /// The ADT `(,…,)` of §1.3, with its fields evaluated left to right.
    Tuple(Rc<[Term]>),
    /// A list, with its elements evaluated left to right.
    List(Rc<[Term]>),
    Record(Rc<[(Name, Term)]>),
    Field(Rc<Term>, Name),
    Update(Rc<Term>, Name, Rc<Term>),
    Task(Rc<Term>),
    Await(Rc<Term>),
    Try(Rc<Term>, Rc<[Arm]>),
    Provide(Rc<Term>, Rc<Term>),
    /// `scope s { e }` (`docs/formal/regions.md` §1.3): binds the region
    /// `s` in the types of `e` and its owner `s : Owner s` in its values.
    Scope(Name, Rc<Term>),
    /// `c v₁ … vₖ`, `K v₁ … vₖ`: built only from values, so closed.
    Apply(Head, Rc<[Term]>),
    /// A service named as a value: what `Layer.succeed` takes.
    Tag(Path),
    /// The compiler-owned instance of class `C` for a record or tuple,
    /// with the dictionaries of its fields (§1.6): its type is that of the
    /// record or tuple it is used at.
    Structural(Name, Rc<[Term]>),
    /// Where the translation of the IR has no λFlick form: running it
    /// stops the machine, and the checker rejects it.
    Stuck(Rc<str>),
    // the forms of §3.1
    /// `fix f. λx. e`, with the `σ` of the `let rec` it came from (§3.6).
    Fix(Name, Option<Rc<Scheme>>, Rc<Term>),
    Loc(usize),
    Env(Rc<Env>),
    Bind(Path, Rc<Term>),
    Join(Rc<Term>, Rc<Term>),
    With(Rc<Env>, Rc<Term>),
    Handle(Rc<Term>, Rc<[Arm]>),
    Raise(Rc<Term>),
    /// `die`, with what the defect says.
    Die(Rc<str>),
    // the forms of regions (`docs/formal/regions.md` §3.1); `π` is an index of Λ
    /// `own π`: the owner of region `π`.
    Own(usize),
    /// `cap π n`: object `n` the foreign binding `Path` gave. Of a region
    /// when the binding acquires into one; without, an object of G4, as
    /// the one `Resource.acquire_release` holds, or a fiber.
    Cap(Path, Option<usize>, usize),
    /// `inside π e`: the body of a `scope` run in region `π`. `Name` is what
    /// the `scope` called the region; the step that opened it put `π` in
    /// its place in the signatures of `e`.
    Inside(usize, Name, Rc<Term>),
    /// `try … with` with arms for operations, or `provide S impl`.
    Handler(Rc<Handler>),
    /// `resume` of a `control` arm (only at run time).
    Cont(Cont),
    /// What unwinds a body to its handler (only at run time, raised).
    Signal(Rc<Signal>),
}

/// `p → e` or `p when g → e`.
#[derive(Clone, Debug)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Term>,
    pub body: Term,
}

#[derive(Clone, Debug)]
pub enum Pat {
    Wild,
    Var(Name),
    Lit(Lit),
    Con(Rc<Con>, Rc<[Pat]>),
    Tuple(Rc<[Pat]>),
    /// Items, then the pattern of the rest of `[a, ...rest]`.
    List(Rc<[Pat]>, Option<Rc<Pat>>),
    /// The fields a pattern mentions: `{ℓ₁ = p₁, …, ..}`.
    Record(Rc<[(Name, Pat)]>),
    Or(Rc<[Pat]>),
}

impl Term {
    pub fn lam(x: &str, body: Term) -> Term {
        Term::Lam(x.into(), Rc::new(body))
    }

    pub fn app(f: Term, arg: Term) -> Term {
        Term::App(Rc::new(f), Rc::new(arg))
    }

    pub fn apps(f: Term, args: impl IntoIterator<Item = Term>) -> Term {
        args.into_iter().fold(f, Term::app)
    }

    pub fn konst(c: Const) -> Term {
        Term::Apply(Head::Const(c), Rc::from([]))
    }

    pub fn prim(p: Prim) -> Term {
        Term::konst(Const::Prim(p))
    }

    pub fn string(s: &str) -> Term {
        Term::Lit(Lit::String(s.into()))
    }

    pub fn unit() -> Term {
        Term::Lit(Lit::Unit)
    }

    pub fn bool(b: bool) -> Term {
        Term::Lit(Lit::Bool(b))
    }

    pub fn is_value(&self) -> bool {
        match self {
            Term::Lam(..)
            | Term::Lit(_)
            | Term::Task(_)
            | Term::Tag(_)
            | Term::Fix(..)
            | Term::Loc(_)
            | Term::Env(_)
            | Term::Own(_)
            | Term::Cont(_)
            | Term::Cap(..) => true,
            Term::Apply(head, args) => {
                args.len() < head.arity()
                    || matches!(head, Head::Con(_))
                    || matches!(head, Head::Const(c) if c.builds_value())
            }
            Term::Tuple(xs) | Term::List(xs) => xs.iter().all(Term::is_value),
            Term::Record(fields) => fields.iter().all(|(_, v)| v.is_value()),
            _ => false,
        }
    }
}
