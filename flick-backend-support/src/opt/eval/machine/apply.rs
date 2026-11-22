//! Single-stage application of closures, constructors, built-in methods, and primitives.

use std::rc::Rc;

use flick_ir::{Lang, Num, Stage};

use super::ops::value_to_const;
use super::{Control, FrameKind, Machine};
use crate::Known;
use crate::opt::eval::pattern::{PatMatch, match_pat};
use crate::opt::eval::value::{
    CLOSURE_BYTES, Callable, DictValue, Env, SLOT_BYTES, Value, bigint_byte_len, rc_str,
};
use crate::opt::eval::{PrimitiveOutcome, StopReason};
use crate::opt::{fold, specialize};

/// Maximum UTF-8 byte length of any finite `f64` formatted by [`fold::js_number`].
const MAX_JS_NUMBER_BYTES: u32 = 25;

impl<'a> Machine<'a, '_> {
    #[inline(never)]
    fn append_arg(
        &mut self,
        args: &[Value<'a>],
        arg: Value<'a>,
    ) -> Result<Vec<Value<'a>>, StopReason> {
        let next_len = args.len() + 1;
        let bytes = u32::try_from(next_len)
            .unwrap_or(u32::MAX)
            .saturating_mul(SLOT_BYTES);
        self.budget.charge_bytes(bytes)?;
        let mut next = Vec::new();
        for item in args {
            next.push(item.clone());
        }
        next.push(arg);
        Ok(next)
    }

    pub(super) fn apply(
        &mut self,
        func: Value<'a>,
        arg: Value<'a>,
    ) -> Result<Control<'a>, StopReason> {
        self.budget.charge_steps(1)?;
        match func {
            Value::Con { ctor, args, dicts } => {
                if args.len() >= ctor.arity {
                    return Err(StopReason::UnsafeContext);
                }
                let next = self.append_arg(&args, arg)?;
                Ok(Control::Return(Value::Con {
                    ctor,
                    args: Rc::new(next),
                    dicts,
                }))
            }
            Value::Callable(callable) => match &*callable {
                Callable::Closure {
                    env,
                    dict_params,
                    params,
                    body,
                    rec_group,
                } => {
                    if !dict_params.is_empty() {
                        return Err(StopReason::UnsafeContext);
                    }
                    let Some((first, rest)) = params.split_first() else {
                        return Err(StopReason::UnsafeContext);
                    };
                    if first.stage != Stage::Pure {
                        return Err(StopReason::UnsafeContext);
                    }
                    let PatMatch::Matched(mut binds) = match_pat(&first.pat, arg, self.budget)?
                    else {
                        return Err(StopReason::PossibleTrap);
                    };
                    let mut call_env = self.closure_env(env, rec_group)?;
                    while let Some((name, val)) = binds.pop() {
                        call_env = call_env.with(name, val);
                    }
                    if !rest.is_empty() {
                        return self.make_closure(call_env, &[], rest, body);
                    }
                    // Tail calls reuse an existing top-of-stack `CallBoundary`; non-tail
                    // calls push a new boundary subject to `max_frame_depth`.
                    if !matches!(
                        self.frames.last().map(|f| &f.kind),
                        Some(FrameKind::CallBoundary)
                    ) {
                        self.push_frame(FrameKind::CallBoundary, Env::default())?;
                    }
                    Ok(Control::Eval {
                        expr: body,
                        env: call_env,
                    })
                }
                Callable::Method { dict, name, args } => {
                    self.apply_builtin_method(dict, name, args, arg)
                }
                Callable::Primitive {
                    module,
                    name,
                    arity,
                    dicts,
                    args,
                } => {
                    let next = self.append_arg(args, arg)?;
                    if next.len() < *arity {
                        Ok(Control::Return(Value::Callable(Rc::new(
                            Callable::Primitive {
                                module: Rc::clone(module),
                                name: Rc::clone(name),
                                arity: *arity,
                                dicts: Rc::clone(dicts),
                                args: Rc::new(next),
                            },
                        ))))
                    } else {
                        match self.primitives.eval(
                            module,
                            name,
                            dicts,
                            &next,
                            self.lang,
                            self.budget,
                        ) {
                            PrimitiveOutcome::Known(v) => Ok(Control::Return(v)),
                            PrimitiveOutcome::Stop(reason) => Err(reason),
                        }
                    }
                }
            },
            _ => Err(StopReason::UnknownOp),
        }
    }

    fn apply_builtin_method(
        &mut self,
        dict: &DictValue<'a>,
        name: &Rc<str>,
        args: &[Value<'a>],
        arg: Value<'a>,
    ) -> Result<Control<'a>, StopReason> {
        let DictValue::Instance {
            module,
            name: instance,
            args: dict_args,
        } = dict
        else {
            return Err(StopReason::UnknownOp);
        };
        if !dict_args.is_empty() {
            return Err(StopReason::UnknownOp);
        }
        // Built-in `Show` / `Display` methods.
        if let Some(is_show) = show_display_kind(module, name, self.lang) {
            return Ok(Control::Return(
                self.eval_show_display(instance, is_show, arg)?,
            ));
        }
        // Built-in `Numeric` / `Integer` / `Floating` methods.
        let Some((num, method_lang)) =
            specialize::numeric_method(module, instance, true, name, self.lang)
        else {
            return Err(StopReason::UnknownOp);
        };
        match method_lang {
            Lang::Negate => Ok(Control::Return(self.eval_neg(num, arg)?)),
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
                Ok(Control::Return(res))
            }
            _ => {
                let op = specialize::binary(method_lang, num).ok_or(StopReason::UnknownOp)?;
                if args.is_empty() {
                    self.budget.charge_bytes(CLOSURE_BYTES)?;
                    Ok(Control::Return(Value::Callable(Rc::new(
                        Callable::Method {
                            dict: dict.clone(),
                            name: Rc::clone(name),
                            args: Rc::new(vec![arg]),
                        },
                    ))))
                } else {
                    Ok(Control::Return(self.eval_binary(op, &args[0], &arg)?))
                }
            }
        }
    }

    fn eval_show_display(
        &mut self,
        instance: &str,
        is_show: bool,
        arg: Value<'a>,
    ) -> Result<Value<'a>, StopReason> {
        if let Value::String(s) = arg {
            return match (is_show, instance) {
                (false, "Display String") => Ok(Value::String(s)),
                (true, "Show String") => {
                    let min_len = u32::try_from(s.len()).unwrap_or(u32::MAX).saturating_add(2);
                    if min_len > self.budget.bytes_remaining {
                        return Err(StopReason::ConstructedBytes);
                    }
                    let scan_steps = u32::try_from(s.len()).unwrap_or(u32::MAX) / 16;
                    self.budget.charge_steps(scan_steps)?;
                    let escaped_len = json_string_len(&s);
                    self.budget.charge_bytes(escaped_len)?;
                    Ok(Value::String(rc_str(&fold::json_string(&s))))
                }
                _ => Err(StopReason::UnknownOp),
            };
        }
        // Charge exact/worst-case scalar formatted bytes before calling `fold::show`/`display`.
        let reserve = match &arg {
            Value::Bool(b) => {
                if *b {
                    4
                } else {
                    5
                }
            }
            Value::Int(n) => bigint_byte_len(i128::from(*n)),
            Value::BigInt(n) => bigint_byte_len(*n).saturating_add(1),
            Value::Float(_) => MAX_JS_NUMBER_BYTES,
            _ => return Err(StopReason::UnknownOp),
        };
        let c = value_to_const(&arg)?;
        self.budget.charge_bytes(reserve)?;
        let text = if is_show {
            fold::show(instance, c)
        } else {
            fold::display(instance, c)
        }
        .ok_or(StopReason::UnknownOp)?;
        let actual = u32::try_from(text.len()).unwrap_or(u32::MAX);
        if actual < reserve {
            self.budget.bytes_remaining =
                self.budget.bytes_remaining.saturating_add(reserve - actual);
        } else if actual > reserve {
            self.budget.charge_bytes(actual - reserve)?;
        }
        Ok(Value::String(rc_str(&text)))
    }
}

/// Returns `Some(true)` for built-in `Show.show`, `Some(false)` for `Display.display`.
fn show_display_kind(module: &str, method: &str, lang: &Known<'_>) -> Option<bool> {
    if lang.get(Lang::Instances).is_none_or(|(m, _)| m != module) {
        return None;
    }
    if lang.get(Lang::ShowMethod).is_some_and(|(_, n)| n == method) {
        Some(true)
    } else if lang
        .get(Lang::DisplayMethod)
        .is_some_and(|(_, n)| n == method)
    {
        Some(false)
    } else {
        None
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
    if let Some(is_show) = show_display_kind(module, method, lang) {
        let prefix = if is_show { "Show " } else { "Display " };
        return instance.strip_prefix(prefix).is_some_and(|ty| {
            matches!(ty, "Float" | "Int" | "Bool" | "String") || (is_show && ty == "BigInt")
        });
    }
    false
}
