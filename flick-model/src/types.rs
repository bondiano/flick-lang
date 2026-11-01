//! Kinds, types and rows of λFlick (`docs/formal/core.md` §1.1, §1.2,
//! §1.6), with the regions and captures of `docs/formal/regions.md` §1.
//! A row is kept in its normal form: elements by name, then variables, so
//! `≡` on rows is equality of the two sets; a set of captures likewise.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};
use std::rc::Rc;

use crate::syntax::{Name, Path};

impl Row {
    /// `{S}`.
    pub fn one(s: Path) -> Row {
        Row {
            elems: BTreeMap::from([(s, Rc::from([]))]),
            vars: BTreeSet::new(),
        }
    }
}

/// A variable of a type or a row: one a `∀` or a signature binds, or one
/// inference has yet to solve.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Var {
    Named(Name),
    Meta(u32),
}

/// `κ ::= Type | SRow | ERow | Row Type | κ → κ | Region | CRow`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Type,
    Services,
    Errors,
    Fields,
    Arrow(Rc<Kind>, Rc<Kind>),
    Region,
    /// A set of captures (`regions.md` §1.1).
    Captures,
}

/// `C ::= ∅ | {k₁, …, kₙ} | c | C ∪ C` in normal form: the atoms `k`, regions
/// and type variables, and the variables `c` of kind `CRow`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub atoms: BTreeSet<Var>,
    pub vars: BTreeSet<Var>,
}

impl Caps {
    pub fn atom(v: Var) -> Caps {
        Caps {
            atoms: BTreeSet::from([v]),
            vars: BTreeSet::new(),
        }
    }

    pub fn var(v: Var) -> Caps {
        Caps {
            atoms: BTreeSet::new(),
            vars: BTreeSet::from([v]),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty() && self.vars.is_empty()
    }

    /// `C ∪ C'`.
    #[must_use]
    pub fn union(&self, other: &Caps) -> Caps {
        let mut out = self.clone();
        out.atoms.extend(other.atoms.iter().cloned());
        out.vars.extend(other.vars.iter().cloned());
        out
    }
}

/// `φ`: the stage of an arrow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Pure,
    Runtime(Row, Row),
}

/// A row of services or errors in normal form: `{a₁, …, aₙ | ρ₁ + … }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// Each element by its name `Module.Name`, with its arguments.
    pub elems: BTreeMap<Path, Rc<[Type]>>,
    pub vars: BTreeSet<Var>,
}

/// The fields of a record and the variable of its tail, if open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fields {
    pub fields: BTreeMap<Name, Type>,
    pub tail: Option<Var>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Var(Var),
    /// A nominal type: an ADT, an error, `Int`, `List` …
    Con(Path),
    /// `T τ`, and `α τ` for a variable of an arrow kind.
    App(Rc<Type>, Rc<Type>),
    Fun(Rc<Type>, Stage, Rc<Type>),
    Eff(Row, Row, Rc<Type>),
    /// `Eff ρ ρ'` before its result, of kind `Type → Type`: what `f` is in
    /// an instance like `Functor (Eff r e)` (§1.6). `EffOf ρ ρ' τ ≡ Eff ρ ρ' τ`.
    EffOf(Row, Row),
    Ref(Rc<Type>),
    Layer(Row, Row, Row),
    Record(Fields),
    Tuple(Rc<[Type]>),
    Errors(Row),
    /// `S.Impl`, the record of a service's operations: `ServiceImpl ρ` of
    /// the checker, which is `S.Impl` at `ρ = {S}`.
    Impl(Row),
    /// `ServiceTag ρ`: a service named as a value, at `ρ = {S}`.
    Tag(Row),
    /// `C.Dict τ̄`, the dictionary of a class (§1.6).
    Dict(Path, Rc<[Type]>),
    /// `Env ρ`, only at run time (§3.6).
    Env(Row),
    /// A `∀` of a higher-rank parameter: checking against it is (Let-Sig).
    Forall(Rc<Scheme>),
    /// `τ^C` with a hidden qualifier: `C` adds to what `τ` shows
    /// (`regions.md` §1.2).
    Captured(Rc<Type>, Caps),
    /// `τ^C` as the bound of a whole value: of a variable, or of data that
    /// stores only its parameters.
    Bounded(Rc<Type>, Caps),
}

