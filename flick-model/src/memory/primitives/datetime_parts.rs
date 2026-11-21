//! `DateTime` calendar parts, unit arithmetic, ISO offset formatting, and JSON leaf codecs.

use flick_core::ir::Lit;
use num_traits::ToPrimitive;

use super::datetime_date::{
    JsDate, MAX_TIME_VALUE, MS_PER_HOUR, MS_PER_MINUTE, offset_to_string, pos_mod, time_clip,
};
use super::datetime_ops::{expect_dt, expect_tz, ok_dt, opt_inner};
use super::datetime_parse::{make_zoned_from_string, parse_instant};
use super::datetime_tzif::{
    dt_to_date, dt_zoned_offset, make_zoned_from_adjusted, tz_to_string, zone_from_string,
    zone_named,
};
use super::json_ops::schema_error_single;
use super::schema_leaves::{is_str_val, number_of};
use super::schema_ops::expected_err;
use super::{Data, DateTimeData, DurationData, Error, Heap, TimeZoneData, Value};

pub(super) fn decode_datetime_leaf(
    heap: &mut Heap,
    kind: &str,
    input: &Value,
) -> Result<Value, Error> {
    match kind {
        "datetime_utc" => {
            if !is_str_val(heap, input)? {
                return expected_err(
                    heap,
                    "a string that will be decoded as a DateTime.Utc",
                    input,
                );
            }
            match parse_instant(&heap.string_lossy(input)?) {
                Some(d) => ok_dt(heap, d),
                None => schema_error_single(heap, "", "an invalid value"),
            }
        }
        "datetime_utc_millis" => {
            let Some(x) = number_of(input) else {
                return expected_err(heap, "a number", input);
            };
            #[expect(clippy::float_cmp, reason = "exact safe-integer check")]
            if !x.is_finite() || x != x.trunc() || x.abs() > 9_007_199_254_740_991.0 {
                return expected_err(heap, "an integer", input);
            }
            if x.abs() <= MAX_TIME_VALUE {
                ok_dt(
                    heap,
                    DateTimeData {
                        ms: time_clip(x),
                        zone: None,
                    },
                )
            } else {
                schema_error_single(heap, "", "Invalid DateTime input")
            }
        }
        "datetime_zoned" => {
            if !is_str_val(heap, input)? {
                return expected_err(
                    heap,
                    "a zoned DateTime string (e.g. 2024-01-01T00:00:00.000+00:00[Europe/London])",
                    input,
                );
            }
            match make_zoned_from_string(&heap.string_lossy(input)?) {
                Some(d) => ok_dt(heap, d),
                None => schema_error_single(heap, "", "an invalid value"),
            }
        }
        "datetime_zone" => {
            if !is_str_val(heap, input)? {
                return expected_err(
                    heap,
                    "a time zone string (IANA identifier or offset like +03:00)",
                    input,
                );
            }
            match zone_from_string(&heap.string_lossy(input)?) {
                Some(z) => {
                    let zv = heap.allocate(Data::TimeZone(z));
                    heap.construct("Prelude.Result.Ok", vec![zv])
                }
                None => schema_error_single(heap, "", "an invalid value"),
            }
        }
        "datetime_zone_named" => {
            if !is_str_val(heap, input)? {
                return expected_err(heap, "an IANA time zone identifier", input);
            }
            match zone_named(&heap.string_lossy(input)?) {
                Some(z) => {
                    let zv = heap.allocate(Data::TimeZone(z));
                    heap.construct("Prelude.Result.Ok", vec![zv])
                }
                None => schema_error_single(heap, "", "an invalid value"),
            }
        }
        "datetime_zone_offset" => {
            let Some(x) = number_of(input).filter(|v| v.is_finite()) else {
                return expected_err(heap, "a finite number", input);
            };
            let zv = heap.allocate(Data::TimeZone(TimeZoneData::Offset(x)));
            heap.construct("Prelude.Result.Ok", vec![zv])
        }
        _ => Err(Error::RangeError(format!("unknown datetime leaf: {kind}"))),
    }
}

