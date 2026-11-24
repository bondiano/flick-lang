//! Independent Rust kernel for `PureTotalV2` contract certificates.
//!
//! Trust-boundary invariants:
//! - Imports ONLY [`omega::check`] (and [`omega::Linear`]); NEVER calls
//!   `omega::decide` or `omega::decide_budgeted` and NEVER imports `eval`.
//! - Re-derives every path condition, `Int` range obligation, callee
//!   precondition, and postcondition directly from `(spec, body, data,
//!   program, verified, lang)` in strict lockstep with `cert`.
//! - Rejects any mismatched, tampered, forged, or incomplete certificate with
//!   [`ContractFailure::Invalid`] (`FLICK-T083`).

mod branch;
mod calls;
mod ops;
mod prove;

use crate::logic::omega::{self, Linear};
use crate::logic::{DOMAIN_V2, Data, Program};
use crate::source::Span;

use super::cert::{ContractCert, LeafCert, PathCert};
use super::defs::AdmittedV2;
use super::state::{
    BoolFormula, Budgets, EvalMode, PathState, SymEnv, SymVal, admit_contract, collect_conjuncts,
    contains_unsat_contracted, is_trivially_unsat_linear, lin_add_const, lin_not_ge, local_span,
    to_bool_formula,
};
use super::term::{LangTypes, Term, TermKind};
use super::{ContractFailure, ContractSpec, VerifiedContracts};

type VerifyEvalCont<'b, 'a> = &'b mut dyn FnMut(
    &mut Verifier<'a>,
    SymVal,
    &mut PathState,
    &PathCert,
) -> Result<(), ContractFailure>;
type VerifyAssumeCont<'b, 'a> =
    &'b mut dyn FnMut(&mut Verifier<'a>, &mut PathState, &PathCert) -> Result<(), ContractFailure>;

/// Independently checks `cert` against `(spec, body, data, program, verified,
/// lang)` using default deterministic budgets.
pub fn check_contract_cert(
    spec: &ContractSpec,
    body: &Term,
    cert: &ContractCert,
    data: &Data,
    program: &dyn Program,
    verified: &VerifiedContracts,
    lang: &LangTypes,
) -> Result<(), ContractFailure> {
    check_contract_cert_with_budgets(
        spec,
        body,
        cert,
        data,
        program,
        verified,
        lang,
        Budgets::default(),
    )
}

/// Independently checks `cert` under explicit deterministic `budgets`.
#[allow(clippy::too_many_arguments)]
pub fn check_contract_cert_with_budgets(
    spec: &ContractSpec,
    body: &Term,
    cert: &ContractCert,
    data: &Data,
    program: &dyn Program,
    verified: &VerifiedContracts,
    lang: &LangTypes,
    budgets: Budgets,
) -> Result<(), ContractFailure> {
    if cert.domain != DOMAIN_V2 {
        return invalid(
            spec.contract_span,
            format!(
                "expected certificate domain `{DOMAIN_V2}`, found `{}`",
                cert.domain
            ),
        );
    }

    let admitted = admit_contract(spec, body, data, program, verified, lang)?;
    let mut state = PathState::default();
    let env = state.init_params(spec, lang)?;
    let mut checker = Verifier {
        spec,
        defs: &admitted.defs,
        data: &admitted.data,
        verified,
        lang,
        steps: budgets.max_steps,
        depth: 0,
        max_depth: budgets.max_depth,
        caller_span: spec.decl_span,
        in_foreign: false,
    };

    let requires: Vec<&Term> = {
        let mut flat = Vec::new();
        for (req, _) in &spec.requires {
            collect_conjuncts(req, &mut flat);
        }
        flat
    };

    checker.assume_terms(
        &requires,
        &env,
        &mut state,
        &cert.root,
        &mut |verifier, state, cert| {
            verifier.eval_term(
                body,
                &env,
                EvalMode::Body,
                state,
                cert,
                &mut |verifier, result_val, state, cert| {
                    if contains_unsat_contracted(&result_val, state) {
                        return super::state::refuse(
                            body.span,
                            "a partially applied contracted function cannot escape",
                        );
                    }
                    verifier.verify_ensures(&spec.ensures, &env, &result_val, state, cert)
                },
            )
        },
    )
}