/// `σ ::= τ | ∀κ̄. Θ ⇒ τ`; class constraints are already dictionary
/// parameters, `Θ` holds the facts `Outlives C ϱ` (`regions.md` §1.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub vars: Vec<(Name, Kind)>,
    pub body: Type,
    pub outlives: Vec<(Caps, Type)>,
}

impl Scheme {
    /// `∀ᾱ. τ` without facts.
    pub fn new(vars: Vec<(Name, Kind)>, body: Type) -> Scheme {
        Scheme {
            vars,
            body,
            outlives: Vec::new(),
        }
    }
}

impl Row {
    pub fn none() -> Row {
        Row::default()
    }

    pub fn var(v: Var) -> Row {
        Row {
            elems: BTreeMap::new(),
            vars: BTreeSet::from([v]),
        }
    }

    pub fn is_none(&self) -> bool {
        self.elems.is_empty() && self.vars.is_empty()
    }

    /// `ρ + ρ'`; `None` when one name has two lists of arguments.
    pub fn sum(&self, other: &Row) -> Option<Row> {
        let mut out = self.clone();
        for (name, args) in &other.elems {
            match out.elems.get(name) {
                Some(mine) if mine != args => return None,
                _ => {
                    out.elems.insert(name.clone(), args.clone());
                }
            }
        }
        out.vars.extend(other.vars.iter().cloned());
        Some(out)
    }

    /// `ρ ⊆ ρ'` on normal forms.
    pub fn within(&self, other: &Row) -> bool {
        self.elems
            .iter()
            .all(|(n, a)| other.elems.get(n) == Some(a))
            && self.vars.is_subset(&other.vars)
    }
}

impl Stage {
    /// `φ ⊔ φ'`; `None` when the rows do not sum.
    pub fn join(&self, other: &Stage) -> Option<Stage> {
        Some(match (self, other) {
            (Stage::Pure, s) | (s, Stage::Pure) => s.clone(),
            (Stage::Runtime(r, e), Stage::Runtime(r2, e2)) => {
                Stage::Runtime(r.sum(r2)?, e.sum(e2)?)
            }
        })
    }

    /// `φ ⊑ φ'`.
    pub fn within(&self, other: &Stage) -> bool {
        match (self, other) {
            (Stage::Pure, _) => true,
            (Stage::Runtime(..), Stage::Pure) => false,
            (Stage::Runtime(r, e), Stage::Runtime(r2, e2)) => r.within(r2) && e.within(e2),
        }
    }
}

/// The element `{Provided R ϱ}` of a row of services before the call
/// splits it by service (`regions.md` §2.7): a scheme's `Fiber.Provided R
/// ϱ` stands in the row of its last runtime arrow, which is where the call
/// is, `R` held in a tag.
pub fn provided() -> Path {
    Path::new("Fiber", "Provided")
}

/// The module of the elements `{Provided {S} ϱ}`, named by the service.
pub const PROVIDED: &str = "Fiber.Provided";

impl Type {
    /// `t` with `Provided r s` in the row of its last runtime arrow, the
    /// one its call performs; unchanged without one.
    #[must_use]
    pub fn with_provided(&self, r: &Row, s: &Type) -> Type {
        match self {
            Type::Fun(p, stage, res) => {
                let inner = res.with_provided(r, s);
                if inner != **res {
                    return Type::Fun(p.clone(), stage.clone(), Rc::new(inner));
                }
                let Stage::Runtime(sr, e) = stage else {
                    return self.clone();
                };
                let mut sr = sr.clone();
                sr.elems
                    .insert(provided(), Rc::from([Type::Tag(r.clone()), s.clone()]));
                Type::Fun(p.clone(), Stage::Runtime(sr, e.clone()), res.clone())
            }
            Type::Captured(t, c) => Type::Captured(Rc::new(t.with_provided(r, s)), c.clone()),
            other => other.clone(),
        }
    }

