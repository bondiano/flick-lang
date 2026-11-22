//! Explicit-frame step machine for bounded concrete evaluation of portable Flick IR.
//!
//! All control flow uses an explicit heap `Vec<Frame>` rather than host Rust
//! recursion, enforcing `max_frame_depth` before every push so deep terms and
//! deep recursion hit the exact same deterministic limit on native and WASM.

mod apply;
mod block;
mod call;
mod frame;
mod ops;
mod ret;

use std::rc::Rc;

use flick_ir::{BinOp, Expr, ExprKind, Stage};

use super::value::{CLOSURE_BYTES, Closure, DictEnv, DictValue, Env, SLOT_BYTES, Value};
use super::{DefTable, EvalBudget, EvalOutcome, PrimitiveEval, StopReason};
use crate::Known;
use frame::{Control, Frame, StepResult};
use ops::fields_bytes;

pub(super) struct Machine<'a, 'b> {
    defs: &'a DefTable,
    lang: &'a Known<'a>,
    primitives: &'b dyn PrimitiveEval,
    budget: &'b mut EvalBudget,
    frames: Vec<Frame<'a>>,
    active_globals: Vec<(&'a str, &'a str)>,
}

impl<'a, 'b> Machine<'a, 'b> {
    pub(super) fn new(
        defs: &'a DefTable,
        lang: &'a Known<'a>,
        primitives: &'b dyn PrimitiveEval,
        budget: &'b mut EvalBudget,
    ) -> Self {
        Machine {
            defs,
            lang,
            primitives,
            budget,
            frames: Vec::new(),
            active_globals: Vec::new(),
        }
    }

    fn push_frame(&mut self, frame: Frame<'a>) -> Result<(), StopReason> {
        self.budget.check_depth(self.frames.len())?;
        self.frames.push(frame);
        Ok(())
    }

