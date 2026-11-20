use std::rc::Rc;

use crate::logic::Def;
use crate::source::Span;

use super::super::cert::PathCert;
use super::super::state::{
    EvalMode, PathState, SymEnv, SymVal, contains_unsat_contracted, refuse, to_bool_formula,
};
use super::super::term::{Term, TermKind};
use super::super::{ContractFailure, ContractSummary};
use super::{Verifier, VerifyAssumeCont, VerifyEvalCont, invalid};

impl<'a> Verifier<'a> {
    pub(super) fn eval_def(
        &mut self,
        def: &Rc<Def>,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        if let Def::Value { module, name } = def.as_ref() {
            if let Some(summary) = self.verified.get(module, name) {
                if summary.params.is_empty() {
                    return self.verify_contracted_call(
                        summary,
                        Vec::new(),
                        span,
                        state,
                        cert,
                        cont,
                    );
                }
                return cont(
                    self,
                    SymVal::Contracted {
                        module: module.clone(),
                        callee: name.clone(),
                        args: Vec::new(),
                    },
                    state,
                    cert,
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
            cert,
            &mut |verifier, val, state, cert| {
                let inner_foreign = verifier.in_foreign;
                let inner_caller = verifier.caller_span;
                verifier.in_foreign = saved_foreign;
                verifier.caller_span = saved_caller;
                let out = cont(verifier, val, state, cert);
                verifier.in_foreign = inner_foreign;
                verifier.caller_span = inner_caller;
                out
            },
        );
        self.in_foreign = saved_foreign;
        self.caller_span = saved_caller;
        res
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_val(
        &mut self,
        func: SymVal,
        arg: SymVal,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
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
                        cert,
                        &mut |verifier, val, state, cert| {
                            let inner_foreign = verifier.in_foreign;
                            let inner_caller = verifier.caller_span;
                            verifier.in_foreign = saved_foreign;
                            verifier.caller_span = saved_caller;
                            let r = cont(verifier, val, state, cert);
                            verifier.in_foreign = inner_foreign;
                            verifier.caller_span = inner_caller;
                            r
                        },
                    );
                    self.in_foreign = saved_foreign;
                    self.caller_span = saved_caller;
                    out
                } else {
                    self.eval_term(&body, &next_env, body_mode, state, cert, cont)
                }
            }
            SymVal::Con(ctor, mut fields) if fields.len() < ctor.arity => {
                fields.push(arg);
                cont(self, SymVal::Con(ctor, fields), state, cert)
            }
            SymVal::Contracted {
                module,
                callee,
                mut args,
            } => {
                let Some(summary) = self.verified.get(&module, &callee) else {
                    self.leave();
                    return invalid(
                        self.err_span(span),
                        format!("callee `{callee}` is not in the verified contract set"),
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
                        cert,
                    )
                } else {
                    self.verify_contracted_call(summary, args, span, state, cert, cont)
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

    fn verify_contracted_call(
        &mut self,
        summary: &'a ContractSummary,
        args: Vec<SymVal>,
        call_span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let diag_span = self.diag_span(call_span);
        for arg in &args {
            if contains_unsat_contracted(arg, state) {
                return refuse(
                    diag_span,
                    "a function value or partially applied contracted function cannot be passed to another contract",
                );
            }
        }
        let PathCert::Call { callee, pre, post } = cert else {
            return invalid(
                self.err_span(call_span),
                format!("expected a `Call` certificate for `{}`", summary.name),
            );
        };
        if callee != &summary.name {
            return invalid(
                self.err_span(call_span),
                format!(
                    "call certificate names `{callee}`, expected `{}`",
                    summary.name
                ),
            );
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
        if let Err(err) =
            self.verify_requires(&summary.requires, &callee_env, diag_span, state, pre)
        {
            self.in_foreign = saved_foreign;
            self.caller_span = saved_caller;
            return Err(err);
        }

        let mark = state.mark();
        let ret_val = match state.fresh_val_for_ty(&summary.result_ty, self.lang, diag_span) {
            Ok(v) => v,
            Err(err) => {
                self.in_foreign = saved_foreign;
                self.caller_span = saved_caller;
                return Err(err);
            }
        };
        let res = self.assume_ensures(
            &summary.ensures,
            &callee_env,
            &ret_val,
            state,
            post,
            &mut |verifier, state, cert| {
                let inner_foreign = verifier.in_foreign;
                let inner_caller = verifier.caller_span;
                verifier.in_foreign = saved_foreign;
                verifier.caller_span = saved_caller;
                let out = cont(verifier, ret_val.clone(), state, cert);
                verifier.in_foreign = inner_foreign;
                verifier.caller_span = inner_caller;
                out
            },
        );
        state.restore(mark);
        self.in_foreign = saved_foreign;
        self.caller_span = saved_caller;
        res
    }

    fn assume_ensures(
        &mut self,
        ensures: &[(Term, Span)],
        callee_env: &SymEnv,
        ret_val: &SymVal,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let Some(((ens_term, ens_span), rest)) = ensures.split_first() else {
            return cont(self, state, cert);
        };
        if let TermKind::Lam(param, body) = &ens_term.kind {
            let next_env = callee_env.with(param.clone(), ret_val.clone());
            return self.assume_term(
                body,
                &next_env,
                state,
                cert,
                &mut |verifier, state, cert| {
                    verifier.assume_ensures(rest, callee_env, ret_val, state, cert, cont)
                },
            );
        }
        let ens_span = *ens_span;
        self.eval_term(
            ens_term,
            callee_env,
            EvalMode::Spec,
            state,
            cert,
            &mut |verifier, pred_val, state, cert| {
                verifier.apply_val(
                    pred_val,
                    ret_val.clone(),
                    EvalMode::Spec,
                    ens_span,
                    state,
                    cert,
                    &mut |verifier, bool_val, state, cert| {
                        let formula = to_bool_formula(&bool_val, state, ens_span)?;
                        verifier.assume_formula(
                            &formula,
                            true,
                            ens_span,
                            state,
                            cert,
                            &mut |verifier, state, cert| {
                                verifier
                                    .assume_ensures(rest, callee_env, ret_val, state, cert, cont)
                            },
                        )
                    },
                )
            },
        )
    }
}
