//! IANA `TZif` zone resolution, DST disambiguation, and shared Go ICU tables.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::{env, fs};

use flick_core::opt::js_number;

use super::datetime_date::{JsDate, MS_PER_DAY, offset_to_string, time_clip};
use super::datetime_tzif_bin::{load_tzif, local_tzif};
use super::{DateTimeData, TimeZoneData};

pub(super) const MIN_EPOCH_MILLIS: f64 = -8_640_000_000_000_000.0 + 12.0 * 3_600_000.0;
pub(super) const MAX_EPOCH_MILLIS: f64 = 8_640_000_000_000_000.0 - 14.0 * 3_600_000.0;

const INTL_GO: &str = include_str!("../../../../runtime-go/flickrt/datetime_intl.go");

pub(super) struct GoIntlRaw {
    pub(super) language_alias: &'static str,
    pub(super) region_alias: &'static str,
    pub(super) zone_data: &'static str,
    pub(super) intl_data: &'static str,
    pub(super) zone_group_data: &'static str,
    pub(super) short_zone_data: &'static str,
    pub(super) long_zone_data: &'static str,
}

pub(super) fn go_intl_raw() -> &'static GoIntlRaw {
    static RAW: OnceLock<GoIntlRaw> = OnceLock::new();
    RAW.get_or_init(|| GoIntlRaw {
        language_alias: extract_go_const("languageAliasData", '"'),
        region_alias: extract_go_const("regionAliasData", '"'),
        zone_data: extract_go_const("zoneData", '"'),
        intl_data: extract_go_const("intlData", '`'),
        zone_group_data: extract_go_const("zoneGroupData", '"'),
        short_zone_data: extract_go_const("shortZoneData", '`'),
        long_zone_data: extract_go_const("longZoneData", '`'),
    })
}

fn extract_go_const(name: &str, delim: char) -> &'static str {
    let needle = format!("\t{name}");
    let start_line = INTL_GO
        .find(&needle)
        .unwrap_or_else(|| panic!("missing {name} in datetime_intl.go"));
    let after_name = &INTL_GO[start_line + needle.len()..];
    let eq = after_name.find('=').expect("const equals");
    let after_eq = &after_name[eq + 1..];
    let open = after_eq.find(delim).expect("const open quote");
    let body = &after_eq[open + 1..];
    let close = body.find(delim).expect("const close quote");
    &body[..close]
}

fn zone_names() -> &'static BTreeMap<String, String> {
    static TABLE: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = BTreeMap::new();
        for entry in go_intl_raw().zone_data.split(';') {
            if entry.is_empty() {
                continue;
            }
            let (name, resolved) = entry.split_once('>').unwrap_or((entry, entry));
            out.insert(name.to_ascii_lowercase(), resolved.to_owned());
        }
        out
    })
}

pub(super) fn zone_table(id: &str) -> Option<String> {
    zone_names().get(&id.to_ascii_lowercase()).cloned()
}

pub(super) fn parse_offset_zone_name(id: &str) -> Option<i32> {
    let mut chars = id.chars();
    let sign = match chars.next()? {
        '+' => 1,
        '-' | '\u{2212}' => -1,
        _ => return None,
    };
    let rest: &str = chars.as_str();
    let bytes = rest.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_digit() || !bytes[1].is_ascii_digit() {
        return None;
    }
    let h = i32::from(bytes[0] - b'0') * 10 + i32::from(bytes[1] - b'0');
    if h > 23 {
        return None;
    }
    let rem = &rest[2..];
    let mm = if rem.is_empty() {
        0
    } else {
        let mstr = rem.strip_prefix(':').unwrap_or(rem);
        let mb = mstr.as_bytes();
        if mb.len() != 2 || !mb[0].is_ascii_digit() || !mb[1].is_ascii_digit() {
            return None;
        }
        let m = i32::from(mb[0] - b'0') * 10 + i32::from(mb[1] - b'0');
        if m > 59 {
            return None;
        }
        m
    };
    Some(sign * (h * 60 + mm) * 60)
}

pub(super) fn zone_named(id: &str) -> Option<TimeZoneData> {
    if let Some(secs) = parse_offset_zone_name(id) {
        let resolved = offset_to_string(f64::from(secs) * 1000.0);
        return Some(TimeZoneData::Named {
            id: resolved,
            fixed_seconds: Some(secs),
            is_local: false,
        });
    }
    let resolved = zone_table(id)?;
    if resolved.is_empty() || resolved == "Local" {
        return None;
    }
    if resolved == "UTC" {
        return Some(TimeZoneData::Named {
            id: resolved,
            fixed_seconds: Some(0),
            is_local: false,
        });
    }
    load_tzif(&resolved)?;
    Some(TimeZoneData::Named {
        id: resolved,
        fixed_seconds: None,
        is_local: false,
    })
}

