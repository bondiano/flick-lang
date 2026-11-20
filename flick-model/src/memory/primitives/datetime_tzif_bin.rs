//! IANA `TZif` v2/v3 binary loader and POSIX `TZ` footer evaluator.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::{env, fs, str};

use super::datetime_date::{civil, days_from_civil};

#[derive(Clone, Debug)]
pub(super) struct TzifZone {
    transitions: Vec<(i64, i32)>,
    initial_offset: i32,
    posix: Option<PosixTz>,
}

#[derive(Clone, Debug)]
struct PosixTz {
    std_offset: i32,
    dst: Option<(i32, PosixRule, PosixRule)>,
}

#[derive(Clone, Debug)]
struct PosixRule {
    kind: RuleKind,
    time_secs: i32,
}

#[derive(Clone, Debug)]
enum RuleKind {
    MonthWeekDay { month: i32, week: i32, day: i32 },
    JulianNoLeap(i32),
    ZeroDay(i32),
}

impl TzifZone {
    pub(super) fn offset_at(&self, unix_secs: i64) -> (i32, Option<i64>) {
        if let Some((last_t, last_off)) = self.transitions.last().copied() {
            if unix_secs >= last_t {
                if let Some(posix) = &self.posix {
                    return posix.offset_at(unix_secs);
                }
                return (last_off, Some(last_t));
            }
            let pos = self.transitions.partition_point(|&(t, _)| t <= unix_secs);
            if pos == 0 {
                return (self.initial_offset, None);
            }
            let (start, off) = self.transitions[pos - 1];
            return (off, Some(start));
        }
        if let Some(posix) = &self.posix {
            return posix.offset_at(unix_secs);
        }
        (self.initial_offset, None)
    }
}

impl PosixTz {
    fn offset_at(&self, unix_secs: i64) -> (i32, Option<i64>) {
        let Some((dst_off, start_rule, end_rule)) = &self.dst else {
            return (self.std_offset, None);
        };
        let (year, _, _) = civil(unix_secs.div_euclid(86_400));
        let start_t = start_rule.to_utc(year, self.std_offset);
        let end_t = end_rule.to_utc(year, *dst_off);
        if start_t < end_t {
            if unix_secs >= start_t && unix_secs < end_t {
                (*dst_off, Some(start_t))
            } else if unix_secs >= end_t {
                (self.std_offset, Some(end_t))
            } else {
                (self.std_offset, Some(end_rule.to_utc(year - 1, *dst_off)))
            }
        } else if unix_secs >= end_t && unix_secs < start_t {
            (self.std_offset, Some(end_t))
        } else if unix_secs >= start_t {
            (*dst_off, Some(start_t))
        } else {
            (*dst_off, Some(start_rule.to_utc(year - 1, self.std_offset)))
        }
    }
}

impl PosixRule {
    fn to_utc(&self, year: i64, offset_secs: i32) -> i64 {
        let day = match self.kind {
            RuleKind::MonthWeekDay { month, week, day } => {
                let first = days_from_civil(year, month, 1);
                #[expect(clippy::cast_possible_truncation, reason = "weekday 0..=6")]
                let first_wd = (first + 4).rem_euclid(7) as i32;
                let mut d = 1 + (day - first_wd).rem_euclid(7) + (week - 1) * 7;
                let dim = i32::try_from(
                    days_from_civil(
                        if month == 12 { year + 1 } else { year },
                        if month == 12 { 1 } else { month + 1 },
                        1,
                    ) - first,
                )
                .unwrap_or(31);
                if d > dim {
                    d -= 7;
                }
                days_from_civil(year, month, d)
            }
            RuleKind::JulianNoLeap(n) => {
                let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
                days_from_civil(year, 1, 1) + i64::from(n - 1) + i64::from(leap && n >= 60)
            }
            RuleKind::ZeroDay(n) => days_from_civil(year, 1, 1) + i64::from(n),
        };
        day * 86_400 + i64::from(self.time_secs) - i64::from(offset_secs)
    }
}