    pub fn con(module: &str, name: &str) -> Type {
        Type::Con(Path::new(module, name))
    }

    pub fn fun(param: Type, stage: Stage, result: Type) -> Type {
        Type::Fun(Rc::new(param), stage, Rc::new(result))
    }

    pub fn app(head: Type, arg: Type) -> Type {
        Type::App(Rc::new(head), Rc::new(arg))
    }
}

impl fmt::Display for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Var::Named(n) => write!(f, "{n}"),
            Var::Meta(m) => write!(f, "?{m}"),
        }
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = self
            .elems
            .iter()
            .map(|(n, args)| {
                let mut s = n.name.to_string();
                for a in args.iter() {
                    let _ = write!(s, " {}", Atom(a));
                }
                s
            })
            .collect();
        parts.extend(self.vars.iter().map(ToString::to_string));
        if parts.is_empty() {
            write!(f, "none")
        } else {
            write!(f, "{{{}}}", parts.join(", "))
        }
    }
}

impl fmt::Display for Caps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self
            .atoms
            .iter()
            .chain(&self.vars)
            .map(ToString::to_string)
            .collect();
        write!(f, "{{{}}}", parts.join(", "))
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stage::Pure => write!(f, "->"),
            Stage::Runtime(r, e) => write!(f, "->{{{r}; {e}}}"),
        }
    }
}

/// A type in argument position: parenthesized unless atomic.
struct Atom<'a>(&'a Type);

impl fmt::Display for Atom<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Type::Var(_)
            | Type::Con(_)
            | Type::Record(_)
            | Type::Tuple(_)
            | Type::Impl(_)
            | Type::Tag(_) => {
                write!(f, "{}", self.0)
            }
            Type::Dict(_, args) if args.is_empty() => write!(f, "{}", self.0),
            t => write!(f, "({t})"),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Var(v) => write!(f, "{v}"),
            Type::Con(p) => write!(f, "{}", p.name),
            Type::App(h, a) => write!(f, "{h} {}", Atom(a)),
            Type::Fun(p, s, r) => write!(f, "{} {s} {r}", Atom(p)),
            Type::Eff(r, e, t) => write!(f, "Eff {r} {e} {}", Atom(t)),
            Type::EffOf(r, e) => write!(f, "Eff {r} {e}"),
            Type::Ref(t) => write!(f, "Ref {}", Atom(t)),
            Type::Layer(i, e, o) => write!(f, "Layer {i} {e} {o}"),
            Type::Record(fs) => {
                let fields: Vec<String> = fs
                    .fields
                    .iter()
                    .map(|(l, t)| format!("{l} : {t}"))
                    .collect();
                match &fs.tail {
                    Some(v) => write!(f, "{{{} | {v}}}", fields.join(", ")),
                    None => write!(f, "{{{}}}", fields.join(", ")),
                }
            }
            Type::Tuple(ts) => {
                let parts: Vec<String> = ts.iter().map(ToString::to_string).collect();
                write!(f, "({})", parts.join(", "))
            }
            Type::Errors(r) => write!(f, "Errors {r}"),
            Type::Impl(r) => match r.elems.keys().next() {
                Some(s) if r.elems.len() == 1 && r.vars.is_empty() => write!(f, "{}.Impl", s.name),
                _ => write!(f, "ServiceImpl {r}"),
            },
            Type::Tag(r) => write!(f, "ServiceTag {r}"),
            Type::Dict(c, args) => {
                write!(f, "{}.Dict", c.name)?;
                for a in args.iter() {
                    write!(f, " {}", Atom(a))?;
                }
                Ok(())
            }
            Type::Env(r) => write!(f, "Env {r}"),
            Type::Forall(s) => {
                let vars: Vec<&str> = s.vars.iter().map(|(n, _)| &**n).collect();
                write!(f, "forall {}. {}", vars.join(" "), s.body)
            }
            Type::Captured(t, c) => write!(f, "{}^{c}", Atom(t)),
            Type::Bounded(t, c) => write!(f, "{}^<={c}", Atom(t)),
        }
    }
}
