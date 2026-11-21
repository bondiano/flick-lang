//! V8 `Date.parse` + ISO / zoned string parsing (`runtime-go/flickrt/datetime_parse.go`).

use super::datetime_date::{MAX_TIME_VALUE, MS_PER_DAY, make_date, make_day, make_time, time_clip};
use super::datetime_parse_scan::{DateScanner, DateToken, TokTag};
use super::datetime_tzif::{make_zoned, parse_offset, utc_of_local, zone_from_string};
use super::{DateTimeData, TimeZoneData};

const DATE_NONE: i32 = i32::MAX;

pub(super) fn parse_instant(s: &str) -> Option<DateTimeData> {
    let adjusted;
    let input = if has_zone_suffix(s) {
        s
    } else {
        adjusted = format!("{s}Z");
        &adjusted
    };
    let ms = parse_js_date(input);
    (!ms.is_nan()).then_some(DateTimeData { ms, zone: None })
}

fn has_zone_suffix(s: &str) -> bool {
    if s.contains('Z') || s.contains("GMT") || s.ends_with(']') {
        return true;
    }
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len >= 3
        && matches!(bytes[len - 3], b'+' | b'-')
        && bytes[len - 2].is_ascii_digit()
        && bytes[len - 1].is_ascii_digit()
    {
        return true;
    }
    if len >= 5
        && matches!(bytes[len - 5], b'+' | b'-')
        && bytes[len - 4..].iter().all(u8::is_ascii_digit)
    {
        return true;
    }
    len >= 6
        && matches!(bytes[len - 6], b'+' | b'-')
        && bytes[len - 5].is_ascii_digit()
        && bytes[len - 4].is_ascii_digit()
        && bytes[len - 3] == b':'
        && bytes[len - 2].is_ascii_digit()
        && bytes[len - 1].is_ascii_digit()
}

pub(super) fn make_zoned_from_string(s: &str) -> Option<DateTimeData> {
    let (iso, zone) = if let Some(inner) = s.strip_suffix(']')
        && let Some(open) = inner.find('[')
        && (17..=35).contains(&open)
        && open + 1 < inner.len()
    {
        let z = zone_from_string(&inner[open + 1..])?;
        (&inner[..open], z)
    } else if let Some(off) = parse_offset(s) {
        (s, TimeZoneData::Offset(off))
    } else {
        return None;
    };
    let d = parse_instant(iso)?;
    make_zoned(&d, zone, false, "").ok()
}

fn consume_date_token(
    sc: &mut DateScanner<'_>,
    token: DateToken,
    has_number: &mut bool,
    day: &mut DayComposer,
    clock: &mut TimeComposer,
    tz: &mut ZoneComposer,
) -> bool {
    match token.tag {
        TokTag::Number => {
            *has_number = true;
            let n = token.value;
            if sc.skip(':') {
                if sc.skip(':') {
                    if clock.index > 0 {
                        return false;
                    }
                    clock.add(n);
                    clock.add(0);
                } else {
                    if !clock.add(n) {
                        return false;
                    }
                    if sc.peek.is_symbol('.') {
                        sc.next();
                    }
                }
            } else if sc.skip('.') && clock.is_expecting(n) {
                clock.add(n);
                if sc.peek.tag != TokTag::Number {
                    return false;
                }
                let ms_tok = sc.next();
                clock.add_final(read_milliseconds(ms_tok));
            } else if tz.is_expecting(n) {
                tz.minute = n;
            } else if clock.is_expecting(n) {
                clock.add_final(n);
                let p = sc.peek;
                if p.tag != TokTag::End && p.tag != TokTag::Space && !p.is_z() && !p.is_sign() {
                    return false;
                }
            } else {
                if !day.add(n) {
                    return false;
                }
                sc.skip('-');
            }
        }
        TokTag::KwAmPm
        | TokTag::KwMonth
        | TokTag::KwZone
        | TokTag::KwTimeSep
        | TokTag::KwInvalid => {
            if token.tag == TokTag::KwAmPm && clock.index > 0 {
                clock.hour_offset = token.value;
            } else if token.tag == TokTag::KwMonth {
                day.named_month = token.value;
                sc.skip('-');
            } else if token.tag == TokTag::KwZone && *has_number {
                tz.set(token.value);
            } else if *has_number || sc.peek.tag == TokTag::Number {
                return false;
            }
        }
        TokTag::Symbol if token.is_sign() && (tz.is_utc() || clock.index > 0) => {
            tz.sign = token.sign();
            let (n, length) = if sc.peek.tag == TokTag::Number {
                let t = sc.next();
                (t.value, t.length)
            } else {
                (0, 0)
            };
            *has_number = true;
            if sc.peek.is_symbol(':') {
                tz.hour = n;
                tz.minute = DATE_NONE;
            } else if length == 1 || length == 2 {
                tz.hour = n;
                tz.minute = 0;
            } else if length == 3 || length == 4 {
                tz.hour = n / 100;
                tz.minute = n % 100;
            } else {
                return false;
            }
        }
        TokTag::Symbol if (token.is_sign() || token.is_symbol(')')) && *has_number => {
            return false;
        }
        _ => {}
    }
    true
}

