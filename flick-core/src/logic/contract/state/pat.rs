//! Symbolic pattern matching and structural term helpers shared by symbolic
//! execution and kernel certificate verification.

use std::rc::Rc;

use crate::logic::{TUPLES, Ty};
use crate::source::Span;

use super::super::ContractFailure;
use super::super::term::{Pat, Term, TermKind};
use super::{BoolFormula, INT_MAX, INT_MIN, PathState, SymId, SymVal, lin_add_const, refuse};

/// Checks whether a value contains an escaping function value (a closure, a
/// partially applied contracted function, or an unresolved application), none
/// of which may escape from or be passed across a first-order contract.
pub fn contains_unsat_contracted(val: &SymVal, state: &PathState) -> bool {
    match state.whnf(val.clone()) {
        SymVal::Contracted { .. } | SymVal::Closure { .. } => true,
        SymVal::Con(_, fields) => fields
            .iter()
            .any(|field| contains_unsat_contracted(field, state)),
        _ => false,
    }
}

/// Flattens syntactic top-level `And`s into a left-to-right list of conjuncts.
pub fn collect_conjuncts<'a>(term: &'a Term, out: &mut Vec<&'a Term>) {
    if let TermKind::And(a, b) = &term.kind {
        collect_conjuncts(a, out);
        collect_conjuncts(b, out);
    } else {
        out.push(term);
    }
}

/// What splitting is needed before a pattern can be decided.
#[derive(Debug, Clone)]
pub enum SplitTarget {
    Data {
        sym_id: SymId,
        module: String,
        name: String,
        args: Vec<Ty>,
    },
    Tuple {
        sym_id: SymId,
        args: Vec<Ty>,
    },
    BoolAtom(SymId),
}

/// Result of matching one `match` arm's pattern against a symbolic value.
#[derive(Debug, Clone)]
pub enum PatMatch {
    /// Pattern matches whenever every condition in `conds` holds, binding
    /// `bindings`.
    Yes {
        bindings: Vec<(Rc<str>, SymVal)>,
        conds: Vec<BoolFormula>,
    },
    /// Pattern definitely does not match.
    No,
    /// A symbolic ADT/tuple/boolean atom must be split before deciding.
    NeedSplit(SplitTarget),
}

pub fn match_pat(
    pat: &Pat,
    val: &SymVal,
    state: &PathState,
    span: Span,
) -> Result<PatMatch, ContractFailure> {
    let mut bindings = Vec::new();
    let mut conds = Vec::new();
    match match_pat_into(pat, val, state, span, &mut bindings, &mut conds)? {
        PatStep::Yes => Ok(PatMatch::Yes { bindings, conds }),
        PatStep::No => Ok(PatMatch::No),
        PatStep::NeedSplit(target) => Ok(PatMatch::NeedSplit(target)),
    }
}

enum PatStep {
    Yes,
    No,
    NeedSplit(SplitTarget),
}

fn match_pat_into(
    pat: &Pat,
    val: &SymVal,
    state: &PathState,
    span: Span,
    bindings: &mut Vec<(Rc<str>, SymVal)>,
    conds: &mut Vec<BoolFormula>,
) -> Result<PatStep, ContractFailure> {
    let val = state.whnf(val.clone());
    match pat {
        Pat::Wild => Ok(PatStep::Yes),
        Pat::Bind(name) => {
            bindings.push((name.clone(), val));
            Ok(PatStep::Yes)
        }
        Pat::Unit => match val {
            SymVal::Unit => Ok(PatStep::Yes),
            _ => refuse(span, "an unknown `Unit` value is outside the domain"),
        },
        Pat::Int(k) => {
            if *k < INT_MIN || *k > INT_MAX {
                return refuse(
                    span,
                    "an `Int` pattern outside 32-bit range is outside the domain",
                );
            }
            match val {
                SymVal::Int(lin) => {
                    let neg_k = k.checked_neg().ok_or(ContractFailure::Exhausted { span })?;
                    let diff =
                        lin_add_const(&lin, neg_k).ok_or(ContractFailure::Exhausted { span })?;
                    if diff.coeffs.is_empty() {
                        Ok(if diff.constant == 0 {
                            PatStep::Yes
                        } else {
                            PatStep::No
                        })
                    } else {
                        conds.push(BoolFormula::LinEq(diff));
                        Ok(PatStep::Yes)
                    }
                }
                _ => refuse(span, "an unknown `Int` value is outside the domain"),
            }
        }
        Pat::Bool(expected) => match val {
            SymVal::Bool(b) => Ok(if b == *expected {
                PatStep::Yes
            } else {
                PatStep::No
            }),
            SymVal::BoolExpr(BoolFormula::True) => {
                Ok(if *expected { PatStep::Yes } else { PatStep::No })
            }
            SymVal::BoolExpr(BoolFormula::False) => {
                Ok(if *expected { PatStep::No } else { PatStep::Yes })
            }
            SymVal::BoolExpr(BoolFormula::Atom(id)) | SymVal::Sym(id, Ty::Bool) => {
                match state.bool_fact(id) {
                    Some(b) => Ok(if b == *expected {
                        PatStep::Yes
                    } else {
                        PatStep::No
                    }),
                    None => Ok(PatStep::NeedSplit(SplitTarget::BoolAtom(id))),
                }
            }
            SymVal::BoolExpr(f) => {
                let cond = if *expected { f } else { BoolFormula::not(f) };
                conds.push(cond);
                Ok(PatStep::Yes)
            }
            _ => refuse(span, "an unknown `Bool` value is outside the domain"),
        },
        Pat::Con(ctor, sub_pats) => match val {
            SymVal::Con(v_ctor, fields) if fields.len() == v_ctor.arity => {
                if v_ctor != *ctor {
                    return Ok(PatStep::No);
                }
                if sub_pats.len() != fields.len() {
                    return refuse(span, "constructor pattern arity mismatch");
                }
                for (sub_pat, field) in sub_pats.iter().zip(fields.iter()) {
                    match match_pat_into(sub_pat, field, state, span, bindings, conds)? {
                        PatStep::Yes => {}
                        other => return Ok(other),
                    }
                }
                Ok(PatStep::Yes)
            }
            SymVal::Sym(sym_id, Ty::Data { module, name, args }) => {
                if module == TUPLES {
                    Ok(PatStep::NeedSplit(SplitTarget::Tuple { sym_id, args }))
                } else {
                    Ok(PatStep::NeedSplit(SplitTarget::Data {
                        sym_id,
                        module,
                        name,
                        args,
                    }))
                }
            }
            _ => refuse(
                span,
                "a `match` on an unknown value of this type is outside the domain",
            ),
        },
    }
}