fn tz_cache() -> &'static Mutex<BTreeMap<String, Option<TzifZone>>> {
    static CACHE: OnceLock<Mutex<BTreeMap<String, Option<TzifZone>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn load_tzif(id: &str) -> Option<TzifZone> {
    if let Ok(guard) = tz_cache().lock()
        && let Some(cached) = guard.get(id)
    {
        return cached.clone();
    }
    let parsed = read_tzif_file(id).and_then(|bytes| parse_tzif(&bytes));
    if let Ok(mut guard) = tz_cache().lock() {
        guard.insert(id.to_owned(), parsed.clone());
    }
    parsed
}

pub(super) fn local_tzif() -> Option<TzifZone> {
    static LOCAL_TZIF: OnceLock<Option<TzifZone>> = OnceLock::new();
    LOCAL_TZIF
        .get_or_init(|| {
            if let Ok(tz) = env::var("TZ")
                && !tz.is_empty()
            {
                let name = tz.strip_prefix(':').unwrap_or(&tz);
                if let Some(parsed) = read_tzif_file(name).and_then(|b| parse_tzif(&b)) {
                    return Some(parsed);
                }
                if let Some(posix) = parse_posix_tz(name) {
                    return Some(TzifZone {
                        transitions: Vec::new(),
                        initial_offset: posix.std_offset,
                        posix: Some(posix),
                    });
                }
            }
            fs::read("/etc/localtime")
                .ok()
                .and_then(|bytes| parse_tzif(&bytes))
        })
        .clone()
}

fn read_tzif_file(id: &str) -> Option<Vec<u8>> {
    let dirs = [
        "/usr/share/zoneinfo",
        "/var/db/timezone/zoneinfo",
        "/usr/lib/zoneinfo",
    ];
    for dir in dirs {
        let path = format!("{dir}/{id}");
        if let Ok(bytes) = fs::read(&path) {
            return Some(bytes);
        }
    }
    None
}

fn parse_tzif(bytes: &[u8]) -> Option<TzifZone> {
    if bytes.len() < 44 || &bytes[0..4] != b"TZif" {
        return None;
    }
    let ver = bytes[4];
    let (off, wide) = if ver == b'2' || ver == b'3' {
        let skip = tzif_block_len(bytes, 0, false)?;
        if bytes.len() < skip + 44 || &bytes[skip..skip + 4] != b"TZif" {
            return None;
        }
        (skip, true)
    } else {
        (0, false)
    };
    let h = |i: usize| -> Option<usize> {
        Some(u32::from_be_bytes(bytes[off + i..off + i + 4].try_into().ok()?) as usize)
    };
    let isutcnt = h(20)?;
    let isstdcnt = h(24)?;
    let leapcnt = h(28)?;
    let timecnt = h(32)?;
    let typecnt = h(36)?;
    let charcnt = h(40)?;
    let t_sz = if wide { 8 } else { 4 };
    let mut pos = off + 44;
    let times_end = pos + timecnt * t_sz;
    let idx_end = times_end + timecnt;
    let types_end = idx_end + typecnt * 6;
    if types_end > bytes.len() || typecnt == 0 {
        return None;
    }
    let mut times = Vec::with_capacity(timecnt);
    for i in 0..timecnt {
        let s = pos + i * t_sz;
        let t = if wide {
            i64::from_be_bytes(bytes[s..s + 8].try_into().ok()?)
        } else {
            i64::from(i32::from_be_bytes(bytes[s..s + 4].try_into().ok()?))
        };
        times.push(t);
    }
    let idxs = &bytes[times_end..idx_end];
    let mut offsets = Vec::with_capacity(typecnt);
    let mut initial_offset = None;
    for i in 0..typecnt {
        let s = idx_end + i * 6;
        let utoff = i32::from_be_bytes(bytes[s..s + 4].try_into().ok()?);
        let isdst = bytes[s + 4];
        if initial_offset.is_none() && isdst == 0 {
            initial_offset = Some(utoff);
        }
        offsets.push(utoff);
    }
    let initial = initial_offset.unwrap_or(offsets[0]);
    let mut transitions = Vec::with_capacity(timecnt);
    for (t, &idx) in times.into_iter().zip(idxs.iter()) {
        transitions.push((t, *offsets.get(usize::from(idx))?));
    }
    pos = types_end + charcnt + leapcnt * (t_sz + 4) + isstdcnt + isutcnt;
    let posix = if wide && pos < bytes.len() && bytes[pos] == b'\n' {
        let tail = str::from_utf8(&bytes[pos + 1..]).ok()?;
        let line = tail.split('\n').next()?.trim();
        if line.is_empty() {
            None
        } else {
            parse_posix_tz(line)
        }
    } else {
        None
    };
    Some(TzifZone {
        transitions,
        initial_offset: initial,
        posix,
    })
}

