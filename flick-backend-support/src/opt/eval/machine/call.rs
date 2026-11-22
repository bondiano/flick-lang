//! Global lookup, dictionary application, and class method resolution.

use std::rc::Rc;

use flick_ir::{Dict, Expr, ExprKind, Lang};

use super::apply::is_builtin_method;
use super::{Control, Frame, Machine, StepResult};
use crate::opt::eval::StopReason;
use crate::opt::eval::value::{CLOSURE_BYTES, Closure, DictEnv, DictValue, Env, SLOT_BYTES, Value};
use crate::opt::specialize;

impl<'a> Machine<'a, '_> {
    pub(super) fn step_global(
        &mut self,
        module: &'a str,
        name: &'a str,
    ) -> Result<StepResult<'a>, StopReason> {
        if self.lang.is(Lang::FloatInfinity, module, name) {
            return Ok(StepResult::Continue(Control::Return(Value::Float(
                f64::INFINITY,
            ))));
        }
        if self.lang.is(Lang::FloatNan, module, name) {
            return Ok(StepResult::Continue(Control::Return(Value::Float(
                f64::NAN,
            ))));
        }
        if let Some((dicts, body)) = self.defs.value(module, name)
            && dicts.is_empty()
        {
            if let ExprKind::Lambda { params, body } = &body.kind {
                self.budget.charge_bytes(CLOSURE_BYTES)?;
                return Ok(StepResult::Continue(Control::Return(Value::Closure(
                    Rc::new(Closure {
                        env: Env::default(),
                        dict_env: DictEnv::default(),
                        dict_params: &[],
                        params,
                        body,
                        rec_group: None,
                    }),
                ))));
            }
            if self.active_globals.contains(&(module, name)) {
                return Err(StopReason::StepBudget);
            }
            self.push_frame(Frame::LeaveGlobal)?;
            self.active_globals.push((module, name));
            return Ok(StepResult::Continue(Control::Eval {
                expr: body,
                env: Env::default(),
                dict_env: DictEnv::default(),
            }));
        }
        if let Some(arity) = self.primitives.arity(module, name, self.lang) {
            self.budget.charge_bytes(CLOSURE_BYTES)?;
            return Ok(StepResult::Continue(Control::Return(Value::Primitive {
                module: Rc::from(module),
                name: Rc::from(name),
                arity,
                dicts: Rc::from([]),
                args: Rc::from([]),
            })));
        }
        Ok(StepResult::Continue(Control::Return(Value::OpaqueGlobal {
            module: Rc::from(module),
            name: Rc::from(name),
        })))
    }

    pub(super) fn step_dicts(
        &mut self,
        inner: &'a Expr,
        dicts: &'a [Dict],
        env: Env<'a>,
        dict_env: DictEnv<'a>,
    ) -> Result<StepResult<'a>, StopReason> {
        let mut resolved = Vec::with_capacity(dicts.len());
        for d in dicts {
            resolved.push(self.resolve_dict(d, &env, &dict_env, 0)?);
        }
        match &inner.kind {
            ExprKind::Con(ctor) if ctor.dicts == resolved.len() => {
                let bytes = u32::try_from(resolved.len())
                    .unwrap_or(u32::MAX)
                    .saturating_mul(SLOT_BYTES);
                self.budget.charge_bytes(bytes)?;
                Ok(StepResult::Continue(Control::Return(Value::Con {
                    ctor: Rc::new(ctor.clone()),
                    args: Rc::from([]),
                    dicts: Rc::from(resolved),
                })))
            }
            ExprKind::Global { module, name } => {
                if let Some((dict_params, body)) = self.defs.value(module, name)
                    && dict_params.len() == resolved.len()
                {
                    let mut next_dicts = DictEnv::default();
                    for (param, dict) in dict_params.iter().zip(resolved) {
                        next_dicts = next_dicts.with(Rc::from(param.as_str()), dict);
                    }
                    if let ExprKind::Lambda { params, body } = &body.kind {
                        self.budget.charge_bytes(CLOSURE_BYTES)?;
                        return Ok(StepResult::Continue(Control::Return(Value::Closure(
                            Rc::new(Closure {
                                env: Env::default(),
                                dict_env: next_dicts,
                                dict_params: &[],
                                params,
                                body,
                                rec_group: None,
                            }),
                        ))));
                    }
                    if self
                        .active_globals
                        .contains(&(module.as_str(), name.as_str()))
                    {
                        return Err(StopReason::StepBudget);
                    }
                    self.push_frame(Frame::LeaveGlobal)?;
                    self.active_globals.push((module, name));
                    return Ok(StepResult::Continue(Control::Eval {
                        expr: body,
                        env: Env::default(),
                        dict_env: next_dicts,
                    }));
                }
                if let Some(arity) = self.primitives.arity(module, name, self.lang) {
                    self.budget.charge_bytes(CLOSURE_BYTES)?;
                    return Ok(StepResult::Continue(Control::Return(Value::Primitive {
                        module: Rc::from(module.as_str()),
                        name: Rc::from(name.as_str()),
                        arity,
                        dicts: Rc::from(resolved),
                        args: Rc::from([]),
                    })));
                }
                Err(StopReason::UnknownOp)
            }
            _ => {
                self.push_frame(Frame::ApplyDicts {
                    dicts: Rc::from(resolved),
                })?;
                Ok(StepResult::Continue(Control::Eval {
                    expr: inner,
                    env,
                    dict_env,
                }))
            }
        }
    }

    pub(super) fn step_method(
        &mut self,
        dict: &'a Dict,
        method: &'a str,
        env: &Env<'a>,
        dict_env: &DictEnv<'a>,
    ) -> Result<StepResult<'a>, StopReason> {
        let resolved = self.resolve_dict(dict, env, dict_env, 0)?;
        self.enter_method(resolved, method, 0)
    }

    fn enter_method(
        &mut self,
        dict: DictValue<'a>,
        method: &'a str,
        depth: usize,
    ) -> Result<StepResult<'a>, StopReason> {
        self.budget.check_depth(self.frames.len() + depth)?;
        match dict {
            DictValue::Record(fields) => {
                let scan_steps = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan_steps)?;
                let (_, val) = fields
                    .iter()
                    .find(|(k, _)| &**k == method)
                    .ok_or(StopReason::UnknownOp)?;
                Ok(StepResult::Continue(Control::Return(val.clone())))
            }
            DictValue::Super { dict: inner, class } => match *inner {
                DictValue::Record(fields) => {
                    let super_key = format!("${class}");
                    let scan_steps = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                    self.budget.charge_steps(scan_steps)?;
                    let (_, val) = fields
                        .iter()
                        .find(|(k, _)| &**k == super_key.as_str())
                        .ok_or(StopReason::UnknownOp)?;
                    match val {
                        Value::Dict(super_dict) => {
                            self.enter_method(super_dict.clone(), method, depth + 1)
                        }
                        _ => Err(StopReason::UnknownOp),
                    }
                }
                _ => Err(StopReason::UnknownOp),
            },
            DictValue::Instance {
                module,
                name: instance,
                args,
            } => {
                if is_builtin_method(&module, &instance, args.is_empty(), method, self.lang) {
                    self.budget.charge_bytes(CLOSURE_BYTES)?;
                    return Ok(StepResult::Continue(Control::Return(Value::Method {
                        dict: DictValue::Instance {
                            module,
                            name: instance,
                            args,
                        },
                        name: Rc::from(method),
                        args: Rc::from([]),
                    })));
                }
                let Some((context, body)) = self.defs.method(&module, &instance, method) else {
                    return Err(StopReason::UnknownOp);
                };
                if matches!(body.kind, ExprKind::Abstract(..)) || context.len() != args.len() {
                    return Err(StopReason::UnknownOp);
                }
                let mut next_dicts = DictEnv::default();
                for (param, arg) in context.iter().zip(args.iter()) {
                    next_dicts = next_dicts.with(Rc::from(param.as_str()), arg.clone());
                }
                if let ExprKind::Lambda { params, body } = &body.kind {
                    self.budget.charge_bytes(CLOSURE_BYTES)?;
                    return Ok(StepResult::Continue(Control::Return(Value::Closure(
                        Rc::new(Closure {
                            env: Env::default(),
                            dict_env: next_dicts,
                            dict_params: &[],
                            params,
                            body,
                            rec_group: None,
                        }),
                    ))));
                }
                if !matches!(self.frames.last(), Some(Frame::CallBoundary)) {
                    self.push_frame(Frame::CallBoundary)?;
                }
                Ok(StepResult::Continue(Control::Eval {
                    expr: body,
                    env: Env::default(),
                    dict_env: next_dicts,
                }))
            }
        }
    }

    fn resolve_dict(
        &mut self,
        dict: &Dict,
        env: &Env<'a>,
        dict_env: &DictEnv<'a>,
        depth: usize,
    ) -> Result<DictValue<'a>, StopReason> {
        self.budget.check_depth(self.frames.len() + depth)?;
        self.budget.charge_steps(1)?;
        match dict {
            Dict::Instance { module, name, args } => {
                let mut resolved_args = Vec::with_capacity(args.len());
                for arg in args {
                    resolved_args.push(self.resolve_dict(arg, env, dict_env, depth + 1)?);
                }
                Ok(DictValue::Instance {
                    module: Rc::from(module.as_str()),
                    name: Rc::from(name.as_str()),
                    args: Rc::from(resolved_args),
                })
            }
            Dict::Given(name) => dict_env
                .get(name, self.budget)?
                .cloned()
                .ok_or(StopReason::DynamicRead),
            Dict::Field(local) => match env.get(local, self.budget)? {
                Some(Value::Dict(d)) => Ok(d.clone()),
                Some(_) => Err(StopReason::UnsafeContext),
                None => Err(StopReason::DynamicRead),
            },
            Dict::Super { dict: inner, class } => {
                let resolved = self.resolve_dict(inner, env, dict_env, depth + 1)?;
                if let Some(ir_inner) = resolved.to_ir_dict() {
                    let super_ir = Dict::Super {
                        dict: Box::new(ir_inner),
                        class: class.clone(),
                    };
                    if let Some(Dict::Instance { module, name, args }) =
                        specialize::known(&super_ir, self.lang)
                        && args.is_empty()
                    {
                        return Ok(DictValue::Instance {
                            module: Rc::from(module.as_str()),
                            name: Rc::from(name.as_str()),
                            args: Rc::from([]),
                        });
                    }
                }
                Ok(DictValue::Super {
                    dict: Box::new(resolved),
                    class: Rc::from(class.as_str()),
                })
            }
            Dict::Opened { .. }
            | Dict::Value { .. }
            | Dict::Structural { .. }
            | Dict::Primitive { .. }
            | Dict::Site { .. }
            | Dict::Error => Err(StopReason::UnknownOp),
        }
    }
}
