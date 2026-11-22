//! Block statement stepping and ordered match-arm dispatch for the CEK machine.

use std::rc::Rc;

use flick_ir::{Arm, Expr, ExprKind, Stmt};

use super::{BlockKind, Control, FrameKind, Machine};
use crate::opt::eval::pattern::{PatMatch, match_pat};
use crate::opt::eval::value::{CLOSURE_BYTES, Callable, Env, Value, rc_str};
use crate::opt::eval::{EvalBudget, StopReason};

/// Binds a polymorphic non-recursive local function (`!dicts.is_empty()`) into
/// `env`, charging steps and constructed bytes before allocating the closure.
pub(crate) fn bind_poly_fun<'a>(
    name: &str,
    dicts: &'a [String],
    value: &'a Expr,
    env: &Env<'a>,
    budget: &mut EvalBudget,
) -> Result<Env<'a>, StopReason> {
    budget.charge_steps(1)?;
    let bytes = CLOSURE_BYTES.saturating_add(u32::try_from(name.len()).unwrap_or(u32::MAX));
    budget.charge_bytes(bytes)?;
    let closure = Value::Callable(Rc::new(Callable::Closure {
        env: env.clone(),
        dict_params: dicts,
        params: &[],
        body: value,
        rec_group: &[],
    }));
    Ok(env.with(rc_str(name), closure))
}

/// Validates and binds a contiguous group of mutually recursive `Stmt::Fun { rec: true }`
/// statements into `env`, charging per-member steps and bytes before allocating.
pub(crate) fn bind_rec_group<'a>(
    group: &'a [Stmt],
    env: &Env<'a>,
    budget: &mut EvalBudget,
) -> Result<Env<'a>, StopReason> {
    // Charge per-member step + bytes and validate before allocating closures.
    for stmt in group {
        let Stmt::Fun {
            name,
            rec: true,
            value,
            ..
        } = stmt
        else {
            return Err(StopReason::UnsafeContext);
        };
        budget.charge_steps(1)?;
        if !matches!(value.kind, ExprKind::Lambda { .. }) {
            return Err(StopReason::UnsafeContext);
        }
        let bytes = CLOSURE_BYTES.saturating_add(u32::try_from(name.len()).unwrap_or(u32::MAX));
        budget.charge_bytes(bytes)?;
    }
    Ok(env.with_rec_group(group))
}

impl<'a> Machine<'a, '_> {
    pub(super) fn step_block(
        &mut self,
        mut stmts: &'a [Stmt],
        result: &'a Expr,
        mut env: Env<'a>,
    ) -> Result<Control<'a>, StopReason> {
        while let Some((first, tail)) = stmts.split_first() {
            match first {
                Stmt::Let(pat, rhs) => {
                    self.push_frame(
                        FrameKind::Block {
                            kind: BlockKind::Let(pat),
                            stmts: tail,
                            result,
                        },
                        env.clone(),
                    )?;
                    return Ok(Control::Eval { expr: rhs, env });
                }
                Stmt::Expr(e) => {
                    self.push_frame(
                        FrameKind::Block {
                            kind: BlockKind::Discard,
                            stmts: tail,
                            result,
                        },
                        env.clone(),
                    )?;
                    return Ok(Control::Eval { expr: e, env });
                }
                Stmt::Fun {
                    name,
                    rec: false,
                    dicts,
                    value,
                } => {
                    if dicts.is_empty() {
                        self.push_frame(
                            FrameKind::Block {
                                kind: BlockKind::Fun(name),
                                stmts: tail,
                                result,
                            },
                            env.clone(),
                        )?;
                        return Ok(Control::Eval { expr: value, env });
                    }
                    env = bind_poly_fun(name, dicts, value, &env, self.budget)?;
                    stmts = tail;
                }
                Stmt::Fun { rec: true, .. } => {
                    let group_len = stmts
                        .iter()
                        .take_while(|s| matches!(s, Stmt::Fun { rec: true, .. }))
                        .count();
                    let (group, rest) = stmts.split_at(group_len);
                    env = bind_rec_group(group, &env, self.budget)?;
                    stmts = rest;
                }
                Stmt::Var(..) | Stmt::Assign(..) => return Err(StopReason::UnsafeContext),
            }
        }
        Ok(Control::Eval { expr: result, env })
    }

    pub(super) fn step_match_arm(
        &mut self,
        scrutinee: Value<'a>,
        mut arms: &'a [Arm],
        outer_env: Env<'a>,
    ) -> Result<Control<'a>, StopReason> {
        while let Some((arm, rest)) = arms.split_first() {
            self.budget.charge_steps(1)?;
            match match_pat(&arm.pat, scrutinee.clone(), self.budget)? {
                PatMatch::NoMatch => arms = rest,
                PatMatch::Matched(mut binds) => {
                    let mut arm_env = outer_env.clone();
                    while let Some((name, val)) = binds.pop() {
                        arm_env = arm_env.with(name, val);
                    }
                    if let Some(guard) = &arm.guard {
                        self.push_frame(
                            FrameKind::MatchGuard {
                                scrutinee,
                                body: &arm.body,
                                rest,
                                arm_env: arm_env.clone(),
                            },
                            outer_env,
                        )?;
                        return Ok(Control::Eval {
                            expr: guard,
                            env: arm_env,
                        });
                    }
                    return Ok(Control::Eval {
                        expr: &arm.body,
                        env: arm_env,
                    });
                }
            }
        }
        Err(StopReason::PossibleTrap)
    }
}
