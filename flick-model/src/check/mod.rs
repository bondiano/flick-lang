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

mod bindings;
mod constants;
mod dicts;
mod expr;
mod generalize;
mod handle;
mod handlers;
mod pattern;
mod regions;
mod rows;
mod runtime;
pub mod trace;
mod unify;
mod zonk;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ffi;
#[cfg(test)]
mod tests_forall;
#[cfg(test)]
mod tests_stores;

use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::hash::BuildHasher;
use std::mem;
use std::rc::Rc;

use flick_core::types::PURE_OVER_CAPABILITY;

use crate::decls::Decls;
use crate::eval::Program;
use crate::syntax::{Name, Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type};

pub(crate) use constants::prim_scheme;
pub(crate) use generalize::{Inst, in_regions, substitute};
pub(crate) use regions::{live_service, scope_service};
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
    /// What the rigid variables equal in the arms of GADT matches being
    /// checked, innermost last: the substitution θ the arm is typed under.
    equalities: Vec<(Name, Solved)>,
    /// Whether `≡` may equate a rigid variable: the result of a constructor
    /// of a GADT against the type of the matched value.
    refining: bool,
    /// The module of the definition being checked: its opaque aliases
    /// are their representations.
    module: Name,
    /// What each subterm was typed at, when a certificate is wanted.
    trace: Option<trace::Trace>,
    /// The scheme and instance of the variable or global just typed.
    instanced: Option<(Scheme, Vec<Inst>)>,
    /// The opaque aliases subsumption is unfolding on both sides: one met
    /// again inside its own unfolding is equal by its arguments.
    unfolding: Vec<Path>,
    /// The top-level value being checked and its scheme; once its body is
    /// skolemized, at the regions the body is checked at, through which its
    /// recursive uses go.
    pub own: Option<(Path, Scheme)>,
}

/// `σ` as the model reads a signature: with the `Live` elision adds
/// (`regions/live.rs`), which `formal/Formal/Live/ElideSchemes.lean` proves
/// to be `Ty.elide` of the core for the Prelude.
pub fn elided(decls: &Decls, s: &Scheme) -> Scheme {
    Checker::new(decls).live_scheme(s)
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
            equalities: Vec::new(),
            refining: false,
            module: Name::from(""),
            trace: None,
            instanced: None,
            unfolding: Vec::new(),
            own: None,
        }
    }

    /// `· ⊢ e : σ ! P` (§2.6): a top-level definition against its scheme,
    /// as (Let-Sig) checks the right side of a `let` with a signature.
    pub fn check_top(&mut self, e: &Term, scheme: &Scheme) -> Check<()> {
        self.stage = Stage::Pure;
        let (expected, rigid) = self.skolemize(scheme);
        if let Some((path, s)) = self.own.take() {
            let own = generalize::own_regions(&self.live_scheme(&s), &s.vars, &rigid);
            self.own = Some((path, own));
        }
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

    /// [`Self::check_top`], and what it typed each subterm at.
    pub fn check_top_traced(&mut self, e: &Term, scheme: &Scheme) -> Check<trace::Trace> {
        self.trace = Some(trace::Trace::default());
        self.check_top(e, scheme)?;
        let trace = self.trace.take().unwrap_or_default();
        Ok(self.zonk_trace(&trace))
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
        checker.module = path.module.clone();
        checker.own = scheme.map(|s| (path.clone(), s.clone()));
        let result = match scheme {
            Some(s) => checker.check_top(term, s),
            None => checker.infer_top(term).map(|_| ()),
        };
        if let Err(e) = result {
            out.push((path.clone(), e));
        }
    }
    let mut foreign: Vec<&Path> = decls
        .foreign
        .iter()
        .filter(|p| modules.contains(&p.module))
        .collect();
    foreign.sort();
    for path in foreign {
        let exempt = PURE_OVER_CAPABILITY.contains(&(&*path.module, &*path.name));
        if !exempt && decls.values.get(path).is_some_and(pure_over_capability) {
            out.push((
                path.clone(),
                TypeError {
                    rule: "G4",
                    message: format!("`{}` reads a capability with no stage, so a call after its scope would read what it released (`FLICK-F009`)", path.name),
                },
            ));
        }
    }
    out
}

/// G4: a parameter reaches a region the result does not carry, and no arrow
/// from it on runs a stage that would need the region live.
pub(super) fn pure_over_capability(s: &Scheme) -> bool {
    let names = |t: &Type| {
        let mut out = BTreeSet::new();
        regions::names_into(t, &mut out);
        out
    };
    let (mut params, mut stages, mut t) = (Vec::new(), Vec::new(), &s.body);
    loop {
        // a qualifier hides the next arrow; the final result keeps its own
        let mut arrow = t;
        while let Type::Captured(x, _) | Type::Bounded(x, _) = arrow {
            arrow = x;
        }
        let Type::Fun(p, stage, r) = arrow else { break };
        params.push(names(p));
        stages.push(stage);
        t = r;
    }
    let result = names(t);
    let used: Vec<&Name> = s
        .vars
        .iter()
        .filter(|(n, k)| *k == Kind::Region && !result.contains(n))
        .map(|(n, _)| n)
        .collect();
    params
        .iter()
        .position(|p| used.iter().any(|u| p.contains(*u)))
        .is_some_and(|i| stages[i..].iter().all(|s| matches!(s, Stage::Pure)))
}
