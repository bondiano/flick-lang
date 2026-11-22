//! Continuation frame return dispatch and `Option` `?` unwinding.

use std::rc::Rc;

use flick_ir::{BinOp, Lang};

use super::ops::fields_bytes;
use super::{BlockKind, Control, Frame, FrameKind, Machine, RecordTarget, UnaryOp};
use crate::opt::eval::StopReason;
use crate::opt::eval::pattern::{PatMatch, match_pat};
use crate::opt::eval::value::{Callable, DictValue, Env, FIELD_BYTES, Value, find_field, rc_str};

impl<'a> Machine<'a, '_> {
    #[allow(
        clippy::too_many_lines,
        reason = "exhaustive continuation frame dispatch"
    )]
    pub(super) fn step_return(
        &mut self,
        frame: Frame<'a>,
        value: Value<'a>,
    ) -> Result<Control<'a>, StopReason> {
        let Frame { kind, mut env } = frame;
        match kind {
            FrameKind::CallBoundary => Ok(Control::Return(value)),
            FrameKind::LeaveGlobal => {
                self.active_globals.pop();
                Ok(Control::Return(value))
            }
            FrameKind::AppSecond { second, arg_first } => {
                self.push_frame(
                    FrameKind::AppReady {
                        first: value,
                        arg_first,
                    },
                    Env::default(),
                )?;
                Ok(Control::Eval { expr: second, env })
            }
            FrameKind::AppReady { first, arg_first } => {
                if arg_first {
                    self.apply(value, first)
                } else {
                    self.apply(first, value)
                }
            }
            FrameKind::ApplyDicts { dicts } => match value {
                Value::Callable(callable) => match &*callable {
                    Callable::Closure {
                        env: c_env,
                        dict_params,
                        params,
                        body,
                        rec_group,
                    } if dict_params.len() == dicts.len() => {
                        let next_env =
                            self.bind_dict_params(c_env, rec_group, dict_params, &dicts, 0)?;
                        if params.is_empty() {
                            Ok(Control::Eval {
                                expr: body,
                                env: next_env,
                            })
                        } else {
                            self.make_closure(next_env, &[], params, body)
                        }
                    }
                    _ => Err(StopReason::UnknownOp),
                },
                _ => Err(StopReason::UnknownOp),
            },
            FrameKind::If {
                then_expr,
                else_expr,
            } => match value {
                Value::Bool(flag) => Ok(Control::Eval {
                    expr: if flag { then_expr } else { else_expr },
                    env,
                }),
                _ => Err(StopReason::UnsafeContext),
            },
            FrameKind::BinaryLhs { op, rhs } => {
                if matches!(op, BinOp::And | BinOp::Or) {
                    return match (op, value) {
                        (BinOp::And, Value::Bool(false)) => Ok(Control::Return(Value::Bool(false))),
                        (BinOp::Or, Value::Bool(true)) => Ok(Control::Return(Value::Bool(true))),
                        (BinOp::And, Value::Bool(true)) | (BinOp::Or, Value::Bool(false)) => {
                            Ok(Control::Eval { expr: rhs, env })
                        }
                        _ => Err(StopReason::UnsafeContext),
                    };
                }
                self.push_frame(FrameKind::BinaryRhs { op, lhs_val: value }, Env::default())?;
                Ok(Control::Eval { expr: rhs, env })
            }
            FrameKind::BinaryRhs { op, lhs_val } => {
                Ok(Control::Return(self.eval_binary(op, &lhs_val, &value)?))
            }
            FrameKind::Unary(op) => match op {
                UnaryOp::Neg(num) => Ok(Control::Return(self.eval_neg(num, value)?)),
                UnaryOp::Not => match value {
                    Value::Bool(b) => Ok(Control::Return(Value::Bool(!b))),
                    _ => Err(StopReason::UnsafeContext),
                },
                UnaryOp::Ignore => Ok(Control::Return(Value::Unit)),
                UnaryOp::Unwrap => self.step_unwrap(value),
            },
            FrameKind::Field { label } => match value {
                Value::Record(fields) => {
                    let scan_steps = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                    self.budget.charge_steps(scan_steps)?;
                    let val = find_field(&fields, label).ok_or(StopReason::UnsafeContext)?;
                    Ok(Control::Return(val.clone()))
                }
                _ => Err(StopReason::UnsafeContext),
            },
            FrameKind::Seq {
                is_list,
                mut done,
                rest,
            } => {
                done.push(value);
                if let Some((next, tail)) = rest.split_first() {
                    self.push_frame(
                        FrameKind::Seq {
                            is_list,
                            done,
                            rest: tail,
                        },
                        env.clone(),
                    )?;
                    Ok(Control::Eval { expr: next, env })
                } else {
                    let seq = if is_list {
                        Value::List(Rc::new(done))
                    } else {
                        Value::Tuple(Rc::new(done))
                    };
                    Ok(Control::Return(seq))
                }
            }
            FrameKind::Record {
                label,
                mut done,
                rest,
                target,
            } => {
                done.push((rc_str(label), value));
                if let Some(((next_label, next_expr), tail)) = rest.split_first() {
                    self.push_frame(
                        FrameKind::Record {
                            label: next_label,
                            done,
                            rest: tail,
                            target,
                        },
                        env.clone(),
                    )?;
                    Ok(Control::Eval {
                        expr: next_expr,
                        env,
                    })
                } else {
                    let out = match target {
                        RecordTarget::Record => Value::Record(Rc::new(done)),
                        RecordTarget::Dict => {
                            Value::Dict(Rc::new(DictValue::Record(Rc::new(done))))
                        }
                        RecordTarget::Update(base) => {
                            let base_bytes = u32::try_from(base.len())
                                .unwrap_or(u32::MAX)
                                .saturating_mul(FIELD_BYTES);
                            self.budget.charge_bytes(base_bytes)?;
                            let scan_steps = u32::try_from(base.len().saturating_mul(done.len()))
                                .unwrap_or(u32::MAX)
                                / 4;
                            self.budget.charge_steps(scan_steps)?;
                            let mut merged = Vec::new();
                            for (k, v) in base.iter() {
                                merged.push((Rc::clone(k), v.clone()));
                            }
                            done.reverse();
                            while let Some((label, val)) = done.pop() {
                                if let Some(slot) = merged.iter_mut().find(|(k, _)| *k == label) {
                                    slot.1 = val;
                                } else {
                                    merged.push((label, val));
                                }
                            }
                            Value::Record(Rc::new(merged))
                        }
                    };
                    Ok(Control::Return(out))
                }
            }
            FrameKind::UpdateBase { fields } => {
                let Value::Record(base) = value else {
                    return Err(StopReason::UnsafeContext);
                };
                let Some(((label, first), rest)) = fields.split_first() else {
                    return Ok(Control::Return(Value::Record(base)));
                };
                self.budget.charge_bytes(fields_bytes(fields))?;
                self.push_frame(
                    FrameKind::Record {
                        label,
                        done: Vec::new(),
                        rest,
                        target: RecordTarget::Update(base),
                    },
                    env.clone(),
                )?;
                Ok(Control::Eval { expr: first, env })
            }
            FrameKind::Block {
                kind,
                stmts,
                result,
            } => {
                match kind {
                    BlockKind::Let(pat) => match match_pat(pat, value, self.budget)? {
                        PatMatch::NoMatch => return Err(StopReason::PossibleTrap),
                        PatMatch::Matched(mut binds) => {
                            while let Some((name, val)) = binds.pop() {
                                env = env.with(name, val);
                            }
                        }
                    },
                    BlockKind::Fun(name) => {
                        env = env.with(rc_str(name), value);
                    }
                    BlockKind::Discard => {}
                }
                self.step_block(stmts, result, env)
            }
            FrameKind::MatchScrutinee { arms } => self.step_match_arm(value, arms, env),
            FrameKind::MatchGuard {
                scrutinee,
                body,
                rest,
                arm_env,
            } => match value {
                Value::Bool(true) => Ok(Control::Eval {
                    expr: body,
                    env: arm_env,
                }),
                Value::Bool(false) => self.step_match_arm(scrutinee, rest, env),
                _ => Err(StopReason::UnsafeContext),
            },
        }
    }

    fn step_unwrap(&mut self, value: Value<'a>) -> Result<Control<'a>, StopReason> {
        let Value::Con { ctor, args, dicts } = value else {
            return Err(StopReason::UnsafeContext);
        };
        if !dicts.is_empty() {
            return Err(StopReason::UnsafeContext);
        }
        if self.lang.is(Lang::Some, &ctor.module, &ctor.name) && args.len() == 1 {
            return Ok(Control::Return(args[0].clone()));
        }
        if self.lang.is(Lang::None, &ctor.module, &ctor.name) && args.is_empty() {
            // Unwind to the enclosing function boundary; if none exists, `o?`
            // escapes the expression being evaluated and must remain residual.
            while let Some(frame) = self.frames.pop() {
                match frame.kind {
                    FrameKind::CallBoundary => {
                        return Ok(Control::Return(Value::Con { ctor, args, dicts }));
                    }
                    FrameKind::LeaveGlobal => {
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
