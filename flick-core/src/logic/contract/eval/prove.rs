//! Verification condition discharging for `require` preconditions, `ensure`
//! postconditions, and boolean/linear formulas via Omega.

use crate::logic::omega::{self, Outcome};
use crate::source::Span;

use super::super::ContractFailure;
use super::super::cert::{LeafCert, PathCert};
use super::super::state::{
    BoolFormula, EvalMode, PathState, SymEnv, SymId, SymVal, is_trivially_unsat_linear, lin_not_ge,
    local_span, to_bool_formula,
};
use super::super::term::Term;
use super::counterexample::format_counterexample;
use super::{FailKind, Solver};

impl<'a> Solver<'a> {
    /// Proves all `require` clauses of a contracted callee under `callee_env`.
    pub(super) fn prove_requires(
        &mut self,
        requires: &[(Term, Span)],
        callee_env: &SymEnv,
        call_span: Span,
        callee: &'a str,
        state: &mut PathState,
    ) -> Result<PathCert, ContractFailure> {
        if requires.is_empty() {
            return Ok(PathCert::Leaf(LeafCert::Trivial));
        }
        let mut certs = Vec::with_capacity(requires.len());
        for (req_term, req_span) in requires {
            let fail_kind = FailKind::Precondition {
                call_span,
                callee,
                callee_require: Some(*req_span),
            };
            let cert = self.eval_term(
                req_term,
                callee_env,
                EvalMode::Spec,
                state,
                &mut |solver, val, state| {
                    let formula = to_bool_formula(&val, state, *req_span)?;
                    solver.prove_formula(&formula, true, &fail_kind, state)
                },
            )?;
            certs.push(cert);
        }
        if certs.len() == 1 {
            Ok(certs.pop().expect("single require"))
        } else {
            Ok(PathCert::And(certs))
        }
    }

    /// Proves all `ensure` clauses of `self.spec` for `result_val`.
    pub(super) fn prove_ensures(
        &mut self,
        ensures: &[(Term, Span)],
        env: &SymEnv,
        result_val: &SymVal,
        decl_span: Span,
        state: &mut PathState,
    ) -> Result<PathCert, ContractFailure> {
        if ensures.is_empty() {
            return Ok(PathCert::Leaf(LeafCert::Trivial));
        }
        let mut certs = Vec::with_capacity(ensures.len());
        for (ens_term, ens_span) in ensures {
            let fail_kind = FailKind::Postcondition {
                span: *ens_span,
                decl_span,
            };
            let cert = self.eval_term(
                ens_term,
                env,
                EvalMode::Spec,
                state,
                &mut |solver, pred_val, state| {
                    solver.apply_val(
                        pred_val,
                        result_val.clone(),
                        EvalMode::Spec,
                        *ens_span,
                        state,
                        &mut |solver, bool_val, state| {
                            let formula = to_bool_formula(&bool_val, state, *ens_span)?;
                            solver.prove_formula(&formula, true, &fail_kind, state)
                        },
                    )
                },
            )?;
            certs.push(cert);
        }
        if certs.len() == 1 {
            Ok(certs.pop().expect("single ensure"))
        } else {
            Ok(PathCert::And(certs))
        }
    }

    /// Proves that `formula == expected` holds for all points satisfying `state`.
    fn prove_formula(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        fail: &FailKind<'a>,
        state: &mut PathState,
    ) -> Result<PathCert, ContractFailure> {
        self.enter(fail.span())?;
        let res = self.prove_formula_inner(formula, expected, fail, state);
        self.leave();
        res
    }