fn invalid<T>(span: Span, why: impl Into<String>) -> Result<T, ContractFailure> {
    Err(ContractFailure::Invalid {
        span,
        why: why.into(),
    })
}

fn check_dead_leaf(state: &PathState, leaf: &LeafCert) -> bool {
    match leaf {
        LeafCert::Trivial => state.has_trivial_contradiction(),
        LeafCert::Omega(proof) => omega::check(&state.facts, state.vars, proof),
    }
}

fn check_negated_linear_leaf(state: &mut PathState, neg: &Linear, leaf: &LeafCert) -> bool {
    match leaf {
        LeafCert::Trivial => is_trivially_unsat_linear(neg) || state.has_trivial_contradiction(),
        LeafCert::Omega(proof) => {
            state.facts.push(neg.clone());
            let ok = omega::check(&state.facts, state.vars, proof);
            state.facts.pop();
            ok
        }
    }
}

struct Verifier<'a> {
    spec: &'a ContractSpec,
    defs: &'a AdmittedV2,
    data: &'a Data,
    verified: &'a VerifiedContracts,
    lang: &'a LangTypes,
    steps: u64,
    depth: u32,
    max_depth: u32,
    caller_span: Span,
    in_foreign: bool,
}

impl<'a> Verifier<'a> {
    fn diag_span(&self, span: Span) -> Span {
        if self.in_foreign {
            self.caller_span
        } else {
            local_span(span, self.caller_span)
        }
    }