pub(super) fn encode_datetime_leaf(
    heap: &mut Heap,
    kind: &str,
    val: &Value,
) -> Result<Value, Error> {
    match kind {
        "datetime_utc" => {
            let Ok(d) = expect_dt(heap, val) else {
                return expected_err(heap, "DateTime.Utc", val);
            };
            if d.zone.is_some() {
                return expected_err(heap, "DateTime.Utc", val);
            }
            let iso = JsDate::new(d.ms).iso_string().map_err(Error::RangeError)?;
            let s = heap.literal(&Lit::String(iso))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "datetime_utc_millis" => {
            let Ok(d) = expect_dt(heap, val) else {
                return expected_err(heap, "DateTime.Utc", val);
            };
            if d.zone.is_some() {
                return expected_err(heap, "DateTime.Utc", val);
            }
            heap.construct("Prelude.Result.Ok", vec![Value::Float(d.ms)])
        }
        "datetime_zoned" => {
            let Ok(d) = expect_dt(heap, val) else {
                return expected_err(heap, "DateTime.Zoned", val);
            };
            if d.zone.is_none() {
                return expected_err(heap, "DateTime.Zoned", val);
            }
            let iso = format_iso_zoned(&d).map_err(Error::RangeError)?;
            let s = heap.literal(&Lit::String(iso))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "datetime_zone" => {
            let Ok(z) = expect_tz(heap, val) else {
                return expected_err(heap, "DateTime.TimeZone", val);
            };
            let s = heap.literal(&Lit::String(tz_to_string(&z)))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "datetime_zone_named" => {
            let Ok(TimeZoneData::Named { id, .. }) = expect_tz(heap, val) else {
                return expected_err(heap, "DateTime.TimeZone.Named", val);
            };
            let s = heap.literal(&Lit::String(id))?;
            heap.construct("Prelude.Result.Ok", vec![s])
        }
        "datetime_zone_offset" => {
            let Ok(TimeZoneData::Offset(off)) = expect_tz(heap, val) else {
                return expected_err(heap, "DateTime.TimeZone.Offset", val);
            };
            heap.construct("Prelude.Result.Ok", vec![Value::Float(off)])
        }
        _ => Err(Error::RangeError(format!("unknown datetime leaf: {kind}"))),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "bounded calendar part ints"
)]
pub(super) fn parts_record(heap: &mut Heap, date: JsDate) -> Result<Value, Error> {
    heap.record(vec![
        ("millisecond".into(), Value::Int(date.millis() as i32)),
        ("second".into(), Value::Int(date.seconds() as i32)),
        ("minute".into(), Value::Int(date.minutes() as i32)),
        ("hour".into(), Value::Int(date.hours() as i32)),
        ("day".into(), Value::Int(date.date() as i32)),
        ("weekDay".into(), Value::Int(date.week_day() as i32)),
        ("month".into(), Value::Int((date.month() + 1.0) as i32)),
        ("year".into(), Value::Int(date.year() as i32)),
    ])
}

pub(super) fn set_parts(heap: &Heap, date: &mut JsDate, parts: &Value) -> Result<(), Error> {
    let part = |name: &str| -> Result<Option<f64>, Error> {
        match opt_inner(heap, heap.named_field(parts, name)?)? {
            Some(Value::Int(i)) => Ok(Some(f64::from(*i))),
            Some(Value::Float(f)) => Ok(Some(*f)),
            _ => Ok(None),
        }
    };
    let (year, month, day) = (part("year")?, part("month")?, part("day")?);
    if year.is_some() || month.is_some() || day.is_some() {
        date.set_full_year(
            year.unwrap_or_else(|| date.year()),
            Some(month.map_or_else(|| date.month(), |m| m - 1.0)),
            Some(day.unwrap_or_else(|| date.date())),
        );
    }
    if let Some(wd) = part("weekDay")? {
        date.set_date(date.date() + wd - date.week_day());
    }
    if let Some(h) = part("hour")? {
        date.set_hours(h, None, None, None);
    }
    if let Some(m) = part("minute")? {
        date.set_minutes(m, None, None);
    }
    if let Some(s) = part("second")? {
        date.set_seconds(s, None);
    }
    if let Some(ms) = part("millisecond")? {
        date.set_milliseconds(ms);
    }
    Ok(())
}

pub(super) fn finish_mutate(d: &DateTimeData, date: JsDate) -> Result<DateTimeData, String> {
    match &d.zone {
        None => Ok(DateTimeData {
            ms: date.t,
            zone: None,
        }),
        Some(z) => make_zoned_from_adjusted(date.t, z.clone(), "compatible"),
    }
}

pub(super) fn add_unit(d: &DateTimeData, unit: &str, n: f64) -> Result<DateTimeData, String> {
    let set = n != 0.0 && !n.is_nan();
    let instant_scale = match unit {
        "Millisecond" => Some(1.0),
        "Second" => Some(1000.0),
        "Minute" => Some(MS_PER_MINUTE),
        "Hour" => Some(MS_PER_HOUR),
        _ => None,
    };
    let mut utc = JsDate::new(d.ms);
    if set && let Some(scale) = instant_scale {
        utc.set_time(utc.t + n * scale);
    }
    let shifted = DateTimeData {
        ms: utc.t,
        zone: d.zone.clone(),
    };
    if !set || instant_scale.is_some() {
        return Ok(shifted);
    }
    let mut date = dt_to_date(&shifted)?;
    match unit {
        "Day" => date.set_date(date.date() + n),
        "Week" => date.set_date(date.date() + n * 7.0),
        "Month" => {
            let day = date.date();
            date.set_month(date.month() + n + 1.0, Some(0.0));
            if day < date.date() {
                date.set_date(day);
            }
        }
        "Year" => {
            let (day, month) = (date.date(), date.month());
            date.set_full_year(date.year() + n, Some(month + 1.0), Some(0.0));
            if day < date.date() {
                date.set_date(day);
            }
        }
        _ => {}
    }
    finish_mutate(&shifted, date)
}

pub(super) fn week_start(heap: &Heap, val: &Value) -> Result<f64, Error> {
    match opt_inner(heap, heap.named_field(val, "weekStartsOn")?)? {
        Some(Value::Int(n)) => Ok(f64::from(n.rem_euclid(7))),
        _ => Ok(0.0),
    }
}

pub(super) fn start_of_date(date: &mut JsDate, unit: &str, week_starts_on: f64) {
    match unit {
        "Second" => date.set_milliseconds(0.0),
        "Minute" => date.set_seconds(0.0, Some(0.0)),
        "Hour" => date.set_minutes(0.0, Some(0.0), Some(0.0)),
        "Day" => date.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0)),
        "Week" => {
            date.set_date(date.date() - pos_mod(date.week_day() - week_starts_on + 7.0, 7.0));
            date.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0));
        }
        "Month" => {
            date.set_date(1.0);
            date.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0));
        }
        "Year" => {
            date.set_month(0.0, Some(1.0));
            date.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0));
        }
        _ => {}
    }
}

