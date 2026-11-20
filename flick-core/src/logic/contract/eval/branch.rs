//! Symbolic branching, constructor/tag splitting, pattern matching, and
//! path-condition assumption with Omega dead-branch pruning.

use std::rc::Rc;

use crate::logic::omega::{self, Outcome};
use crate::logic::term::tuple;
use crate::logic::{Ctor, Ty};
use crate::source::Span;

use super::super::ContractFailure;
use super::super::cert::{LeafCert, PathCert};
use super::super::state::{
    BoolFormula, EvalMode, PatMatch, PathState, SplitTarget, SymEnv, SymId, SymVal, lin_add_const,
    lin_not_ge, local_span, lookup_data, match_pat, refuse, to_bool_formula,
};
use super::super::term::{Arm, Term};
use super::{AssumeCont, EvalCont, Solver};

impl<'a> Solver<'a> {
    pub(super) fn eval_if(
        &mut self,
        (guard, then_t, else_t): (&Term, &Term, &Term),
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.eval_term(guard, env, mode, state, &mut |solver, cval, state| {
            let formula = to_bool_formula(&cval, state, guard.span)?;
            match formula {
                BoolFormula::True => solver.eval_term(then_t, env, mode, state, cont),
                BoolFormula::False => solver.eval_term(else_t, env, mode, state, cont),
                other => {
                    solver.add_paths(1, span)?;
                    let then_branch = solver.assume_formula(
                        &other,
                        true,
                        guard.span,
                        state,
                        &mut |solver, state| solver.eval_term(then_t, env, mode, state, cont),
                    )?;
                    let else_branch = solver.assume_formula(
                        &other,
                        false,
                        guard.span,
                        state,
                        &mut |solver, state| solver.eval_term(else_t, env, mode, state, cont),
                    )?;
                    Ok(PathCert::Branch {
                        then_branch: Box::new(then_branch),
                        else_branch: Box::new(else_branch),
                    })
                }
            }
        })
    }

    pub(super) fn eval_tag_eq(
        &mut self,
        eq: bool,
        lhs: SymVal,
        rhs: SymVal,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.enter(span)?;
        let res = self.eval_tag_eq_inner(eq, lhs, rhs, span, state, cont);
        self.leave();
        res
    }