    fn enter(&mut self, span: Span) -> Result<(), ContractFailure> {
        if self.steps == 0 || self.depth >= self.max_depth {
            return Err(ContractFailure::Exhausted {
                span: self.err_span(span),
            });
        }
        self.steps -= 1;
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn err_span(&self, span: Span) -> Span {
        if self.in_foreign {
            self.caller_span
        } else {
            local_span(span, self.spec.contract_span)
        }
    }

    fn assume_terms(
        &mut self,
        terms: &[&Term],
        env: &SymEnv,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let Some((first, rest)) = terms.split_first() else {
            return cont(self, state, cert);
        };
        self.assume_term(first, env, state, cert, &mut |verifier, state, cert| {
            verifier.assume_terms(rest, env, state, cert, cont)
        })
    }

    fn assume_term(
        &mut self,
        term: &Term,
        env: &SymEnv,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        if let TermKind::And(a, b) = &term.kind {
            return self.assume_term(a, env, state, cert, &mut |verifier, state, cert| {
                verifier.assume_term(b, env, state, cert, cont)
            });
        }
        let span = term.span;
        self.eval_term(
            term,
            env,
            EvalMode::Spec,
            state,
            cert,
            &mut |verifier, val, state, cert| {
                let formula = to_bool_formula(&val, state, span)?;
                verifier.assume_formula(&formula, true, span, state, cert, cont)
            },
        )
    }

    fn assume_formula(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.enter(span)?;
        let res = self.assume_formula_inner(formula, expected, span, state, cert, cont);
        self.leave();
        res
    }

    fn assume_formula_inner(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        match (formula, expected) {
            (BoolFormula::True, true) | (BoolFormula::False, false) => cont(self, state, cert),
            (BoolFormula::True, false) | (BoolFormula::False, true) => {
                if matches!(cert, PathCert::Dead(LeafCert::Trivial)) {
                    Ok(())
                } else {
                    invalid(
                        self.err_span(span),
                        "expected `Dead(Trivial)` certificate for literal contradiction",
                    )
                }
            }
            (BoolFormula::Not(inner), exp) => {
                self.assume_formula(inner, !exp, span, state, cert, cont)
            }
            (BoolFormula::Atom(id), exp) => match state.bool_fact(*id) {
                Some(prev) if prev == exp => cont(self, state, cert),
                Some(_) => {
                    if matches!(cert, PathCert::Dead(LeafCert::Trivial)) {
                        Ok(())
                    } else {
                        invalid(
                            self.err_span(span),
                            "expected `Dead(Trivial)` certificate for contradictory boolean atom",
                        )
                    }
                }
                None => {
                    let mark = state.mark();
                    state.bool_facts.push((*id, exp));
                    let res = cont(self, state, cert);
                    state.restore(mark);
                    res
                }
            },
            (BoolFormula::LinGe(f), true) => self.assume_lin_ge(f, span, state, cert, cont),
            (BoolFormula::LinGe(f), false) => {
                let nf = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
                    span: self.err_span(span),
                })?;
                self.assume_lin_ge(&nf, span, state, cert, cont)
            }
            (BoolFormula::LinEq(f), exp) => self.assume_lin_eq(f, exp, span, state, cert, cont),
            (BoolFormula::And(a, b), true) => {
                self.assume_formula(a, true, span, state, cert, &mut |verifier, state, cert| {
                    verifier.assume_formula(b, true, span, state, cert, cont)
                })
            }
            (BoolFormula::Or(a, b), false) => {
                self.assume_formula(a, false, span, state, cert, &mut |verifier, state, cert| {
                    verifier.assume_formula(b, false, span, state, cert, cont)
                })
            }
            (BoolFormula::Or(a, b), true) => {
                let PathCert::Branch {
                    then_branch,
                    else_branch,
                } = cert
                else {
                    return invalid(
                        self.err_span(span),
                        "expected a `Branch` certificate for disjunction assumption",
                    );
                };
                self.assume_formula(a, true, span, state, then_branch, cont)?;
                self.assume_formula(
                    a,
                    false,
                    span,
                    state,
                    else_branch,
                    &mut |verifier, state, cert| {
                        verifier.assume_formula(b, true, span, state, cert, cont)
                    },
                )
            }
            (BoolFormula::And(a, b), false) => {
                let PathCert::Branch {
                    then_branch,
                    else_branch,
                } = cert
                else {
                    return invalid(
                        self.err_span(span),
                        "expected a `Branch` certificate for negated conjunction assumption",
                    );
                };
                self.assume_formula(a, false, span, state, then_branch, cont)?;
                self.assume_formula(
                    a,
                    true,
                    span,
                    state,
                    else_branch,
                    &mut |verifier, state, cert| {
                        verifier.assume_formula(b, false, span, state, cert, cont)
                    },
                )
            }
        }
    }

    fn assume_lin_eq(
        &mut self,
        f: &Linear,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        if f.coeffs.is_empty() {
            return if (f.constant == 0) == expected {
                cont(self, state, cert)
            } else if matches!(cert, PathCert::Dead(LeafCert::Trivial)) {
                Ok(())
            } else {
                invalid(
                    self.err_span(span),
                    "expected `Dead(Trivial)` certificate for constant equality contradiction",
                )
            };
        }
        if expected {
            let nf = f.neg().ok_or(ContractFailure::Exhausted {
                span: self.err_span(span),
            })?;
            let mark = state.mark();
            state.facts.push(f.clone());
            state.facts.push(nf);
            let res = if let PathCert::Dead(leaf) = cert
                && check_dead_leaf(state, leaf)
            {
                Ok(())
            } else {
                cont(self, state, cert)
            };
            state.restore(mark);
            return res;
        }
        let PathCert::Branch {
            then_branch,
            else_branch,
        } = cert
        else {
            return invalid(
                self.err_span(span),
                "expected a `Branch` certificate for `!=` split",
            );
        };
        let gt = lin_add_const(f, -1).ok_or(ContractFailure::Exhausted {
            span: self.err_span(span),
        })?;
        let lt = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
            span: self.err_span(span),
        })?;
        self.assume_lin_ge(&gt, span, state, then_branch, cont)?;
        self.assume_lin_ge(&lt, span, state, else_branch, cont)
    }

    fn assume_lin_ge(
        &mut self,
        f: &Linear,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        if f.coeffs.is_empty() {
            return if f.constant >= 0 {
                cont(self, state, cert)
            } else if matches!(cert, PathCert::Dead(LeafCert::Trivial)) {
                Ok(())
            } else {
                invalid(
                    self.err_span(span),
                    "expected `Dead(Trivial)` certificate for constant contradiction",
                )
            };
        }
        let mark = state.mark();
        state.facts.push(f.clone());
        let res = if let PathCert::Dead(leaf) = cert
            && check_dead_leaf(state, leaf)
        {
            Ok(())
        } else {
            cont(self, state, cert)
        };
        state.restore(mark);
        res
    }
}
