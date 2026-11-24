//! Path-sensitive CPS symbolic execution and verification condition solver for
//! `PureTotalV2` contracts.
//!
//! Produces a [`ContractCert`] whose leaves are Omega refutation proofs (or
//! verified tautologies) and formats concrete Omega counterexamples on failure
//! (`T087` postcondition, `T088` call precondition, `T089` `Int` overflow).
//! Every produced certificate MUST still pass [`super::verify::check_contract_cert`].

mod branch;
mod counterexample;
mod ops;
mod prove;

use crate::logic::omega::{self, Outcome};
use crate::logic::{DOMAIN_V2, Data, Program};
use crate::source::Span;

use super::cert::{ContractCert, PathCert};
use super::defs::AdmittedV2;
use super::state::{
    Budgets, EvalMode, INT_MAX, INT_MIN, PathState, SymEnv, SymVal, admit_contract,
    collect_conjuncts, combine_bool_eq, contains_unsat_contracted, lin_const, local_span,
    negate_bool_val, refuse, to_bool_formula,
};
use super::term::{LangTypes, Term, TermKind};
use super::{ContractFailure, ContractSpec, Counterexample, VerifiedContracts};

type EvalCont<'b, 'a> =
    &'b mut dyn FnMut(&mut Solver<'a>, SymVal, &mut PathState) -> Result<PathCert, ContractFailure>;
type AssumeCont<'b, 'a> =
    &'b mut dyn FnMut(&mut Solver<'a>, &mut PathState) -> Result<PathCert, ContractFailure>;

/// Context identifying which diagnostic to report if an obligation fails.
#[derive(Clone)]
enum FailKind<'a> {
    Postcondition {
        span: Span,
        decl_span: Span,
    },
    Precondition {
        call_span: Span,
        callee: &'a str,
        callee_require: Option<Span>,
    },
}

impl FailKind<'_> {
    fn span(&self) -> Span {
        match self {
            FailKind::Postcondition { span, .. } => *span,
            FailKind::Precondition { call_span, .. } => *call_span,
        }
    }

    fn into_failure(self, counterexample: Option<Counterexample>) -> ContractFailure {
        match self {
            FailKind::Postcondition { span, decl_span } => ContractFailure::Postcondition {
                span,
                decl_span,
                counterexample,
            },
            FailKind::Precondition {
                call_span,
                callee,
                callee_require,
            } => ContractFailure::Precondition {
                span: call_span,
                callee: callee.to_owned(),
                callee_require,
                counterexample,
            },
        }
    }
}

/// Runs symbolic execution and VC solving for `(spec, body)` using default
/// deterministic budgets.
pub fn prove(
    spec: &ContractSpec,
    body: &Term,
    data: &Data,
    program: &dyn Program,
    verified: &VerifiedContracts,
    lang: &LangTypes,
) -> Result<ContractCert, ContractFailure> {
    prove_with_budgets(
        spec,
        body,
        data,
        program,
        verified,
        lang,
        Budgets::default(),
    )
}

/// Runs symbolic execution and VC solving for `(spec, body)` under `budgets`.
pub fn prove_with_budgets(
    spec: &ContractSpec,
    body: &Term,
    data: &Data,
    program: &dyn Program,
    verified: &VerifiedContracts,
    lang: &LangTypes,
    budgets: Budgets,
) -> Result<ContractCert, ContractFailure> {
    let admitted = admit_contract(spec, body, data, program, verified, lang)?;
    let mut state = PathState::default();
    let env = state.init_params(spec, lang)?;
    let mut solver = Solver {
        spec,
        defs: &admitted.defs,
        data: &admitted.data,
        verified,
        lang,
        steps: budgets.max_steps,
        depth: 0,
        max_depth: budgets.max_depth,
        paths: 1,
        max_paths: budgets.max_paths,
        omega_budget: budgets.omega_budget,
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

    let root = solver.assume_terms(&requires, &env, &mut state, &mut |solver, state| {
        solver.eval_term(
            body,
            &env,
            EvalMode::Body,
            state,
            &mut |solver, result_val, state| {
                if contains_unsat_contracted(&result_val, state) {
                    return refuse(
                        body.span,
                        "a partially applied contracted function cannot escape",
                    );
                }
                solver.prove_ensures(&spec.ensures, &env, &result_val, spec.decl_span, state)
            },
        )
    })?;

    Ok(ContractCert {
        domain: DOMAIN_V2,
        root,
    })
}

struct Solver<'a> {
    spec: &'a ContractSpec,
    defs: &'a AdmittedV2,
    data: &'a Data,
    verified: &'a VerifiedContracts,
    lang: &'a LangTypes,
    steps: u64,
    depth: u32,
    max_depth: u32,
    paths: u32,
    max_paths: u32,
    omega_budget: u32,
    caller_span: Span,
    in_foreign: bool,
}

impl<'a> Solver<'a> {
    fn diag_span(&self, span: Span) -> Span {
        if self.in_foreign {
            self.caller_span
        } else {
            local_span(span, self.caller_span)
        }
    }

