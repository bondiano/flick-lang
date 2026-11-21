//! Executable model of B1/K1' class-instance elaboration and coherence
//! (`docs/formal/elaboration.md` §4, `docs/formal/regions.md` §2.2.1, Lean
//! `Formal/Elab/{Syntax,Variance,Hyp,Coherence}.lean`).
//!
//! Independent of `flick-core`: given a `Decls` whose class and data parameter
//! variances have been inferred by [`crate::variance`], this module models:
//! - variance-directed constraint service (`Cn.Serves`, [`serves`]),
//! - order-independent direction-aware dictionary resolution (`Ent.inst`,
//!   [`entail`]), producing core λFlick⁺ dictionary [`Term`]s that
//!   [`crate::check::Checker`] type-checks under `LeLive.dict` and `SafeUse`,
//! - symmetric K1' overlap checking ([`instances_overlap`], [`check_coherence`]).

use std::collections::HashMap;
use std::rc::Rc;

mod matching;
mod overlap;
mod sub;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_coherence;

pub use matching::match_instance;
pub use overlap::instances_overlap;
pub use sub::{caps_sub, reach_of, subst_type, ty_le};

use crate::decls::Decls;
use crate::syntax::{Name, Path, Term};
use crate::types::{Caps, Kind, Row, Scheme, Stage, Type};
use crate::variance::Sign;
use sub::strip_outer_caps;
/// Maximum entailment recursion depth (matches K5 well-foundedness guard).
const MAX_ENTAIL_DEPTH: usize = 64;

/// A class constraint `C τ̄`, corresponding to dictionary type `Type::Dict(C, τ̄)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    pub class: Path,
    pub args: Vec<Type>,
}

impl Constraint {
    pub fn new(class: Path, args: Vec<Type>) -> Self {
        Self { class, args }
    }

    pub fn to_dict_type(&self) -> Type {
        Type::Dict(self.class.clone(), Rc::from(self.args.as_slice()))
    }

    pub fn from_dict_type(t: &Type) -> Option<Self> {
        match strip_outer_caps(t) {
            Type::Dict(class, args) => Some(Self {
                class: class.clone(),
                args: args.to_vec(),
            }),
            _ => None,
        }
    }

    #[must_use]
    pub fn subst(&self, decls: &Decls, sigma: &TypeSubst) -> Self {
        Self {
            class: self.class.clone(),
            args: self
                .args
                .iter()
                .map(|a| subst_type(decls, a, sigma))
                .collect(),
        }
    }
}

/// An instance declaration `∀β̄. Δ ⇒ C H̄`, named by its dictionary global.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceDecl {
    pub name: Path,
    pub vars: Vec<(Name, Kind)>,
    pub context: Vec<Constraint>,
    pub head: Constraint,
}

/// An overlap detected by K1': two distinct instances of the same class that
/// can serve a common query under variance-directed resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlapError {
    pub class: Path,
    pub first: Path,
    pub second: Path,
}

/// Errors from order-independent B1 instance resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElabError {
    NoInstance(Constraint),
    Ambiguous(Constraint, Vec<Path>),
    DepthLimit(Constraint),
}

/// What a head variable of an instance scheme substitutes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubstVal {
    Type(Type),
    Row(Row),
    Caps(Caps),
}

pub type TypeSubst = HashMap<Name, SubstVal>;

/// Extracts all class instances from `decls` in deterministic order by `Path`.
pub fn instances_of(decls: &Decls) -> Vec<InstanceDecl> {
    let mut entries: Vec<(&Path, &Scheme)> = decls.instances.iter().collect();
    entries.sort_by_key(|(a, _)| *a);
    entries
        .into_iter()
        .filter_map(|(name, scheme)| {
            let mut context = Vec::new();
            let mut cur = &scheme.body;
            while let Type::Fun(param, Stage::Pure, rest) = cur {
                let c = Constraint::from_dict_type(param)?;
                context.push(c);
                cur = rest;
            }
            let head = Constraint::from_dict_type(cur)?;
            Some(InstanceDecl {
                name: name.clone(),
                vars: scheme.vars.clone(),
                context,
                head,
            })
        })
        .collect()
}

/// The inferred variance of parameter `j` of class `class`, defaulting to `Inv`.
pub fn class_sign(decls: &Decls, class: &Path, j: usize) -> Sign {
    decls
        .class_signs
        .get(class)
        .and_then(|s| s.get(j))
        .copied()
        .unwrap_or(Sign::Inv)
}

/// The inferred variance of parameter `j` of data constructor `con`, defaulting to `Inv`.
pub fn data_sign(decls: &Decls, con: &Path, j: usize) -> Sign {
    decls
        .signs
        .get(con)
        .and_then(|s| s.get(j))
        .copied()
        .unwrap_or(Sign::Inv)
}

