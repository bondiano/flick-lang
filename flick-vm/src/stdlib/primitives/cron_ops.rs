//! Counted `Cron` primitives (`packages/std/src/Cron.fl`), matching `runtime-go/flickrt/cron.go`.

use flick_core::ir::Lit;
use flick_core::ir::memory::CronIntrinsic;
use flick_core::opt::json_string;

use super::cron_step::step_cron;
use super::datetime_ops::{expect_dt, expect_tz};
use super::datetime_tzif::{dt_to_date, make_zoned, tz_equals};
use super::list_ops::slice_reps;
use super::{CronData, Data, DateTimeData, Error, Fuel, Heap, TimeZoneData, Value, charge};

pub(super) const SEC: usize = 0;
pub(super) const MIN: usize = 1;
pub(super) const HOUR: usize = 2;
pub(super) const DAY: usize = 3;
pub(super) const MONTH: usize = 4;
pub(super) const WDAY: usize = 5;

const FIELDS: [(&str, i32, i32, usize); 6] = [
    ("seconds", 0, 59, 60),
    ("minutes", 0, 59, 60),
    ("hours", 0, 23, 24),
    ("days", 1, 31, 32),
    ("months", 1, 12, 13),
    ("weekdays", 0, 7, 7),
];

pub(super) fn cron_equals_raw(a: &CronData, b: &CronData) -> bool {
    tz_equals(&a.tz, &b.tz) && a.and == b.and && a.fields == b.fields
}

pub(super) fn run(
    heap: &mut Heap,
    op: CronIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        CronIntrinsic::Make => cron_make(heap, &args[0], fuel),
        CronIntrinsic::Parse => {
            let tz = expect_tz(heap, &args[0])?;
            let text = heap.string_lossy(&args[1])?;
            match parse_cron(tz, &text) {
                Ok(c) => {
                    let cv = heap.allocate(Data::Cron(Box::new(c)));
                    heap.construct("Prelude.Result.Ok", vec![cv])
                }
                Err(msg) => cron_error(heap, &msg),
            }
        }
        CronIntrinsic::Format => {
            let inc = heap.named_field(&args[0], "includeSeconds")?;
            let include_seconds = heap.tag(inc)? == "Prelude.Option.Some"
                && matches!(heap.field(inc, 0)?, Value::Bool(true));
            let c = expect_cron(heap, &args[1])?;
            heap.literal(&Lit::String(format_cron(&c, include_seconds)))
        }
        CronIntrinsic::Match => {
            let now = expect_dt(heap, &args[0])?;
            let c = expect_cron(heap, &args[1])?;
            Ok(Value::Bool(
                match_cron(&c, &now).map_err(Error::RangeError)?,
            ))
        }
        CronIntrinsic::Next => {
            let now = expect_dt(heap, &args[0])?;
            let c = expect_cron(heap, &args[1])?;
            match step_cron(&c, &now, false) {
                Some(ms) => {
                    let dt = heap.allocate(Data::DateTime(DateTimeData { ms, zone: None }));
                    heap.construct("Prelude.Option.Some", vec![dt])
                }
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        CronIntrinsic::Prev => {
            let now = expect_dt(heap, &args[0])?;
            let c = expect_cron(heap, &args[1])?;
            match step_cron(&c, &now, true) {
                Some(ms) => {
                    let dt = heap.allocate(Data::DateTime(DateTimeData { ms, zone: None }));
                    heap.construct("Prelude.Option.Some", vec![dt])
                }
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        CronIntrinsic::Sequence => {
            let Value::Int(n) = args[0] else {
                return Err(Error::ExpectedInt);
            };
            let mut at = expect_dt(heap, &args[1])?;
            let c = expect_cron(heap, &args[2])?;
            let limit = usize::try_from(n).unwrap_or(0);
            let mut items = Vec::with_capacity(limit.min(1024));
            while items.len() < limit {
                if let Err(err) = charge(fuel, 1) {
                    heap.drop_values(items)?;
                    return Err(err);
                }
                let Some(ms) = step_cron(&c, &at, false) else {
                    break;
                };
                at = DateTimeData { ms, zone: None };
                items.push(heap.allocate(Data::DateTime(at.clone())));
            }
            heap.construct("$list", items)
        }
        CronIntrinsic::Equals => {
            let a = expect_cron(heap, &args[0])?;
            let b = expect_cron(heap, &args[1])?;
            Ok(Value::Bool(cron_equals_raw(&a, &b)))
        }
    }
}

fn expect_cron(heap: &Heap, val: &Value) -> Result<CronData, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Cron"));
    };
    let Data::Cron(c) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Cron"));
    };
    Ok((**c).clone())
}

