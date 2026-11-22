//! Iterative structural equality and pattern matching for the bounded evaluator.
//!
//! Uses explicit worklists and backtracking frames bounded by [`EvalBudget`] so
//! deep structures and patterns hit the exact same limits on native and WASM.

use std::rc::Rc;

use flick_ir::{Lit, Pat};

use super::value::{SLOT_BYTES, Value, bigint_value};
use super::{EvalBudget, StopReason};

/// Iterative structural equality (`==`), matching Flick's JS and Go runtimes:
/// `SameValueZero` on `Float` (`NaN == NaN`, `-0 == +0`), structural on tuples,
/// lists, records, and saturated constructors (ignoring GADT dictionary fields).
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "SameValueZero compares floats directly after NaN check; exhaustive value equality"
)]
pub(super) fn value_eq<'a>(
    lhs: &'a Value<'a>,
    rhs: &'a Value<'a>,
    budget: &mut EvalBudget,
) -> Result<bool, StopReason> {
    let mut work: Vec<(&'a Value<'a>, &'a Value<'a>)> = vec![(lhs, rhs)];
    while let Some((a, b)) = work.pop() {
        budget.charge_steps(1)?;
        match (a, b) {
            (Value::Unit, Value::Unit) => {}
            (Value::Bool(x), Value::Bool(y)) => {
                if x != y {
                    return Ok(false);
                }
            }
            (Value::Int(x), Value::Int(y)) => {
                if x != y {
                    return Ok(false);
                }
            }
            (Value::Float(x), Value::Float(y)) => {
                if !(x == y || (x.is_nan() && y.is_nan())) {
                    return Ok(false);
                }
            }
            (Value::BigInt(x), Value::BigInt(y)) => {
                if x != y {
                    return Ok(false);
                }
            }
            (Value::String(x), Value::String(y)) => {
                let scan_steps = u32::try_from(x.len().min(y.len()) / 16).unwrap_or(u32::MAX);
                budget.charge_steps(scan_steps)?;
                if x != y {
                    return Ok(false);
                }
            }
            (Value::Utf16(x), Value::Utf16(y)) => {
                let scan_steps = u32::try_from(x.len().min(y.len()) / 8).unwrap_or(u32::MAX);
                budget.charge_steps(scan_steps)?;
                if x != y {
                    return Ok(false);
                }
            }
            (Value::String(x), Value::Utf16(y)) | (Value::Utf16(y), Value::String(x)) => {
                let scan_steps = u32::try_from(y.len() / 8).unwrap_or(u32::MAX);
                budget.charge_steps(scan_steps)?;
                if !x.encode_utf16().eq(y.iter().copied()) {
                    return Ok(false);
                }
            }
            (Value::Tuple(xs), Value::Tuple(ys)) | (Value::List(xs), Value::List(ys)) => {
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                for (x, y) in xs.iter().zip(ys.iter()).rev() {
                    budget.check_depth(work.len())?;
                    work.push((x, y));
                }
            }
            (Value::Record(xs), Value::Record(ys)) => {
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                let linear = u32::try_from(xs.len()).unwrap_or(u32::MAX);
                let quad = u32::try_from(xs.len().saturating_mul(ys.len())).unwrap_or(u32::MAX) / 4;
                budget.charge_steps(linear.max(quad))?;
                for (kx, vx) in xs.iter().rev() {
                    let Some((_, vy)) = ys.iter().find(|(ky, _)| ky == kx) else {
                        return Ok(false);
                    };
                    budget.check_depth(work.len())?;
                    work.push((vx, vy));
                }
            }
            (
                Value::Con {
                    ctor: cx,
                    args: ax,
                    dicts: dx,
                },
                Value::Con {
                    ctor: cy,
                    args: ay,
                    dicts: dy,
                },
            ) => {
                if ax.len() != cx.arity
                    || ay.len() != cy.arity
                    || dx.len() != cx.dicts
                    || dy.len() != cy.dicts
                {
                    return Err(StopReason::UnknownOp);
                }
                if cx.module != cy.module || cx.ty != cy.ty {
                    return Err(StopReason::UnsafeContext);
                }
                if cx.name != cy.name {
                    return Ok(false);
                }
                // GADT dictionaries are ignored by `==` on both JS and Go.
                for (x, y) in ax.iter().zip(ay.iter()).rev() {
                    budget.check_depth(work.len())?;
                    work.push((x, y));
                }
            }
            _ => return Err(StopReason::UnknownOp),
        }
    }
    Ok(true)
}

