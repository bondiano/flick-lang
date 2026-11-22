//! Bounded, target-exact evaluators for each reviewed native primitive (#805).
//!
//! Every operation charges step and byte budgets BEFORE scanning or allocating,
//! preserves internal UTF-16 values without substituting `U+FFFD`, and declines
//! (`StopReason::UnknownOp` or budget exhaustion) instead of synthesizing a
//! Flick `None`/`Err` on resource limits or target-divergent edge cases.

use std::rc::Rc;

use flick_ir::Constructor;

use super::super::materialize::decode_utf16;
use super::super::value::{SLOT_BYTES, Utf16Source, Value, empty_dicts, empty_values, rc_str};
use super::super::{EvalBudget, PrimitiveOutcome, StopReason};

/// ECMA-262 `WhiteSpace` and `LineTerminator` code units (all BMP, matching JS
/// `String.prototype.trim` and Go `flickrt.isJSSpace`, excluding U+0085).
fn is_js_space_u16(u: u16) -> bool {
    matches!(
        u,
        0x0009..=0x000D
            | 0x0020
            | 0x00A0
            | 0x1680
            | 0x2000..=0x200A
            | 0x2028
            | 0x2029
            | 0x202F
            | 0x205F
            | 0x3000
            | 0xFEFF
    )
}

/// Parses a signed decimal 32-bit integer from a UTF-16 unit stream after
/// ignoring leading and trailing ECMA-262 whitespace and line terminators.
/// Accumulates negatively so `i32::MIN` never overflows and `-0` normalizes to `0`.
fn parse_int_source(mut src: Utf16Source<'_>) -> Option<i32> {
    let mut cur = loop {
        let u = src.next_unit()?;
        if !is_js_space_u16(u) {
            break u;
        }
    };
    let neg = match cur {
        0x002D => {
            cur = src.next_unit()?;
            true
        }
        0x002B => {
            cur = src.next_unit()?;
            false
        }
        _ => false,
    };
    let mut acc: i32 = 0;
    let mut digits = false;
    let mut trailing = None;
    loop {
        if (0x0030..=0x0039).contains(&cur) {
            digits = true;
            acc = acc.checked_mul(10)?.checked_sub(i32::from(cur - 0x0030))?;
            let Some(next) = src.next_unit() else {
                break;
            };
            cur = next;
        } else {
            trailing = Some(cur);
            break;
        }
    }
    if !digits {
        return None;
    }
    if let Some(first_tail) = trailing {
        if !is_js_space_u16(first_tail) {
            return None;
        }
        while let Some(u) = src.next_unit() {
            if !is_js_space_u16(u) {
                return None;
            }
        }
    }
    if neg { Some(acc) } else { acc.checked_neg() }
}

fn utf16_source_of<'a>(v: &'a Value<'a>) -> Option<(u32, Utf16Source<'a>)> {
    match v {
        Value::String(s) => Some((
            u32::try_from(s.len() / 16).unwrap_or(u32::MAX),
            Utf16Source::from_str(s),
        )),
        Value::Utf16(u) => Some((
            u32::try_from(u.len() / 8).unwrap_or(u32::MAX),
            Utf16Source::from_units(u),
        )),
        _ => None,
    }
}

pub(super) fn eval_int_parse<'a>(
    arg: &Value<'a>,
    some_ctor: &Rc<Constructor>,
    none_ctor: &Rc<Constructor>,
    budget: &mut EvalBudget,
) -> PrimitiveOutcome<'a> {
    let Some((scan, src)) = utf16_source_of(arg) else {
        return PrimitiveOutcome::Stop(StopReason::UnknownOp);
    };
    if let Err(reason) = budget.charge_steps(scan) {
        return PrimitiveOutcome::Stop(reason);
    }
    let parsed = parse_int_source(src);
    let bytes = if parsed.is_some() {
        SLOT_BYTES.saturating_mul(2)
    } else {
        SLOT_BYTES
    };
    if let Err(reason) = budget.charge_bytes(bytes) {
        return PrimitiveOutcome::Stop(reason);
    }
    let (ctor, args) = if let Some(n) = parsed {
        (Rc::clone(some_ctor), Rc::new(vec![Value::Int(n)]))
    } else {
        (Rc::clone(none_ctor), empty_values())
    };
    PrimitiveOutcome::Known(Value::Con {
        ctor,
        args,
        dicts: empty_dicts(),
    })
}

/// Counts UTF-16 code units of a well-formed UTF-8 string in one byte pass.
fn utf16_len_of_str(s: &str) -> usize {
    let mut count = 0_usize;
    for &b in s.as_bytes() {
        if b < 0x80 || (0xC0..0xF0).contains(&b) {
            count += 1;
        } else if b >= 0xF0 {
            count += 2;
        }
    }
    count
}

