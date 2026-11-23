//! Global lookup, dictionary application, and class method resolution.

use std::ptr;
use std::rc::Rc;

use flick_ir::{Dict, Expr, ExprKind, Lang};

use super::apply::is_builtin_method;
use super::{Control, FrameKind, Machine};
use crate::opt::eval::StopReason;
use crate::opt::eval::value::{
    CLOSURE_BYTES, Callable, DictValue, Env, SLOT_BYTES, Value, empty_dicts, empty_values,
    find_field, rc_str,
};

impl<'a> Machine<'a, '_> {
    pub(super) fn step_global(
        &mut self,
        module: &'a str,
        name: &'a str,
    ) -> Result<Control<'a>, StopReason> {
        if self.lang.is(Lang::FloatInfinity, module, name) {
            return Ok(Control::Return(Value::Float(f64::INFINITY)));
        }
        if self.lang.is(Lang::FloatNan, module, name) {
            return Ok(Control::Return(Value::Float(f64::NAN)));
        }
        if let Some((dicts, body)) = self.defs.value(module, name)
            && dicts.is_empty()
        {
            return self.enter_global(body, Env::default());
        }
        if let Some(arity) = self.primitives.arity(module, name, self.lang) {
            return self.make_primitive(module, name, arity, empty_dicts());
        }
        Ok(Control::Return(Value::OpaqueGlobal))
    }

    fn enter_global(&mut self, body: &'a Expr, env: Env<'a>) -> Result<Control<'a>, StopReason> {
        if let ExprKind::Lambda { params, body } = &body.kind {
            return self.make_closure(env, &[], params, body);
        }
        if self.active_globals.iter().any(|&p| ptr::eq(p, body)) {
            return Err(StopReason::StepBudget);
        }
        self.push_frame(FrameKind::LeaveGlobal, Env::default())?;
        self.active_globals.push(body);
        Ok(Control::Eval { expr: body, env })
    }

    pub(super) fn step_dicts(
        &mut self,
        inner: &'a Expr,
        dicts: &'a [Dict],
        env: Env<'a>,
    ) -> Result<Control<'a>, StopReason> {
        let vec_bytes = u32::try_from(dicts.len())
            .unwrap_or(u32::MAX)
            .saturating_mul(SLOT_BYTES);
        self.budget.charge_bytes(vec_bytes)?;
        let mut resolved = Vec::new();
        for d in dicts {
            resolved.push(self.resolve_dict(d, &env, 0)?);
        }
        match &inner.kind {
            ExprKind::Con(ctor) if ctor.dicts == resolved.len() => {
                self.budget.charge_bytes(SLOT_BYTES)?;
                Ok(Control::Return(Value::Con {
                    ctor: Rc::new(ctor.clone()),
                    args: empty_values(),
                    dicts: Rc::new(resolved),
                }))
            }
            ExprKind::Global { module, name, .. } => {
                if let Some((dict_params, body)) = self.defs.value(module, name)
                    && dict_params.len() == resolved.len()
                {
                    let next_env =
                        self.bind_dict_params(&Env::default(), &[], dict_params, &resolved, 0)?;
                    return self.enter_global(body, next_env);
                }
                if let Some(arity) = self.primitives.arity(module, name, self.lang) {
                    return self.make_primitive(module, name, arity, Rc::new(resolved));
                }
                Err(StopReason::UnknownOp)
            }
            _ => {
                self.push_frame(
                    FrameKind::ApplyDicts {
                        dicts: Rc::new(resolved),
                    },
                    Env::default(),
                )?;
                Ok(Control::Eval { expr: inner, env })
            }
        }
    }

    pub(super) fn step_method(
        &mut self,
        dict: &'a Dict,
        method: &'a str,
        env: &Env<'a>,
    ) -> Result<Control<'a>, StopReason> {
        let resolved = self.resolve_dict(dict, env, 0)?;
        self.enter_method(resolved, method, 0)
    }

    fn enter_method(
        &mut self,
        dict: DictValue<'a>,
        method: &'a str,
        depth: usize,
    ) -> Result<Control<'a>, StopReason> {
        self.budget.check_depth(self.frames.len() + depth)?;
        match dict {
            DictValue::Record(fields) => {
                let scan_steps = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                self.budget.charge_steps(scan_steps)?;
                let val = find_field(&fields, method).ok_or(StopReason::UnknownOp)?;
                Ok(Control::Return(val.clone()))
            }
            DictValue::Super { dict: inner, class } => match &*inner {
                DictValue::Record(fields) => {
                    let scan_steps = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                    self.budget.charge_steps(scan_steps)?;
                    let (_, val) = fields
                        .iter()
                        .find(|(k, _)| k.strip_prefix('$') == Some(&*class))
                        .ok_or(StopReason::UnknownOp)?;
                    match val {
                        Value::Dict(super_dict) => {
                            self.enter_method((**super_dict).clone(), method, depth + 1)
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
                    let bytes = CLOSURE_BYTES
                        .saturating_add(u32::try_from(method.len()).unwrap_or(u32::MAX));
                    self.budget.charge_bytes(bytes)?;
                    return Ok(Control::Return(Value::Callable(Rc::new(
                        Callable::Method {
                            dict: DictValue::Instance {
                                module,
                                name: instance,
                                args,
                            },
                            name: rc_str(method),
                            args: empty_values(),
                        },
                    ))));
                }
                let Some((context, body)) = self.defs.method(&module, &instance, method) else {
                    return Err(StopReason::UnknownOp);
                };
                if matches!(body.kind, ExprKind::Abstract(..)) || context.len() != args.len() {
                    return Err(StopReason::UnknownOp);
                }
                let base_bytes = u32::try_from(context.len())
                    .unwrap_or(u32::MAX)
                    .saturating_mul(SLOT_BYTES);
                let next_env =
                    self.bind_dict_params(&Env::default(), &[], context, &args, base_bytes)?;
                if let ExprKind::Lambda { params, body } = &body.kind {
                    return self.make_closure(next_env, &[], params, body);
                }
                if !matches!(
                    self.frames.last().map(|f| &f.kind),
                    Some(FrameKind::CallBoundary)
                ) {
                    self.push_frame(FrameKind::CallBoundary, Env::default())?;
                }
                Ok(Control::Eval {
                    expr: body,
                    env: next_env,
                })
            }
        }
    }

    fn resolve_dict(
        &mut self,
        dict: &Dict,
        env: &Env<'a>,
        depth: usize,
    ) -> Result<DictValue<'a>, StopReason> {
        self.budget.check_depth(self.frames.len() + depth)?;
        self.budget.charge_steps(1)?;
        match dict {
            Dict::Instance { module, name, args } => {
                let bytes = SLOT_BYTES
                    .saturating_add(
                        u32::try_from(args.len())
                            .unwrap_or(u32::MAX)
                            .saturating_mul(SLOT_BYTES),
                    )
                    .saturating_add(u32::try_from(module.len()).unwrap_or(u32::MAX))
                    .saturating_add(u32::try_from(name.len()).unwrap_or(u32::MAX));
                self.budget.charge_bytes(bytes)?;
                let mut resolved_args = Vec::new();
                for arg in args {
                    resolved_args.push(self.resolve_dict(arg, env, depth + 1)?);
                }
                Ok(DictValue::Instance {
                    module: rc_str(module),
                    name: rc_str(name),
                    args: Rc::new(resolved_args),
                })
            }
            Dict::Given(name) => env
                .get_dict(name, self.budget)?
                .cloned()
                .ok_or(StopReason::DynamicRead),
            Dict::Field(local) => match env.get(local, self.budget)? {
                Some(Value::Dict(d)) => Ok((**d).clone()),
                Some(_) => Err(StopReason::UnsafeContext),
                None => Err(StopReason::DynamicRead),
            },
            Dict::Super { dict: inner, class } => {
                let resolved = self.resolve_dict(inner, env, depth + 1)?;
                let mut cur = &resolved;
                while let DictValue::Super { dict: nested, .. } = cur {
                    self.budget.charge_steps(1)?;
                    cur = nested;
                }
                if let DictValue::Instance { module, name, args } = cur
                    && args.is_empty()
                    && let Some((num_mod, _)) = self.lang.get(Lang::Numeric)
                    && num_mod == &**module
                    && let Some(num_suffix) = name.rsplit(' ').next()
                {
                    let class_short = class.rsplit('.').next().unwrap_or(class);
                    let inst_len = u32::try_from(class_short.len())
                        .unwrap_or(u32::MAX)
                        .saturating_add(1)
                        .saturating_add(u32::try_from(num_suffix.len()).unwrap_or(u32::MAX));
                    let bytes = SLOT_BYTES
                        .saturating_add(u32::try_from(num_mod.len()).unwrap_or(u32::MAX))
                        .saturating_add(inst_len);
                    self.budget.charge_bytes(bytes)?;
                    let mut inst_name =
                        String::with_capacity(class_short.len() + 1 + num_suffix.len());
                    inst_name.push_str(class_short);
                    inst_name.push(' ');
                    inst_name.push_str(num_suffix);
                    return Ok(DictValue::Instance {
                        module: rc_str(num_mod),
                        name: rc_str(&inst_name),
                        args: empty_dicts(),
                    });
                }
                let super_bytes =
                    SLOT_BYTES.saturating_add(u32::try_from(class.len()).unwrap_or(u32::MAX));
                self.budget.charge_bytes(super_bytes)?;
                Ok(DictValue::Super {
                    dict: Rc::new(resolved),
                    class: rc_str(class),
                })
            }
            _ => Err(StopReason::UnknownOp),
        }
    }
}
