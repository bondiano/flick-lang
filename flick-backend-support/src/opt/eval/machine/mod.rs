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

pub(super) use block::{bind_poly_fun, bind_rec_group};

use std::rc::Rc;

use flick_ir::{Expr, ExprKind, Param, Stage, Stmt};

use super::value::{
    CLOSURE_BYTES, Callable, DictEnv, DictValue, Env, SLOT_BYTES, Value, empty_dicts, empty_fields,
    empty_values, rc_str,
};
use super::{DefTable, EvalBudget, EvalOutcome, PrimitiveEval, StopReason};
use crate::Known;
use frame::{BlockKind, Control, Frame, FrameKind, RecordTarget, UnaryOp};
use ops::fields_bytes;

pub(super) struct Machine<'a, 'b> {
    defs: &'a DefTable,
    lang: &'a Known<'a>,
    primitives: &'b dyn PrimitiveEval,
    budget: &'b mut EvalBudget,
    frames: Vec<Frame<'a>>,
    active_globals: Vec<&'a Expr>,
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

    #[inline(never)]
    fn push_frame(&mut self, kind: FrameKind<'a>, env: Env<'a>) -> Result<(), StopReason> {
        self.budget.check_depth(self.frames.len())?;
        self.frames.push(Frame { kind, env });
        Ok(())
    }

