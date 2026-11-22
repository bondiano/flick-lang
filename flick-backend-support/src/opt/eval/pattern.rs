//! Iterative structural equality and pattern matching for the bounded evaluator.
//!
//! Uses explicit worklists and backtracking frames bounded by [`EvalBudget`] so
//! deep structures and patterns hit the exact same limits on native and WASM.

use std::rc::Rc;

use flick_ir::{Lit, Pat};

use super::value::{
    SLOT_BYTES, Utf16Source, Value, bigint_value, empty_values, eq_utf16, find_field, rc_str,
};
use super::{EvalBudget, StopReason};

/// Iterative structural equality (`==`), matching Flick's JS and Go runtimes:
/// `SameValueZero` on `Float` (`NaN == NaN`, `-0 == +0`), structural on tuples,
/// lists, records, and saturated constructors (ignoring GADT dictionary fields).
pub(super) fn value_eq<'a>(
    lhs: &'a Value<'a>,
    rhs: &'a Value<'a>,
    budget: &mut EvalBudget,
) -> Result<bool, StopReason> {
    compare_values(lhs, rhs, false, budget)
}

/// Strict identity/value equality (`===`) for types whose identity is their equality.
pub(super) fn value_same<'a>(
    lhs: &'a Value<'a>,
    rhs: &'a Value<'a>,
    budget: &mut EvalBudget,
) -> Result<bool, StopReason> {
    compare_values(lhs, rhs, true, budget)
}

fn push_val_pairs<'a>(
    work: &mut Vec<(&'a Value<'a>, &'a Value<'a>)>,
    xs: &'a [Value<'a>],
    ys: &'a [Value<'a>],
    budget: &EvalBudget,
) -> Result<(), StopReason> {
    let mut i = xs.len().min(ys.len());
    while i > 0 {
        i -= 1;
        budget.check_depth(work.len())?;
        work.push((&xs[i], &ys[i]));
    }
    Ok(())
}

#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "SameValueZero vs strict float comparison; unified structural/identity equality"
)]
fn compare_values<'a>(
    lhs: &'a Value<'a>,
    rhs: &'a Value<'a>,
    strict: bool,
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
                if x != y && (strict || !(x.is_nan() && y.is_nan())) {
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
                if !eq_utf16(Utf16Source::from_str(x), Utf16Source::from_units(y)) {
                    return Ok(false);
                }
            }
            (Value::Tuple(xs), Value::Tuple(ys)) | (Value::List(xs), Value::List(ys))
                if !strict =>
            {
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                push_val_pairs(&mut work, xs, ys, budget)?;
            }
            (Value::Record(xs), Value::Record(ys)) if !strict => {
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                let linear = u32::try_from(xs.len()).unwrap_or(u32::MAX);
                let quad = u32::try_from(xs.len().saturating_mul(ys.len())).unwrap_or(u32::MAX) / 4;
                budget.charge_steps(linear.max(quad))?;
                for (kx, vx) in xs.iter().rev() {
                    let Some(vy) = find_field(ys, kx) else {
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
                if strict
                    && !(cx.arity == 0
                        && cy.arity == 0
                        && ax.is_empty()
                        && ay.is_empty()
                        && dx.is_empty()
                        && dy.is_empty())
                {
                    return Err(StopReason::UnknownOp);
                }
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
                push_val_pairs(&mut work, ax, ay, budget)?;
            }
            _ => return Err(StopReason::UnknownOp),
        }
    }
    Ok(true)
}

/// Outcome of matching a pattern against a concrete value.
pub(super) enum PatMatch<'a> {
    Matched(Vec<(Rc<str>, Value<'a>)>),
    NoMatch,
}

#[inline(never)]
fn push_pat_work<'a>(
    work: &mut Vec<(&'a Pat, Value<'a>)>,
    choices_len: usize,
    pat: &'a Pat,
    val: Value<'a>,
    budget: &EvalBudget,
) -> Result<(), StopReason> {
    budget.check_depth(work.len() + choices_len)?;
    work.push((pat, val));
    Ok(())
}

