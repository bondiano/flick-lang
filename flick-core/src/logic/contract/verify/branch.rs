use std::rc::Rc;

use crate::logic::term::tuple;
use crate::logic::{Ctor, Ty};
use crate::source::Span;

use super::super::ContractFailure;
use super::super::cert::PathCert;
use super::super::state::{
    BoolFormula, EvalMode, PatMatch, PathState, SplitTarget, SymEnv, SymId, SymVal, lookup_data,
    match_pat, refuse, to_bool_formula,
};
use super::super::term::{Arm, Term};
use super::{Verifier, VerifyAssumeCont, VerifyEvalCont, check_dead_leaf, invalid};

impl<'a> Verifier<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn verify_if(
        &mut self,
        (guard, then_t, else_t): (&Term, &Term, &Term),
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.eval_term(
            guard,
            env,
            mode,
            state,
            cert,
            &mut |verifier, cval, state, cert| {
                let formula = to_bool_formula(&cval, state, guard.span)?;
                match formula {
                    BoolFormula::True => verifier.eval_term(then_t, env, mode, state, cert, cont),
                    BoolFormula::False => verifier.eval_term(else_t, env, mode, state, cert, cont),
                    other => {
                        let PathCert::Branch {
                            then_branch,
                            else_branch,
                        } = cert
                        else {
                            return invalid(
                                verifier.err_span(span),
                                "expected a `Branch` certificate for conditional split",
                            );
                        };
                        verifier.assume_formula(
                            &other,
                            true,
                            guard.span,
                            state,
                            then_branch,
                            &mut |verifier, state, cert| {
                                verifier.eval_term(then_t, env, mode, state, cert, cont)
                            },
                        )?;
                        verifier.assume_formula(
                            &other,
                            false,
                            guard.span,
                            state,
                            else_branch,
                            &mut |verifier, state, cert| {
                                verifier.eval_term(else_t, env, mode, state, cert, cont)
                            },
                        )
                    }
                }
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn verify_tag_eq(
        &mut self,
        eq: bool,
        lhs: SymVal,
        rhs: SymVal,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.enter(span)?;
        let res = self.verify_tag_eq_inner(eq, lhs, rhs, span, state, cert, cont);
        self.leave();
        res
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_tag_eq_inner(
        &mut self,
        eq: bool,
        lhs: SymVal,
        rhs: SymVal,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let l = state.whnf(lhs);
        let r = state.whnf(rhs);
        if let SymVal::Sym(sym_id, Ty::Data { module, name, args }) = &l {
            return self.verify_split_data(
                *sym_id,
                module,
                name,
                args,
                span,
                state,
                cert,
                &mut |verifier, state, cert| {
                    verifier.verify_tag_eq(eq, l.clone(), r.clone(), span, state, cert, cont)
                },
            );
        }
        if let SymVal::Sym(sym_id, Ty::Data { module, name, args }) = &r {
            return self.verify_split_data(
                *sym_id,
                module,
                name,
                args,
                span,
                state,
                cert,
                &mut |verifier, state, cert| {
                    verifier.verify_tag_eq(eq, l.clone(), r.clone(), span, state, cert, cont)
                },
            );
        }
        match (l, r) {
            (SymVal::Con(c1, _), SymVal::Con(c2, _)) => {
                cont(self, SymVal::Bool((c1 == c2) == eq), state, cert)
            }
            _ => refuse(span, "tag equality on this type is outside the domain"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn verify_match_arms(
        &mut self,
        scrut: &SymVal,
        arms: &[Arm],
        arm_idx: usize,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        self.enter(span)?;
        let res =
            self.verify_match_arms_inner(scrut, arms, arm_idx, env, mode, span, state, cert, cont);
        self.leave();
        res
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_match_arms_inner(
        &mut self,
        scrut: &SymVal,
        arms: &[Arm],
        arm_idx: usize,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let Some(arm) = arms.get(arm_idx) else {
            let PathCert::Dead(leaf) = cert else {
                return invalid(
                    self.err_span(span),
                    "expected a `Dead` certificate for an unreachable match fallthrough",
                );
            };
            if check_dead_leaf(state, leaf) {
                return Ok(());
            }
            return invalid(
                self.err_span(span),
                "invalid `Dead` certificate for match fallthrough",
            );
        };

        match match_pat(&arm.pat, scrut, state, span)? {
            PatMatch::No => {
                self.verify_match_arms(scrut, arms, arm_idx + 1, env, mode, span, state, cert, cont)
            }
            PatMatch::NeedSplit(SplitTarget::Data {
                sym_id,
                module,
                name,
                args,
            }) => self.verify_split_data(
                sym_id,
                &module,
                &name,
                &args,
                span,
                state,
                cert,
                &mut |verifier, state, cert| {
                    verifier
                        .verify_match_arms(scrut, arms, arm_idx, env, mode, span, state, cert, cont)
                },
            ),
            PatMatch::NeedSplit(SplitTarget::Tuple { sym_id, args }) => {
                let mark = state.mark();
                let mut fields = Vec::with_capacity(args.len());
                for arg in &args {
                    fields.push(state.fresh_val_for_ty(arg, self.lang, span)?);
                }
                state
                    .sym_refinements
                    .push((sym_id, SymVal::Con(tuple(args.len()), fields)));
                let res = self
                    .verify_match_arms(scrut, arms, arm_idx, env, mode, span, state, cert, cont);
                state.restore(mark);
                res
            }
            PatMatch::NeedSplit(SplitTarget::BoolAtom(sym_id)) => self.verify_match_bool_split(
                sym_id, scrut, arms, arm_idx, env, mode, span, state, cert, cont,
            ),
            PatMatch::Yes { bindings, conds } => self.verify_match_yes(
                scrut, arms, arm_idx, bindings, conds, env, mode, span, state, cert, cont,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_match_bool_split(
        &mut self,
        sym_id: SymId,
        scrut: &SymVal,
        arms: &[Arm],
        arm_idx: usize,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let PathCert::Branch {
            then_branch,
            else_branch,
        } = cert
        else {
            return invalid(
                self.err_span(span),
                "expected a `Branch` certificate for `Bool` match split",
            );
        };
        let mark = state.mark();
        state.bool_facts.push((sym_id, true));
        let r1 = self.verify_match_arms(
            scrut,
            arms,
            arm_idx,
            env,
            mode,
            span,
            state,
            then_branch,
            cont,
        );
        state.restore(mark);
        r1?;

        state.bool_facts.push((sym_id, false));
        let r2 = self.verify_match_arms(
            scrut,
            arms,
            arm_idx,
            env,
            mode,
            span,
            state,
            else_branch,
            cont,
        );
        state.restore(mark);
        r2
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_match_yes(
        &mut self,
        scrut: &SymVal,
        arms: &[Arm],
        arm_idx: usize,
        bindings: Vec<(Rc<str>, SymVal)>,
        conds: Vec<BoolFormula>,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyEvalCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let next_env = bindings
            .into_iter()
            .fold(env.clone(), |e, (k, v)| e.with(k, v));
        let Some(guard) = conds.into_iter().reduce(BoolFormula::and) else {
            return self.eval_term(&arms[arm_idx].body, &next_env, mode, state, cert, cont);
        };
        let PathCert::Branch {
            then_branch,
            else_branch,
        } = cert
        else {
            return invalid(
                self.err_span(span),
                "expected a `Branch` certificate for pattern condition split",
            );
        };
        self.assume_formula(
            &guard,
            true,
            span,
            state,
            then_branch,
            &mut |verifier, state, cert| {
                verifier.eval_term(&arms[arm_idx].body, &next_env, mode, state, cert, cont)
            },
        )?;
        self.assume_formula(
            &guard,
            false,
            span,
            state,
            else_branch,
            &mut |verifier, state, cert| {
                verifier.verify_match_arms(
                    scrut,
                    arms,
                    arm_idx + 1,
                    env,
                    mode,
                    span,
                    state,
                    cert,
                    cont,
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_split_data(
        &mut self,
        sym_id: SymId,
        module: &str,
        name: &str,
        args: &[Ty],
        span: Span,
        state: &mut PathState,
        cert: &PathCert,
        cont: VerifyAssumeCont<'_, 'a>,
    ) -> Result<(), ContractFailure> {
        let decl = lookup_data(self.data, module, name, span)?;
        let PathCert::MatchData { branches } = cert else {
            return invalid(
                self.err_span(span),
                format!("expected a `MatchData` certificate for `{name}`"),
            );
        };
        if branches.len() != decl.constructors.len() {
            return invalid(
                self.err_span(span),
                format!(
                    "`MatchData` certificate for `{name}` has {} branches, expected {}",
                    branches.len(),
                    decl.constructors.len()
                ),
            );
        }
        for ((expected_ctor, field_tys), (cert_ctor, branch_cert)) in
            decl.constructors.iter().zip(branches.iter())
        {
            if cert_ctor != expected_ctor {
                return invalid(
                    self.err_span(span),
                    format!(
                        "`MatchData` certificate branch `{cert_ctor}` does not match constructor `{expected_ctor}` of `{name}`"
                    ),
                );
            }
            let mark = state.mark();
            let mut fields = Vec::with_capacity(field_tys.len());
            for ft in field_tys {
                let instantiated = ft.instantiate(&decl.params, args);
                fields.push(state.fresh_val_for_ty(&instantiated, self.lang, span)?);
            }
            let ctor = Rc::new(Ctor {
                module: module.to_owned(),
                ty: name.to_owned(),
                name: expected_ctor.clone(),
                arity: field_tys.len(),
            });
            state
                .sym_refinements
                .push((sym_id, SymVal::Con(ctor, fields)));
            let res = cont(self, state, branch_cert);
            state.restore(mark);
            res?;
        }
        Ok(())
    }
}