pub(super) fn parse_js_date(s: &str) -> f64 {
    let mut sc = DateScanner::new(s);
    let mut day = DayComposer::default();
    let mut clock = TimeComposer::default();
    let mut tz = ZoneComposer::default();

    let mut token = parse_es5_date(&mut sc, &mut day, &mut clock, &mut tz);
    if token.tag == TokTag::Invalid {
        return f64::NAN;
    }
    let mut has_number = day.index > 0;
    while token.tag != TokTag::End {
        if !consume_date_token(
            &mut sc,
            token,
            &mut has_number,
            &mut day,
            &mut clock,
            &mut tz,
        ) {
            return f64::NAN;
        }
        token = sc.next();
    }

    let Some((year, month, date)) = day.write() else {
        return f64::NAN;
    };
    let Some((hour, minute, second, ms)) = clock.write() else {
        return f64::NAN;
    };
    let Some(offset) = tz.write() else {
        return f64::NAN;
    };
    let mut t = make_date(
        make_day(f64::from(year), f64::from(month - 1), f64::from(date)),
        make_time(
            f64::from(hour),
            f64::from(minute),
            f64::from(second),
            f64::from(ms),
        ),
    );
    if offset.is_nan() {
        let abs_t = t.abs();
        if abs_t.is_nan() || abs_t > MAX_TIME_VALUE + 10.0 * MS_PER_DAY {
            return f64::NAN;
        }
        t = utc_of_local(t);
    } else {
        t -= offset * 1000.0;
    }
    time_clip(t)
}

fn parse_es5_time(
    sc: &mut DateScanner<'_>,
    clock: &mut TimeComposer,
    tz: &mut ZoneComposer,
) -> bool {
    sc.next();
    if !sc.peek.is_fixed_number(2) || !between(sc.peek.value, 0, 24) {
        return false;
    }
    let hour24 = sc.peek.value == 24;
    let h = sc.next().value;
    clock.add(h);
    if !sc.skip(':') {
        return false;
    }
    if !sc.peek.is_fixed_number(2)
        || !between(sc.peek.value, 0, 59)
        || (hour24 && sc.peek.value > 0)
    {
        return false;
    }
    let m = sc.next().value;
    clock.add(m);
    if sc.skip(':') {
        if !sc.peek.is_fixed_number(2)
            || !between(sc.peek.value, 0, 59)
            || (hour24 && sc.peek.value > 0)
        {
            return false;
        }
        let s = sc.next().value;
        clock.add(s);
        if sc.skip('.') {
            if sc.peek.tag != TokTag::Number || (hour24 && sc.peek.value > 0) {
                return false;
            }
            let ms_tok = sc.next();
            clock.add(read_milliseconds(ms_tok));
        }
    }
    if sc.peek.is_z() {
        sc.next();
        tz.set(0);
    } else if sc.peek.is_symbol('+') || sc.peek.is_symbol('-') {
        tz.sign = sc.next().sign();
        if sc.peek.is_fixed_number(4) {
            let hm = sc.next().value;
            if !between(hm / 100, 0, 23) || !between(hm % 100, 0, 59) {
                return false;
            }
            tz.hour = hm / 100;
            tz.minute = hm % 100;
        } else {
            if !sc.peek.is_fixed_number(2) || !between(sc.peek.value, 0, 23) {
                return false;
            }
            tz.hour = sc.next().value;
            if !sc.skip(':') {
                return false;
            }
            if !sc.peek.is_fixed_number(2) || !between(sc.peek.value, 0, 59) {
                return false;
            }
            tz.minute = sc.next().value;
        }
    }
    sc.peek.tag == TokTag::End
}

fn parse_es5_date(
    sc: &mut DateScanner<'_>,
    day: &mut DayComposer,
    clock: &mut TimeComposer,
    tz: &mut ZoneComposer,
) -> DateToken {
    let invalid = DateToken::simple(TokTag::Invalid);
    if sc.peek.is_sign() {
        let sign = sc.next();
        if !sc.peek.is_fixed_number(6) {
            return sign;
        }
        let year = sc.next().value;
        if sign.sign() < 0 && year == 0 {
            return sign;
        }
        day.add(sign.sign() * year);
    } else if sc.peek.is_fixed_number(4) {
        let y = sc.next().value;
        day.add(y);
    } else {
        return sc.next();
    }
    if sc.skip('-') {
        if !sc.peek.is_fixed_number(2) || !between(sc.peek.value, 1, 12) {
            return sc.next();
        }
        let m = sc.next().value;
        day.add(m);
        if sc.skip('-') {
            if !sc.peek.is_fixed_number(2) || !between(sc.peek.value, 1, 31) {
                return sc.next();
            }
            let d = sc.next().value;
            day.add(d);
        }
    }
    if sc.peek.tag == TokTag::KwTimeSep {
        if !parse_es5_time(sc, clock, tz) {
            return invalid;
        }
    } else if sc.peek.tag != TokTag::End {
        return sc.next();
    }
    if tz.hour == DATE_NONE && clock.index == 0 {
        tz.set(0);
    }
    day.iso = true;
    DateToken::simple(TokTag::End)
}