fn cron_error(heap: &mut Heap, msg: &str) -> Result<Value, Error> {
    let s = heap.literal(&Lit::String(msg.into()))?;
    let e = heap.construct("Cron.CronParseError.CronParseError", vec![s])?;
    heap.construct("Prelude.Result.Err", vec![e])
}

fn cron_ints(heap: &Heap, val: &Value, fuel: Fuel<'_>) -> Result<Vec<i32>, Error> {
    let reps = slice_reps(heap, val)?;
    charge(fuel, reps.len())?;
    let mut out = Vec::with_capacity(reps.len());
    for item in &reps {
        let Value::Int(i) = *item else {
            return Err(Error::ExpectedInt);
        };
        out.push(i);
    }
    Ok(out)
}

fn cron_make(heap: &mut Heap, fields: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let sec_field = heap.named_field(fields, "seconds")?;
    let seconds = if heap.tag(sec_field)? == "Prelude.Option.Some" {
        cron_ints(heap, heap.field(sec_field, 0)?, fuel)?
    } else {
        vec![0]
    };
    let and_field = heap.named_field(fields, "and")?;
    let and = heap.tag(and_field)? == "Prelude.Option.Some"
        && matches!(heap.field(and_field, 0)?, Value::Bool(true));
    let tz = expect_tz(heap, heap.named_field(fields, "tz")?)?;
    let values = [
        seconds,
        cron_ints(heap, heap.named_field(fields, "minutes")?, fuel)?,
        cron_ints(heap, heap.named_field(fields, "hours")?, fuel)?,
        cron_ints(heap, heap.named_field(fields, "days")?, fuel)?,
        cron_ints(heap, heap.named_field(fields, "months")?, fuel)?,
        cron_ints(heap, heap.named_field(fields, "weekdays")?, fuel)?,
    ];
    match make_cron(tz, &values, and) {
        Ok(c) => {
            let cv = heap.allocate(Data::Cron(Box::new(c)));
            heap.construct("Prelude.Result.Ok", vec![cv])
        }
        Err(msg) => cron_error(heap, &msg),
    }
}

fn make_cron(tz: TimeZoneData, values: &[Vec<i32>; 6], and: bool) -> Result<CronData, String> {
    let firsts = [0, 0, 0, 1, 1, 0];
    let lasts = [59, 59, 23, 31, 12, 6];
    let mut fields: [Vec<i32>; 6] = Default::default();
    let mut first = [0i32; 6];
    let mut last = [0i32; 6];
    let mut next: [Vec<i32>; 6] = Default::default();
    let mut prev: [Vec<i32>; 6] = Default::default();
    for f in 0..6 {
        let (name, lo, hi, size) = FIELDS[f];
        let mut set = Vec::with_capacity(values[f].len());
        for &v in &values[f] {
            if v < lo || v > hi {
                return Err(format!(
                    "{name} must contain only integers between {lo} and {hi}"
                ));
            }
            set.push(if f == WDAY && v == 7 { 0 } else { v });
        }
        set.sort_unstable();
        set.dedup();
        if let (Some(&a), Some(&b)) = (set.first(), set.last()) {
            first[f] = a;
            last[f] = b;
        } else {
            first[f] = firsts[f];
            last[f] = lasts[f];
        }
        next[f] = cron_lookup(&set, size, true);
        prev[f] = cron_lookup(&set, size, false);
        fields[f] = set;
    }
    first[MONTH] -= 1;
    last[MONTH] -= 1;
    Ok(CronData {
        tz,
        fields,
        and,
        first,
        last,
        next,
        prev,
    })
}