    fn prove_formula_inner(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        fail: &FailKind<'a>,
        state: &mut PathState,
    ) -> Result<PathCert, ContractFailure> {
        match (formula, expected) {
            (BoolFormula::True, true) | (BoolFormula::False, false) => {
                Ok(PathCert::Leaf(LeafCert::Trivial))
            }
            (BoolFormula::True, false) | (BoolFormula::False, true) => {
                self.prove_state_unsat(fail, state, None)
            }
            (BoolFormula::Not(inner), exp) => self.prove_formula(inner, !exp, fail, state),
            (BoolFormula::Atom(id), exp) => {
                if state.bool_fact(*id) == Some(exp) {
                    Ok(PathCert::Leaf(LeafCert::Trivial))
                } else {
                    self.prove_state_unsat(fail, state, Some((*id, !exp)))
                }
            }
            (BoolFormula::LinGe(f), true) => self.prove_lin_ge(f, fail, state),
            (BoolFormula::LinGe(f), false) => {
                let nf = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
                    span: local_span(fail.span(), self.spec.contract_span),
                })?;
                self.prove_lin_ge(&nf, fail, state)
            }
            (BoolFormula::LinEq(f), true) => {
                if f.coeffs.is_empty() {
                    return if f.constant == 0 {
                        Ok(PathCert::Leaf(LeafCert::Trivial))
                    } else {
                        self.prove_state_unsat(fail, state, None)
                    };
                }
                let nf = f.neg().ok_or(ContractFailure::Exhausted {
                    span: local_span(fail.span(), self.spec.contract_span),
                })?;
                let p1 = self.prove_lin_ge(f, fail, state)?;
                let p2 = self.prove_lin_ge(&nf, fail, state)?;
                Ok(PathCert::And(vec![p1, p2]))
            }
            (BoolFormula::LinEq(f), false) => {
                if f.coeffs.is_empty() {
                    return if f.constant != 0 {
                        Ok(PathCert::Leaf(LeafCert::Trivial))
                    } else {
                        self.prove_state_unsat(fail, state, None)
                    };
                }
                let nf = f.neg().ok_or(ContractFailure::Exhausted {
                    span: local_span(fail.span(), self.spec.contract_span),
                })?;
                let mark = state.mark();
                state.facts.push(f.clone());
                state.facts.push(nf);
                let outcome = self.decide(state);
                state.restore(mark);
                match outcome {
                    Outcome::Refuted(proof) => Ok(PathCert::Leaf(LeafCert::Omega(proof))),
                    Outcome::Model(model) => Err(fail
                        .clone()
                        .into_failure(Some(format_counterexample(state, &model, None)))),
                    Outcome::Unknown => Err(ContractFailure::Exhausted {
                        span: local_span(fail.span(), self.spec.contract_span),
                    }),
                }
            }
            (BoolFormula::And(a, b), true) => {
                let p1 = self.prove_formula(a, true, fail, state)?;
                let p2 = self.prove_formula(b, true, fail, state)?;
                Ok(PathCert::And(vec![p1, p2]))
            }
            (BoolFormula::Or(a, b), false) => {
                let p1 = self.prove_formula(a, false, fail, state)?;
                let p2 = self.prove_formula(b, false, fail, state)?;
                Ok(PathCert::And(vec![p1, p2]))
            }
            (BoolFormula::Or(a, b), true) => {
                self.assume_formula(a, false, fail.span(), state, &mut |solver, state| {
                    solver.prove_formula(b, true, fail, state)
                })
            }
            (BoolFormula::And(a, b), false) => {
                self.assume_formula(a, true, fail.span(), state, &mut |solver, state| {
                    solver.prove_formula(b, false, fail, state)
                })
            }
        }
    }

    fn prove_lin_ge(
        &mut self,
        f: &omega::Linear,
        fail: &FailKind<'a>,
        state: &mut PathState,
    ) -> Result<PathCert, ContractFailure> {
        let neg = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
            span: local_span(fail.span(), self.spec.contract_span),
        })?;
        if is_trivially_unsat_linear(&neg) {
            return Ok(PathCert::Leaf(LeafCert::Trivial));
        }
        state.facts.push(neg);
        let outcome = self.decide(state);
        state.facts.pop();
        match outcome {
            Outcome::Refuted(proof) => Ok(PathCert::Leaf(LeafCert::Omega(proof))),
            Outcome::Model(model) => Err(fail
                .clone()
                .into_failure(Some(format_counterexample(state, &model, None)))),
            Outcome::Unknown => Err(ContractFailure::Exhausted {
                span: local_span(fail.span(), self.spec.contract_span),
            }),
        }
    }

    fn prove_state_unsat(
        &mut self,
        fail: &FailKind<'a>,
        state: &PathState,
        bool_override: Option<(SymId, bool)>,
    ) -> Result<PathCert, ContractFailure> {
        if state.has_trivial_contradiction() {
            return Ok(PathCert::Leaf(LeafCert::Trivial));
        }
        match self.decide(state) {
            Outcome::Refuted(proof) => Ok(PathCert::Leaf(LeafCert::Omega(proof))),
            Outcome::Model(model) => Err(fail.clone().into_failure(Some(format_counterexample(
                state,
                &model,
                bool_override,
            )))),
            Outcome::Unknown => Err(ContractFailure::Exhausted {
                span: local_span(fail.span(), self.spec.contract_span),
            }),
        }
    }
}
