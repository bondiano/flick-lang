//! Classes and instances: superclasses and functional dependencies of a
//! class, coherence of instances (no overlap, no orphans, context-assisted
//! coverage, structural context restrictions), the budgeted instance solver,
//! the row primitives and structural instances of records, and `deriving`.
//! Rules: `docs/src/content/docs/reference/classes.md`.

mod coherence;
mod declare;
mod derive;
mod enum_instance;
mod generic;
mod matching;
mod prims;
mod report;
mod solve;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::Checker;
use super::env::{Form, Fundep};
use super::ty::Type;
use crate::source::Span;
use crate::types::lang::{self, Lang};
use crate::types::regions;
#[cfg(feature = "ide")]
pub(crate) use matching::{Match, match_type};
pub use solve::Outcome;

/// The row primitives: the compiler solves them from the fields of known
/// records.
pub const RECORD_CLASSES: [Lang; 8] = [
    Lang::HasField,
    Lang::Lacks,
    Lang::Remove,
    Lang::Project,
    Lang::UnionDisjoint,
    Lang::MapFields,
    Lang::Labels,
    Lang::PathParams,
];
/// The closed numeric classes, whose module has every instance.
pub const NUMERIC_CLASSES: [Lang; 3] = [Lang::Numeric, Lang::Integer, Lang::Floating];

/// Classes whose instances for records and tuples the compiler owns.
pub const STRUCTURAL: [Lang; 4] = [Lang::Eq, Lang::Ord, Lang::Hash, Lang::Show];

/// The lang item among `items` that `module.name` is.
pub fn which(items: &[Lang], module: &str, name: &str) -> Option<Lang> {
    items
        .iter()
        .copied()
        .find(|&item| lang::is(item, module, name))
}

/// The arguments of the constraint `c` when its class is `item`.
pub fn args_of(c: &Type, item: Lang) -> Option<Vec<&Type>> {
    let ((module, name), args) = split(c)?;
    lang::is(item, module, name).then_some(args)
}

/// The dictionary a class constraint resolved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl Dict {
    /// Whether it is built from an assumption, so holds only where that is.
    fn assumes(&self) -> bool {
        match self {
            Self::Given(_) => true,
            Self::Super { dict, .. } => dict.assumes(),
            Self::Instance { args: dicts, .. } | Self::Structural { fields: dicts, .. } => {
                dicts.iter().any(Self::assumes)
            }
            Self::Primitive { .. } | Self::Error => false,
        }
    }
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
    /// Dictionaries of constraints without unknowns or variables, built
    /// from instances alone.
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
    /// The schemes of the defaults with a signature of their own.
    pub default_sigs: Vec<(String, Type)>,
}

/// The field of a dictionary holding the superclass at `index` of `supers`:
/// the class's name, numbered when another superclass has the same name.
pub(super) fn super_slot(supers: &[Type], index: usize) -> String {
    fn name(t: &Type) -> &str {
        split(t).map_or("", |((_, n), _)| n)
    }
    let own = name(&supers[index]);
    if supers.iter().filter(|s| name(s) == own).count() > 1 {
        format!("{own}{index}")
    } else {
        own.to_owned()
    }
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
        if let Some(args) = args_of(&constraint, Lang::Outlives)
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
        if let Some(args) = args_of(&constraint, Lang::Provided)
            && let [row, owner] = args[..]
        {
            self.want_provided(row.clone(), owner.clone(), span);
            return;
        }
        self.want_under(constraint, span, self.classes.givens.clone());
    }

    /// Requires `constraint` at `span` where `givens` are in scope.
    pub fn want_under(&mut self, constraint: Type, span: Span, givens: Vec<Type>) {
        self.classes.required += 1;
        self.classes.wanted.push(Wanted {
            constraint,
            span,
            givens,
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
                default_sigs,
                ..
            } => Some(ClassInfo {
                params: params.clone(),
                supers: supers.clone(),
                fundeps: fundeps.clone(),
                methods: methods.clone(),
                defaults: defaults.clone(),
                default_sigs: default_sigs.clone(),
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
        which(&STRUCTURAL, module, name).is_some()
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
