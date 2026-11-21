//! ECMAScript `Date` UTC calendar arithmetic (`runtime-go/flickrt/datetime_date.go`).

pub(super) const MS_PER_DAY: f64 = 86_400_000.0;
pub(super) const MS_PER_HOUR: f64 = 3_600_000.0;
pub(super) const MS_PER_MINUTE: f64 = 60_000.0;
pub(super) const MAX_TIME_VALUE: f64 = 8.64e15;

#[derive(Clone, Copy, Debug)]
pub(super) struct JsDate {
    pub(super) t: f64,
}

pub(super) fn pos_mod(a: f64, b: f64) -> f64 {
    let mut r = a % b;
    if r < 0.0 {
        r += b;
    }
    r + 0.0
}

pub(super) fn floor_div(a: i64, b: i64) -> i64 {
    let mut q = a / b;
    if a % b != 0 && (a < 0) != (b < 0) {
        q -= 1;
    }
    q
}

pub(super) fn civil(days: i64) -> (i64, i32, i32) {
    let z = days + 719_468;
    let era = floor_div(z, 146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    #[expect(clippy::cast_possible_truncation, reason = "calendar day 1..=31")]
    let day = (doy - (153 * mp + 2) / 5 + 1) as i32;
    #[expect(clippy::cast_possible_truncation, reason = "calendar month 1..=12")]
    let mut month = (mp + 3) as i32;
    if month > 12 {
        month -= 12;
    }
    let mut year = yoe + era * 400;
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

pub(super) fn days_from_civil(mut year: i64, month: i32, day: i32) -> i64 {
    if month <= 2 {
        year -= 1;
    }
    let era = floor_div(year, 400);
    let yoe = year - era * 400;
    let doy = i64::from((153 * ((month + 9) % 12) + 2) / 5 + day - 1);
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "ECMAScript MakeDay bounded by 1e9 years"
)]
pub(super) fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !year.is_finite() || !month.is_finite() || !date.is_finite() {
        return f64::NAN;
    }
    let m = month.trunc();
    let ym = year.trunc() + (m / 12.0).floor();
    if ym.abs() > 1e9 {
        return f64::NAN;
    }
    let month_1 = (pos_mod(m, 12.0) as i32) + 1;
    (days_from_civil(ym as i64, month_1, 1) as f64) + date.trunc() - 1.0
}

pub(super) fn make_time(hour: f64, minute: f64, second: f64, ms: f64) -> f64 {
    if !hour.is_finite() || !minute.is_finite() || !second.is_finite() || !ms.is_finite() {
        return f64::NAN;
    }
    hour.trunc() * MS_PER_HOUR
        + minute.trunc() * MS_PER_MINUTE
        + second.trunc() * 1000.0
        + ms.trunc()
}

pub(super) fn make_date(day: f64, time: f64) -> f64 {
    if !day.is_finite() || !time.is_finite() {
        return f64::NAN;
    }
    day * MS_PER_DAY + time
}

pub(super) fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > MAX_TIME_VALUE {
        return f64::NAN;
    }
    t.trunc() + 0.0
}