pub(super) fn eval_string_length<'a>(
    arg: &Value<'a>,
    budget: &mut EvalBudget,
) -> PrimitiveOutcome<'a> {
    let len = match arg {
        Value::String(s) => {
            if let Err(reason) =
                budget.charge_steps(u32::try_from(s.len() / 16).unwrap_or(u32::MAX))
            {
                return PrimitiveOutcome::Stop(reason);
            }
            utf16_len_of_str(s)
        }
        Value::Utf16(units) => units.len(),
        _ => return PrimitiveOutcome::Stop(StopReason::UnknownOp),
    };
    match i32::try_from(len) {
        Ok(n) => PrimitiveOutcome::Known(Value::Int(n)),
        Err(_) => PrimitiveOutcome::Stop(StopReason::UnknownOp),
    }
}

fn clamp_slice_index(idx: i32, len: usize) -> usize {
    let len_i64 = i64::try_from(len).unwrap_or(i64::MAX);
    let mut pos = i64::from(idx);
    if pos < 0 {
        pos += len_i64;
    }
    usize::try_from(pos.clamp(0, len_i64)).unwrap_or(0)
}

pub(super) fn eval_string_slice<'a>(
    start_val: &Value<'a>,
    end_val: &Value<'a>,
    subject: &Value<'a>,
    budget: &mut EvalBudget,
) -> PrimitiveOutcome<'a> {
    let (Value::Int(start), Value::Int(end)) = (start_val, end_val) else {
        return PrimitiveOutcome::Stop(StopReason::UnknownOp);
    };
    match subject {
        Value::String(s) => slice_utf8(*start, *end, s, budget),
        Value::Utf16(units) => slice_utf16(*start, *end, units, budget),
        _ => PrimitiveOutcome::Stop(StopReason::UnknownOp),
    }
}

fn slice_utf8<'a>(start: i32, end: i32, s: &str, budget: &mut EvalBudget) -> PrimitiveOutcome<'a> {
    if let Err(reason) = budget.charge_steps(u32::try_from(s.len() / 16).unwrap_or(u32::MAX)) {
        return PrimitiveOutcome::Stop(reason);
    }
    let total_units = utf16_len_of_str(s);
    let from = clamp_slice_index(start, total_units);
    let to = clamp_slice_index(end, total_units);
    if from >= to {
        return PrimitiveOutcome::Known(Value::String(rc_str("")));
    }
    if let Some(slice) = aligned_utf8_slice(s, from, to) {
        if let Err(reason) = budget.charge_bytes(u32::try_from(slice.len()).unwrap_or(u32::MAX)) {
            return PrimitiveOutcome::Stop(reason);
        }
        return PrimitiveOutcome::Known(Value::String(rc_str(slice)));
    }
    // At least one index splits a surrogate pair: preserve exact UTF-16
    // code units internally (never substituting U+FFFD).
    let slice_units = to - from;
    let bytes = u32::try_from(slice_units)
        .unwrap_or(u32::MAX)
        .saturating_mul(2);
    if let Err(reason) = budget.charge_bytes(bytes) {
        return PrimitiveOutcome::Stop(reason);
    }
    if let Err(reason) = budget.charge_steps(u32::try_from(slice_units / 8).unwrap_or(u32::MAX)) {
        return PrimitiveOutcome::Stop(reason);
    }
    let mut out = Vec::with_capacity(slice_units);
    let mut src = Utf16Source::from_str(s);
    let mut unit_pos = 0_usize;
    while let Some(u) = src.next_unit() {
        if unit_pos >= from {
            out.push(u);
        }
        unit_pos += 1;
        if unit_pos >= to {
            break;
        }
    }
    PrimitiveOutcome::Known(Value::Utf16(Rc::new(out)))
}

fn aligned_utf8_slice(s: &str, from: usize, to: usize) -> Option<&str> {
    let mut pos = 0_usize;
    let mut byte_from = None;
    let mut byte_to = None;
    for (byte_idx, ch) in s.char_indices() {
        if pos == from {
            byte_from = Some(byte_idx);
        }
        pos += ch.len_utf16();
        if pos == to {
            byte_to = Some(byte_idx + ch.len_utf8());
        }
        if pos >= to {
            break;
        }
    }
    Some(&s[byte_from?..byte_to?])
}

fn slice_utf16<'a>(
    start: i32,
    end: i32,
    units: &[u16],
    budget: &mut EvalBudget,
) -> PrimitiveOutcome<'a> {
    let from = clamp_slice_index(start, units.len());
    let to = clamp_slice_index(end, units.len());
    if from >= to {
        return PrimitiveOutcome::Known(Value::String(rc_str("")));
    }
    let sub = &units[from..to];
    if let Err(reason) = budget.charge_steps(u32::try_from(sub.len() / 8).unwrap_or(u32::MAX)) {
        return PrimitiveOutcome::Stop(reason);
    }
    if let Ok(utf8_len) = decode_utf16(sub, None) {
        if let Err(reason) = budget.charge_bytes(utf8_len) {
            return PrimitiveOutcome::Stop(reason);
        }
        let mut decoded = String::with_capacity(utf8_len as usize);
        if decode_utf16(sub, Some(&mut decoded)).is_ok() {
            return PrimitiveOutcome::Known(Value::String(rc_str(&decoded)));
        }
    }
    let bytes = u32::try_from(sub.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(2);
    if let Err(reason) = budget.charge_bytes(bytes) {
        return PrimitiveOutcome::Stop(reason);
    }
    PrimitiveOutcome::Known(Value::Utf16(Rc::new(sub.to_vec())))
}