    pub(super) fn run(
        mut self,
        expr: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    ) -> EvalOutcome<'a> {
        let mut control = Control::Eval {
            expr,
            env,
            dict_env,
        };
        loop {
            match self.step(control) {
                Ok(StepResult::Continue(next)) => control = next,
                Ok(StepResult::Done(val)) => return EvalOutcome::Known(val),
                Err(reason) => return EvalOutcome::Stopped(reason),
            }
        }
    }

    fn step(&mut self, control: Control<'a>) -> Result<StepResult<'a>, StopReason> {
        self.budget.charge_steps(1)?;
        match control {
            Control::Eval {
                expr,
                env,
                dict_env,
            } => self.step_eval(expr, env, dict_env),
            Control::Return(val) => self.step_return(val),
            Control::BlockStep {
                stmts,
                index,
                result,
                env,
                dict_env,
            } => self.step_block(stmts, index, result, env, dict_env),
            Control::MatchArm {
                scrutinee,
                arms,
                arm_idx,
                outer_env,
                dict_env,
            } => self.step_match_arm(scrutinee, arms, arm_idx, outer_env, dict_env),
        }
    }

    #[allow(clippy::too_many_lines, reason = "exhaustive IR expression dispatch")]
    fn step_eval(
        &mut self,
        expr: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    ) -> Result<StepResult<'a>, StopReason> {
        match &expr.kind {
            ExprKind::Lit(lit) => Ok(StepResult::Continue(Control::Return(self.eval_lit(lit)?))),
            ExprKind::Local(name) => match env.get(name, self.budget)? {
                Some(val) => Ok(StepResult::Continue(Control::Return(val.clone()))),
                None => Err(StopReason::DynamicRead),
            },
            ExprKind::Var(_) => Err(StopReason::DynamicRead),
            ExprKind::Global { module, name } => self.step_global(module, name),
            ExprKind::Method { dict, name } => self.step_method(dict, name, &env, &dict_env),
            ExprKind::Con(ctor) => {
                self.budget.charge_bytes(SLOT_BYTES)?;
                Ok(StepResult::Continue(Control::Return(Value::Con {
                    ctor: Rc::new(ctor.clone()),
                    args: Rc::from([]),
                    dicts: Rc::from([]),
                })))
            }
            ExprKind::Dicts(inner, dicts) => self.step_dicts(inner, dicts, env, dict_env),
            ExprKind::Abstract(names, inner) => {
                self.budget.charge_bytes(CLOSURE_BYTES)?;
                let closure = Value::Closure(Rc::new(Closure {
                    env,
                    dict_env,
                    dict_params: names,
                    params: &[],
                    body: inner,
                    rec_group: None,
                }));
                Ok(StepResult::Continue(Control::Return(closure)))
            }
            ExprKind::Lambda { params, body } => {
                self.budget.charge_bytes(CLOSURE_BYTES)?;
                let closure = Value::Closure(Rc::new(Closure {
                    env,
                    dict_env,
                    dict_params: &[],
                    params,
                    body,
                    rec_group: None,
                }));
                Ok(StepResult::Continue(Control::Return(closure)))
            }
            ExprKind::App {
                func,
                arg,
                stage,
                arg_first,
            } => {
                if *stage != Stage::Pure {
                    return Err(StopReason::UnsafeContext);
                }
                if *arg_first {
                    self.push_frame(Frame::AppArgFirst {
                        func,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: arg,
                        env,
                        dict_env,
                    }))
                } else {
                    self.push_frame(Frame::AppFunc {
                        arg,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: func,
                        env,
                        dict_env,
                    }))
                }
            }
            ExprKind::Block(stmts, result) => {
                if stmts.is_empty() {
                    Ok(StepResult::Continue(Control::Eval {
                        expr: result,
                        env,
                        dict_env,
                    }))
                } else {
                    Ok(StepResult::Continue(Control::BlockStep {
                        stmts,
                        index: 0,
                        result,
                        env,
                        dict_env,
                    }))
                }
            }
            ExprKind::If(cond, then_expr, else_expr) => {
                self.push_frame(Frame::If {
                    then_expr,
                    else_expr,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: cond,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Match(scrutinee, arms) => {
                self.push_frame(Frame::MatchScrutinee {
                    arms,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: scrutinee,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Tuple(items) => {
                let bytes = u32::try_from(items.len())
                    .unwrap_or(u32::MAX)
                    .saturating_mul(SLOT_BYTES);
                self.budget.charge_bytes(bytes)?;
                if items.is_empty() {
                    Ok(StepResult::Continue(Control::Return(Value::Tuple(
                        Rc::from([]),
                    ))))
                } else {
                    self.push_frame(Frame::Tuple {
                        done: Vec::with_capacity(items.len()),
                        rest: items,
                        index: 1,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: &items[0],
                        env,
                        dict_env,
                    }))
                }
            }
            ExprKind::List(items) => {
                let bytes = u32::try_from(items.len())
                    .unwrap_or(u32::MAX)
                    .saturating_mul(SLOT_BYTES);
                self.budget.charge_bytes(bytes)?;
                if items.is_empty() {
                    Ok(StepResult::Continue(Control::Return(Value::List(
                        Rc::from([]),
                    ))))
                } else {
                    self.push_frame(Frame::List {
                        done: Vec::with_capacity(items.len()),
                        rest: items,
                        index: 1,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: &items[0],
                        env,
                        dict_env,
                    }))
                }
            }
            ExprKind::Record(fields) | ExprKind::Dictionary(fields) => {
                let as_dict = matches!(expr.kind, ExprKind::Dictionary(_));
                let bytes = fields_bytes(fields);
                self.budget.charge_bytes(bytes)?;
                if fields.is_empty() {
                    let value = if as_dict {
                        Value::Dict(DictValue::Record(Rc::from([])))
                    } else {
                        Value::Record(Rc::from([]))
                    };
                    Ok(StepResult::Continue(Control::Return(value)))
                } else {
                    self.push_frame(Frame::Record {
                        done: Vec::with_capacity(fields.len()),
                        rest: fields,
                        index: 1,
                        as_dict,
                        env: env.clone(),
                        dict_env: dict_env.clone(),
                    })?;
                    Ok(StepResult::Continue(Control::Eval {
                        expr: &fields[0].1,
                        env,
                        dict_env,
                    }))
                }
            }
            ExprKind::Update(base, fields) => {
                self.push_frame(Frame::UpdateBase {
                    fields,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: base,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Field(record, label) => {
                self.push_frame(Frame::Field { label })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: record,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs) => {
                self.push_frame(Frame::ShortCircuit {
                    op: *op,
                    rhs,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: lhs,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Binary(op, lhs, rhs) => {
                self.push_frame(Frame::BinaryLhs {
                    op: *op,
                    rhs,
                    env: env.clone(),
                    dict_env: dict_env.clone(),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: lhs,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Neg(num, inner) => {
                self.push_frame(Frame::Neg(*num))?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: inner,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Not(inner) => {
                self.push_frame(Frame::Not)?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: inner,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Ignore(inner) => {
                self.push_frame(Frame::Ignore)?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: inner,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Unwrap(inner) => {
                self.push_frame(Frame::Unwrap)?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: inner,
                    env,
                    dict_env,
                }))
            }
            ExprKind::Label(_) => Err(StopReason::UnknownOp),
            ExprKind::Perform { .. }
            | ExprKind::ServiceTag(_)
            | ExprKind::Generate { .. }
            | ExprKind::Each { .. }
            | ExprKind::Yield(_)
            | ExprKind::Loop { .. }
            | ExprKind::ControlLoop { .. }
            | ExprKind::LoopExit { .. }
            | ExprKind::Await(_)
            | ExprKind::Task(_)
            | ExprKind::Within(..)
            | ExprKind::ReadOwnerEvidence
            | ExprKind::WithOwnerEvidence(..)
            | ExprKind::Handle(_)
            | ExprKind::Lift(..) => Err(StopReason::UnsafeContext),
        }
    }
}
