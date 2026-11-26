//! Counted `DateTime` primitives (`packages/std/src/DateTime.fl`) dispatch.

use flick_core::ir::Lit;
use flick_core::ir::memory::DateTimeIntrinsic;
use flick_core::opt::{js_number, json_string};

use super::datetime_date::{JsDate, offset_to_string, time_clip};
use super::datetime_format::{FormatMode, format_dt};
use super::datetime_parse::{make_zoned_from_string, parse_instant};
use super::datetime_parts::{
    add_unit, duration_millis, end_of_date, finish_mutate, format_iso_offset, format_iso_zoned,
    parts_record, set_parts, start_of_date, week_start,
};
pub(super) use super::datetime_parts::{decode_datetime_leaf, encode_datetime_leaf};
use super::datetime_tzif::{
    dt_to_date, dt_zoned_offset, local_zone, make_zoned, make_zoned_from_adjusted, tz_to_string,
    zone_from_string, zone_named,
};
use super::duration_ops::from_millis;
use super::{Data, DateTimeData, Error, Heap, TimeZoneData, Value, charge};

pub(super) fn dt_eq(a: &DateTimeData, b: &DateTimeData) -> bool {
    a.ms == b.ms
}

#[expect(
    clippy::too_many_lines,
    reason = "centralized DateTimeIntrinsic dispatch"
)]
pub(super) fn run(
    heap: &mut Heap,
    op: DateTimeIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        DateTimeIntrinsic::Make => {
            let s = heap.string_lossy(&args[0])?;
            match parse_instant(&s) {
                Some(d) if d.ms.is_finite() => ok_dt(heap, d),
                _ => dt_error(heap, &format!("not a date-time: {}", json_string(&s))),
            }
        }
        DateTimeIntrinsic::FromParts => {
            let mut date = JsDate::new(0.0);
            set_parts(heap, &mut date, &args[0])?;
            if date.t.is_finite() {
                ok_dt(
                    heap,
                    DateTimeData {
                        ms: date.t,
                        zone: None,
                    },
                )
            } else {
                dt_error(heap, "invalid date parts")
            }
        }
        DateTimeIntrinsic::FromEpochSeconds => {
            let n = expect_float(&args[0])?;
            let ms = time_clip(n * 1000.0);
            if ms.is_finite() {
                ok_dt(heap, DateTimeData { ms, zone: None })
            } else {
                dt_error(
                    heap,
                    &format!("epoch seconds out of range: {}", js_number(n)),
                )
            }
        }
        DateTimeIntrinsic::MakeZoned => {
            let (adjust, disamb) = zone_options(heap, &args[0])?;
            let zone = expect_tz(heap, &args[1])?;
            let d = expect_dt(heap, &args[2])?;
            match make_zoned(&d, zone, adjust, &disamb) {
                Ok(res) if res.ms.is_finite() => ok_dt(heap, res),
                _ => dt_error(heap, "cannot place the date-time in the zone"),
            }
        }
        DateTimeIntrinsic::MakeZonedFromString => {
            let s = heap.string_lossy(&args[0])?;
            match make_zoned_from_string(&s) {
                Some(d) if d.ms.is_finite() => ok_dt(heap, d),
                _ => dt_error(heap, &format!("not a zoned date-time: {}", json_string(&s))),
            }
        }
        DateTimeIntrinsic::UnsafeFromEpochMillis => {
            let ms = expect_float(&args[0])?;
            Ok(heap.allocate(Data::DateTime(DateTimeData { ms, zone: None })))
        }
        DateTimeIntrinsic::IsUtc => Ok(Value::Bool(expect_dt(heap, &args[0])?.zone.is_none())),
        DateTimeIntrinsic::IsZoned => Ok(Value::Bool(expect_dt(heap, &args[0])?.zone.is_some())),
        DateTimeIntrinsic::ToUtc => {
            let d = expect_dt(heap, &args[0])?;
            Ok(heap.allocate(Data::DateTime(DateTimeData {
                ms: d.ms,
                zone: None,
            })))
        }
        DateTimeIntrinsic::SetZone => {
            let (adjust, disamb) = zone_options(heap, &args[0])?;
            let zone = expect_tz(heap, &args[1])?;
            let d = expect_dt(heap, &args[2])?;
            set_zone_res(heap, "set_zone", &d, zone, adjust, &disamb)
        }
        DateTimeIntrinsic::SetZoneOffset => {
            let (adjust, disamb) = zone_options(heap, &args[0])?;
            let off = expect_float(&args[1])?;
            let d = expect_dt(heap, &args[2])?;
            set_zone_res(
                heap,
                "set_zone_offset",
                &d,
                TimeZoneData::Offset(off),
                adjust,
                &disamb,
            )
        }
        DateTimeIntrinsic::SetZoneNamed => {
            let (adjust, disamb) = zone_options(heap, &args[0])?;
            let id = heap.string_lossy(&args[1])?;
            let d = expect_dt(heap, &args[2])?;
            let Some(zone) = zone_named(&id) else {
                return dt_error(heap, &format!("invalid time zone: {}", json_string(&id)));
            };
            set_zone_res(heap, "set_zone_named", &d, zone, adjust, &disamb)
        }
        DateTimeIntrinsic::ZoneMakeOffset => {
            let off = expect_float(&args[0])?;
            Ok(heap.allocate(Data::TimeZone(TimeZoneData::Offset(off))))
        }
        DateTimeIntrinsic::ZoneMakeNamed => {
            let id = heap.string_lossy(&args[0])?;
            match zone_named(&id) {
                Some(z) => {
                    let zv = heap.allocate(Data::TimeZone(z));
                    heap.construct("Prelude.Result.Ok", vec![zv])
                }
                None => dt_error(heap, &format!("invalid time zone: {}", json_string(&id))),
            }
        }
        DateTimeIntrinsic::ZoneMakeLocal => Ok(heap.allocate(Data::TimeZone(local_zone()))),
        DateTimeIntrinsic::ZoneFromString => {
            let s = heap.string_lossy(&args[0])?;
            match zone_from_string(&s) {
                Some(z) => {
                    let zv = heap.allocate(Data::TimeZone(z));
                    heap.construct("Prelude.Result.Ok", vec![zv])
                }
                None => dt_error(heap, &format!("invalid time zone: {}", json_string(&s))),
            }
        }
        DateTimeIntrinsic::ZoneToString => {
            let z = expect_tz(heap, &args[0])?;
            heap.literal(&Lit::String(tz_to_string(&z)))
        }
        DateTimeIntrinsic::ToEpochMillis => Ok(Value::Float(expect_dt(heap, &args[0])?.ms)),
        DateTimeIntrinsic::ToEpochSeconds => Ok(Value::Float(
            (expect_dt(heap, &args[0])?.ms / 1000.0).floor(),
        )),
        DateTimeIntrinsic::WithEpochMillis => {
            let ms = expect_float(&args[0])?;
            let d = expect_dt(heap, &args[1])?;
            Ok(heap.allocate(Data::DateTime(DateTimeData { ms, zone: d.zone })))
        }
        DateTimeIntrinsic::ToParts => {
            let d = expect_dt(heap, &args[0])?;
            let date = dt_to_date(&d).map_err(Error::RangeError)?;
            parts_record(heap, date)
        }
        DateTimeIntrinsic::ToPartsUtc => {
            let d = expect_dt(heap, &args[0])?;
            parts_record(heap, JsDate::new(d.ms))
        }
        DateTimeIntrinsic::SetParts => {
            let d = expect_dt(heap, &args[1])?;
            let mut date = dt_to_date(&d).map_err(Error::RangeError)?;
            set_parts(heap, &mut date, &args[0])?;
            let out = finish_mutate(&d, date).map_err(Error::RangeError)?;
            Ok(heap.allocate(Data::DateTime(out)))
        }
        DateTimeIntrinsic::SetPartsUtc => {
            let d = expect_dt(heap, &args[1])?;
            let mut date = JsDate::new(d.ms);
            set_parts(heap, &mut date, &args[0])?;
            Ok(heap.allocate(Data::DateTime(DateTimeData {
                ms: date.t,
                zone: d.zone,
            })))
        }
        DateTimeIntrinsic::RemoveTime => {
            let d = expect_dt(heap, &args[0])?;
            let mut date = dt_to_date(&d).map_err(Error::RangeError)?;
            date.set_hours(0.0, Some(0.0), Some(0.0), Some(0.0));
            Ok(heap.allocate(Data::DateTime(DateTimeData {
                ms: date.t,
                zone: None,
            })))
        }
        DateTimeIntrinsic::ZonedOffset => {
            let d = expect_dt(heap, &args[0])?;
            if d.zone.is_none() {
                heap.construct("Prelude.Option.None", vec![])
            } else {
                let off = dt_zoned_offset(&d).map_err(Error::RangeError)?;
                heap.construct("Prelude.Option.Some", vec![Value::Float(off)])
            }
        }
        DateTimeIntrinsic::ZonedOffsetIso => {
            let d = expect_dt(heap, &args[0])?;
            if d.zone.is_none() {
                heap.construct("Prelude.Option.None", vec![])
            } else {
                let off = dt_zoned_offset(&d).map_err(Error::RangeError)?;
                let s = heap.literal(&Lit::String(offset_to_string(off)))?;
                heap.construct("Prelude.Option.Some", vec![s])
            }
        }
        DateTimeIntrinsic::Add => {
            let unit = ctor_name(heap, &args[0])?;
            let n = expect_float(&args[1])?;
            let d = expect_dt(heap, &args[2])?;
            let out = add_unit(&d, &unit, n).map_err(Error::RangeError)?;
            Ok(heap.allocate(Data::DateTime(out)))
        }
        DateTimeIntrinsic::Subtract => {
            let unit = ctor_name(heap, &args[0])?;
            let n = expect_float(&args[1])?;
            let d = expect_dt(heap, &args[2])?;
            let out = add_unit(&d, &unit, -n).map_err(Error::RangeError)?;
            Ok(heap.allocate(Data::DateTime(out)))
        }
        DateTimeIntrinsic::AddDuration | DateTimeIntrinsic::SubtractDuration => {
            let dur_ms = duration_millis(heap, &args[0])?;
            let d = expect_dt(heap, &args[1])?;
            let ms = if op == DateTimeIntrinsic::AddDuration {
                d.ms + dur_ms
            } else {
                d.ms - dur_ms
            };
            Ok(heap.allocate(Data::DateTime(DateTimeData { ms, zone: d.zone })))
        }
        DateTimeIntrinsic::StartOf | DateTimeIntrinsic::EndOf | DateTimeIntrinsic::Nearest => {
            let week = week_start(heap, &args[0])?;
            let unit = ctor_name(heap, &args[1])?;
            let d = expect_dt(heap, &args[2])?;
            let mut date = dt_to_date(&d).map_err(Error::RangeError)?;
            match op {
                DateTimeIntrinsic::StartOf => start_of_date(&mut date, &unit, week),
                DateTimeIntrinsic::EndOf => end_of_date(&mut date, &unit, week),
                _ if unit != "Millisecond" => {
                    let mut s = JsDate::new(date.t);
                    let mut e = JsDate::new(date.t);
                    start_of_date(&mut s, &unit, week);
                    end_of_date(&mut e, &unit, week);
                    if date.t - s.t < e.t + 1.0 - date.t {
                        date.set_time(s.t);
                    } else {
                        date.set_time(e.t + 1.0);
                    }
                }
                _ => {}
            }
            let out = finish_mutate(&d, date).map_err(Error::RangeError)?;
            Ok(heap.allocate(Data::DateTime(out)))
        }
        DateTimeIntrinsic::Distance => {
            let other = expect_dt(heap, &args[0])?;
            let self_d = expect_dt(heap, &args[1])?;
            Ok(heap.allocate(Data::Duration(from_millis((other.ms - self_d.ms).abs()))))
        }
        DateTimeIntrinsic::Compare => {
            let a = expect_dt(heap, &args[0])?;
            let b = expect_dt(heap, &args[1])?;
            let tag = if a.ms < b.ms {
                "Prelude.Ordering.LT"
            } else if a.ms > b.ms {
                "Prelude.Ordering.GT"
            } else {
                "Prelude.Ordering.EQ"
            };
            heap.construct(tag, vec![])
        }
        DateTimeIntrinsic::Format
        | DateTimeIntrinsic::FormatLocal
        | DateTimeIntrinsic::FormatUtc
        | DateTimeIntrinsic::FormatIntl => {
            let (what, mode) = match op {
                DateTimeIntrinsic::Format => ("format", FormatMode::Zone),
                DateTimeIntrinsic::FormatLocal => ("format_local", FormatMode::Local),
                DateTimeIntrinsic::FormatUtc => ("format_utc", FormatMode::Utc),
                _ => ("format_intl", FormatMode::Intl),
            };
            let loc = opt_string(heap, heap.named_field(&args[0], "locale")?)?;
            let ds = opt_string(heap, heap.named_field(&args[0], "dateStyle")?)?;
            let ts = opt_string(heap, heap.named_field(&args[0], "timeStyle")?)?;
            let d = expect_dt(heap, &args[1])?;
            match format_dt(mode, loc.as_deref(), ds.as_deref(), ts.as_deref(), &d) {
                Ok(text) => {
                    let s = heap.literal(&Lit::String(text))?;
                    heap.construct("Prelude.Result.Ok", vec![s])
                }
                Err(msg) => dt_error(heap, &format!("{what}: {msg}")),
            }
        }
        DateTimeIntrinsic::FormatIso => {
            let d = expect_dt(heap, &args[0])?;
            let iso = JsDate::new(d.ms).iso_string().map_err(Error::RangeError)?;
            heap.literal(&Lit::String(iso))
        }
        DateTimeIntrinsic::FormatIsoDate => {
            let d = expect_dt(heap, &args[0])?;
            let iso = dt_to_date(&d)
                .and_then(JsDate::iso_string)
                .map_err(Error::RangeError)?;
            heap.literal(&Lit::String(iso[..10].to_owned()))
        }
        DateTimeIntrinsic::FormatIsoDateUtc => {
            let d = expect_dt(heap, &args[0])?;
            let iso = JsDate::new(d.ms).iso_string().map_err(Error::RangeError)?;
            heap.literal(&Lit::String(iso[..10].to_owned()))
        }
        DateTimeIntrinsic::FormatIsoOffset => {
            let d = expect_dt(heap, &args[0])?;
            let iso = format_iso_offset(&d).map_err(Error::RangeError)?;
            heap.literal(&Lit::String(iso))
        }
        DateTimeIntrinsic::FormatIsoZoned => {
            let d = expect_dt(heap, &args[0])?;
            if d.zone.is_none() {
                heap.construct("Prelude.Option.None", vec![])
            } else {
                let iso = format_iso_zoned(&d).map_err(Error::RangeError)?;
                let s = heap.literal(&Lit::String(iso))?;
                heap.construct("Prelude.Option.Some", vec![s])
            }
        }
    }
}