/// Strict identity/value equality (`===`) for types whose identity is their equality.
#[allow(
    clippy::float_cmp,
    reason = "strict `===` compares floats directly (`NaN !== NaN`, `-0 === +0`)"
)]
pub(super) fn value_same(
    lhs: &Value<'_>,
    rhs: &Value<'_>,
    budget: &mut EvalBudget,
) -> Result<bool, StopReason> {
    budget.charge_steps(1)?;
    match (lhs, rhs) {
        (Value::Unit, Value::Unit) => Ok(true),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        (Value::Float(a), Value::Float(b)) => Ok(a == b),
        (Value::BigInt(a), Value::BigInt(b)) => Ok(a == b),
        (Value::String(a), Value::String(b)) => {
            let scan_steps = u32::try_from(a.len().min(b.len()) / 16).unwrap_or(u32::MAX);
            budget.charge_steps(scan_steps)?;
            Ok(a == b)
        }
        (Value::Utf16(a), Value::Utf16(b)) => {
            let scan_steps = u32::try_from(a.len().min(b.len()) / 8).unwrap_or(u32::MAX);
            budget.charge_steps(scan_steps)?;
            Ok(a == b)
        }
        (Value::String(a), Value::Utf16(b)) | (Value::Utf16(b), Value::String(a)) => {
            let scan_steps = u32::try_from(b.len() / 8).unwrap_or(u32::MAX);
            budget.charge_steps(scan_steps)?;
            Ok(a.encode_utf16().eq(b.iter().copied()))
        }
        (
            Value::Con {
                ctor: cx,
                args: ax,
                dicts: dx,
            },
            Value::Con {
                ctor: cy,
                args: ay,
                dicts: dy,
            },
        ) if cx.arity == 0
            && cy.arity == 0
            && ax.is_empty()
            && ay.is_empty()
            && dx.is_empty()
            && dy.is_empty() =>
        {
            if cx.module != cy.module || cx.ty != cy.ty {
                return Err(StopReason::UnsafeContext);
            }
            Ok(cx.name == cy.name)
        }
        _ => Err(StopReason::UnknownOp),
    }
}

