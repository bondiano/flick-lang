//! The logic of `proof` declarations: statements about terms of the pure,
//! total domain `PureTotalV1`, and the checker of their certificates. It is
//! host code, the same for the command line, the editor and WASM; it runs
//! no Flick program and trusts no claim of success it did not check.

mod cert;
mod check;
mod defs;
mod eval;
#[cfg(test)]
pub(crate) mod lean;
mod show;
mod term;

use std::fmt;
use std::rc::Rc;

use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;

pub use cert::{Arg, Cert, Rule, cert};
pub use defs::{Admitted, Program, admit};
pub use term::{Ctor, Def, LEMMAS, Literal, Pat, RULES, Refused, TUPLES, Term, term};

/// The domain every statement is about, and the version of its checker.
pub const DOMAIN: &str = "PureTotalV1";
pub const CHECKER: u32 = 1;

/// The module of a class's parameter applied in a law, `f a`: a type the
/// instance of a bundle fills in.
pub const CLASS: &str = "$class";

/// A type of the domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// A type variable of the statement: any type of the domain.
    Rigid(String),
    Bool,
    /// A data type, or a base type without constructors, applied.
    Data {
        module: String,
        name: String,
        args: Vec<Ty>,
    },
    Fun(Box<Ty>, Box<Ty>),
    /// A type the domain does not have, and why.
    Refused(String),
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let atom = |t: &Ty| match t {
            Ty::Fun(..) => format!("({t})"),
            Ty::Data { args, .. } if !args.is_empty() => format!("({t})"),
            _ => t.to_string(),
        };
        match self {
            Ty::Rigid(a) => write!(f, "{a}"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Data { name, args, .. } => {
                write!(f, "{name}")?;
                args.iter().try_for_each(|a| write!(f, " {}", atom(a)))
            }
            Ty::Fun(a, b) => match **a {
                Ty::Fun(..) => write!(f, "({a}) -> {b}"),
                _ => write!(f, "{a} -> {b}"),
            },
            Ty::Refused(why) => write!(f, "<{why}>"),
        }
    }
}

impl Ty {
    /// `self` with each variable of `params` given its argument.
    #[must_use]
    pub fn instantiate(&self, params: &[String], args: &[Ty]) -> Ty {
        match self {
            Ty::Rigid(a) => params
                .iter()
                .position(|p| p == a)
                .and_then(|i| args.get(i).cloned())
                .unwrap_or_else(|| self.clone()),
            Ty::Data {
                module,
                name,
                args: own,
            } => {
                let own = own.iter().map(|t| t.instantiate(params, args));
                // a class's parameter applied: the instance's type takes the
                // arguments after its own
                let class = (module == CLASS)
                    .then(|| params.iter().position(|p| p == name))
                    .flatten()
                    .and_then(|i| args.get(i));
                match class {
                    Some(Ty::Data {
                        module,
                        name,
                        args: first,
                    }) => Ty::Data {
                        module: module.clone(),
                        name: name.clone(),
                        args: first.iter().cloned().chain(own).collect(),
                    },
                    Some(_) => Ty::Refused("a type applied that takes no arguments".into()),
                    None => Ty::Data {
                        module: module.clone(),
                        name: name.clone(),
                        args: own.collect(),
                    },
                }
            }
            Ty::Fun(a, b) => Ty::Fun(
                Box::new(a.instantiate(params, args)),
                Box::new(b.instantiate(params, args)),
            ),
            Ty::Bool | Ty::Refused(_) => self.clone(),
        }
    }
}

/// A data type a proof may split: its parameters and constructors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDecl {
    pub params: Vec<String>,
    pub constructors: Vec<(String, Vec<Ty>)>,
    /// It names itself, through its fields or aliases: outside the domain.
    pub recursive: bool,
}

/// The data types a proof may name, by module and name: a handful.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Data(Vec<((String, String), DataDecl)>);