    fn err_span(&self, span: Span) -> Span {
        if self.in_foreign {
            self.caller_span
        } else {
            local_span(span, self.spec.contract_span)
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

    fn add_paths(&mut self, extra: u32, span: Span) -> Result<(), ContractFailure> {
        let next = self.paths.saturating_add(extra);
        if next > self.max_paths {
            return Err(ContractFailure::Exhausted {
                span: self.err_span(span),
            });
        }
        self.paths = next;
        Ok(())
    }

    fn decide(&mut self, state: &PathState) -> Outcome {
        omega::decide_budgeted(&state.facts, state.vars, &mut self.omega_budget)
    }

    /// Evaluates a slice of boolean terms in `Spec` mode, assuming each true
    /// before continuing to `cont`.
    fn assume_terms(
        &mut self,
        terms: &[&Term],
        env: &SymEnv,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let Some((first, rest)) = terms.split_first() else {
            return cont(self, state);
        };
        self.assume_term(first, env, state, &mut |solver, state| {
            solver.assume_terms(rest, env, state, cont)
        })
    }

    /// Assumes a single boolean specification term is `true`, flattening
    /// top-level `And`s without creating redundant dead branches.
    fn assume_term(
        &mut self,
        term: &Term,
        env: &SymEnv,
        state: &mut PathState,
        cont: AssumeCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        if let TermKind::And(a, b) = &term.kind {
            return self.assume_term(a, env, state, &mut |solver, state| {
                solver.assume_term(b, env, state, cont)
            });
        }
        let span = term.span;
        self.eval_term(
            term,
            env,
            EvalMode::Spec,
            state,
            &mut |solver, val, state| {
                let formula = to_bool_formula(&val, state, span)?;
                solver.assume_formula(&formula, true, span, state, cont)
            },
        )
    }

    /// Core CPS symbolic evaluator for `PureTotalV2` terms.
    fn eval_term(
        &mut self,
        term: &Term,
        env: &SymEnv,
        mode: EvalMode,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        self.enter(term.span)?;
        let res = self.eval_term_inner(term, env, mode, state, cont);
        self.leave();
        res
    }

    fn eval_term_inner(
        &mut self,
        term: &Term,
        env: &SymEnv,
        mode: EvalMode,
        state: &mut PathState,
        cont: EvalCont<'_, 'a>,
    ) -> Result<PathCert, ContractFailure> {
        let span = term.span;
        match &term.kind {
            TermKind::Var(name) => {
                let Some(val) = env.get(name).cloned() else {
                    return refuse(
                        self.diag_span(span),
                        format!("unbound variable `{name}` is outside the domain"),
                    );
                };
                cont(self, val, state)
            }
            TermKind::IntLit(n) => {
                if mode == EvalMode::Body && (*n < INT_MIN || *n > INT_MAX) {
                    return refuse(
                        self.diag_span(span),
                        "an `Int` literal outside 32-bit range is outside the domain",
                    );
                }
                cont(self, SymVal::Int(lin_const(*n)), state)
            }
            TermKind::BoolLit(b) => cont(self, SymVal::Bool(*b), state),
            TermKind::UnitLit => cont(self, SymVal::Unit, state),
            TermKind::Con(ctor) => cont(self, SymVal::Con(ctor.clone(), Vec::new()), state),
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
            ),
            TermKind::IntBin(op, lhs, rhs) => {
                self.eval_int_bin(*op, lhs, rhs, env, mode, span, state, cont)
            }
            TermKind::IntNeg(inner) => self.eval_int_neg(inner, env, mode, span, state, cont),
            TermKind::IntCmp(op, lhs, rhs) => {
                self.eval_int_cmp_term(*op, lhs, rhs, env, mode, span, state, cont)
            }
            TermKind::BoolEq { eq, lhs, rhs } => {
                let eq = *eq;
                self.eval_term(lhs, env, mode, state, &mut |solver, lval, state| {
                    solver.eval_term(rhs, env, mode, state, &mut |solver, rval, state| {
                        let out = combine_bool_eq(eq, &lval, &rval, state, span)?;
                        cont(solver, out, state)
                    })
                })
            }
            TermKind::TagEq { eq, lhs, rhs } => {
                let eq = *eq;
                self.eval_term(lhs, env, mode, state, &mut |solver, lval, state| {
                    solver.eval_term(rhs, env, mode, state, &mut |solver, rval, state| {
                        solver.eval_tag_eq(eq, lval.clone(), rval, span, state, cont)
                    })
                })
            }
            TermKind::Not(inner) => {
                self.eval_term(inner, env, mode, state, &mut |solver, val, state| {
                    let out = negate_bool_val(&val, state, span)?;
                    cont(solver, out, state)
                })
            }
            TermKind::And(lhs, rhs) => {
                let false_term = Term::new(TermKind::BoolLit(false), span);
                self.eval_if((lhs, rhs, &false_term), env, mode, span, state, cont)
            }
            TermKind::Or(lhs, rhs) => {
                let true_term = Term::new(TermKind::BoolLit(true), span);
                self.eval_if((lhs, &true_term, rhs), env, mode, span, state, cont)
            }
            TermKind::If(guard, then_t, else_t) => {
                self.eval_if((guard, then_t, else_t), env, mode, span, state, cont)
            }
            TermKind::Let(name, value, body) => {
                let name = name.clone();
                self.eval_term(value, env, mode, state, &mut |solver, val, state| {
                    let next_env = env.with(name.clone(), val);
                    solver.eval_term(body, &next_env, mode, state, cont)
                })
            }
            TermKind::Match(scrut, arms) => {
                self.eval_term(scrut, env, mode, state, &mut |solver, val, state| {
                    solver.eval_match_arms(&val, arms, 0, env, mode, span, state, cont)
                })
            }
            TermKind::App {
                func,
                arg,
                arg_first,
            } => self.eval_app(func, arg, *arg_first, env, mode, span, state, cont),
            TermKind::Def(def) => self.eval_def(def, mode, span, state, cont),
        }
    }
}
