use crate::logic::omega::Linear;
use crate::source::Span;

use super::super::ContractFailure;
use super::super::cert::{LeafCert, PathCert};
use super::super::state::{
    BoolFormula, EvalMode, PathState, SymEnv, SymVal, lin_not_ge, to_bool_formula,
};
use super::super::term::Term;
use super::{Verifier, check_dead_leaf, check_negated_linear_leaf, invalid};

impl Verifier<'_> {
    pub(super) fn verify_requires(
        &mut self,
        requires: &[(Term, Span)],
        callee_env: &SymEnv,
        call_span: Span,
        state: &mut PathState,
        cert: &PathCert,
    ) -> Result<(), ContractFailure> {
        if requires.is_empty() {
            return if matches!(cert, PathCert::Leaf(LeafCert::Trivial)) {
                Ok(())
            } else {
                invalid(
                    self.err_span(call_span),
                    "expected `Leaf(Trivial)` certificate for empty precondition list",
                )
            };
        }
        if requires.len() == 1 {
            let (req_term, req_span) = &requires[0];
            return self.eval_term(
                req_term,
                callee_env,
                EvalMode::Spec,
                state,
                cert,
                &mut |verifier, val, state, cert| {
                    let formula = to_bool_formula(&val, state, *req_span)?;
                    verifier.verify_formula(&formula, true, call_span, state, cert)
                },
            );
        }
        let PathCert::And(subs) = cert else {
            return invalid(
                self.err_span(call_span),
                "expected an `And` certificate for multiple callee preconditions",
            );
        };
        if subs.len() != requires.len() {
            return invalid(
                self.err_span(call_span),
                format!(
                    "expected {} precondition sub-certificates, found {}",
                    requires.len(),
                    subs.len()
                ),
            );
        }
        for ((req_term, req_span), sub_cert) in requires.iter().zip(subs.iter()) {
            self.eval_term(
                req_term,
                callee_env,
                EvalMode::Spec,
                state,
                sub_cert,
                &mut |verifier, val, state, cert| {
                    let formula = to_bool_formula(&val, state, *req_span)?;
                    verifier.verify_formula(&formula, true, call_span, state, cert)
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn verify_ensures(
        &mut self,
        ensures: &[(Term, Span)],
        env: &SymEnv,
        result_val: &SymVal,
        state: &mut PathState,
        cert: &PathCert,
    ) -> Result<(), ContractFailure> {
        if ensures.is_empty() {
            return if matches!(cert, PathCert::Leaf(LeafCert::Trivial)) {
                Ok(())
            } else {
                invalid(
                    self.spec.contract_span,
                    "expected `Leaf(Trivial)` certificate for empty postcondition list",
                )
            };
        }
        if ensures.len() == 1 {
            let (ens_term, ens_span) = &ensures[0];
            return self.eval_term(
                ens_term,
                env,
                EvalMode::Spec,
                state,
                cert,
                &mut |verifier, pred_val, state, cert| {
                    verifier.apply_val(
                        pred_val,
                        result_val.clone(),
                        EvalMode::Spec,
                        *ens_span,
                        state,
                        cert,
                        &mut |verifier, bool_val, state, cert| {
                            let formula = to_bool_formula(&bool_val, state, *ens_span)?;
                            verifier.verify_formula(&formula, true, *ens_span, state, cert)
                        },
                    )
                },
            );
        }
        let PathCert::And(subs) = cert else {
            return invalid(
                self.spec.contract_span,
                "expected an `And` certificate for multiple postconditions",
            );
        };
        if subs.len() != ensures.len() {
            return invalid(
                self.spec.contract_span,
                format!(
                    "expected {} postcondition sub-certificates, found {}",
                    ensures.len(),
                    subs.len()
                ),
            );
        }
        for ((ens_term, ens_span), sub_cert) in ensures.iter().zip(subs.iter()) {
            self.eval_term(
                ens_term,
                env,
                EvalMode::Spec,
                state,
                sub_cert,
                &mut |verifier, pred_val, state, cert| {
                    verifier.apply_val(
                        pred_val,
                        result_val.clone(),
                        EvalMode::Spec,
                        *ens_span,
                        state,
                        cert,
                        &mut |verifier, bool_val, state, cert| {
                            let formula = to_bool_formula(&bool_val, state, *ens_span)?;
                            verifier.verify_formula(&formula, true, *ens_span, state, cert)
                        },
                    )
                },
            )?;
        }
        Ok(())
    }

    fn verify_formula(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
    ) -> Result<(), ContractFailure> {
        self.enter(span)?;
        let res = self.verify_formula_inner(formula, expected, span, state, cert);
        self.leave();
        res
    }

    fn verify_formula_inner(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
    ) -> Result<(), ContractFailure> {
        match (formula, expected) {
            (BoolFormula::True, true) | (BoolFormula::False, false) => {
                if matches!(cert, PathCert::Leaf(LeafCert::Trivial)) {
                    Ok(())
                } else {
                    invalid(
                        self.err_span(span),
                        "expected `Leaf(Trivial)` certificate for tautology",
                    )
                }
            }
            (BoolFormula::True, false) | (BoolFormula::False, true) => {
                self.verify_state_unsat(span, state, cert)
            }
            (BoolFormula::Not(inner), exp) => self.verify_formula(inner, !exp, span, state, cert),
            (BoolFormula::Atom(id), exp) => {
                if state.bool_fact(*id) == Some(exp) {
                    if matches!(cert, PathCert::Leaf(LeafCert::Trivial)) {
                        Ok(())
                    } else {
                        invalid(
                            self.err_span(span),
                            "expected `Leaf(Trivial)` certificate for known boolean fact",
                        )
                    }
                } else {
                    self.verify_state_unsat(span, state, cert)
                }
            }
            (BoolFormula::LinGe(f), true) => self.verify_lin_ge(f, span, state, cert),
            (BoolFormula::LinGe(f), false) => {
                let nf = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
                    span: self.err_span(span),
                })?;
                self.verify_lin_ge(&nf, span, state, cert)
            }
            (BoolFormula::LinEq(f), exp) => self.verify_lin_eq(f, exp, span, state, cert),
            (BoolFormula::And(a, b), true) => {
                let PathCert::And(subs) = cert else {
                    return invalid(
                        self.err_span(span),
                        "expected an `And` certificate for conjunction obligation",
                    );
                };
                if subs.len() != 2 {
                    return invalid(
                        self.err_span(span),
                        format!(
                            "expected 2 sub-certificates for conjunction, found {}",
                            subs.len()
                        ),
                    );
                }
                self.verify_formula(a, true, span, state, &subs[0])?;
                self.verify_formula(b, true, span, state, &subs[1])
            }
            (BoolFormula::Or(a, b), false) => {
                let PathCert::And(subs) = cert else {
                    return invalid(
                        self.err_span(span),
                        "expected an `And` certificate for negated disjunction obligation",
                    );
                };
                if subs.len() != 2 {
                    return invalid(
                        self.err_span(span),
                        format!(
                            "expected 2 sub-certificates for negated disjunction, found {}",
                            subs.len()
                        ),
                    );
                }
                self.verify_formula(a, false, span, state, &subs[0])?;
                self.verify_formula(b, false, span, state, &subs[1])
            }
            (BoolFormula::Or(a, b), true) => {
                self.assume_formula(a, false, span, state, cert, &mut |verifier, state, cert| {
                    verifier.verify_formula(b, true, span, state, cert)
                })
            }
            (BoolFormula::And(a, b), false) => {
                self.assume_formula(a, true, span, state, cert, &mut |verifier, state, cert| {
                    verifier.verify_formula(b, false, span, state, cert)
                })
            }
        }
    }

    fn verify_lin_eq(
        &mut self,
        f: &Linear,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
    ) -> Result<(), ContractFailure> {
        if f.coeffs.is_empty() {
            return if (f.constant == 0) == expected {
                if matches!(cert, PathCert::Leaf(LeafCert::Trivial)) {
                    Ok(())
                } else {
                    invalid(
                        self.err_span(span),
                        "expected `Leaf(Trivial)` certificate for constant equality",
                    )
                }
            } else {
                self.verify_state_unsat(span, state, cert)
            };
        }
        let nf = f.neg().ok_or(ContractFailure::Exhausted {
            span: self.err_span(span),
        })?;
        if expected {
            let PathCert::And(subs) = cert else {
                return invalid(
                    self.err_span(span),
                    "expected an `And` certificate of two bounds for equality",
                );
            };
            if subs.len() != 2 {
                return invalid(
                    self.err_span(span),
                    format!(
                        "expected 2 sub-certificates for equality, found {}",
                        subs.len()
                    ),
                );
            }
            self.verify_lin_ge(f, span, state, &subs[0])?;
            return self.verify_lin_ge(&nf, span, state, &subs[1]);
        }
        let PathCert::Leaf(leaf) = cert else {
            return invalid(
                self.err_span(span),
                "expected a `Leaf` certificate for `!=` obligation",
            );
        };
        let mark = state.mark();
        state.facts.push(f.clone());
        state.facts.push(nf);
        let ok = check_dead_leaf(state, leaf);
        state.restore(mark);
        if ok {
            Ok(())
        } else {
            invalid(self.err_span(span), "invalid `!=` refutation certificate")
        }
    }

    fn verify_lin_ge(
        &mut self,
        f: &Linear,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
    ) -> Result<(), ContractFailure> {
        let PathCert::Leaf(leaf) = cert else {
            return invalid(
                self.err_span(span),
                "expected a `Leaf` certificate for linear inequality obligation",
            );
        };
        let neg = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
            span: self.err_span(span),
        })?;
        if check_negated_linear_leaf(state, &neg, leaf) {
            Ok(())
        } else {
            invalid(
                self.err_span(span),
                "invalid linear inequality refutation certificate",
            )
        }
    }

    fn verify_state_unsat(
        &mut self,
        span: Span,
        state: &PathState,
        cert: &PathCert,
    ) -> Result<(), ContractFailure> {
        let PathCert::Leaf(leaf) = cert else {
            return invalid(
                self.err_span(span),
                "expected a `Leaf` certificate refuting an unsatisfiable path",
            );
        };
        if check_dead_leaf(state, leaf) {
            Ok(())
        } else {
            invalid(
                self.err_span(span),
                "invalid refutation certificate for unsatisfied obligation",
            )
        }
    }
}