fn cron_lookup(values: &[i32], size: usize, is_next: bool) -> Vec<i32> {
    let mut out = vec![-1; size];
    for (i, slot) in out.iter_mut().enumerate() {
        let idx = i32::try_from(i).unwrap_or(i32::MAX);
        for &v in values {
            if (is_next && v >= idx && *slot == -1) || (!is_next && v <= idx) {
                *slot = v;
            }
        }
    }
    out
}

fn alias_number(s: &str, f: usize) -> Option<f64> {
    let lower = s.to_ascii_lowercase();
    if f == MONTH {
        return match lower.as_str() {
            "jan" => Some(1.0),
            "feb" => Some(2.0),
            "mar" => Some(3.0),
            "apr" => Some(4.0),
            "may" => Some(5.0),
            "jun" => Some(6.0),
            "jul" => Some(7.0),
            "aug" => Some(8.0),
            "sep" => Some(9.0),
            "oct" => Some(10.0),
            "nov" => Some(11.0),
            "dec" => Some(12.0),
            _ => None,
        };
    }
    if f == WDAY {
        return match lower.as_str() {
            "sun" => Some(0.0),
            "mon" => Some(1.0),
            "tue" => Some(2.0),
            "wed" => Some(3.0),
            "thu" => Some(4.0),
            "fri" => Some(5.0),
            "sat" => Some(6.0),
            _ => None,
        };
    }
    None
}

fn cron_number(s: &str, f: usize, allow_alias: bool) -> f64 {
    if allow_alias && let Some(v) = alias_number(s, f) {
        return v;
    }
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

#[expect(clippy::float_cmp, reason = "exact integer check")]
fn is_integer(x: f64) -> bool {
    x.is_finite() && x == x.trunc()
}

fn split_step(s: &str, f: usize) -> (&str, f64, bool) {
    if let Some((raw, step)) = s.split_once('/') {
        (raw, cron_number(step, f, false), true)
    } else {
        (s, 0.0, false)
    }
}

#[expect(
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    reason = "bounded cron field integers"
)]
fn parse_segment(input: &str, f: usize) -> Result<(Vec<i32>, bool), String> {
    let (_, lo, hi, _) = FIELDS[f];
    let mut values = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();
    let (first_raw, first_step, first_has_step) = split_step(parts[0], f);
    let wildcard = first_raw == "*";
    let ignore_add = wildcard && (!first_has_step || first_step == 1.0);
    let mut add = |mut v: i32| {
        if ignore_add {
            return;
        }
        if f == WDAY && v == 7 {
            v = 0;
        }
        if !values.contains(&v) {
            values.push(v);
        }
    };
    let between = format!("Expected a value between {lo} and {hi}");
    for (index, &text) in parts.iter().enumerate() {
        let (raw, step, has_step) = if index == 0 {
            (first_raw, first_step, first_has_step)
        } else {
            split_step(text, f)
        };
        let mut by = 1i32;
        if has_step {
            if !is_integer(step) {
                return Err("Expected step value to be a positive integer".into());
            }
            if step < 1.0 {
                return Err("Expected step value to be greater than 0".into());
            }
            if step > f64::from(hi) {
                return Err(format!(
                    "Expected step value to be less than or equal to {hi}"
                ));
            }
            by = step as i32;
        }
        if raw == "*" {
            if index == 0 && by == 1 {
                continue;
            }
            let mut i = lo;
            while i <= hi {
                add(i);
                i += by;
            }
            continue;
        }
        let (left_text, right_opt) = match raw.split_once('-') {
            Some((l, r)) => (l, Some(r)),
            None => (raw, None),
        };
        let left = cron_number(left_text, f, true);
        if !is_integer(left) {
            return Err("Expected a positive integer".into());
        }
        if left < f64::from(lo) || left > f64::from(hi) {
            return Err(between);
        }
        let Some(right_text) = right_opt else {
            let end = if has_step { hi } else { left as i32 };
            let mut i = left as i32;
            while i <= end {
                add(i);
                i += by;
            }
            continue;
        };
        let right = cron_number(right_text, f, true);
        if !is_integer(right) {
            return Err("Expected a positive integer".into());
        }
        if right < f64::from(lo) || right > f64::from(hi) {
            return Err(between);
        }
        if left > right {
            return Err("Invalid value range".into());
        }
        let mut i = left as i32;
        while i <= right as i32 {
            add(i);
            i += by;
        }
    }
    Ok((values, wildcard))
}