impl Data {
    pub fn new() -> Self {
        Data::default()
    }

    pub fn get(&self, key: &(String, String)) -> Option<&DataDecl> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, d)| d)
    }

    pub fn get_mut(&mut self, key: &(String, String)) -> Option<&mut DataDecl> {
        self.0.iter_mut().find(|(k, _)| k == key).map(|(_, d)| d)
    }

    /// Adds the data types of `other` this has not.
    pub fn extend(&mut self, other: &Data) {
        for (key, decl) in &other.0 {
            if self.get(key).is_none() {
                self.insert(key.clone(), decl.clone());
            }
        }
    }

    pub fn insert(&mut self, key: (String, String), decl: DataDecl) {
        match self.get_mut(&key) {
            Some(d) => *d = decl,
            None => self.0.push((key, decl)),
        }
    }
}

/// `left = right`, both of type `ty`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Equation {
    pub ty: Ty,
    pub left: Term,
    pub right: Term,
}

impl Equation {
    fn swapped(&self) -> Equation {
        Equation {
            ty: self.ty.clone(),
            left: self.right.clone(),
            right: self.left.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Goal {
    Equal(Equation),
    SameType(Ty, Ty),
}

/// What a proof declaration states: for all values of its parameters, and
/// given its premises, its goal.
#[derive(Debug, Clone)]
pub struct Statement {
    pub name: String,
    pub span: Span,
    pub params: Vec<(Rc<str>, Ty)>,
    pub premises: Vec<(Rc<str>, Equation)>,
    pub goal: Goal,
}

/// A proof checked before, which a later proof may use.
#[derive(Debug, Clone)]
pub struct Lemma {
    pub statement: Statement,
    /// Its parameters in the order declared, each with whether it is a
    /// premise.
    pub order: Vec<(Rc<str>, bool)>,
}

/// Why a proof gives no certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The two sides are different values.
    Disproved {
        span: Span,
        left: String,
        right: String,
    },
    /// Nothing here makes the two sides the same.
    Unproved {
        span: Span,
        left: String,
        right: String,
        note: Option<String>,
    },
    /// A form or a type outside the domain.
    Refused(Refused),
    /// The proof is no certificate of its goal.
    Invalid { span: Span, why: String },
    /// The budget ran out; nothing is known.
    Exhausted { span: Span },
}

impl Failure {
    pub fn diagnostic(&self, proof: &str) -> Diagnostic {
        match self {
            Failure::Disproved { span, left, right } => Diagnostic::error(
                codes::T080,
                format!("`{proof}` is false: `{left}` and `{right}` are different values"),
                *span,
            ),
            Failure::Unproved {
                span,
                left,
                right,
                note,
            } => {
                let note = note.as_deref().map_or(String::new(), |n| format!("; {n}"));
                Diagnostic::error(
                    codes::T081,
                    format!(
                        "the proof of `{proof}` does not close the goal `{left} = {right}`{note}"
                    ),
                    *span,
                )
            }
            Failure::Refused(r) => Diagnostic::error(
                codes::T082,
                format!(
                    "`{proof}` cannot be proved in the domain `{DOMAIN}`: {}",
                    r.why
                ),
                r.span,
            ),
            Failure::Invalid { span, why } => Diagnostic::error(
                codes::T083,
                format!("the proof of `{proof}` is no valid certificate: {why}"),
                *span,
            ),
            Failure::Exhausted { span } => Diagnostic::error(
                codes::T084,
                format!("the proof of `{proof}` ran out of its budget: nothing is proved"),
                *span,
            ),
        }
    }
}

/// Checks the certificate of `statement`.
pub fn prove(
    statement: &Statement,
    proof: &Cert,
    data: &Data,
    program: &dyn Program,
    lemmas: &[Lemma],
) -> Result<(), Failure> {
    check::prove(statement, proof, data, program, lemmas)
}

#[cfg(test)]
mod tests;