pub(super) fn expect_dt(heap: &Heap, val: &Value) -> Result<DateTimeData, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("DateTime"));
    };
    let Data::DateTime(d) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("DateTime"));
    };
    Ok(d.clone())
}

pub(super) fn expect_tz(heap: &Heap, val: &Value) -> Result<TimeZoneData, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("TimeZone"));
    };
    let Data::TimeZone(z) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("TimeZone"));
    };
    Ok(z.clone())
}

fn expect_float(val: &Value) -> Result<f64, Error> {
    match *val {
        Value::Float(f) => Ok(f),
        Value::Int(i) => Ok(f64::from(i)),
        _ => Err(Error::ExpectedFloat),
    }
}

pub(super) fn ok_dt(heap: &mut Heap, d: DateTimeData) -> Result<Value, Error> {
    let v = heap.allocate(Data::DateTime(d));
    heap.construct("Prelude.Result.Ok", vec![v])
}

fn dt_error(heap: &mut Heap, msg: &str) -> Result<Value, Error> {
    let s = heap.literal(&Lit::String(msg.into()))?;
    let e = heap.construct("DateTime.DateTimeError.DateTimeError", vec![s])?;
    heap.construct("Prelude.Result.Err", vec![e])
}

fn set_zone_res(
    heap: &mut Heap,
    what: &str,
    d: &DateTimeData,
    zone: TimeZoneData,
    adjust: bool,
    disamb: &str,
) -> Result<Value, Error> {
    let res = if adjust {
        make_zoned_from_adjusted(d.ms, zone, disamb)
    } else {
        Ok(DateTimeData {
            ms: d.ms,
            zone: Some(zone),
        })
    };
    match res {
        Ok(out) => ok_dt(heap, out),
        Err(msg) => dt_error(heap, &format!("{what}: {msg}")),
    }
}

pub(super) fn opt_inner<'a>(heap: &'a Heap, val: &'a Value) -> Result<Option<&'a Value>, Error> {
    if heap.tag(val)? == "Prelude.Option.Some" {
        Ok(Some(heap.field(val, 0)?))
    } else {
        Ok(None)
    }
}

fn opt_string(heap: &Heap, val: &Value) -> Result<Option<String>, Error> {
    match opt_inner(heap, val)? {
        Some(v) => Ok(Some(heap.string_lossy(v)?)),
        None => Ok(None),
    }
}

fn ctor_name(heap: &Heap, val: &Value) -> Result<String, Error> {
    let tag = heap.tag(val)?;
    Ok(tag.rsplit('.').next().unwrap_or(tag).to_owned())
}

fn zone_options(heap: &Heap, val: &Value) -> Result<(bool, String), Error> {
    let adjust = match opt_inner(heap, heap.named_field(val, "adjustForTimeZone")?)? {
        Some(Value::Bool(b)) => *b,
        _ => false,
    };
    let disamb = match opt_inner(heap, heap.named_field(val, "disambiguation")?)? {
        Some(v) => ctor_name(heap, v)?.to_ascii_lowercase(),
        None => "compatible".into(),
    };
    Ok((adjust, disamb))
}