fn parse_cron(tz: TimeZoneData, text: &str) -> Result<CronData, String> {
    let mut segments: Vec<&str> = text.split_whitespace().collect();
    if segments.len() != 5 && segments.len() != 6 {
        return Err(format!(
            "Invalid number of segments in cron expression: {}",
            json_string(text)
        ));
    }
    if segments.len() == 5 {
        segments.insert(0, "0");
    }
    let mut values: [Vec<i32>; 6] = Default::default();
    let mut wildcards = [false; 6];
    for (f, seg) in segments.iter().enumerate() {
        let (v, wc) = parse_segment(seg, f).map_err(|m| format!("{m}: {}", json_string(text)))?;
        values[f] = v;
        wildcards[f] = wc;
    }
    let and =
        (wildcards[DAY] || wildcards[WDAY]) && !values[DAY].is_empty() && !values[WDAY].is_empty();
    make_cron(tz, &values, and)
}

fn format_segment(values: &[i32]) -> String {
    if values.is_empty() {
        return "*".into();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < values.len() {
        let start = values[i];
        if i + 2 < values.len() && values[i + 2] - values[i + 1] == values[i + 1] - start {
            let step = values[i + 1] - start;
            let mut end = i + 2;
            while end + 1 < values.len() && values[end + 1] - values[end] == step {
                end += 1;
            }
            if step == 1 {
                out.push(format!("{start}-{}", values[end]));
            } else {
                out.push(format!("{start}-{}/{step}", values[end]));
            }
            i = end + 1;
        } else {
            out.push(start.to_string());
            i += 1;
        }
    }
    out.join(",")
}

fn format_cron(c: &CronData, include_seconds: bool) -> String {
    let mut segs: Vec<String> = c.fields.iter().map(|f| format_segment(f)).collect();
    if !include_seconds && c.fields[SEC] == [0] {
        segs.remove(0);
    }
    segs.join(" ")
}

#[expect(clippy::cast_possible_truncation, reason = "bounded calendar parts")]
fn match_cron(c: &CronData, now: &DateTimeData) -> Result<bool, String> {
    let date = dt_to_date(&make_zoned(now, c.tz.clone(), false, "")?)?;
    let parts = [
        date.seconds() as i32,
        date.minutes() as i32,
        date.hours() as i32,
        date.date() as i32,
        date.month() as i32 + 1,
        date.week_day() as i32,
    ];
    let has = |f: usize| c.fields[f].contains(&parts[f]);
    for f in [SEC, MIN, HOUR, MONTH] {
        if !c.fields[f].is_empty() && !has(f) {
            return Ok(false);
        }
    }
    let (days, wdays) = (!c.fields[DAY].is_empty(), !c.fields[WDAY].is_empty());
    Ok(if !days && !wdays {
        true
    } else if c.and {
        (!days || has(DAY)) && (!wdays || has(WDAY))
    } else if !wdays {
        has(DAY)
    } else if !days {
        has(WDAY)
    } else {
        has(DAY) || has(WDAY)
    })
}