    #[inline(never)]
    fn make_closure(
        &mut self,
        env: Env<'a>,
        dict_params: &'a [String],
        params: &'a [Param],
        body: &'a Expr,
    ) -> Result<Control<'a>, StopReason> {
        self.budget.charge_bytes(CLOSURE_BYTES)?;
        Ok(Control::Return(Value::Callable(Rc::new(
            Callable::Closure {
                env,
                dict_params,
                params,
                body,
                rec_group: &[],
            },
        ))))
    }

    #[inline(never)]
    fn make_primitive(
        &mut self,
        module: &str,
        name: &str,
        arity: usize,
        dicts: Rc<Vec<DictValue<'a>>>,
    ) -> Result<Control<'a>, StopReason> {
        let bytes = CLOSURE_BYTES
            .saturating_add(u32::try_from(module.len()).unwrap_or(u32::MAX))
            .saturating_add(u32::try_from(name.len()).unwrap_or(u32::MAX));
        self.budget.charge_bytes(bytes)?;
        Ok(Control::Return(Value::Callable(Rc::new(
            Callable::Primitive {
                module: rc_str(module),
                name: rc_str(name),
                arity,
                dicts,
                args: empty_values(),
            },
        ))))
    }

    fn closure_env(&mut self, env: &Env<'a>, rec_group: &'a [Stmt]) -> Result<Env<'a>, StopReason> {
        if rec_group.is_empty() {
            return Ok(env.clone());
        }
        if rec_group.len() > 1 {
            let extra = u32::try_from(rec_group.len()).unwrap_or(u32::MAX);
            self.budget.charge_steps(extra)?;
        }
        Ok(env.with_rec_group(rec_group))
    }

    #[inline(never)]
    fn bind_dict_params(
        &mut self,
        env: &Env<'a>,
        rec_group: &'a [Stmt],
        params: &[String],
        dicts: &[DictValue<'a>],
        base_bytes: u32,
    ) -> Result<Env<'a>, StopReason> {
        let mut bytes = base_bytes;
        for param in params {
            bytes = bytes.saturating_add(u32::try_from(param.len()).unwrap_or(u32::MAX));
        }
        self.budget.charge_bytes(bytes)?;
        let mut next = self.closure_env(env, rec_group)?;
        let n = params.len().min(dicts.len());
        let mut i = 0;
        while i < n {
            next = next.with_dict(rc_str(&params[i]), dicts[i].clone());
            i += 1;
        }
        Ok(next)
    }

    pub(super) fn run(
        mut self,
        expr: &'a Expr,
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    ) -> EvalOutcome<'a> {
        let env = if dict_env.is_empty() {
            env
        } else {
            env.merge_dicts(dict_env)
        };
        let mut control = Control::Eval { expr, env };
        loop {
            if let Err(reason) = self.budget.charge_steps(1) {
                return EvalOutcome::Stopped(reason);
            }
            let next = match control {
                Control::Eval { expr, env } => self.step_eval(expr, env),
                Control::Return(val) => match self.frames.pop() {
                    Some(frame) => self.step_return(frame, val),
                    None => return EvalOutcome::Known(val),
                },
            };
            match next {
                Ok(c) => control = c,
                Err(reason) => return EvalOutcome::Stopped(reason),
            }
        }
    }

    #[allow(clippy::too_many_lines, reason = "exhaustive IR expression dispatch")]
    fn step_eval(&mut self, expr: &'a Expr, env: Env<'a>) -> Result<Control<'a>, StopReason> {
        let (frame, next, keep_env) = match &expr.kind {
            ExprKind::Lit(lit) => return Ok(Control::Return(self.eval_lit(lit)?)),
            ExprKind::Local(name) => {
                return match env.get(name, self.budget)? {
                    Some(val) => Ok(Control::Return(val.clone())),
                    None => Err(StopReason::DynamicRead),
                };
            }
            ExprKind::Var(_) => return Err(StopReason::DynamicRead),
            ExprKind::Global { module, name, .. } => return self.step_global(module, name),
            ExprKind::Method { dict, name, .. } => return self.step_method(dict, name, &env),
            ExprKind::Con(ctor) => {
                self.budget.charge_bytes(SLOT_BYTES)?;
                return Ok(Control::Return(Value::Con {
                    ctor: Rc::new(ctor.clone()),
                    args: empty_values(),
                    dicts: empty_dicts(),
                }));
            }
            ExprKind::Dicts(inner, dicts) => return self.step_dicts(inner, dicts, env),
            ExprKind::Abstract(names, inner) => return self.make_closure(env, names, &[], inner),
            ExprKind::Lambda { params, body } => return self.make_closure(env, &[], params, body),
            ExprKind::Block(stmts, result) => return self.step_block(stmts, result, env),
            ExprKind::App {
                func,
                arg,
                stage,
                arg_first,
            } => {
                if *stage != Stage::Pure {
                    return Err(StopReason::UnsafeContext);
                }
                let (first, second) = if *arg_first { (arg, func) } else { (func, arg) };
                (
                    FrameKind::AppSecond {
                        second,
                        arg_first: *arg_first,
                    },
                    first.as_ref(),
                    true,
                )
            }
            ExprKind::If(cond, then_expr, else_expr) => (
                FrameKind::If {
                    then_expr,
                    else_expr,
                },
                cond.as_ref(),
                true,
            ),
            ExprKind::Match(scrutinee, arms) => {
                (FrameKind::MatchScrutinee { arms }, scrutinee.as_ref(), true)
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let is_list = matches!(expr.kind, ExprKind::List(_));
                let bytes = u32::try_from(items.len())
                    .unwrap_or(u32::MAX)
                    .saturating_mul(SLOT_BYTES);
                self.budget.charge_bytes(bytes)?;
                let Some((first, rest)) = items.split_first() else {
                    let empty = if is_list {
                        Value::List(empty_values())
                    } else {
                        Value::Tuple(empty_values())
                    };
                    return Ok(Control::Return(empty));
                };
                (
                    FrameKind::Seq {
                        is_list,
                        done: Vec::new(),
                        rest,
                    },
                    first,
                    true,
                )
            }
            ExprKind::Record(fields) | ExprKind::Dictionary(fields) => {
                let as_dict = matches!(expr.kind, ExprKind::Dictionary(_));
                self.budget.charge_bytes(fields_bytes(fields))?;
                let Some(((label, first), rest)) = fields.split_first() else {
                    let value = if as_dict {
                        Value::Dict(Rc::new(DictValue::Record(empty_fields())))
                    } else {
                        Value::Record(empty_fields())
                    };
                    return Ok(Control::Return(value));
                };
                let target = if as_dict {
                    RecordTarget::Dict
                } else {
                    RecordTarget::Record
                };
                (
                    FrameKind::Record {
                        label,
                        done: Vec::new(),
                        rest,
                        target,
                    },
                    first,
                    true,
                )
            }
            ExprKind::Update(base, fields) => {
                (FrameKind::UpdateBase { fields }, base.as_ref(), true)
            }
            ExprKind::Binary(op, lhs, rhs) => {
                (FrameKind::BinaryLhs { op: *op, rhs }, lhs.as_ref(), true)
            }
            ExprKind::Field(record, label) => (FrameKind::Field { label }, record.as_ref(), false),
            ExprKind::Neg(num, inner) => {
                (FrameKind::Unary(UnaryOp::Neg(*num)), inner.as_ref(), false)
            }
            ExprKind::Not(inner) => (FrameKind::Unary(UnaryOp::Not), inner.as_ref(), false),
            ExprKind::Ignore(inner) => (FrameKind::Unary(UnaryOp::Ignore), inner.as_ref(), false),
            ExprKind::Unwrap(inner) => (FrameKind::Unary(UnaryOp::Unwrap), inner.as_ref(), false),
            ExprKind::Label(_) => return Err(StopReason::UnknownOp),
            _ => return Err(StopReason::UnsafeContext),
        };
        let frame_env = if keep_env {
            env.clone()
        } else {
            Env::default()
        };
        self.push_frame(frame, frame_env)?;
        Ok(Control::Eval { expr: next, env })
    }
}
