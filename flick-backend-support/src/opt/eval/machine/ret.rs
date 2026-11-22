//! Continuation frame return dispatch and `Option` `?` unwinding.

use std::rc::Rc;

use flick_ir::{BinOp, Lang};

use super::{Control, Frame, Machine, StepResult};
use crate::opt::eval::StopReason;
use crate::opt::eval::pattern::{PatMatch, match_pat};
use crate::opt::eval::value::{CLOSURE_BYTES, Closure, DictValue, FIELD_BYTES, Value};

impl<'a> Machine<'a, '_> {
    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive continuation frame dispatch"
    )]
    pub(super) fn step_return(&mut self, value: Value<'a>) -> Result<StepResult<'a>, StopReason> {
        let Some(frame) = self.frames.pop() else {
            return Ok(StepResult::Done(value));
        };
        match frame {
            Frame::CallBoundary => Ok(StepResult::Continue(Control::Return(value))),
            Frame::LeaveGlobal => {
                self.active_globals.pop();
                Ok(StepResult::Continue(Control::Return(value)))
            }
            Frame::AppFunc { arg, env, dict_env } => {
                self.push_frame(Frame::AppReadyFunc { func_val: value })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: arg,
                    env,
                    dict_env,
                }))
            }
            Frame::AppReadyFunc { func_val } => self.apply(func_val, value),
            Frame::AppArgFirst {
                func,
                env,
                dict_env,
            } => {
                self.push_frame(Frame::AppReadyArg { arg_val: value })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: func,
                    env,
                    dict_env,
                }))
            }
            Frame::AppReadyArg { arg_val } => self.apply(value, arg_val),
            Frame::ApplyDicts { dicts } => match value {
                Value::Closure(closure) if closure.dict_params.len() == dicts.len() => {
                    let mut next_dicts = closure.dict_env.clone();
                    for (param, dict) in closure.dict_params.iter().zip(dicts.iter()) {
                        next_dicts = next_dicts.with(Rc::from(param.as_str()), dict.clone());
                    }
                    let base_env = match &closure.rec_group {
                        Some(group) => {
                            if group.defs.len() > 1 {
                                let extra = u32::try_from(group.defs.len()).unwrap_or(u32::MAX);
                                self.budget.charge_steps(extra)?;
                            }
                            closure.env.with_rec_group(group)
                        }
                        None => closure.env.clone(),
                    };
                    if closure.params.is_empty() {
                        Ok(StepResult::Continue(Control::Eval {
                            expr: closure.body,
                            env: base_env,
                            dict_env: next_dicts,
                        }))
                    } else {
                        self.budget.charge_bytes(CLOSURE_BYTES)?;
                        Ok(StepResult::Continue(Control::Return(Value::Closure(
                            Rc::new(Closure {
                                env: base_env,
                                dict_env: next_dicts,
                                dict_params: &[],
                                params: closure.params,
                                body: closure.body,
                                rec_group: None,
                            }),
                        ))))
                    }
                }
                _ => Err(StopReason::UnknownOp),
            },
            Frame::If {
                then_expr,
                else_expr,
                env,
                dict_env,
            } => match value {
                Value::Bool(true) => Ok(StepResult::Continue(Control::Eval {
                    expr: then_expr,
                    env,
                    dict_env,
                })),
                Value::Bool(false) => Ok(StepResult::Continue(Control::Eval {
                    expr: else_expr,
                    env,
                    dict_env,
                })),
                _ => Err(StopReason::UnsafeContext),
            },
            Frame::ShortCircuit {
                op,
                rhs,
                env,
                dict_env,
            } => match (op, value) {
                (BinOp::And, Value::Bool(false)) => {
                    Ok(StepResult::Continue(Control::Return(Value::Bool(false))))
                }
                (BinOp::Or, Value::Bool(true)) => {
                    Ok(StepResult::Continue(Control::Return(Value::Bool(true))))
                }
                (BinOp::And, Value::Bool(true)) | (BinOp::Or, Value::Bool(false)) => {
                    Ok(StepResult::Continue(Control::Eval {
                        expr: rhs,
                        env,
                        dict_env,
                    }))
                }
                _ => Err(StopReason::UnsafeContext),
            },
            Frame::BinaryLhs {
                op,
                rhs,
                env,
                dict_env,
            } => {
                self.push_frame(Frame::BinaryRhs { op, lhs_val: value })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: rhs,
                    env,
                    dict_env,
                }))
            }
            Frame::BinaryRhs { op, lhs_val } => {
                let result = self.eval_binary(op, lhs_val, value)?;
                Ok(StepResult::Continue(Control::Return(result)))
            }
            Frame::Neg(num) => {
                let result = self.eval_neg(num, value)?;
                Ok(StepResult::Continue(Control::Return(result)))
            }
            Frame::Not => match value {
                Value::Bool(b) => Ok(StepResult::Continue(Control::Return(Value::Bool(!b)))),
                _ => Err(StopReason::UnsafeContext),
            },
            Frame::Ignore => Ok(StepResult::Continue(Control::Return(Value::Unit))),
            Frame::Unwrap => self.step_unwrap(value),
            Frame::Field { label } => match value {
                Value::Record(fields) => {
                    let scan_steps = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                    self.budget.charge_steps(scan_steps)?;
                    let (_, val) = fields
                        .iter()
                        .find(|(k, _)| &**k == label)
                        .ok_or(StopReason::UnsafeContext)?;
                    Ok(StepResult::Continue(Control::Return(val.clone())))
                }
                _ => Err(StopReason::UnsafeContext),
            },
            Frame::Tuple {
                mut done,
                rest,
                index,
                env,
                dict_env,
            } => {
                done.push(value);
                if index < rest.len() {
                    self.push_frame(Frame::Tuple {
                        done,
                        rest,
                        index: index + 1,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: &rest[index],
                        env,
                        dict_env,
                    }))
                } else {
                    Ok(StepResult::Continue(Control::Return(Value::Tuple(
                        Rc::from(done),
                    ))))
                }
            }
            Frame::List {
                mut done,
                rest,
                index,
                env,
                dict_env,
            } => {
                done.push(value);
                if index < rest.len() {
                    self.push_frame(Frame::List {
                        done,
                        rest,
                        index: index + 1,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: &rest[index],
                        env,
                        dict_env,
                    }))
                } else {
                    Ok(StepResult::Continue(Control::Return(Value::List(
                        Rc::from(done),
                    ))))
                }
            }
            Frame::Record {
                mut done,
                rest,
                index,
                as_dict,
                env,
                dict_env,
            } => {
                done.push((Rc::from(rest[index - 1].0.as_str()), value));
                if index < rest.len() {
                    self.push_frame(Frame::Record {
                        done,
                        rest,
                        index: index + 1,
                        as_dict,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: &rest[index].1,
                        env,
                        dict_env,
                    }))
                } else if as_dict {
                    Ok(StepResult::Continue(Control::Return(Value::Dict(
                        DictValue::Record(Rc::from(done)),
                    ))))
                } else {
                    Ok(StepResult::Continue(Control::Return(Value::Record(
                        Rc::from(done),
                    ))))
                }
            }
            Frame::UpdateBase {
                fields,
                env,
                dict_env,
            } => {
                let Value::Record(base) = value else {
                    return Err(StopReason::UnsafeContext);
                };
                if fields.is_empty() {
                    return Ok(StepResult::Continue(Control::Return(Value::Record(base))));
                }
                self.push_frame(Frame::UpdateFields {
                    base,
                    done: Vec::with_capacity(fields.len()),
                    rest: fields,
                    index: 1,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: &fields[0].1,
                    env,
                    dict_env,
                }))
            }
            Frame::UpdateFields {
                base,
                mut done,
                rest,
                index,
                env,
                dict_env,
            } => {
                done.push((Rc::from(rest[index - 1].0.as_str()), value));
                if index < rest.len() {
                    self.push_frame(Frame::UpdateFields {
                        base,
                        done,
                        rest,
                        index: index + 1,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: &rest[index].1,
                        env,
                        dict_env,
                    }))
                } else {
                    let est_fields = base.len().saturating_add(done.len());
                    let bytes = u32::try_from(est_fields)
                        .unwrap_or(u32::MAX)
                        .saturating_mul(FIELD_BYTES);
                    self.budget.charge_bytes(bytes)?;
                    let scan_steps = u32::try_from(base.len().saturating_mul(done.len()))
                        .unwrap_or(u32::MAX)
                        / 4;
                    self.budget.charge_steps(scan_steps)?;
                    let mut merged: Vec<(Rc<str>, Value<'a>)> = base.to_vec();
                    for (label, val) in done {
                        if let Some(slot) = merged.iter_mut().find(|(k, _)| *k == label) {
                            slot.1 = val;
                        } else {
                            merged.push((label, val));
                        }
                    }
                    Ok(StepResult::Continue(Control::Return(Value::Record(
                        Rc::from(merged),
                    ))))
                }
            }
            Frame::BlockLet {
                pat,
                stmts,
                next_index,
                result,
                mut env,
                dict_env,
            } => match match_pat(pat, value, self.budget)? {
                PatMatch::NoMatch => Err(StopReason::PossibleTrap),
                PatMatch::Matched(binds) => {
                    for (name, val) in binds {
                        env = env.with(name, val);
                    }
                    Ok(StepResult::Continue(Control::BlockStep {
                        stmts,
                        index: next_index,
                        result,
                        env,
                        dict_env,
                    }))
                }
            },
            Frame::BlockFun {
                name,
                stmts,
                next_index,
                result,
                env,
                dict_env,
            } => {
                let next_env = env.with(Rc::from(name), value);
                Ok(StepResult::Continue(Control::BlockStep {
                    stmts,
                    index: next_index,
                    result,
                    env: next_env,
                    dict_env,
                }))
            }
            Frame::BlockDiscard {
                stmts,
                next_index,
                result,
                env,
                dict_env,
            } => Ok(StepResult::Continue(Control::BlockStep {
                stmts,
                index: next_index,
                result,
                env,
                dict_env,
            })),
            Frame::MatchScrutinee {
                arms,
                env,
                dict_env,
            } => Ok(StepResult::Continue(Control::MatchArm {
                scrutinee: value,
                arms,
                arm_idx: 0,
                outer_env: env,
                dict_env,
            })),
            Frame::MatchGuard {
                scrutinee,
                arms,
                arm_idx,
                arm_env,
                outer_env,
                dict_env,
            } => match value {
                Value::Bool(true) => Ok(StepResult::Continue(Control::Eval {
                    expr: &arms[arm_idx].body,
                    env: arm_env,
                    dict_env,
                })),
                Value::Bool(false) => Ok(StepResult::Continue(Control::MatchArm {
                    scrutinee,
                    arms,
                    arm_idx: arm_idx + 1,
                    outer_env,
                    dict_env,
                })),
                _ => Err(StopReason::UnsafeContext),
            },
        }
    }

    fn step_unwrap(&mut self, value: Value<'a>) -> Result<StepResult<'a>, StopReason> {
        let Value::Con { ctor, args, dicts } = value else {
            return Err(StopReason::UnsafeContext);
        };
        if !dicts.is_empty() {
            return Err(StopReason::UnsafeContext);
        }
        if self.lang.is(Lang::Some, &ctor.module, &ctor.name) && args.len() == 1 {
            return Ok(StepResult::Continue(Control::Return(args[0].clone())));
        }
        if self.lang.is(Lang::None, &ctor.module, &ctor.name) && args.is_empty() {
            // Unwind to the enclosing function boundary; if none exists, `o?`
            // escapes the expression being evaluated and must remain residual.
            while let Some(frame) = self.frames.pop() {
                match frame {
                    Frame::CallBoundary => {
                        return Ok(StepResult::Continue(Control::Return(Value::Con {
                            ctor,
                            args,
                            dicts,
                        })));
                    }
                    Frame::LeaveGlobal => {
                        self.active_globals.pop();
                    }
                    _ => {}
                }
            }
            return Err(StopReason::UnsafeContext);
        }
        Err(StopReason::UnsafeContext)
    }
}