fn tzif_block_len(bytes: &[u8], off: usize, wide: bool) -> Option<usize> {
    let h = |i: usize| -> Option<usize> {
        Some(u32::from_be_bytes(bytes[off + i..off + i + 4].try_into().ok()?) as usize)
    };
    let t_sz = if wide { 8 } else { 4 };
    Some(44 + h(32)? * (t_sz + 1) + h(36)? * 6 + h(40)? + h(28)? * (t_sz + 4) + h(24)? + h(20)?)
}

fn parse_posix_tz(s: &str) -> Option<PosixTz> {
    let rest = skip_tz_name(s)?;
    let (std_west, rest) = parse_posix_offset(rest)?;
    let std_offset = -std_west;
    if rest.is_empty() {
        return Some(PosixTz {
            std_offset,
            dst: None,
        });
    }
    let rest = skip_tz_name(rest)?;
    let (dst_offset, rest) = if rest.starts_with(',') {
        (std_offset + 3600, rest)
    } else {
        let (west, r) = parse_posix_offset(rest)?;
        (-west, r)
    };
    let rest = rest.strip_prefix(',')?;
    let (start_s, end_s) = rest.split_once(',')?;
    Some(PosixTz {
        std_offset,
        dst: Some((
            dst_offset,
            parse_posix_rule(start_s)?,
            parse_posix_rule(end_s)?,
        )),
    })
}

fn skip_tz_name(s: &str) -> Option<&str> {
    if let Some(after) = s.strip_prefix('<') {
        let end = after.find('>')?;
        Some(&after[end + 1..])
    } else {
        let end = s
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(s.len());
        (end > 0).then(|| &s[end..])
    }
}

fn parse_posix_offset(s: &str) -> Option<(i32, &str)> {
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') {
        (-1, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (1, r)
    } else {
        (1, s)
    };
    let (h, rest) = parse_uint(rest)?;
    let (m, rest) = if let Some(r) = rest.strip_prefix(':') {
        parse_uint(r)?
    } else {
        (0, rest)
    };
    let (sec, rest) = if let Some(r) = rest.strip_prefix(':') {
        parse_uint(r)?
    } else {
        (0, rest)
    };
    Some((sign * (h * 3600 + m * 60 + sec), rest))
}

fn parse_posix_rule(s: &str) -> Option<PosixRule> {
    let (date_part, time_part) = s.split_once('/').unwrap_or((s, "2"));
    let (time_secs, _) = parse_posix_offset(time_part)?;
    let kind = if let Some(rest) = date_part.strip_prefix('M') {
        let mut parts = rest.split('.');
        let month: i32 = parts.next()?.parse().ok()?;
        let week: i32 = parts.next()?.parse().ok()?;
        let day: i32 = parts.next()?.parse().ok()?;
        RuleKind::MonthWeekDay { month, week, day }
    } else if let Some(rest) = date_part.strip_prefix('J') {
        RuleKind::JulianNoLeap(rest.parse().ok()?)
    } else {
        RuleKind::ZeroDay(date_part.parse().ok()?)
    };
    Some(PosixRule { kind, time_secs })
}

fn parse_uint(s: &str) -> Option<(i32, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((s[..end].parse().ok()?, &s[end..]))
}