    fn eval_tag_eq_inner(
        &mut self,
        eq: bool,
        lhs: SymVal,
        rhs: SymVal,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let l = state.whnf(lhs);
        let r = state.whnf(rhs);
        if let SymVal::Sym(sym_id, Ty::Data { module, name, args }) = &l {
            return self.split_data(
                *sym_id,
                module,
                name,
                args,
                span,
                state,
                &mut |solver, state| {
                    solver.eval_tag_eq(eq, l.clone(), r.clone(), span, state, cont)
                },
            );
        }
        if let SymVal::Sym(sym_id, Ty::Data { module, name, args }) = &r {
            return self.split_data(
                *sym_id,
                module,
                name,
                args,
                span,
                state,
                &mut |solver, state| {
                    solver.eval_tag_eq(eq, l.clone(), r.clone(), span, state, cont)
                },
            );
        }
        match (l, r) {
            (SymVal::Con(c1, _), SymVal::Con(c2, _)) => {
                cont(self, SymVal::Bool((c1 == c2) == eq), state)
            }
            _ => refuse(span, "tag equality on this type is outside the domain"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn eval_match_arms(
        &mut self,
        scrut: &SymVal,
        arms: &[Arm],
        arm_idx: usize,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.enter(span)?;
        let res = self.eval_match_arms_inner(scrut, arms, arm_idx, env, mode, span, state, cont);
        self.leave();
        res
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_match_arms_inner(
        &mut self,
        scrut: &SymVal,
        arms: &[Arm],
        arm_idx: usize,
        env: &SymEnv,
        mode: EvalMode,
        span: Span,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let Some(arm) = arms.get(arm_idx) else {
            if state.has_trivial_contradiction() {
                return Ok(PathCert::Dead(LeafCert::Trivial));
            }
            return match self.decide(state) {
                Outcome::Refuted(proof) => Ok(PathCert::Dead(LeafCert::Omega(proof))),
                Outcome::Model(_) => refuse(span, "a `match` here has no arm for its value"),
                Outcome::Unknown => Err(ContractFailure::Exhausted {
                    span: local_span(span, self.spec.contract_span),
                }),
            };
        };

        match match_pat(&arm.pat, scrut, state, span)? {
            PatMatch::No => {
                self.eval_match_arms(scrut, arms, arm_idx + 1, env, mode, span, state, cont)
            }
            PatMatch::NeedSplit(SplitTarget::Data {
                sym_id,
                module,
                name,
                args,
            }) => self.split_data(
                sym_id,
                &module,
                &name,
                &args,
                span,
                state,
                &mut |solver, state| {
                    solver.eval_match_arms(scrut, arms, arm_idx, env, mode, span, state, cont)
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
                let res = self.eval_match_arms(scrut, arms, arm_idx, env, mode, span, state, cont);
                state.restore(mark);
                res
            }
            PatMatch::NeedSplit(SplitTarget::BoolAtom(sym_id)) => {
                self.add_paths(1, span)?;
                let mark = state.mark();
                state.bool_facts.push((sym_id, true));
                let then_branch =
                    self.eval_match_arms(scrut, arms, arm_idx, env, mode, span, state, cont);
                state.restore(mark);
                let then_branch = then_branch?;

                state.bool_facts.push((sym_id, false));
                let else_branch =
                    self.eval_match_arms(scrut, arms, arm_idx, env, mode, span, state, cont);
                state.restore(mark);
                let else_branch = else_branch?;

                Ok(PathCert::Branch {
                    then_branch: Box::new(then_branch),
                    else_branch: Box::new(else_branch),
                })
            }
            PatMatch::Yes { bindings, conds } => self.eval_match_yes(
                scrut, arms, arm_idx, bindings, conds, env, mode, span, state, cont,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_match_yes(
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
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let next_env = bindings
            .into_iter()
            .fold(env.clone(), |e, (k, v)| e.with(k, v));
        let Some(guard) = conds.into_iter().reduce(BoolFormula::and) else {
            return self.eval_term(&arms[arm_idx].body, &next_env, mode, state, cont);
        };
        self.add_paths(1, span)?;
        let then_branch =
            self.assume_formula(&guard, true, span, state, &mut |solver, state| {
                solver.eval_term(&arms[arm_idx].body, &next_env, mode, state, cont)
            })?;
        let else_branch =
            self.assume_formula(&guard, false, span, state, &mut |solver, state| {
                solver.eval_match_arms(scrut, arms, arm_idx + 1, env, mode, span, state, cont)
            })?;
        Ok(PathCert::Branch {
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn split_data(
        &mut self,
        sym_id: SymId,
        module: &str,
        name: &str,
        args: &[Ty],
        span: Span,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let decl = lookup_data(self.data, module, name, span)?;
        if decl.constructors.len() > 1 {
            let extra = u32::try_from(decl.constructors.len() - 1).unwrap_or(u32::MAX);
            self.add_paths(extra, span)?;
        }
        let mut branches = Vec::with_capacity(decl.constructors.len());
        for (ctor_name, field_tys) in &decl.constructors {
            let mark = state.mark();
            let mut fields = Vec::with_capacity(field_tys.len());
            for ft in field_tys {
                let instantiated = ft.instantiate(&decl.params, args);
                fields.push(state.fresh_val_for_ty(&instantiated, self.lang, span)?);
            }
            let ctor = Rc::new(Ctor {
                module: module.to_owned(),
                ty: name.to_owned(),
                name: ctor_name.clone(),
                arity: field_tys.len(),
            });
            state
                .sym_refinements
                .push((sym_id, SymVal::Con(ctor, fields)));
            let cert = cont(self, state);
            state.restore(mark);
            branches.push((ctor_name.clone(), cert?));
        }
        Ok(PathCert::MatchData { branches })
    }

    /// Assumes `formula == expected` along the current path, pruning dead
    /// branches immediately via Omega.
    pub(super) fn assume_formula(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.enter(span)?;
        let res = self.assume_formula_inner(formula, expected, span, state, cont);
        self.leave();
        res
    }

    fn assume_formula_inner(
        &mut self,
        formula: &BoolFormula,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        match (formula, expected) {
            (BoolFormula::True, true) | (BoolFormula::False, false) => cont(self, state),
            (BoolFormula::True, false) | (BoolFormula::False, true) => {
                Ok(PathCert::Dead(LeafCert::Trivial))
            }
            (BoolFormula::Not(inner), exp) => self.assume_formula(inner, !exp, span, state, cont),
            (BoolFormula::Atom(id), exp) => match state.bool_fact(*id) {
                Some(prev) if prev == exp => cont(self, state),
                Some(_) => Ok(PathCert::Dead(LeafCert::Trivial)),
                None => {
                    let mark = state.mark();
                    state.bool_facts.push((*id, exp));
                    let res = cont(self, state);
                    state.restore(mark);
                    res
                }
            },
            (BoolFormula::LinGe(f), true) => self.assume_lin_ge(f, span, state, cont),
            (BoolFormula::LinGe(f), false) => {
                let nf = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
                    span: local_span(span, self.spec.contract_span),
                })?;
                self.assume_lin_ge(&nf, span, state, cont)
            }
            (BoolFormula::LinEq(f), exp) => self.assume_lin_eq(f, exp, span, state, cont),
            (BoolFormula::And(a, b), true) => {
                self.assume_formula(a, true, span, state, &mut |solver, state| {
                    solver.assume_formula(b, true, span, state, cont)
                })
            }
            (BoolFormula::Or(a, b), false) => {
                self.assume_formula(a, false, span, state, &mut |solver, state| {
                    solver.assume_formula(b, false, span, state, cont)
                })
            }
            (BoolFormula::Or(a, b), true) => {
                self.add_paths(1, span)?;
                let then_branch = self.assume_formula(a, true, span, state, cont)?;
                let else_branch =
                    self.assume_formula(a, false, span, state, &mut |solver, state| {
                        solver.assume_formula(b, true, span, state, cont)
                    })?;
                Ok(PathCert::Branch {
                    then_branch: Box::new(then_branch),
                    else_branch: Box::new(else_branch),
                })
            }
            (BoolFormula::And(a, b), false) => {
                self.add_paths(1, span)?;
                let then_branch = self.assume_formula(a, false, span, state, cont)?;
                let else_branch =
                    self.assume_formula(a, true, span, state, &mut |solver, state| {
                        solver.assume_formula(b, false, span, state, cont)
                    })?;
                Ok(PathCert::Branch {
                    then_branch: Box::new(then_branch),
                    else_branch: Box::new(else_branch),
                })
            }
        }
    }

    fn assume_lin_eq(
        &mut self,
        f: &omega::Linear,
        expected: bool,
        span: Span,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        if f.coeffs.is_empty() {
            return if (f.constant == 0) == expected {
                cont(self, state)
            } else {
                Ok(PathCert::Dead(LeafCert::Trivial))
            };
        }
        if expected {
            let nf = f.neg().ok_or(ContractFailure::Exhausted {
                span: local_span(span, self.spec.contract_span),
            })?;
            let mark = state.mark();
            state.facts.push(f.clone());
            state.facts.push(nf);
            let out = match self.decide(state) {
                Outcome::Refuted(proof) => Ok(PathCert::Dead(LeafCert::Omega(proof))),
                Outcome::Model(_) => cont(self, state),
                Outcome::Unknown => Err(ContractFailure::Exhausted {
                    span: local_span(span, self.spec.contract_span),
                }),
            };
            state.restore(mark);
            return out;
        }
        let gt = lin_add_const(f, -1).ok_or(ContractFailure::Exhausted {
            span: local_span(span, self.spec.contract_span),
        })?;
        let lt = lin_not_ge(f).ok_or(ContractFailure::Exhausted {
            span: local_span(span, self.spec.contract_span),
        })?;
        self.add_paths(1, span)?;
        let then_branch = self.assume_lin_ge(&gt, span, state, cont)?;
        let else_branch = self.assume_lin_ge(&lt, span, state, cont)?;
        Ok(PathCert::Branch {
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        })
    }

    fn assume_lin_ge(
        &mut self,
        f: &omega::Linear,
        span: Span,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        if f.coeffs.is_empty() {
            return if f.constant >= 0 {
                cont(self, state)
            } else {
                Ok(PathCert::Dead(LeafCert::Trivial))
            };
        }
        let mark = state.mark();
        state.facts.push(f.clone());
        let out = match self.decide(state) {
            Outcome::Refuted(proof) => Ok(PathCert::Dead(LeafCert::Omega(proof))),
            Outcome::Model(_) => cont(self, state),
            Outcome::Unknown => Err(ContractFailure::Exhausted {
                span: local_span(span, self.spec.contract_span),
            }),
        };
        state.restore(mark);
        out
    }
}
