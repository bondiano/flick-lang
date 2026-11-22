//! Single-stage application of closures, constructors, built-in methods, and primitives.

use std::rc::Rc;

use flick_ir::{Lang, Num, Stage};

use super::{Control, Frame, Machine, StepResult};
use crate::Known;
use crate::opt::eval::pattern::{PatMatch, match_pat};
use crate::opt::eval::value::{
    CLOSURE_BYTES, Closure, DictValue, SLOT_BYTES, Value, bigint_byte_len,
};
use crate::opt::eval::{PrimitiveOutcome, StopReason};
use crate::opt::{fold, specialize};

/// Maximum UTF-8 byte length of any finite `f64` formatted by [`fold::js_number`].
const MAX_JS_NUMBER_BYTES: u32 = 25;

impl<'a> Machine<'a, '_> {
    #[allow(
        clippy::too_many_lines,
        reason = "single-stage application over all callable Value kinds"
    )]
    pub(super) fn apply(
        &mut self,
        func: Value<'a>,
        arg: Value<'a>,
    ) -> Result<StepResult<'a>, StopReason> {
        self.budget.charge_steps(1)?;
        match func {
            Value::Closure(closure) => {
                if !closure.dict_params.is_empty() {
                    return Err(StopReason::UnsafeContext);
                }
                let Some((first, rest)) = closure.params.split_first() else {
                    return Err(StopReason::UnsafeContext);
                };
                if first.stage != Stage::Pure {
                    return Err(StopReason::UnsafeContext);
                }
                let PatMatch::Matched(binds) = match_pat(&first.pat, arg, self.budget)? else {
                    return Err(StopReason::PossibleTrap);
                };
                let mut call_env = match &closure.rec_group {
                    Some(group) => {
                        if group.defs.len() > 1 {
                            let extra = u32::try_from(group.defs.len()).unwrap_or(u32::MAX);
                            self.budget.charge_steps(extra)?;
                        }
                        closure.env.with_rec_group(group)
                    }
                    None => closure.env.clone(),
                };
                for (name, val) in binds {
                    call_env = call_env.with(name, val);
                }
                if !rest.is_empty() {
                    self.budget.charge_bytes(CLOSURE_BYTES)?;
                    return Ok(StepResult::Continue(Control::Return(Value::Closure(
                        Rc::new(Closure {
                            env: call_env,
                            dict_env: closure.dict_env.clone(),
                            dict_params: &[],
                            params: rest,
                            body: closure.body,
                            rec_group: None,
                        }),
                    ))));
                }
                // Tail calls reuse an existing top-of-stack `CallBoundary`; non-tail
                // calls push a new boundary subject to `max_frame_depth`.
                if !matches!(self.frames.last(), Some(Frame::CallBoundary)) {
                    self.push_frame(Frame::CallBoundary)?;
                }
                Ok(StepResult::Continue(Control::Eval {
                    expr: closure.body,
                    env: call_env,
                    dict_env: closure.dict_env.clone(),
                }))
            }
            Value::Con { ctor, args, dicts } => {
                if args.len() >= ctor.arity {
                    return Err(StopReason::UnsafeContext);
                }
                let next_len = args.len() + 1;
                let bytes = u32::try_from(next_len)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(SLOT_BYTES);
                self.budget.charge_bytes(bytes)?;
                let mut next = Vec::with_capacity(next_len);
                next.extend(args.iter().cloned());
                next.push(arg);
                Ok(StepResult::Continue(Control::Return(Value::Con {
                    ctor,
                    args: Rc::from(next),
                    dicts,
                })))
            }
            Value::Method { dict, name, args } => self.apply_builtin_method(dict, name, &args, arg),
            Value::Primitive {
                module,
                name,
                arity,
                dicts,
                args,
            } => {
                let next_len = args.len() + 1;
                let bytes = u32::try_from(next_len)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(SLOT_BYTES);
                self.budget.charge_bytes(bytes)?;
                let mut next = Vec::with_capacity(next_len);
                next.extend(args.iter().cloned());
                next.push(arg);
                if next.len() < arity {
                    Ok(StepResult::Continue(Control::Return(Value::Primitive {
                        module,
                        name,
                        arity,
                        dicts,
                        args: Rc::from(next),
                    })))
                } else {
                    match self.primitives.eval(
                        &module,
                        &name,
                        &dicts,
                        &next,
                        self.lang,
                        self.budget,
                    ) {
                        PrimitiveOutcome::Known(v) => Ok(StepResult::Continue(Control::Return(v))),
                        PrimitiveOutcome::Stop(reason) => Err(reason),
                    }
                }
            }
            _ => Err(StopReason::UnknownOp),
        }
    }

    fn apply_builtin_method(
        &mut self,
        dict: DictValue<'a>,
        name: Rc<str>,
        args: &[Value<'a>],
        arg: Value<'a>,
    ) -> Result<StepResult<'a>, StopReason> {
        let DictValue::Instance {
            module,
            name: instance,
            args: dict_args,
        } = &dict
        else {
            return Err(StopReason::UnknownOp);
        };
        if !dict_args.is_empty() {
            return Err(StopReason::UnknownOp);
        }
        // Built-in `Show` / `Display` methods. Charge input size before formatting.
        if self
            .lang
            .get(Lang::Instances)
            .is_some_and(|(m, _)| m == &**module)
        {
            let is_show = self
                .lang
                .get(Lang::ShowMethod)
                .is_some_and(|(_, n)| n == &*name);
            let is_display = !is_show
                && self
                    .lang
                    .get(Lang::DisplayMethod)
                    .is_some_and(|(_, n)| n == &*name);
            if is_show || is_display {
                let res = self.eval_show_display(instance, is_show, arg)?;
                return Ok(StepResult::Continue(Control::Return(res)));
            }
        }
        // Built-in `Numeric` / `Integer` / `Floating` methods.
        let Some((num, method_lang)) =
            specialize::numeric_method(module, instance, true, &name, self.lang)
        else {
            return Err(StopReason::UnknownOp);
        };
        match method_lang {
            Lang::Negate => {
                let res = self.eval_neg(num, arg)?;
                Ok(StepResult::Continue(Control::Return(res)))
            }
            Lang::FromInt | Lang::FromFloat => {
                let res = match (num, arg) {
                    (Num::Int, Value::Int(n)) => Value::Int(n),
                    (Num::Float, Value::Int(n)) => Value::Float(f64::from(n)),
                    (Num::BigInt, Value::Int(n)) => {
                        self.budget.charge_bytes(16)?;
                        Value::BigInt(i128::from(n))
                    }
                    (Num::Float, Value::Float(x)) => Value::Float(x),
                    _ => return Err(StopReason::UnknownOp),
                };
                Ok(StepResult::Continue(Control::Return(res)))
            }
            _ => {
                let op = specialize::binary(method_lang, num).ok_or(StopReason::UnknownOp)?;
                if args.is_empty() {
                    self.budget.charge_bytes(CLOSURE_BYTES)?;
                    Ok(StepResult::Continue(Control::Return(Value::Method {
                        dict,
                        name,
                        args: Rc::from([arg]),
                    })))
                } else {
                    let res = self.eval_binary(op, args[0].clone(), arg)?;
                    Ok(StepResult::Continue(Control::Return(res)))
                }
            }
        }
    }
    #[allow(
        clippy::float_cmp,
        reason = "exact zero check for signed-zero JS formatting"
    )]
    fn eval_show_display(
        &mut self,
        instance: &str,
        is_show: bool,
        arg: Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        match (is_show, instance, arg) {
            (false, "Display String", Value::String(s)) => Ok(Value::String(s)),
            (true, "Show String", Value::String(s)) => {
                let min_len = u32::try_from(s.len()).unwrap_or(u32::MAX).saturating_add(2);
                if min_len > self.budget.bytes_remaining {
                    return Err(StopReason::ConstructedBytes);
                }
                let scan_steps = u32::try_from(s.len()).unwrap_or(u32::MAX) / 16;
                self.budget.charge_steps(scan_steps)?;
                let escaped_len = json_string_len(&s);
                self.budget.charge_bytes(escaped_len)?;
                Ok(Value::String(Rc::from(fold::json_string(&s))))
            }
            (true, "Show Bool", Value::Bool(b)) | (false, "Display Bool", Value::Bool(b)) => {
                let text = if b { "true" } else { "false" };
                let len = u32::try_from(text.len()).unwrap_or(u32::MAX);
                self.budget.charge_bytes(len)?;
                Ok(Value::String(Rc::from(text)))
            }
            (true, "Show Int", Value::Int(n)) | (false, "Display Int", Value::Int(n)) => {
                let len = bigint_byte_len(i128::from(n));
                self.budget.charge_bytes(len)?;
                Ok(Value::String(Rc::from(n.to_string())))
            }
            (true, "Show BigInt", Value::BigInt(n)) => {
                let len = bigint_byte_len(n).saturating_add(1);
                self.budget.charge_bytes(len)?;
                Ok(Value::String(Rc::from(format!("{n}n"))))
            }
            (true, "Show Float", Value::Float(x)) | (false, "Display Float", Value::Float(x)) => {
                let special = if x.is_nan() {
                    Some("NaN")
                } else if x.is_infinite() {
                    Some(if x.is_sign_negative() {
                        "-Infinity"
                    } else {
                        "Infinity"
                    })
                } else if x == 0.0 {
                    Some(if x.is_sign_negative() { "-0" } else { "0" })
                } else {
                    None
                };
                if let Some(text) = special {
                    let len = u32::try_from(text.len()).unwrap_or(u32::MAX);
                    self.budget.charge_bytes(len)?;
                    return Ok(Value::String(Rc::from(text)));
                }
                // Charge worst-case formatted byte length before calling `js_number`.
                self.budget.charge_bytes(MAX_JS_NUMBER_BYTES)?;
                let text = fold::js_number(x);
                let actual = u32::try_from(text.len()).unwrap_or(u32::MAX);
                if actual < MAX_JS_NUMBER_BYTES {
                    self.budget.bytes_remaining = self
                        .budget
                        .bytes_remaining
                        .saturating_add(MAX_JS_NUMBER_BYTES - actual);
                } else if actual > MAX_JS_NUMBER_BYTES {
                    self.budget.charge_bytes(actual - MAX_JS_NUMBER_BYTES)?;
                }
                Ok(Value::String(Rc::from(text)))
            }
            _ => Err(StopReason::UnknownOp),
        }
    }
}

/// Computes the exact UTF-8 byte length of [`fold::json_string`] without allocating.
fn json_string_len(s: &str) -> u32 {
    let mut len: u32 = 2;
    for c in s.chars() {
        let add = match c {
            '"' | '\\' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
            c if c < ' ' => 6,
            c => u32::try_from(c.len_utf8()).unwrap_or(4),
        };
        len = len.saturating_add(add);
    }
    len
}

pub(super) fn is_builtin_method(
    module: &str,
    instance: &str,
    args_empty: bool,
    method: &str,
    lang: &Known<'_>,
) -> bool {
    if !args_empty {
        return false;
    }
    if specialize::numeric_method(module, instance, true, method, lang).is_some() {
        return true;
    }
    if lang.get(Lang::Instances).is_some_and(|(m, _)| m == module)
        && (lang.get(Lang::ShowMethod).is_some_and(|(_, n)| n == method)
            || lang
                .get(Lang::DisplayMethod)
                .is_some_and(|(_, n)| n == method))
    {
        return matches!(
            instance,
            "Show Float"
                | "Show Int"
                | "Show BigInt"
                | "Show Bool"
                | "Show String"
                | "Display Float"
                | "Display Int"
                | "Display Bool"
                | "Display String"
        );
    }
    false
}
