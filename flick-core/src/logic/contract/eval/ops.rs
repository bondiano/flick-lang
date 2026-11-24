//! Symbolic integer operations, range obligations, definitions, applications,
//! and contracted call summaries.

use std::rc::Rc;

use crate::logic::Def;
use crate::logic::omega::{self, Outcome};
use crate::source::Span;

use super::super::cert::{LeafCert, PathCert, RangeCert};
use super::super::state::{
    EvalMode, PathState, SymEnv, SymVal, contains_unsat_contracted, eval_int_cmp, expect_int,
    is_trivially_unsat_linear, lin_add, lin_sub, local_span, range_negations, refuse,
    to_bool_formula,
};
use super::super::term::{IntBinOp, IntCmpOp, Term, TermKind};
use super::super::{ArithOp, ContractFailure, ContractSummary};
use super::counterexample::format_counterexample;
use super::{AssumeCont, EvalCont, Solver};

impl<'a> Solver<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn eval_int_neg(
        &mut self,
        inner: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.eval_term(inner, env, mode, state, &mut |solver, val, state| {
            let a = expect_int(&val, state, span)?;
            let res = a.neg().ok_or(ContractFailure::Exhausted {
                span: solver.err_span(span),
            })?;
            solver.finish_int_op(res, ArithOp::Neg, mode, span, state, cont)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn eval_int_cmp_term(
        &mut self,
        op: IntCmpOp,
        lhs: &Term,
        rhs: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.eval_term(lhs, env, mode, state, &mut |solver, lval, state| {
            solver.eval_term(rhs, env, mode, state, &mut |solver, rval, state| {
                let a = expect_int(&lval, state, span)?;
                let b = expect_int(&rval, state, span)?;
                let out = eval_int_cmp(op, &a, &b, span)?;
                cont(solver, out, state)
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn eval_int_bin(
        &mut self,
        op: IntBinOp,
        lhs: &Term,
        rhs: &Term,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.eval_term(lhs, env, mode, state, &mut |solver, lval, state| {
            solver.eval_term(rhs, env, mode, state, &mut |solver, rval, state| {
                let a = expect_int(&lval, state, span)?;
                let b = expect_int(&rval, state, span)?;
                let (arith_op, res) = match op {
                    IntBinOp::Add => (
                        ArithOp::Add,
                        lin_add(&a, &b).ok_or(ContractFailure::Exhausted {
                            span: local_span(span, solver.spec.contract_span),
                        })?,
                    ),
                    IntBinOp::Sub => (
                        ArithOp::Sub,
                        lin_sub(&a, &b).ok_or(ContractFailure::Exhausted {
                            span: local_span(span, solver.spec.contract_span),
                        })?,
                    ),
                };
                solver.finish_int_op(res, arith_op, mode, span, state, cont)
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn eval_app(
        &mut self,
        func: &Term,
        arg: &Term,
        arg_first: bool,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        if arg_first {
            self.eval_term(arg, env, mode, state, &mut |solver, aval, state| {
                solver.eval_term(func, env, mode, state, &mut |solver, fval, state| {
                    solver.apply_val(fval, aval.clone(), mode, span, state, cont)
                })
            })
        } else {
            self.eval_term(func, env, mode, state, &mut |solver, fval, state| {
                solver.eval_term(arg, env, mode, state, &mut |solver, aval, state| {
                    solver.apply_val(fval.clone(), aval, mode, span, state, cont)
                })
            })
        }
    }

    fn finish_int_op(
        &mut self,
        res: omega::Linear,
        op: ArithOp,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        match mode {
            EvalMode::Spec => return cont(self, SymVal::Int(res), state),
            EvalMode::SpecHelper => {
                return refuse(
                    self.diag_span(span),
                    "arithmetic inside an uncontracted helper used in a contract specification is outside the domain",
                );
            }
            EvalMode::Body => {}
        }
        let diag_span = self.diag_span(span);
        let (neg_lo, neg_hi) = range_negations(&res).ok_or(ContractFailure::Exhausted {
            span: self.err_span(span),
        })?;
        let lower = self.discharge_range_bound(&neg_lo, op, diag_span, state)?;
        let upper = self.discharge_range_bound(&neg_hi, op, diag_span, state)?;
        let next = cont(self, SymVal::Int(res), state)?;
        Ok(PathCert::Range {
            cert: RangeCert { lower, upper },
            next: Box::new(next),
        })
    }

    fn discharge_range_bound(
        &mut self,
        neg_bound: &omega::Linear,
        op: ArithOp,
        span: Span,
        state: &mut PathState,
    ) -> Result<LeafCert, ContractFailure> {
        if is_trivially_unsat_linear(neg_bound) {
            return Ok(LeafCert::Trivial);
        }
        state.facts.push(neg_bound.clone());
        let outcome = self.decide(state);
        state.facts.pop();
        match outcome {
            Outcome::Refuted(proof) => Ok(LeafCert::Omega(proof)),
            Outcome::Model(model) => Err(ContractFailure::Overflow {
                span,
                op,
                counterexample: Some(format_counterexample(state, &model, None)),
            }),
            Outcome::Unknown => Err(ContractFailure::Exhausted {
                span: self.err_span(span),
            }),
        }
    }

    pub(super) fn eval_def(
        &mut self,
        def: &Rc<Def>,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        if let Def::Value { module, name } = def.as_ref() {
            if let Some(summary) = self.verified.get(module, name) {
                if summary.params.is_empty() {
                    return self.eval_contracted_call(summary, Vec::new(), span, state, cont);
                }
                return cont(
                    self,
                    SymVal::Contracted {
                        module: module.clone(),
                        callee: name.clone(),
                        args: Vec::new(),
                    },
                    state,
                );
            }
            if self.verified.is_unverified(module, name)
                || (module == &self.spec.module && name == &self.spec.name)
            {
                return refuse(
                    self.diag_span(span),
                    format!("`{name}`'s contract did not check, so it cannot be assumed"),
                );
            }
        }
        let Some(def_body) = self.defs.body(def) else {
            return refuse(
                self.diag_span(span),
                format!("the definition of `{}` is not admitted", def.shown()),
            );
        };
        let is_local = self.defs.is_local(def);
        let next_mode = match (mode, def.as_ref()) {
            (EvalMode::Body, _) => EvalMode::Body,
            (EvalMode::Spec | EvalMode::SpecHelper, Def::Value { .. }) => EvalMode::SpecHelper,
            (m, Def::Method { .. }) => m,
        };
        let saved_foreign = self.in_foreign;
        let saved_caller = self.caller_span;
        if !is_local {
            if !self.in_foreign {
                self.caller_span = span;
            }
            self.in_foreign = true;
        }
        let empty = SymEnv::default();
        let res = self.eval_term(
            def_body,
            &empty,
            next_mode,
            state,
            &mut |solver, val, state| {
                let inner_foreign = solver.in_foreign;
                let inner_caller = solver.caller_span;
                solver.in_foreign = saved_foreign;
                solver.caller_span = saved_caller;
                let out = cont(solver, val, state);
                solver.in_foreign = inner_foreign;
                solver.caller_span = inner_caller;
                out
            },
        );
        self.in_foreign = saved_foreign;
        self.caller_span = saved_caller;
        res
    }

    pub(super) fn apply_val(
        &mut self,
        func: SymVal,
        arg: SymVal,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.enter(span)?;
        let res = match state.whnf(func) {
            SymVal::Closure {
                env: cenv,
                param,
                body,
                from_helper,
                foreign_caller,
            } => {
                let next_env = cenv.with(param, arg);
                let body_mode = if from_helper && mode == EvalMode::Spec {
                    EvalMode::SpecHelper
                } else {
                    mode
                };
                if let Some(caller_span) = foreign_caller {
                    let saved_foreign = self.in_foreign;
                    let saved_caller = self.caller_span;
                    self.in_foreign = true;
                    self.caller_span = caller_span;
                    let out = self.eval_term(
                        &body,
                        &next_env,
                        body_mode,
                        state,
                        &mut |solver, val, state| {
                            let inner_foreign = solver.in_foreign;
                            let inner_caller = solver.caller_span;
                            solver.in_foreign = saved_foreign;
                            solver.caller_span = saved_caller;
                            let r = cont(solver, val, state);
                            solver.in_foreign = inner_foreign;
                            solver.caller_span = inner_caller;
                            r
                        },
                    );
                    self.in_foreign = saved_foreign;
                    self.caller_span = saved_caller;
                    out
                } else {
                    self.eval_term(&body, &next_env, body_mode, state, cont)
                }
            }
            SymVal::Con(ctor, mut fields) if fields.len() < ctor.arity => {
                fields.push(arg);
                cont(self, SymVal::Con(ctor, fields), state)
            }
            SymVal::Contracted {
                module,
                callee,
                mut args,
            } => {
                let Some(summary) = self.verified.get(&module, &callee) else {
                    self.leave();
                    return refuse(
                        self.diag_span(span),
                        format!("`{callee}`'s contract did not check, so it cannot be assumed"),
                    );
                };
                args.push(arg);
                if args.len() < summary.params.len() {
                    cont(
                        self,
                        SymVal::Contracted {
                            module,
                            callee,
                            args,
                        },
                        state,
                    )
                } else {
                    self.eval_contracted_call(summary, args, span, state, cont)
                }
            }
            _ => {
                self.leave();
                return refuse(
                    self.diag_span(span),
                    "calling an unknown or higher-order function is outside the domain",
                );
            }
        };
        self.leave();
        res
    }

    fn eval_contracted_call(
        &mut self,
        summary: &'a ContractSummary,
        args: Vec<SymVal>,
        call_span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let diag_span = self.diag_span(call_span);
        for arg in &args {
            if contains_unsat_contracted(arg, state) {
                return refuse(
                    diag_span,
                    "a function value or partially applied contracted function cannot be passed to another contract",
                );
            }
        }
        let mut callee_env = SymEnv::default();
        for ((param_name, _), arg_val) in summary.params.iter().zip(args) {
            callee_env = callee_env.with(param_name.clone(), arg_val);
        }
        let saved_foreign = self.in_foreign;
        let saved_caller = self.caller_span;
        if summary.module != self.spec.module {
            if !self.in_foreign {
                self.caller_span = diag_span;
            }
            self.in_foreign = true;
        }
        let pre_res = self.prove_requires(
            &summary.requires,
            &callee_env,
            diag_span,
            &summary.name,
            state,
        );
        let pre = match pre_res {
            Ok(pre) => pre,
            Err(err) => {
                self.in_foreign = saved_foreign;
                self.caller_span = saved_caller;
                return Err(err);
            }
        };

        let mark = state.mark();
        let ret_val = match state.fresh_val_for_ty(&summary.result_ty, self.lang, diag_span) {
            Ok(v) => v,
            Err(err) => {
                self.in_foreign = saved_foreign;
                self.caller_span = saved_caller;
                return Err(err);
            }
        };
        if let SymVal::Int(ref lin) = ret_val
            && let [(v, 1)] = lin.coeffs.as_slice()
        {
            state
                .call_results
                .push((format!("{}(...)", summary.name), *v));
        }
        let post = self.assume_ensures(
            &summary.ensures,
            &callee_env,
            &ret_val,
            state,
            &mut |solver, state| {
                let inner_foreign = solver.in_foreign;
                let inner_caller = solver.caller_span;
                solver.in_foreign = saved_foreign;
                solver.caller_span = saved_caller;
                let out = cont(solver, ret_val.clone(), state);
                solver.in_foreign = inner_foreign;
                solver.caller_span = inner_caller;
                out
            },
        );
        state.restore(mark);
        self.in_foreign = saved_foreign;
        self.caller_span = saved_caller;
        let post = post?;
        Ok(PathCert::Call {
            callee: summary.name.clone(),
            pre: Box::new(pre),
            post: Box::new(post),
        })
    }

    fn assume_ensures(
        &mut self,
        ensures: &[(Term, Span)],
        callee_env: &SymEnv,
        ret_val: &SymVal,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let Some(((ens_term, ens_span), rest)) = ensures.split_first() else {
            return cont(self, state);
        };
        if let TermKind::Lam(param, body) = &ens_term.kind {
            let next_env = callee_env.with(param.clone(), ret_val.clone());
            return self.assume_term(body, &next_env, state, &mut |solver, state| {
                solver.assume_ensures(rest, callee_env, ret_val, state, cont)
            });
        }
        let ens_span = *ens_span;
        self.eval_term(
            ens_term,
            callee_env,
            EvalMode::Spec,
            state,
            &mut |solver, pred_val, state| {
                solver.apply_val(
                    pred_val,
                    ret_val.clone(),
                    EvalMode::Spec,
                    ens_span,
                    state,
                    &mut |solver, bool_val, state| {
                        let formula = to_bool_formula(&bool_val, state, ens_span)?;
                        solver.assume_formula(
                            &formula,
                            true,
                            ens_span,
                            state,
                            &mut |solver, state| {
                                solver.assume_ensures(rest, callee_env, ret_val, state, cont)
                            },
                        )
                    },
                )
            },
        )
    }
}
