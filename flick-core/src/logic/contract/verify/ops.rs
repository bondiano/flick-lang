use std::rc::Rc;

use crate::logic::omega::Linear;
use crate::source::Span;

use super::super::ContractFailure;
use super::super::cert::{PathCert, RangeCert};
use super::super::state::{
    EvalMode, INT_MAX, INT_MIN, PathState, SymEnv, SymVal, combine_bool_eq, eval_int_cmp,
    expect_int, lin_add, lin_const, lin_sub, negate_bool_val, range_negations, refuse,
};
use super::super::term::{IntBinOp, IntCmpOp, Term, TermKind};
use super::{Verifier, VerifyEvalCont, check_negated_linear_leaf, invalid};

impl<'a> Verifier<'a> {
    pub(super) fn eval_term(
        &mut self,
        term: &Term,
        env: &SymEnv,
        mode: EvalMode,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.enter(term.span)?;
        let res = self.eval_term_inner(term, env, mode, state, cert, cont);
        self.leave();
        res
    }

    fn eval_term_inner(
        &mut self,
        term: &Term,
        env: &SymEnv,
        mode: EvalMode,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let span = term.span;
        match &term.kind {
            TermKind::Var(name) => {
                let Some(val) = env.get(name).cloned() else {
                    return refuse(
                        self.diag_span(span),
                        format!("unbound variable `{name}` is outside the domain"),
                    );
                };
                cont(self, val, state, cert)
            }
            TermKind::IntLit(n) => {
                if mode == EvalMode::Body && (*n < INT_MIN || *n > INT_MAX) {
                    return refuse(
                        self.diag_span(span),
                        "an `Int` literal outside 32-bit range is outside the domain",
                    );
                }
                cont(self, SymVal::Int(lin_const(*n)), state, cert)
            }
            TermKind::BoolLit(b) => cont(self, SymVal::Bool(*b), state, cert),
            TermKind::UnitLit => cont(self, SymVal::Unit, state, cert),
            TermKind::Con(ctor) => cont(self, SymVal::Con(ctor.clone(), Vec::new()), state, cert),
            TermKind::Lam(param, body) => cont(
                self,
                SymVal::Closure {
                    env: env.clone(),
                    param: param.clone(),
                    body: body.clone(),
                    from_helper: mode == EvalMode::SpecHelper,
                    foreign_caller: self.in_foreign.then_some(self.caller_span),
                },
                state,
                cert,
            ),
            TermKind::IntBin(op, lhs, rhs) => {
                self.verify_int_bin(*op, lhs, rhs, env, mode, span, state, cert, cont)
            }
            TermKind::IntNeg(inner) => {
                self.verify_int_neg(inner, env, mode, span, state, cert, cont)
            }
            TermKind::IntCmp(op, lhs, rhs) => {
                self.verify_int_cmp(*op, lhs, rhs, env, mode, span, state, cert, cont)
            }
            TermKind::BoolEq { eq, lhs, rhs } => {
                self.verify_bool_eq(*eq, lhs, rhs, env, mode, span, state, cert, cont)
            }
            TermKind::TagEq { eq, lhs, rhs } => {
                self.verify_tag_eq_term(*eq, lhs, rhs, env, mode, span, state, cert, cont)
            }
            TermKind::Not(inner) => self.eval_term(
                inner,
                env,
                mode,
                state,
                cert,
                &mut |verifier, val, state, cert| {
                    let out = negate_bool_val(&val, state, span)?;
                    cont(verifier, out, state, cert)
                },
            ),
            TermKind::And(lhs, rhs) => {
                let false_term = Term::new(TermKind::BoolLit(false), span);
                self.verify_if((lhs, rhs, &false_term), env, mode, span, state, cert, cont)
            }
            TermKind::Or(lhs, rhs) => {
                let true_term = Term::new(TermKind::BoolLit(true), span);
                self.verify_if((lhs, &true_term, rhs), env, mode, span, state, cert, cont)
            }
            TermKind::If(guard, then_t, else_t) => {
                self.verify_if((guard, then_t, else_t), env, mode, span, state, cert, cont)
            }
            TermKind::Let(name, value, body) => {
                self.verify_let(name, value, body, env, mode, state, cert, cont)
            }
            TermKind::Match(scrut, arms) => self.eval_term(
                scrut,
                env,
                mode,
                state,
                cert,
                &mut |verifier, val, state, cert| {
                    verifier.verify_match_arms(&val, arms, 0, env, mode, span, state, cert, cont)
                },
            ),
            TermKind::App {
                func,
                arg,
                arg_first,
            } => self.verify_app(func, arg, *arg_first, env, mode, span, state, cert, cont),
            TermKind::Def(def) => self.eval_def(def, mode, span, state, cert, cont),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_tag_eq_term(
        &mut self,
        eq: bool,
        lhs: &Term,
        rhs: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.eval_term(
            lhs,
            env,
            mode,
            state,
            cert,
            &mut |verifier, lval, state, cert| {
                verifier.eval_term(
                    rhs,
                    env,
                    mode,
                    state,
                    cert,
                    &mut |verifier, rval, state, cert| {
                        verifier.verify_tag_eq(eq, lval.clone(), rval, span, state, cert, cont)
                    },
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_let(
        &mut self,
        name: &Rc<str>,
        value: &Term,
        body: &Term,
        env: &SymEnv,
        mode: EvalMode,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let name = name.clone();
        self.eval_term(
            value,
            env,
            mode,
            state,
            cert,
            &mut |verifier, val, state, cert| {
                let next_env = env.with(name.clone(), val);
                verifier.eval_term(body, &next_env, mode, state, cert, cont)
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_int_neg(
        &mut self,
        inner: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.eval_term(
            inner,
            env,
            mode,
            state,
            cert,
            &mut |verifier, val, state, cert| {
                let a = expect_int(&val, state, span)?;
                let res = a.neg().ok_or(ContractFailure::Exhausted {
                    span: verifier.err_span(span),
                })?;
                verifier.verify_int_op(res, mode, span, state, cert, cont)
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_int_bin(
        &mut self,
        op: IntBinOp,
        lhs: &Term,
        rhs: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.eval_term(
            lhs,
            env,
            mode,
            state,
            cert,
            &mut |verifier, lval, state, cert| {
                verifier.eval_term(
                    rhs,
                    env,
                    mode,
                    state,
                    cert,
                    &mut |verifier, rval, state, cert| {
                        let a = expect_int(&lval, state, span)?;
                        let b = expect_int(&rval, state, span)?;
                        let res = match op {
                            IntBinOp::Add => lin_add(&a, &b).ok_or(ContractFailure::Exhausted {
                                span: verifier.err_span(span),
                            })?,
                            IntBinOp::Sub => lin_sub(&a, &b).ok_or(ContractFailure::Exhausted {
                                span: verifier.err_span(span),
                            })?,
                        };
                        verifier.verify_int_op(res, mode, span, state, cert, cont)
                    },
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_int_cmp(
        &mut self,
        op: IntCmpOp,
        lhs: &Term,
        rhs: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.eval_term(
            lhs,
            env,
            mode,
            state,
            cert,
            &mut |verifier, lval, state, cert| {
                verifier.eval_term(
                    rhs,
                    env,
                    mode,
                    state,
                    cert,
                    &mut |verifier, rval, state, cert| {
                        let a = expect_int(&lval, state, span)?;
                        let b = expect_int(&rval, state, span)?;
                        let out = eval_int_cmp(op, &a, &b, span)?;
                        cont(verifier, out, state, cert)
                    },
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_bool_eq(
        &mut self,
        eq: bool,
        lhs: &Term,
        rhs: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.eval_term(
            lhs,
            env,
            mode,
            state,
            cert,
            &mut |verifier, lval, state, cert| {
                verifier.eval_term(
                    rhs,
                    env,
                    mode,
                    state,
                    cert,
                    &mut |verifier, rval, state, cert| {
                        let out = combine_bool_eq(eq, &lval, &rval, state, span)?;
                        cont(verifier, out, state, cert)
                    },
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_app(
        &mut self,
        func: &Term,
        arg: &Term,
        arg_first: bool,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        if arg_first {
            self.eval_term(
                arg,
                env,
                mode,
                state,
                cert,
                &mut |verifier, aval, state, cert| {
                    verifier.eval_term(
                        func,
                        env,
                        mode,
                        state,
                        cert,
                        &mut |verifier, fval, state, cert| {
                            verifier.apply_val(fval, aval.clone(), mode, span, state, cert, cont)
                        },
                    )
                },
            )
        } else {
            self.eval_term(
                func,
                env,
                mode,
                state,
                cert,
                &mut |verifier, fval, state, cert| {
                    verifier.eval_term(
                        arg,
                        env,
                        mode,
                        state,
                        cert,
                        &mut |verifier, aval, state, cert| {
                            verifier.apply_val(fval.clone(), aval, mode, span, state, cert, cont)
                        },
                    )
                },
            )
        }
    }

    fn verify_int_op(
        &mut self,
        res: Linear,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        match mode {
            EvalMode::Spec => return cont(self, SymVal::Int(res), state, cert),
            EvalMode::SpecHelper => {
                return refuse(
                    self.diag_span(span),
                    "arithmetic inside an uncontracted helper used in a contract specification is outside the domain",
                );
            }
            EvalMode::Body => {}
        }
        let PathCert::Range {
            cert: RangeCert { lower, upper },
            next,
        } = cert
        else {
            return invalid(
                self.err_span(span),
                "expected a `Range` certificate for `Int` arithmetic",
            );
        };
        let (neg_lo, neg_hi) = range_negations(&res).ok_or(ContractFailure::Exhausted {
            span: self.err_span(span),
        })?;
        if !check_negated_linear_leaf(state, &neg_lo, lower) {
            return invalid(self.err_span(span), "invalid lower `Int` range certificate");
        }
        if !check_negated_linear_leaf(state, &neg_hi, upper) {
            return invalid(self.err_span(span), "invalid upper `Int` range certificate");
        }
        cont(self, SymVal::Int(res), state, next)
    }
}
