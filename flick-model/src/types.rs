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
            ..Row::default()
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
    /// `(ρ, S)`: the variable `ρ` stands here without `S`, as `{-S | ρ}`
    /// writes it. Only a name without arguments is taken out of a variable.
    pub less: BTreeSet<(Var, Path)>,
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
            vars: BTreeSet::from([v]),
            ..Row::default()
        }
    }

    /// The names the variable `v` is taken without.
    pub fn less_of<'a>(&'a self, v: &'a Var) -> impl Iterator<Item = &'a Path> {
        (self.less.iter()).filter_map(move |(w, n)| (w == v).then_some(n))
    }

    /// The variable `v` of this row alone, without what it is taken without.
    #[must_use]
    pub fn only(&self, v: &Var) -> Row {
        Row {
            elems: BTreeMap::new(),
            vars: BTreeSet::from([v.clone()]),
            less: self.less.iter().filter(|(w, _)| w == v).cloned().collect(),
        }
    }

    /// `self + (ρ − gone)`: a row with `ρ` twice holds what either gives,
    /// so `ρ` is without the names both are without.
    pub fn add_var(&mut self, v: &Var, gone: &BTreeSet<Path>) {
        if self.vars.insert(v.clone()) {
            self.less
                .extend(gone.iter().map(|n| (v.clone(), n.clone())));
        } else {
            self.less.retain(|(w, n)| w != v || gone.contains(n));
        }
    }

    /// `self + (inner − gone)`: what a variable taken without `gone` gives
    /// once it stands for `inner`. An element this row has keeps its
    /// arguments.
    pub fn add_less(&mut self, inner: &Row, gone: &BTreeSet<Path>) {
        for (n, args) in &inner.elems {
            if !gone.contains(n) {
                self.elems.entry(n.clone()).or_insert_with(|| args.clone());
            }
        }
        for v in &inner.vars {
            let gone = inner.less_of(v).chain(gone).cloned().collect();
            self.add_var(v, &gone);
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
        for v in &other.vars {
            out.add_var(v, &other.less_of(v).cloned().collect());
        }
        Some(out)
    }

    /// The names `other` takes out of `v` that this row keeps in it and
    /// `other` does not name itself: what `v` may hold that `other` lacks.
    /// `ρ ⊆ {S | ρ − S}`, whatever `ρ` is.
    pub fn owed<'a>(&'a self, v: &'a Var, other: &'a Row) -> impl Iterator<Item = &'a Path> {
        (other.less_of(v)).filter(move |n| {
            !self.less.contains(&(v.clone(), (*n).clone())) && !other.elems.contains_key(*n)
        })
    }

    /// `ρ ⊆ ρ'` on normal forms.
    pub fn within(&self, other: &Row) -> bool {
        self.elems
            .iter()
            .all(|(n, a)| other.elems.get(n) == Some(a))
            && (self.vars.iter())
                .all(|v| other.vars.contains(v) && self.owed(v, other).next().is_none())
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
        parts.extend(self.vars.iter().map(|v| {
            let mut s = v.to_string();
            for n in self.less_of(v) {
                let _ = write!(s, " - {}", n.name);
            }
            s
        }));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn name(n: &str) -> Path {
        Path::new("M", n)
    }

    /// `{-names | s}`.
    fn less(names: &[&str]) -> Row {
        let mut row = Row::none();
        row.add_var(
            &Var::Named("s".into()),
            &names.iter().map(|n| name(n)).collect(),
        );
        row
    }

    #[test]
    fn a_variable_is_within_what_names_all_it_is_taken_without() {
        let (s, log) = (less(&[]), Row::one(name("Log")));
        assert!(less(&["Log"]).within(&s));
        assert!(!s.within(&less(&["Log"])));
        assert!(s.within(&log.sum(&less(&["Log"])).expect("no name twice")));
        assert!(!s.within(&log.sum(&less(&["Clock", "Log"])).expect("no name twice")));
        assert!(
            less(&["Clock"]).within(&log.sum(&less(&["Clock", "Log"])).expect("no name twice"))
        );
    }

    #[test]
    fn a_variable_twice_is_without_what_both_are_without() {
        assert_eq!(
            less(&["Log"]).sum(&less(&[])).expect("no name twice"),
            less(&[])
        );
        let both = less(&["Clock", "Log"])
            .sum(&less(&["Log"]))
            .expect("no name twice");
        assert_eq!(both, less(&["Log"]));
    }

    #[test]
    fn what_stands_for_a_variable_loses_the_names_taken_out() {
        let mut inner = Row::one(name("Log"))
            .sum(&Row::one(name("Clock")))
            .expect("no name twice");
        inner.add_var(&Var::Named("r".into()), &BTreeSet::new());
        let mut out = Row::none();
        out.add_less(&inner, &BTreeSet::from([name("Log")]));
        let mut want = Row::one(name("Clock"));
        want.add_var(&Var::Named("r".into()), &BTreeSet::from([name("Log")]));
        assert_eq!(out, want);
    }
}