pub(super) fn local_zone() -> TimeZoneData {
    static LOCAL: OnceLock<TimeZoneData> = OnceLock::new();
    LOCAL
        .get_or_init(|| {
            let mut name = env::var("TZ").unwrap_or_default();
            if let Some(stripped) = name.strip_prefix(':') {
                name = stripped.to_owned();
            }
            if name.is_empty()
                && let Ok(link) = fs::read_link("/etc/localtime")
                && let Some(s) = link.to_str()
                && let Some((_, after)) = s.split_once("zoneinfo/")
            {
                after.clone_into(&mut name);
            }
            let resolved = zone_table(&name).unwrap_or_else(|| "UTC".into());
            TimeZoneData::Named {
                id: resolved,
                fixed_seconds: None,
                is_local: true,
            }
        })
        .clone()
}

pub(super) fn parse_offset(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    if bytes.len() < 6 {
        return None;
    }
    let tail = &bytes[bytes.len() - 6..];
    let sign = match tail[0] {
        b'+' => 1.0,
        b'-' => -1.0,
        _ => return None,
    };
    if tail[3] != b':'
        || !tail[1].is_ascii_digit()
        || !tail[2].is_ascii_digit()
        || !tail[4].is_ascii_digit()
        || !tail[5].is_ascii_digit()
    {
        return None;
    }
    let h = i32::from(tail[1] - b'0') * 10 + i32::from(tail[2] - b'0');
    let m = i32::from(tail[4] - b'0') * 10 + i32::from(tail[5] - b'0');
    Some(sign * f64::from((h * 60 + m) * 60 * 1000))
}

pub(super) fn zone_from_string(s: &str) -> Option<TimeZoneData> {
    if s.starts_with("GMT") || s.starts_with('+') || s.starts_with('-') {
        return parse_offset(s).map(TimeZoneData::Offset);
    }
    zone_named(s)
}

pub(super) fn tz_to_string(z: &TimeZoneData) -> String {
    match z {
        TimeZoneData::Named { id, .. } => id.clone(),
        TimeZoneData::Offset(off) => offset_to_string(*off),
    }
}

#[expect(clippy::float_cmp, reason = "exact TimeZone equality")]
pub(super) fn tz_equals(a: &TimeZoneData, b: &TimeZoneData) -> bool {
    match (a, b) {
        (TimeZoneData::Named { id: x, .. }, TimeZoneData::Named { id: y, .. }) => x == y,
        (TimeZoneData::Offset(x), TimeZoneData::Offset(y)) => x == y,
        _ => false,
    }
}

pub(super) fn dt_to_date(d: &DateTimeData) -> Result<JsDate, String> {
    match &d.zone {
        None => Ok(JsDate::new(d.ms)),
        Some(TimeZoneData::Offset(off)) => Ok(JsDate::new(d.ms + off)),
        Some(z @ TimeZoneData::Named { .. }) => {
            Ok(JsDate::new(time_clip(d.ms) + named_offset_ms(z, d.ms)?))
        }
    }
}

pub(super) fn dt_zoned_offset(d: &DateTimeData) -> Result<f64, String> {
    Ok(dt_to_date(d)?.t - d.ms)
}

pub(super) fn named_offset_ms(z: &TimeZoneData, ms: f64) -> Result<f64, String> {
    let t = time_clip(ms);
    if t.is_nan() {
        return Err("Invalid time value".into());
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "time_clip bounds within ±8.64e15 ms"
    )]
    let secs = (t as i64).div_euclid(1000);
    Ok(f64::from(zone_offset_seconds(z, secs)) * 1000.0)
}

pub(super) fn zone_offset_seconds(z: &TimeZoneData, unix_secs: i64) -> i32 {
    match z {
        TimeZoneData::Offset(off) => {
            #[expect(clippy::cast_possible_truncation, reason = "offset seconds")]
            {
                (off / 1000.0).round() as i32
            }
        }
        TimeZoneData::Named {
            fixed_seconds: Some(s),
            ..
        } => *s,
        TimeZoneData::Named {
            id,
            fixed_seconds: None,
            is_local,
        } => {
            if *is_local {
                return local_tzif().map_or(0, |tz| tz.offset_at(unix_secs).0);
            }
            load_tzif(id).map_or(0, |tz| tz.offset_at(unix_secs).0)
        }
    }
}