fn push_pat_pairs<'a>(
    work: &mut Vec<(&'a Pat, Value<'a>)>,
    choices_len: usize,
    pats: &'a [Pat],
    vals: &[Value<'a>],
    budget: &EvalBudget,
) -> Result<(), StopReason> {
    let mut i = pats.len().min(vals.len());
    while i > 0 {
        i -= 1;
        push_pat_work(work, choices_len, &pats[i], vals[i].clone(), budget)?;
    }
    Ok(())
}

fn clone_work<'a>(src: &[(&'a Pat, Value<'a>)]) -> Vec<(&'a Pat, Value<'a>)> {
    let mut out = Vec::new();
    for (p, v) in src {
        out.push((*p, v.clone()));
    }
    out
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
                binds.push((rc_str(name), v));
                true
            }
            Pat::Lit(lit) => match_lit(lit, &v, budget)?,
            Pat::Tuple(pats) => match v {
                Value::Tuple(items) if items.len() == pats.len() => {
                    push_pat_pairs(&mut work, choices.len(), pats, &items, budget)?;
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
                                Value::List(empty_values())
                            } else {
                                let slice_len = items.len() - pats.len();
                                let bytes = u32::try_from(slice_len)
                                    .unwrap_or(u32::MAX)
                                    .saturating_mul(SLOT_BYTES);
                                budget.charge_bytes(bytes)?;
                                let steps = u32::try_from(slice_len / 8).unwrap_or(u32::MAX);
                                budget.charge_steps(steps)?;
                                let mut tail = Vec::new();
                                for item in &items[pats.len()..] {
                                    tail.push(item.clone());
                                }
                                Value::List(Rc::new(tail))
                            };
                            push_pat_work(&mut work, choices.len(), rest_pat, rest_val, budget)?;
                        }
                        push_pat_pairs(&mut work, choices.len(), pats, &items, budget)?;
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
                        let Some(sub_v) = find_field(&rec_fields, label) else {
                            return Err(StopReason::UnsafeContext);
                        };
                        push_pat_work(&mut work, choices.len(), sub_p, sub_v.clone(), budget)?;
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
                    } else {
                        let has_dicts =
                            pats.len() == ctor.arity + ctor.dicts && dicts.len() == ctor.dicts;
                        if pats.len() != ctor.arity && !has_dicts {
                            return Err(StopReason::UnsafeContext);
                        }
                        if has_dicts {
                            let dict_pats = &pats[ctor.arity..];
                            let mut i = dict_pats.len().min(dicts.len());
                            while i > 0 {
                                i -= 1;
                                push_pat_work(
                                    &mut work,
                                    choices.len(),
                                    &dict_pats[i],
                                    Value::Dict(Rc::new(dicts[i].clone())),
                                    budget,
                                )?;
                            }
                        }
                        push_pat_pairs(
                            &mut work,
                            choices.len(),
                            &pats[..ctor.arity],
                            &args,
                            budget,
                        )?;
                        true
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
                            binds_mark: binds.len(),
                            saved_work: clone_work(&work),
                        });
                    }
                    push_pat_work(&mut work, choices.len(), first, v, budget)?;
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
                if let Some((alt, rest)) = choice.alts.split_first() {
                    let restore_steps = u32::try_from(choice.saved_work.len()).unwrap_or(u32::MAX);
                    budget.charge_steps(restore_steps)?;
                    let val = if rest.is_empty() {
                        work = choice.saved_work;
                        choice.value
                    } else {
                        let val = choice.value.clone();
                        work = clone_work(&choice.saved_work);
                        choice.alts = rest;
                        choices.push(choice);
                        val
                    };
                    push_pat_work(&mut work, choices.len(), alt, val, budget)?;
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
            Ok(eq_utf16(
                Utf16Source::from_str(a),
                Utf16Source::from_units(b),
            ))
        }
        _ => Err(StopReason::UnsafeContext),
    }
}