#[derive(Clone, Copy)]
pub(super) enum StringPred {
    StartsWith,
    Contains,
    EndsWith,
}

fn eval_str_pred<'a>(
    pred: StringPred,
    needle: &str,
    haystack: &str,
    budget: &mut EvalBudget,
) -> PrimitiveOutcome<'a> {
    let scan_bytes = match pred {
        StringPred::StartsWith | StringPred::EndsWith => needle.len().min(haystack.len()),
        StringPred::Contains => haystack.len().saturating_add(needle.len()),
    };
    if let Err(reason) = budget.charge_steps(u32::try_from(scan_bytes / 16).unwrap_or(u32::MAX)) {
        return PrimitiveOutcome::Stop(reason);
    }
    let matched = match pred {
        StringPred::StartsWith => haystack.starts_with(needle),
        StringPred::Contains => haystack.contains(needle),
        StringPred::EndsWith => haystack.ends_with(needle),
    };
    PrimitiveOutcome::Known(Value::Bool(matched))
}

pub(super) fn eval_string_pred<'a>(
    pred: StringPred,
    needle: &Value<'a>,
    haystack: &Value<'a>,
    budget: &mut EvalBudget,
) -> PrimitiveOutcome<'a> {
    if let (Value::String(n), Value::String(h)) = (needle, haystack) {
        return eval_str_pred(pred, n, h, budget);
    }

    // Decline lone-surrogate needles: JS searches UTF-16 code units while Go's
    // `flickrt.Includes`/`StartsWith`/`EndsWith` performs WTF-8 byte search.
    let (needle_utf16_len, needle_utf8_len) = match needle {
        Value::String(n) => {
            if let Err(reason) =
                budget.charge_steps(u32::try_from(n.len() / 16).unwrap_or(u32::MAX))
            {
                return PrimitiveOutcome::Stop(reason);
            }
            (utf16_len_of_str(n), None)
        }
        Value::Utf16(nu) => {
            if let Err(reason) =
                budget.charge_steps(u32::try_from(nu.len() / 8).unwrap_or(u32::MAX))
            {
                return PrimitiveOutcome::Stop(reason);
            }
            let Ok(utf8_len) = decode_utf16(nu, None) else {
                return PrimitiveOutcome::Stop(StopReason::UnknownOp);
            };
            (nu.len(), Some(utf8_len))
        }
        _ => return PrimitiveOutcome::Stop(StopReason::UnknownOp),
    };

    match haystack {
        Value::Utf16(hu) => {
            let work = match pred {
                StringPred::StartsWith | StringPred::EndsWith => hu.len().min(needle_utf16_len) / 8,
                StringPred::Contains => {
                    let starts = hu.len().saturating_sub(needle_utf16_len).saturating_add(1);
                    starts.saturating_mul(needle_utf16_len.max(1)) / 16
                }
            };
            if let Err(reason) = budget.charge_steps(u32::try_from(work).unwrap_or(u32::MAX)) {
                return PrimitiveOutcome::Stop(reason);
            }
            let matched = match pred {
                StringPred::StartsWith => utf16_starts_with(hu, needle),
                StringPred::EndsWith => {
                    needle_utf16_len <= hu.len()
                        && utf16_starts_with(&hu[hu.len() - needle_utf16_len..], needle)
                }
                StringPred::Contains => utf16_contains(hu, needle_utf16_len, needle),
            };
            PrimitiveOutcome::Known(Value::Bool(matched))
        }
        Value::String(h) => {
            let (Value::Utf16(nu), Some(utf8_len)) = (needle, needle_utf8_len) else {
                return PrimitiveOutcome::Stop(StopReason::UnknownOp);
            };
            if let Err(reason) = budget.charge_bytes(utf8_len) {
                return PrimitiveOutcome::Stop(reason);
            }
            let mut decoded = String::with_capacity(utf8_len as usize);
            if decode_utf16(nu, Some(&mut decoded)).is_err() {
                return PrimitiveOutcome::Stop(StopReason::UnknownOp);
            }
            eval_str_pred(pred, &decoded, h, budget)
        }
        _ => PrimitiveOutcome::Stop(StopReason::UnknownOp),
    }
}

fn utf16_starts_with(haystack: &[u16], needle: &Value<'_>) -> bool {
    let Some((_, mut src)) = utf16_source_of(needle) else {
        return false;
    };
    let mut i = 0;
    while let Some(u) = src.next_unit() {
        if haystack.get(i) != Some(&u) {
            return false;
        }
        i += 1;
    }
    true
}

fn utf16_contains(haystack: &[u16], needle_len: usize, needle: &Value<'_>) -> bool {
    if needle_len == 0 {
        return true;
    }
    if needle_len > haystack.len() {
        return false;
    }
    let last_start = haystack.len() - needle_len;
    let mut start = 0;
    while start <= last_start {
        if utf16_starts_with(&haystack[start..], needle) {
            return true;
        }
        start += 1;
    }
    false
}