#[expect(
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "exact V8 DateCache::ToUTC local wall-clock resolution"
)]
pub(super) fn utc_of_local(local: f64) -> f64 {
    let Some(tz) = local_tzif() else {
        return local;
    };
    let later_sec = ((local + MS_PER_DAY) as i64).div_euclid(1000);
    let before_sec = ((local - MS_PER_DAY) as i64).div_euclid(1000);
    let (before_off, _) = tz.offset_at(before_sec);
    let (later_off, start_sec) = tz.offset_at(later_sec);
    let before = f64::from(before_off) * 1000.0;
    let mut offset = f64::from(later_off) * 1000.0;
    if before != offset
        && let Some(start) = start_sec
        && local < (start as f64) * 1000.0 + before.max(offset)
    {
        offset = before;
    }
    local - offset
}

fn calculate_named_offset(zone: &TimeZoneData, utc: f64, adjusted: f64) -> Result<f64, String> {
    let offset = named_offset_ms(zone, utc)?;
    if offset % 60_000.0 != 0.0 {
        return Ok(named_offset_ms(zone, adjusted)? + time_clip(adjusted) - adjusted);
    }
    Ok(offset)
}

pub(super) fn make_zoned(
    d: &DateTimeData,
    zone: TimeZoneData,
    adjust: bool,
    disambiguation: &str,
) -> Result<DateTimeData, String> {
    if d.ms < MIN_EPOCH_MILLIS || d.ms > MAX_EPOCH_MILLIS {
        return Err(format!("Epoch millis out of range: {}", js_number(d.ms)));
    }
    if !adjust {
        return Ok(DateTimeData {
            ms: d.ms,
            zone: Some(zone),
        });
    }
    make_zoned_from_adjusted(d.ms, zone, disambiguation)
}

#[expect(clippy::float_cmp, reason = "exact DST transition offset comparison")]
pub(super) fn make_zoned_from_adjusted(
    adjusted: f64,
    zone: TimeZoneData,
    disambiguation: &str,
) -> Result<DateTimeData, String> {
    if let TimeZoneData::Offset(off) = zone {
        return Ok(DateTimeData {
            ms: adjusted - off,
            zone: Some(zone),
        });
    }
    let before = calculate_named_offset(&zone, adjusted - MS_PER_DAY, adjusted)?;
    let after = calculate_named_offset(&zone, adjusted + MS_PER_DAY, adjusted)?;
    if before == after {
        return Ok(DateTimeData {
            ms: adjusted - before,
            zone: Some(zone),
        });
    }
    if before < after {
        if calculate_named_offset(&zone, adjusted - after, adjusted)? == after {
            return Ok(DateTimeData {
                ms: adjusted - after,
                zone: Some(zone),
            });
        }
        let earlier = DateTimeData {
            ms: adjusted - before,
            zone: Some(zone.clone()),
        };
        if adjusted != dt_to_date(&earlier)?.t {
            match disambiguation {
                "reject" => {
                    return Err(format!(
                        "Gap time: {} does not exist in time zone {}",
                        JsDate::new(adjusted).iso_string()?,
                        tz_to_string(&zone)
                    ));
                }
                "earlier" => {
                    return Ok(DateTimeData {
                        ms: adjusted - after,
                        zone: Some(zone),
                    });
                }
                _ => {}
            }
        }
        return Ok(earlier);
    }
    if calculate_named_offset(&zone, adjusted - before, adjusted)? == before {
        if disambiguation == "earlier" || disambiguation == "compatible" {
            return Ok(DateTimeData {
                ms: adjusted - before,
                zone: Some(zone),
            });
        }
        let transition = before - after;
        if calculate_named_offset(&zone, adjusted - before + transition, adjusted + transition)?
            == before
        {
            return Ok(DateTimeData {
                ms: adjusted - before,
                zone: Some(zone),
            });
        }
        if disambiguation == "reject" {
            return Err(format!(
                "Ambiguous time: {} occurs twice in time zone {}",
                JsDate::new(adjusted).iso_string()?,
                tz_to_string(&zone)
            ));
        }
    }
    Ok(DateTimeData {
        ms: adjusted - after,
        zone: Some(zone),
    })
}
