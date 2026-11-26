//! Forward/reverse cron occurrence stepper (`runtime-go/flickrt/cron_step.go`).

use super::cron_ops::{DAY, HOUR, MIN, MONTH, SEC, WDAY};
use super::datetime_date::{JsDate, MS_PER_DAY, make_date, make_day};
use super::datetime_tzif::{dt_to_date, dt_zoned_offset, make_zoned, make_zoned_from_adjusted};
use super::{CronData, DateTimeData, TimeZoneData};

fn days_in_month(date: JsDate) -> f64 {
    JsDate::new(make_date(
        make_day(date.year(), date.month() + 1.0, 0.0),
        0.0,
    ))
    .date()
}

#[expect(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "exact port of runtime-go/flickrt/cron_step.go"
)]
pub(super) fn step_cron(c: &CronData, now: &DateTimeData, reverse: bool) -> Option<f64> {
    let zoned = make_zoned(now, c.tz.clone(), false, "").ok()?;
    let zone = zoned.zone.as_ref()?;
    let tick = if reverse { -1.0 } else { 1.0 };
    let table = if reverse { &c.prev } else { &c.next };
    let boundary = if reverse { &c.last } else { &c.first };
    let needs_step = |n: i32, cur: i32| if reverse { n < cur } else { n > cur };
    let utc = matches!(zone, TimeZoneData::Named { id, .. } if id == "UTC");

    let wall = |wall_ms: f64| -> Option<DateTimeData> {
        if wall_ms.is_nan() {
            return None;
        }
        make_zoned(
            &DateTimeData {
                ms: wall_ms,
                zone: None,
            },
            zone.clone(),
            true,
            "compatible",
        )
        .ok()
    };
    let resolve = |wall_ms: f64| -> Option<f64> { Some(wall(wall_ms)?.ms) };
    let adjust_dst = |current: &mut JsDate| -> Option<()> {
        if utc || reverse {
            return Some(());
        }
        let drift = current.t - dt_to_date(&wall(current.t)?).ok()?.t;
        if drift > 0.0 {
            current.set_time(current.t + drift);
        }
        Some(())
    };
    let is_out_of_range = |wall_ms: f64| -> Option<bool> {
        if utc {
            return Some(false);
        }
        let r = resolve(wall_ms)?;
        Some(if reverse {
            r >= zoned.ms
        } else {
            r <= zoned.ms
        })
    };
    let mut fold_seconds = f64::NAN;
    let b = |f: usize| f64::from(boundary[f]);
    let has = |f: usize| !c.fields[f].is_empty();
    let next_day = |current: &mut JsDate, days: f64| -> Option<()> {
        current.set_date(current.date() + days);
        current.set_hours(b(HOUR), Some(b(MIN)), Some(b(SEC)), None);
        adjust_dst(current)
    };

    let mut current = dt_to_date(&zoned).ok()?;
    if !utc {
        let shift = zoned.ms - resolve(current.t)?;
        if (reverse && shift > 0.0) || (!reverse && shift < 0.0) {
            current.set_time(current.t + shift);
        }
    }
    let offset = if reverse && current.millis() > 0.0 {
        0.0
    } else {
        tick
    };
    current.set_seconds(current.seconds() + offset, Some(0.0));
    let mut i: i32 = 0;
    while i < 10_000 {
        if current.t.is_nan() {
            return None;
        }
        if has(SEC) {
            let cur = current.seconds() as i32;
            let nxt = table[SEC][cur as usize];
            if nxt == -1 {
                current.set_minutes(current.minutes() + tick, Some(b(SEC)), None);
                adjust_dst(&mut current)?;
                i += 1;
                continue;
            } else if needs_step(nxt, cur) {
                current.set_seconds(f64::from(nxt), None);
                adjust_dst(&mut current)?;
                i += 1;
                continue;
            }
        }
        if has(MIN) {
            let cur = current.minutes() as i32;
            let nxt = table[MIN][cur as usize];
            if nxt == -1 {
                current.set_hours(current.hours() + tick, Some(b(MIN)), Some(b(SEC)), None);
                adjust_dst(&mut current)?;
                i += 1;
                continue;
            } else if needs_step(nxt, cur) {
                current.set_minutes(f64::from(nxt), Some(b(SEC)), None);
                adjust_dst(&mut current)?;
                i += 1;
                continue;
            }
        }
        if has(HOUR) {
            let cur = current.hours() as i32;
            let nxt = table[HOUR][cur as usize];
            if nxt == -1 {
                next_day(&mut current, tick)?;
                i += 1;
                continue;
            } else if needs_step(nxt, cur) {
                current.set_hours(f64::from(nxt), Some(b(MIN)), Some(b(SEC)), None);
                adjust_dst(&mut current)?;
                i += 1;
                continue;
            }
        }
        if has(WDAY) || has(DAY) {
            if c.and {
                let day_ok = !has(DAY) || c.fields[DAY].contains(&(current.date() as i32));
                let weekday_ok =
                    !has(WDAY) || c.fields[WDAY].contains(&(current.week_day() as i32));
                if !day_ok || !weekday_ok {
                    next_day(&mut current, tick)?;
                    i += 1;
                    continue;
                }
            } else {
                let days = days_to_match(c, current, reverse, table, boundary)?;
                if days != 0.0 {
                    next_day(&mut current, days)?;
                    i += 1;
                    continue;
                }
            }
        }
        if has(MONTH) {
            let cur = current.month() as i32 + 1;
            let clamp = |year: f64, target: f64| -> f64 {
                let last = days_in_month(JsDate::new(make_date(
                    make_day(year, target + 1.0, 0.0),
                    0.0,
                )));
                if has(DAY) && !has(WDAY) && reverse {
                    let day = table[DAY][last as usize];
                    if day == -1 { last } else { f64::from(day) }
                } else if has(DAY) && !has(WDAY) {
                    b(DAY)
                } else if reverse {
                    last
                } else {
                    1.0
                }
            };
            let nxt = table[MONTH][cur as usize];
            if nxt == -1 {
                current.set_full_year(current.year() + tick, None, None);
                let yr = current.year();
                current.set_month(b(MONTH), Some(clamp(yr, b(MONTH))));
                current.set_hours(b(HOUR), Some(b(MIN)), Some(b(SEC)), None);
                adjust_dst(&mut current)?;
                i += 1;
                continue;
            } else if needs_step(nxt, cur) {
                let target = f64::from(nxt - 1);
                let yr = current.year();
                current.set_month(target, Some(clamp(yr, target)));
                current.set_hours(b(HOUR), Some(b(MIN)), Some(b(SEC)), None);
                adjust_dst(&mut current)?;
                i += 1;
                continue;
            }
        }
        if is_out_of_range(current.t)? {
            if fold_seconds.is_nan() {
                let off_at = |ms: f64| {
                    dt_zoned_offset(&DateTimeData {
                        ms,
                        zone: Some(zone.clone()),
                    })
                    .ok()
                };
                fold_seconds = ((off_at(zoned.ms - 86_400_000.0)?
                    - off_at(zoned.ms + 86_400_000.0)?)
                    / 1000.0)
                    .floor();
            }
            let after = |secs: f64| current.t + tick * secs * 1000.0;
            let (mut out_r, mut in_r) = (0.0, 1.0);
            if fold_seconds > 0.0 && !is_out_of_range(after(fold_seconds))? {
                in_r = fold_seconds;
                while in_r - out_r > 1.0 {
                    let mid = out_r + ((in_r - out_r) / 2.0).floor();
                    if is_out_of_range(after(mid))? {
                        out_r = mid;
                    } else {
                        in_r = mid;
                    }
                }
            }
            current.set_time(after(in_r));
            i = 0;
            continue;
        }
        let result = make_zoned_from_adjusted(current.t, zone.clone(), "compatible").ok()?;
        return Some(result.ms);
    }
    None
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "exact cron days_to_match port"
)]
fn days_to_match(
    c: &CronData,
    current: JsDate,
    reverse: bool,
    table: &[Vec<i32>; 6],
    boundary: &[i32; 6],
) -> Option<f64> {
    let (mut a, mut b) = if reverse {
        (f64::NEG_INFINITY, f64::NEG_INFINITY)
    } else {
        (f64::INFINITY, f64::INFINITY)
    };
    if !c.fields[WDAY].is_empty() {
        let now = current.week_day() as i32;
        let nxt = table[WDAY][now as usize];
        a = if nxt != -1 {
            f64::from(nxt - now)
        } else if reverse {
            f64::from(boundary[WDAY] - 7 - now)
        } else {
            f64::from(7 - now + boundary[WDAY])
        };
    }
    if !c.fields[DAY].is_empty() && a != 0.0 {
        let now = current.date();
        let nxt = table[DAY][now as usize];
        if nxt == -1 && reverse {
            let mut prev_d = JsDate::new(current.t);
            prev_d.set_date(0.0);
            let mut day = table[DAY][prev_d.date() as usize];
            if day == -1 {
                prev_d.set_date(0.0);
                day = table[DAY][prev_d.date() as usize];
            }
            if day == -1 {
                return None;
            }
            prev_d.set_date(f64::from(day));
            b = (prev_d.t - current.t) / MS_PER_DAY;
        } else if nxt == -1 || (!reverse && f64::from(nxt) > days_in_month(current)) {
            b = days_in_month(current) - now + f64::from(boundary[DAY]);
        } else {
            b = f64::from(nxt) - now;
        }
    }
    Some(if reverse { a.max(b) } else { a.min(b) })
}