fn between(x: i32, lo: i32, hi: i32) -> bool {
    x >= lo && x <= hi
}

struct DayComposer {
    comp: [i32; 3],
    index: usize,
    named_month: i32,
    iso: bool,
}

impl Default for DayComposer {
    fn default() -> Self {
        Self {
            comp: [0; 3],
            index: 0,
            named_month: DATE_NONE,
            iso: false,
        }
    }
}

impl DayComposer {
    fn add(&mut self, n: i32) -> bool {
        if self.index < 3 {
            self.comp[self.index] = n;
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn write(&mut self) -> Option<(i32, i32, i32)> {
        if self.index < 1 {
            return None;
        }
        while self.index < 3 {
            self.comp[self.index] = 1;
            self.index += 1;
        }
        let is_day = |x: i32| between(x, 1, 31);
        let (mut year, month, day) =
            if self.named_month == DATE_NONE && (self.iso || !is_day(self.comp[0])) {
                (self.comp[0], self.comp[1], self.comp[2])
            } else if self.named_month == DATE_NONE {
                (self.comp[2], self.comp[0], self.comp[1])
            } else if !is_day(self.comp[0]) {
                (self.comp[0], self.named_month, self.comp[1])
            } else {
                (self.comp[1], self.named_month, self.comp[0])
            };
        if !self.iso {
            if between(year, 0, 49) {
                year += 2000;
            } else if between(year, 50, 99) {
                year += 1900;
            }
        }
        (between(month, 1, 12) && is_day(day)).then_some((year, month, day))
    }
}

struct TimeComposer {
    comp: [i32; 4],
    index: usize,
    hour_offset: i32,
}

impl Default for TimeComposer {
    fn default() -> Self {
        Self {
            comp: [0; 4],
            index: 0,
            hour_offset: DATE_NONE,
        }
    }
}

impl TimeComposer {
    fn is_expecting(&self, n: i32) -> bool {
        (self.index == 1 && between(n, 0, 59))
            || (self.index == 2 && between(n, 0, 59))
            || (self.index == 3 && between(n, 0, 999))
    }

    fn add(&mut self, n: i32) -> bool {
        if self.index < 4 {
            self.comp[self.index] = n;
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn add_final(&mut self, n: i32) -> bool {
        if !self.add(n) {
            return false;
        }
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        true
    }

    fn write(&mut self) -> Option<(i32, i32, i32, i32)> {
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        let (mut hour, minute, second, ms) =
            (self.comp[0], self.comp[1], self.comp[2], self.comp[3]);
        if self.hour_offset != DATE_NONE {
            if !between(hour, 0, 12) {
                return None;
            }
            hour = hour % 12 + self.hour_offset;
        }
        if !between(hour, 0, 23)
            || !between(minute, 0, 59)
            || !between(second, 0, 59)
            || !between(ms, 0, 999)
        {
            return (hour == 24 && minute == 0 && second == 0 && ms == 0)
                .then_some((hour, minute, second, ms));
        }
        Some((hour, minute, second, ms))
    }
}

struct ZoneComposer {
    sign: i32,
    hour: i32,
    minute: i32,
}

impl Default for ZoneComposer {
    fn default() -> Self {
        Self {
            sign: DATE_NONE,
            hour: DATE_NONE,
            minute: DATE_NONE,
        }
    }
}

impl ZoneComposer {
    fn set(&mut self, hours: i32) {
        self.sign = if hours < 0 { -1 } else { 1 };
        self.hour = hours * self.sign;
        self.minute = 0;
    }

    fn is_expecting(&self, n: i32) -> bool {
        self.hour != DATE_NONE && self.minute == DATE_NONE && between(n, 0, 59)
    }

    fn is_utc(&self) -> bool {
        self.hour == 0 && self.minute == 0
    }

    fn write(&mut self) -> Option<f64> {
        if self.sign == DATE_NONE {
            return Some(f64::NAN);
        }
        if self.hour == DATE_NONE {
            self.hour = 0;
        }
        if self.minute == DATE_NONE {
            self.minute = 0;
        }
        let total = u32::try_from(self.hour)
            .ok()?
            .wrapping_mul(3600)
            .wrapping_add(u32::try_from(self.minute).ok()?.wrapping_mul(60));
        if total > u32::try_from(i32::MAX).unwrap_or(u32::MAX) {
            return None;
        }
        Some(f64::from(self.sign) * f64::from(total))
    }
}

fn read_milliseconds(t: DateToken) -> i32 {
    let mut n = t.value;
    match t.length {
        1 => n * 100,
        2 => n * 10,
        l if l > 3 => {
            let mut len = l.min(9);
            while len > 3 {
                n /= 10;
                len -= 1;
            }
            n
        }
        _ => n,
    }
}