/// Outcome of matching a pattern against a concrete value.
pub(super) enum PatMatch<'a> {
    Matched(Vec<(Rc<str>, Value<'a>)>),
    NoMatch,
}

/// Iterative pattern matching against a concrete [`Value`], supporting all IR
/// pattern forms (`Wild`, `Bind`, `Lit`, `Tuple`, `List`, `Record`, `Con`, `Or`)
/// with explicit backtracking frames so deep patterns cannot exhaust the host stack.
#[allow(
    clippy::too_many_lines,
    reason = "iterative pattern matcher over all IR Pat variants"
)]
pub(super) fn match_pat<'a>(
    pat: &'a Pat,
    value: Value<'a>,
    budget: &mut EvalBudget,
) -> Result<PatMatch<'a>, StopReason> {
    struct Choice<'a> {
        value: Value<'a>,
        alts: &'a [Pat],
        next_alt: usize,
        binds_mark: usize,
        saved_work: Vec<(&'a Pat, Value<'a>)>,
    }

    let mut work: Vec<(&'a Pat, Value<'a>)> = vec![(pat, value)];
    let mut binds: Vec<(Rc<str>, Value<'a>)> = Vec::new();
    let mut choices: Vec<Choice<'a>> = Vec::new();

    loop {
        let Some((p, v)) = work.pop() else {
            return Ok(PatMatch::Matched(binds));
        };
        budget.charge_steps(1)?;
        let matched = match p {
            Pat::Wild => true,
            Pat::Bind(name) => {
                binds.push((Rc::from(name.as_str()), v));
                true
            }
            Pat::Lit(lit) => match_lit(lit, &v, budget)?,
            Pat::Tuple(pats) => match v {
                Value::Tuple(items) if items.len() == pats.len() => {
                    for (sub_p, sub_v) in pats.iter().zip(items.iter()).rev() {
                        budget.check_depth(work.len() + choices.len())?;
                        work.push((sub_p, sub_v.clone()));
                    }
                    true
                }
                _ => return Err(StopReason::UnsafeContext),
            },
            Pat::List(pats, rest) => match v {
                Value::List(items) => {
                    let ok_len = if rest.is_some() {
                        items.len() >= pats.len()
                    } else {
                        items.len() == pats.len()
                    };
                    if ok_len {
                        if let Some(rest_pat) = rest {
                            let rest_val = if matches!(**rest_pat, Pat::Wild) {
                                Value::List(Rc::from([]))
                            } else {
                                let slice_len = items.len() - pats.len();
                                let bytes = u32::try_from(slice_len)
                                    .unwrap_or(u32::MAX)
                                    .saturating_mul(SLOT_BYTES);
                                budget.charge_bytes(bytes)?;
                                let steps = u32::try_from(slice_len / 8).unwrap_or(u32::MAX);
                                budget.charge_steps(steps)?;
                                Value::List(Rc::from(&items[pats.len()..]))
                            };
                            budget.check_depth(work.len() + choices.len())?;
                            work.push((rest_pat, rest_val));
                        }
                        for (sub_p, sub_v) in pats.iter().zip(items.iter()).rev() {
                            budget.check_depth(work.len() + choices.len())?;
                            work.push((sub_p, sub_v.clone()));
                        }
                        true
                    } else {
                        false
                    }
                }
                _ => return Err(StopReason::UnsafeContext),
            },
            Pat::Record(fields) => match v {
                Value::Record(rec_fields) => {
                    let scan_steps = u32::try_from(fields.len().saturating_mul(rec_fields.len()))
                        .unwrap_or(u32::MAX)
                        / 4;
                    budget.charge_steps(scan_steps)?;
                    for (label, sub_p) in fields.iter().rev() {
                        let Some((_, sub_v)) = rec_fields.iter().find(|(k, _)| &**k == label)
                        else {
                            return Err(StopReason::UnsafeContext);
                        };
                        budget.check_depth(work.len() + choices.len())?;
                        work.push((sub_p, sub_v.clone()));
                    }
                    true
                }
                _ => return Err(StopReason::UnsafeContext),
            },
            Pat::Con(c, pats) => match v {
                Value::Con { ctor, args, dicts } => {
                    if ctor.module != c.module || ctor.ty != c.ty {
                        return Err(StopReason::UnsafeContext);
                    }
                    if ctor.name != c.name {
                        false
                    } else if args.len() != ctor.arity {
                        return Err(StopReason::UnsafeContext);
                    } else if pats.len() == ctor.arity {
                        for (sub_p, sub_v) in pats.iter().zip(args.iter()).rev() {
                            budget.check_depth(work.len() + choices.len())?;
                            work.push((sub_p, sub_v.clone()));
                        }
                        true
                    } else if pats.len() == ctor.arity + ctor.dicts && dicts.len() == ctor.dicts {
                        for (sub_p, sub_d) in pats[ctor.arity..].iter().zip(dicts.iter()).rev() {
                            budget.check_depth(work.len() + choices.len())?;
                            work.push((sub_p, Value::Dict(sub_d.clone())));
                        }
                        for (sub_p, sub_v) in pats[..ctor.arity].iter().zip(args.iter()).rev() {
                            budget.check_depth(work.len() + choices.len())?;
                            work.push((sub_p, sub_v.clone()));
                        }
                        true
                    } else {
                        return Err(StopReason::UnsafeContext);
                    }
                }
                _ => return Err(StopReason::UnsafeContext),
            },
            Pat::Or(alts) => {
                if let Some((first, rest)) = alts.split_first() {
                    if !rest.is_empty() {
                        let work_len = u32::try_from(work.len()).unwrap_or(u32::MAX);
                        budget.charge_steps(work_len)?;
                        budget.charge_bytes(work_len.saturating_mul(SLOT_BYTES))?;
                        budget.check_depth(work.len() + choices.len())?;
                        choices.push(Choice {
                            value: v.clone(),
                            alts: rest,
                            next_alt: 0,
                            binds_mark: binds.len(),
                            saved_work: work.clone(),
                        });
                    }
                    budget.check_depth(work.len() + choices.len())?;
                    work.push((first, v));
                    true
                } else {
                    false
                }
            }
        };

        if !matched {
            let mut resumed = false;
            while let Some(mut choice) = choices.pop() {
                binds.truncate(choice.binds_mark);
                if choice.next_alt < choice.alts.len() {
                    let alt = &choice.alts[choice.next_alt];
                    let val = choice.value.clone();
                    let restore_steps = u32::try_from(choice.saved_work.len()).unwrap_or(u32::MAX);
                    budget.charge_steps(restore_steps)?;
                    work.clone_from(&choice.saved_work);
                    budget.check_depth(work.len() + choices.len())?;
                    work.push((alt, val));
                    choice.next_alt += 1;
                    if choice.next_alt < choice.alts.len() {
                        choices.push(choice);
                    }
                    resumed = true;
                    break;
                }
            }
            if !resumed {
                return Ok(PatMatch::NoMatch);
            }
        }
    }
}

/// Literal pattern comparison matching JS `===` and Go `==` (`-0.0 == 0.0`, `NaN != NaN`).
#[allow(
    clippy::float_cmp,
    reason = "pattern literal matching follows JS `===` and Go `==`"
)]
fn match_lit(lit: &Lit, value: &Value<'_>, budget: &mut EvalBudget) -> Result<bool, StopReason> {
    match (lit, value) {
        (Lit::Unit, Value::Unit) => Ok(true),
        (Lit::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Lit::Int(a), Value::Int(b)) => Ok(a == b),
        (Lit::Float(a), Value::Float(b)) => Ok(a == b),
        (Lit::BigInt(digits), Value::BigInt(b)) => {
            let a = bigint_value(digits).ok_or(StopReason::UnknownOp)?;
            Ok(a == *b)
        }
        (Lit::String(a), Value::String(b)) => {
            let scan_steps = u32::try_from(a.len().min(b.len()) / 16).unwrap_or(u32::MAX);
            budget.charge_steps(scan_steps)?;
            Ok(a.as_str() == &**b)
        }
        (Lit::String(a), Value::Utf16(b)) => {
            let scan_steps = u32::try_from(b.len() / 8).unwrap_or(u32::MAX);
            budget.charge_steps(scan_steps)?;
            Ok(a.encode_utf16().eq(b.iter().copied()))
        }
        _ => Err(StopReason::UnsafeContext),
    }
}
