//! Classes and instances: superclasses and functional dependencies of a
//! class, coherence of instances (no overlap, no orphans, strict coverage,
//! structural termination), the instance solver with its budget, the row
//! primitives and the structural instances of records, and `deriving`.
//! Rules: `docs/reference/classes.md`.

mod declare;
mod derive;
mod matching;
mod prims;
mod solve;

use std::collections::HashMap;

use super::Checker;
use super::env::{Form, Fundep};
use super::ty::{PRELUDE, Type};
use crate::source::Span;
use crate::types::regions;
pub use solve::Outcome;

/// The module of the row primitives.
pub const RECORD: &str = "Record";

/// Classes whose instances for records and tuples the compiler owns.
const STRUCTURAL: [&str; 4] = ["Eq", "Ord", "Hash", "Show"];

/// The dictionary a class constraint resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dict {
    /// An instance applied to the dictionaries of its context.
    Instance {
        module: String,
        name: String,
        args: Vec<Dict>,
    },
    /// A constraint of the enclosing signature or instance.
    Given(String),
    /// The superclass `class` of another dictionary.
    Super { dict: Box<Dict>, class: String },
    /// The compiler-owned instance of a record or tuple, by field.
    Structural { class: String, fields: Vec<Dict> },
    /// A row primitive, solved by the compiler: the constraint as shown, and
    /// the labels its operation needs at run time (those `Project` keeps).
    Primitive {
        constraint: String,
        labels: Vec<String>,
    },
    /// A constraint on a type that already produced a diagnostic.
    Error,
}

/// A constraint to solve, with what was assumed where it was required.
#[derive(Debug, Clone)]
pub struct Wanted {
    pub constraint: Type,
    pub span: Span,
    pub givens: Vec<Type>,
    /// Position among the constraints required so far, which orders the
    /// dictionaries of one use.
    pub order: u32,
}

#[derive(Debug, Default)]
pub struct State {
    /// Constraints of the definition being checked, not solved yet.
    pub wanted: Vec<Wanted>,
    /// Constraints assumed at this point: of the signature being checked,
    /// of an instance, of an enclosing higher-rank `forall`.
    pub givens: Vec<Type>,
    pub evidence: Vec<(Span, Dict)>,
    /// Solver steps spent on the current definition.
    pub steps: u32,
    /// Dictionaries of constraints without unknowns or variables.
    cache: HashMap<Type, Dict>,
    required: u32,
}

/// What a class declares, with the parameters it is written over.
pub struct ClassInfo {
    pub params: Vec<String>,
    pub supers: Vec<Type>,
    pub fundeps: Vec<Fundep>,
    pub methods: Vec<String>,
    pub defaults: Vec<String>,
}

/// The class and the arguments of a constraint `C t1 … tn`.
pub fn split(c: &Type) -> Option<((&str, &str), Vec<&Type>)> {
    let (head, args) = c.spine();
    match head {
        Type::Con { module, name } => Some(((module.as_str(), name.as_str()), args)),
        _ => None,
    }
}

impl Checker<'_> {
    /// Requires `constraint` at `span`.
    pub fn want(&mut self, constraint: Type, span: Span) {
        if let Some((("Fiber", "Outlives"), args)) = split(&constraint)
            && let [captures, owner] = args[..]
        {
            let outlives = regions::Outlives {
                captures: captures.clone(),
                owner: owner.clone(),
                givens: self.classes.givens.clone(),
                span,
            };
            self.regions.outlives.push(outlives);
            return;
        }
        self.classes.required += 1;
        self.classes.wanted.push(Wanted {
            constraint,
            span,
            givens: self.classes.givens.clone(),
            order: self.classes.required,
        });
    }

    pub fn class_info(&self, module: &str, name: &str) -> Option<ClassInfo> {
        match &self.env.type_def(module, name)?.form {
            Form::Class {
                params,
                supers,
                fundeps,
                methods,
                defaults,
            } => Some(ClassInfo {
                params: params.clone(),
                supers: supers.clone(),
                fundeps: fundeps.clone(),
                methods: methods.clone(),
                defaults: defaults.clone(),
            }),
            _ => None,
        }
    }

    /// The superclass constraints of `c`, over its arguments.
    fn supers_of(&self, c: &Type) -> Vec<Type> {
        let Some(((module, name), args)) = split(c) else {
            return Vec::new();
        };
        let Some(info) = self.class_info(module, name) else {
            return Vec::new();
        };
        let subst: Vec<(String, Type)> = info
            .params
            .into_iter()
            .zip(args.into_iter().cloned())
            .collect();
        info.supers.iter().map(|s| s.substitute(&subst)).collect()
    }

    /// `t` with what is known filled in, aliases expanded and fixed
    /// projections reduced at every level: the form instances match against.
    pub fn canon(&self, t: &Type) -> Type {
        self.expand(&self.zonk(t))
    }

    fn expand(&self, t: &Type) -> Type {
        self.whnf(t).map_children(&mut |c| self.expand(c))
    }

    /// Whether `class` is one whose records and tuples the compiler serves.
    fn structural(module: &str, name: &str) -> bool {
        module == PRELUDE && STRUCTURAL.contains(&name)
    }
}

/// Whether `t` holds unification variables.
fn has_metas(t: &Type) -> bool {
    t.contains(&mut |x| matches!(x, Type::Meta(_)))
}

/// Type variables of `t`, with repeats, in order of appearance.
fn vars(t: &Type) -> Vec<String> {
    let mut out = Vec::new();
    t.visit(&mut |x| {
        if let Type::Var(v) = x {
            out.push(v.clone());
        }
    });
    out
}

#[cfg(test)]
mod tests;