impl JsDate {
    pub(super) fn new(ms: f64) -> Self {
        Self { t: time_clip(ms) }
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "time_clip bounds days within ±1e8"
    )]
    pub(super) fn civil(self) -> (i64, i32, i32) {
        civil((self.t / MS_PER_DAY).floor() as i64)
    }

    #[expect(clippy::cast_precision_loss, reason = "bounded civil year")]
    pub(super) fn year(self) -> f64 {
        if self.t.is_nan() {
            return f64::NAN;
        }
        self.civil().0 as f64
    }

    pub(super) fn month(self) -> f64 {
        if self.t.is_nan() {
            return f64::NAN;
        }
        f64::from(self.civil().1 - 1)
    }

    pub(super) fn date(self) -> f64 {
        if self.t.is_nan() {
            return f64::NAN;
        }
        f64::from(self.civil().2)
    }

    pub(super) fn week_day(self) -> f64 {
        pos_mod((self.t / MS_PER_DAY).floor() + 4.0, 7.0)
    }

    pub(super) fn hours(self) -> f64 {
        pos_mod((self.t / MS_PER_HOUR).floor(), 24.0)
    }

    pub(super) fn minutes(self) -> f64 {
        pos_mod((self.t / MS_PER_MINUTE).floor(), 60.0)
    }

    pub(super) fn seconds(self) -> f64 {
        pos_mod((self.t / 1000.0).floor(), 60.0)
    }

    pub(super) fn millis(self) -> f64 {
        pos_mod(self.t, 1000.0)
    }

    pub(super) fn within_day(self) -> f64 {
        pos_mod(self.t, MS_PER_DAY)
    }

    pub(super) fn set_full_year(&mut self, year: f64, month: Option<f64>, date: Option<f64>) {
        if self.t.is_nan() {
            self.t = 0.0;
        }
        let day = make_day(
            year,
            month.unwrap_or_else(|| self.month()),
            date.unwrap_or_else(|| self.date()),
        );
        self.t = time_clip(make_date(day, self.within_day()));
    }

    pub(super) fn set_month(&mut self, month: f64, date: Option<f64>) {
        if self.t.is_nan() {
            return;
        }
        let day = make_day(self.year(), month, date.unwrap_or_else(|| self.date()));
        self.t = time_clip(make_date(day, self.within_day()));
    }

    pub(super) fn set_date(&mut self, date: f64) {
        if self.t.is_nan() {
            return;
        }
        let day = make_day(self.year(), self.month(), date);
        self.t = time_clip(make_date(day, self.within_day()));
    }

    pub(super) fn set_time_of_day(&mut self, hour: f64, minute: f64, second: f64, ms: f64) {
        self.t = time_clip(make_date(
            (self.t / MS_PER_DAY).floor(),
            make_time(hour, minute, second, ms),
        ));
    }

    pub(super) fn set_hours(
        &mut self,
        hour: f64,
        minute: Option<f64>,
        second: Option<f64>,
        ms: Option<f64>,
    ) {
        if self.t.is_nan() {
            return;
        }
        self.set_time_of_day(
            hour,
            minute.unwrap_or_else(|| self.minutes()),
            second.unwrap_or_else(|| self.seconds()),
            ms.unwrap_or_else(|| self.millis()),
        );
    }

    pub(super) fn set_minutes(&mut self, minute: f64, second: Option<f64>, ms: Option<f64>) {
        if self.t.is_nan() {
            return;
        }
        self.set_time_of_day(
            self.hours(),
            minute,
            second.unwrap_or_else(|| self.seconds()),
            ms.unwrap_or_else(|| self.millis()),
        );
    }

    pub(super) fn set_seconds(&mut self, second: f64, ms: Option<f64>) {
        if self.t.is_nan() {
            return;
        }
        self.set_time_of_day(
            self.hours(),
            self.minutes(),
            second,
            ms.unwrap_or_else(|| self.millis()),
        );
    }

    pub(super) fn set_milliseconds(&mut self, ms: f64) {
        if self.t.is_nan() {
            return;
        }
        self.set_time_of_day(self.hours(), self.minutes(), self.seconds(), ms);
    }

    pub(super) fn set_time(&mut self, t: f64) {
        self.t = time_clip(t);
    }

    #[expect(clippy::cast_possible_truncation, reason = "bounded UTC time fields")]
    pub(super) fn iso_string(self) -> Result<String, String> {
        if self.t.is_nan() {
            return Err("Invalid time value".into());
        }
        let (y, m, day) = self.civil();
        let year = if (0..=9999).contains(&y) {
            format!("{y:04}")
        } else {
            format!("{y:+07}")
        };
        Ok(format!(
            "{year}-{m:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
            self.hours() as i32,
            self.minutes() as i32,
            self.seconds() as i32,
            self.millis() as i32
        ))
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    reason = "ECMAScript offset formatting"
)]
pub(super) fn offset_to_string(offset: f64) -> String {
    let abs = offset.abs();
    let mut hours = (abs / 3_600_000.0).floor();
    let mut minutes = ((abs % 3_600_000.0) / 60_000.0).round();
    if minutes == 60.0 {
        hours += 1.0;
        minutes = 0.0;
    }
    let sign = if offset < 0.0 { "-" } else { "+" };
    format!("{sign}{:02}:{:02}", hours as i64, minutes as i64)
}
