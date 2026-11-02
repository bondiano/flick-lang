//! The statics of λFlick (`docs/formal/core.md` §2) as type inference.
//!
//! Each rule of §2.3 is one function, named after it, in `expr.rs` and
//! `handlers.rs`; patterns and `Exh` (§2.4) are in `pattern.rs`, `Gen`
//! (§2.5) in `generalize.rs`, and dictionaries (§1.6) in `dicts.rs`. The parts a declarative rule guesses are found
//! by unification: a type is a meta until a use fixes it, a row grows
//! from the inclusions `ρ ⊆ ρ'` its uses need and is settled to the least
//! one (`rows.rs`). The checker only accepts what the rules derive; it may
//! reject what they derive by a guess it does not make (as flick-core
//! may), and a program of the corpus it rejects is a discrepancy to record.
//!
//! The judgement `Γ ⊢ e : τ ! φ` is `infer`/`check` returning `τ`, with
//! `φ` accumulated in `self.stage`. The forms of the run time (§3.6),
//! which the translation never makes, are typed in `runtime.rs`, so that
//! Preservation can be checked on the states of the machine (the model properties, `just model`).

mod constants;
mod dicts;
mod expr;
mod generalize;
mod handlers;
mod pattern;
mod regions;
mod rows;
mod runtime;
mod unify;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::fmt;
use std::hash::BuildHasher;
use std::mem;
use std::rc::Rc;

use crate::decls::Decls;
use crate::eval::Program;
use crate::syntax::{Name, Path, Term};
use crate::types::{Caps, Fields, Row, Scheme, Stage, Type};

pub(crate) use generalize::{Inst, substitute};
pub(crate) use regions::scope_service;
pub use runtime::check_config;

/// A rule whose premise fails, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError {
    pub rule: &'static str,
    pub message: String,
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}) {}", self.rule, self.message)
    }
}

pub type Check<T> = Result<T, TypeError>;

pub fn error<T>(rule: &'static str, message: impl Into<String>) -> Check<T> {
    Err(TypeError {
        rule,
        message: message.into(),
    })
}

/// What a meta is solved to.
#[derive(Clone, Debug)]
enum Solved {
    Type(Type),
    Row(Row),
    Fields(Fields),
    Caps(Caps),
}

#[derive(Clone)]
struct Slot {
    level: u32,
    solved: Option<Solved>,
}

/// Cloned to try one more way of typing a configuration (`handlers.rs`
/// `match_`).
#[derive(Clone)]
pub struct Checker<'d> {
    decls: &'d Decls,
    slots: Vec<Slot>,
    /// How many `let`s deep the right side being checked is, for `Gen`.
    level: u32,
    /// `Γ`: the locals in scope, innermost last.
    locals: Vec<(Name, Scheme)>,
    /// `φ` of the expression being checked so far.
    stage: Stage,
    /// `ρ ⊆ ρ'` whose left side waits for a meta to be solved.
    pending: Vec<(Row, Row)>,
    /// Compiler-owned instances waiting for the type of their record.
    structural: Vec<(Name, Rc<[Term]>, Type)>,
    /// Operands of `<`, `<=`, `>`, `>=`, checked once their type is known.
    ordered: Vec<Type>,
    /// Counter of the rigid variables a signature introduces.
    rigid: u32,
    /// `Ψ`: the type of each cell of `μ`, when a configuration is checked.
    store: Vec<Type>,
    /// Whether a configuration is checked: the rules of §3.6 apply.
    config: bool,
    /// Obligations and `scope`s of `docs/formal/regions.md` §2.
    regions: regions::Regions,
}

impl<'d> Checker<'d> {
    pub fn new(decls: &'d Decls) -> Self {
        Checker {
            decls,
            slots: Vec::new(),
            level: 0,
            locals: Vec::new(),
            stage: Stage::Pure,
            pending: Vec::new(),
            structural: Vec::new(),
            ordered: Vec::new(),
            rigid: 0,
            store: Vec::new(),
            config: false,
            regions: regions::Regions::default(),
        }
    }

    /// `· ⊢ e : σ ! P` (§2.6): a top-level definition against its scheme,
    /// as (Let-Sig) checks the right side of a `let` with a signature.
    pub fn check_top(&mut self, e: &Term, scheme: &Scheme) -> Check<()> {
        self.stage = Stage::Pure;
        let expected = self.skolemize(scheme);
        self.check(e, &expected)?;
        self.finish()?;
        if self.stage != Stage::Pure {
            return error(
                "Module",
                format!("the initialization is not pure: {}", self.stage),
            );
        }
        Ok(())
    }

    /// `· ⊢ e : τ ! φ`, with the rows of `τ` and `φ` settled.
    pub fn infer_top(&mut self, e: &Term) -> Check<(Type, Stage)> {
        self.stage = Stage::Pure;
        let t = self.infer(e)?;
        self.finish()?;
        let stage = self.zonk_stage(&self.stage.clone());
        Ok((self.zonk(&t), stage))
    }

    /// Settles the rows and checks what waited for a type.
    fn finish(&mut self) -> Check<()> {
        for (class, fields, t) in mem::take(&mut self.structural) {
            self.structural_fields(&class, &fields, &t)?;
        }
        for t in mem::take(&mut self.ordered) {
            self.ordered_operand(&t)?;
        }
        self.settle()?;
        self.solve_regions()
    }
}

/// Checks the top-level values of the modules `modules` of `program`
/// against their schemes in `decls`; values the model writes itself
/// (derived instances) are trusted like natives.
pub fn check_program<S: BuildHasher>(
    program: &Program,
    decls: &Decls,
    modules: &HashSet<Name, S>,
) -> Vec<(Path, TypeError)> {
    let mut globals: Vec<(&Path, &Term)> = program
        .globals
        .iter()
        .filter(|(p, _)| modules.contains(&p.module) && !program.derived.contains(*p))
        .collect();
    globals.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = Vec::new();
    for (path, term) in globals {
        let scheme = decls.values.get(path).or_else(|| decls.instances.get(path));
        let mut checker = Checker::new(decls);
        let result = match scheme {
            Some(s) => checker.check_top(term, s),
            None => checker.infer_top(term).map(|_| ()),
        };
        if let Err(e) = result {
            out.push((path.clone(), e));
        }
    }
    out
}