pub(super) fn end_of_date(date: &mut JsDate, unit: &str, week_starts_on: f64) {
    match unit {
        "Second" => date.set_milliseconds(999.0),
        "Minute" => date.set_seconds(59.0, Some(999.0)),
        "Hour" => date.set_minutes(59.0, Some(59.0), Some(999.0)),
        "Day" => date.set_hours(23.0, Some(59.0), Some(59.0), Some(999.0)),
        "Week" => {
            date.set_date(date.date() - pos_mod(date.week_day() - week_starts_on + 7.0, 7.0) + 6.0);
            date.set_hours(23.0, Some(59.0), Some(59.0), Some(999.0));
        }
        "Month" => {
            date.set_month(date.month() + 1.0, Some(0.0));
            date.set_hours(23.0, Some(59.0), Some(59.0), Some(999.0));
        }
        "Year" => {
            date.set_month(11.0, Some(31.0));
            date.set_hours(23.0, Some(59.0), Some(59.0), Some(999.0));
        }
        _ => {}
    }
}

pub(super) fn duration_millis(heap: &Heap, val: &Value) -> Result<f64, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Duration"));
    };
    let Data::Duration(d) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Duration"));
    };
    Ok(match d {
        DurationData::Millis(m) => *m,
        DurationData::Nanos(n) => n.to_f64().unwrap_or(f64::INFINITY) / 1_000_000.0,
        DurationData::Infinite => f64::INFINITY,
        DurationData::NegativeInfinite => f64::NEG_INFINITY,
    })
}

pub(super) fn format_iso_offset(d: &DateTimeData) -> Result<String, String> {
    let iso = dt_to_date(d)?.iso_string()?;
    if d.zone.is_none() {
        return Ok(iso);
    }
    Ok(format!(
        "{}{}",
        &iso[..iso.len() - 1],
        offset_to_string(dt_zoned_offset(d)?)
    ))
}

pub(super) fn format_iso_zoned(d: &DateTimeData) -> Result<String, String> {
    let base = format_iso_offset(d)?;
    match &d.zone {
        Some(TimeZoneData::Named { id, .. }) => Ok(format!("{base}[{id}]")),
        _ => Ok(base),
    }
}
