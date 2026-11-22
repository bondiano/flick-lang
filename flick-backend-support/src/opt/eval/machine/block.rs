//! Block statement stepping and ordered match-arm dispatch for the CEK machine.

use std::rc::Rc;

use flick_ir::{Arm, Expr, ExprKind, Stmt};

use super::{Control, Frame, Machine, StepResult};
use crate::opt::eval::StopReason;
use crate::opt::eval::pattern::{PatMatch, match_pat};
use crate::opt::eval::value::{CLOSURE_BYTES, Closure, DictEnv, Env, RecFun, RecGroup, Value};

impl<'a> Machine<'a, '_> {
    #[allow(clippy::too_many_lines, reason = "exhaustive block statement stepping")]
    pub(super) fn step_block(
        &mut self,
        stmts: &'a [Stmt],
        index: usize,
        result: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    ) -> Result<StepResult<'a>, StopReason> {
        if index >= stmts.len() {
            return Ok(StepResult::Continue(Control::Eval {
                expr: result,
                env,
                dict_env,
            }));
        }
        match &stmts[index] {
            Stmt::Let(pat, rhs) => {
                self.push_frame(Frame::BlockLet {
                    pat,
                    stmts,
                    next_index: index + 1,
                    result,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: rhs,
                    env,
                    dict_env,
                }))
            }
            Stmt::Expr(e) => {
                self.push_frame(Frame::BlockDiscard {
                    stmts,
                    next_index: index + 1,
                    result,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: e,
                    env,
                    dict_env,
                }))
            }
            Stmt::Fun {
                name,
                rec: false,
                dicts,
                value,
            } => {
                if dicts.is_empty() {
                    self.push_frame(Frame::BlockFun {
                        name,
                        stmts,
                        next_index: index + 1,
                        result,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: value,
                        env,
                        dict_env,
                    }))
                } else {
                    let bytes =
                        CLOSURE_BYTES.saturating_add(u32::try_from(name.len()).unwrap_or(u32::MAX));
                    self.budget.charge_bytes(bytes)?;
                    let closure = Value::Closure(Rc::new(Closure {
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                        dict_params: dicts,
                        params: &[],
                        body: value,
                        rec_group: None,
                    }));
                    let next_env = env.with(Rc::from(name.as_str()), closure);
                    Ok(StepResult::Continue(Control::BlockStep {
                        stmts,
                        index: index + 1,
                        result,
                        env: next_env,
                        dict_env,
                    }))
                }
            }
            Stmt::Fun { rec: true, .. } => {
                // First pass (zero allocation): charge per-member step + bytes
                // and validate every sibling before allocating `defs` / `RecGroup`.
                let mut end = index;
                while let Some(Stmt::Fun {
                    name,
                    rec: true,
                    value,
                    ..
                }) = stmts.get(end)
                {
                    self.budget.charge_steps(1)?;
                    if !matches!(value.kind, ExprKind::Lambda { .. }) {
                        return Err(StopReason::UnsafeContext);
                    }
                    let bytes =
                        CLOSURE_BYTES.saturating_add(u32::try_from(name.len()).unwrap_or(u32::MAX));
                    self.budget.charge_bytes(bytes)?;
                    end += 1;
                }
                let mut defs = Vec::with_capacity(end - index);
                for stmt in &stmts[index..end] {
                    let Stmt::Fun {
                        name,
                        rec: true,
                        dicts,
                        value,
                    } = stmt
                    else {
                        return Err(StopReason::UnsafeContext);
                    };
                    let ExprKind::Lambda { params, body } = &value.kind else {
                        return Err(StopReason::UnsafeContext);
                    };
                    defs.push(RecFun {
                        name: Rc::from(name.as_str()),
                        dict_params: dicts.as_slice(),
                        params: params.as_slice(),
                        body: body.as_ref(),
                    });
                }
                let group = Rc::new(RecGroup {
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                    defs: Rc::from(defs),
                });
                let next_env = env.with_rec_group(&group);
                Ok(StepResult::Continue(Control::BlockStep {
                    stmts,
                    index: end,
                    result,
                    env: next_env,
                    dict_env,
                }))
            }
            Stmt::Var(..) | Stmt::Assign(..) => Err(StopReason::UnsafeContext),
        }
    }

    pub(super) fn step_match_arm(
        &mut self,
        scrutinee: Value<'a>,
        arms: &'a [Arm],
        mut arm_idx: usize,
        outer_env: Env<'a>,
        dict_env: DictEnv<'a>,
    ) -> Result<StepResult<'a>, StopReason> {
        while arm_idx < arms.len() {
            self.budget.charge_steps(1)?;
            let arm = &arms[arm_idx];
            match match_pat(&arm.pat, scrutinee.clone(), self.budget)? {
                PatMatch::NoMatch => arm_idx += 1,
                PatMatch::Matched(binds) => {
                    let mut arm_env = outer_env.clone();
                    for (name, val) in binds {
                        arm_env = arm_env.with(name, val);
                    }
                    if let Some(guard) = &arm.guard {
                        self.push_frame(Frame::MatchGuard {
                            scrutinee,
                            arms,
                            arm_idx,
                            arm_env: arm_env.clone(),
                            outer_env,
                            dict_env: dict_env.clone(),
                        })?;
                        return Ok(StepResult::Continue(Control::Eval {
                            expr: guard,
                            env: arm_env,
                            dict_env,
                        }));
                    }
                    return Ok(StepResult::Continue(Control::Eval {
                        expr: &arm.body,
                        env: arm_env,
                        dict_env,
                    }));
                }
            }
        }
        Err(StopReason::PossibleTrap)
    }
}