/// `head` can serve `query` under the class's inferred parameter variances.
///
/// - `+` (producer-only): `head_j ≤ query_j`
/// - `-` (consumer-only): `query_j ≤ head_j` and `⌈head_j⌉ ⊆ ⌈query_j⌉` (`SafeUse`)
/// - `inv` or `bi`: `query_j = head_j`
pub fn serves(decls: &Decls, query: &Constraint, head: &Constraint) -> bool {
    query.class == head.class
        && query.args.len() == head.args.len()
        && serves_args(decls, &query.class, &query.args, &head.args)
}

pub(super) fn serves_args(
    decls: &Decls,
    class: &Path,
    query_args: &[Type],
    head_args: &[Type],
) -> bool {
    query_args
        .iter()
        .zip(head_args.iter())
        .enumerate()
        .all(|(j, (q, h))| match class_sign(decls, class, j) {
            Sign::Pos => ty_le(decls, h, q),
            Sign::Neg => ty_le(decls, q, h) && caps_sub(&reach_of(decls, h), &reach_of(decls, q)),
            Sign::Inv | Sign::Bi => q == h,
        })
}

/// Transitive superclass closure `Clo` of the given constraints.
fn expand_givens(decls: &Decls, givens: &[(Name, Constraint)]) -> Vec<(Term, Constraint)> {
    let mut out: Vec<(Term, Constraint)> = givens
        .iter()
        .map(|(x, c)| (Term::Var(x.clone()), c.clone()))
        .collect();
    let mut i = 0;
    while i < out.len() {
        let (term, c) = out[i].clone();
        i += 1;
        let Some(class) = decls.classes.get(&c.class) else {
            continue;
        };
        if class.params.len() != c.args.len() {
            continue;
        }
        let sigma: TypeSubst = class
            .params
            .iter()
            .zip(&c.args)
            .map(|(p, a)| (p.clone(), SubstVal::Type(a.clone())))
            .collect();
        for (slot_idx, sup_ty) in class.supers.iter().enumerate() {
            let Some(slot) = class.super_slot(slot_idx) else {
                continue;
            };
            let Some(sup_c) = Constraint::from_dict_type(&subst_type(decls, sup_ty, &sigma)) else {
                continue;
            };
            if !out.iter().any(|(_, existing)| existing == &sup_c) {
                let proj = Term::Field(Rc::new(term.clone()), Name::from(slot));
                out.push((proj, sup_c));
            }
        }
    }
    out
}

/// Order-independent B1 dictionary entailment (`Ent.given` + `Ent.inst`).
///
/// Checks superclass-closed givens first, then searches all declared instances
/// of `query.class` in `decls`. If more than one instance matches under
/// [`serves`], returns [`ElabError::Ambiguous`] regardless of declaration order.
pub fn entail(
    decls: &Decls,
    givens: &[(Name, Constraint)],
    query: &Constraint,
) -> Result<Term, ElabError> {
    let closed_givens = expand_givens(decls, givens);
    let instances = instances_of(decls);
    entail_inner(decls, &closed_givens, &instances, query, 0)
}

fn entail_inner(
    decls: &Decls,
    givens: &[(Term, Constraint)],
    instances: &[InstanceDecl],
    query: &Constraint,
    depth: usize,
) -> Result<Term, ElabError> {
    if depth >= MAX_ENTAIL_DEPTH {
        return Err(ElabError::DepthLimit(query.clone()));
    }
    if let Some((term, _)) = givens.iter().find(|(_, c)| serves(decls, query, c)) {
        return Ok(term.clone());
    }
    let mut matches: Vec<(&InstanceDecl, TypeSubst)> = Vec::new();
    for inst in instances {
        if let Some(sigma) = match_instance(decls, inst, query) {
            matches.push((inst, sigma));
        }
    }
    match matches.len() {
        0 => Err(ElabError::NoInstance(query.clone())),
        1 => {
            let (inst, sigma) = &matches[0];
            let mut ctx_terms = Vec::with_capacity(inst.context.len());
            for c in &inst.context {
                let sub_q = c.subst(decls, sigma);
                ctx_terms.push(entail_inner(decls, givens, instances, &sub_q, depth + 1)?);
            }
            Ok(Term::apps(Term::Global(inst.name.clone()), ctx_terms))
        }
        _ => Err(ElabError::Ambiguous(
            query.clone(),
            matches.into_iter().map(|(i, _)| i.name.clone()).collect(),
        )),
    }
}

/// Checks K1' coherence across every pair of distinct instances in `decls`.
pub fn check_coherence(decls: &Decls) -> Result<(), Vec<OverlapError>> {
    let instances = instances_of(decls);
    let mut overlaps = Vec::new();
    for i in 0..instances.len() {
        for j in (i + 1)..instances.len() {
            if instances_overlap(decls, &instances[i], &instances[j]) {
                overlaps.push(OverlapError {
                    class: instances[i].head.class.clone(),
                    first: instances[i].name.clone(),
                    second: instances[j].name.clone(),
                });
            }
        }
    }
    if overlaps.is_empty() {
        Ok(())
    } else {
        Err(overlaps)
    }
}
