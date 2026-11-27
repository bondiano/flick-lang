//! Locale validation + `Intl.DateTimeFormat` formatting (`runtime-go/flickrt/{locale,datetime_format,datetime_metazone}.go`).

use std::collections::{BTreeMap, BTreeSet};
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
use std::env;
use std::sync::OnceLock;

use flick_core::opt::json_string;

use super::datetime_date::{JsDate, days_from_civil, offset_to_string, time_clip};
use super::datetime_tzif::{dt_zoned_offset, go_intl_raw, local_zone, zone_offset_seconds};
use super::{DateTimeData, TimeZoneData};

#[derive(Clone, Debug, Default)]
struct LocaleId {
    language: String,
    script: String,
    region: String,
}

const INCORRECT_LOCALE: &str = "Incorrect locale information provided";

fn is_alpha(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphabetic())
}
fn is_digits(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}
fn is_alnum(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn alnum_of(s: &str, lo: usize, hi: usize) -> bool {
    s.len() >= lo && s.len() <= hi && is_alnum(s)
}
fn language_subtag(s: &str) -> bool {
    is_alpha(s) && ((2..=3).contains(&s.len()) || (5..=8).contains(&s.len()))
}
fn script_subtag(s: &str) -> bool {
    s.len() == 4 && is_alpha(s)
}
fn region_subtag(s: &str) -> bool {
    (s.len() == 2 && is_alpha(s)) || (s.len() == 3 && is_digits(s))
}
fn variant_subtag(s: &str) -> bool {
    alnum_of(s, 5, 8) || (s.len() == 4 && s.as_bytes()[0].is_ascii_digit() && is_alnum(s))
}
fn is_singleton(s: &str) -> bool {
    alnum_of(s, 1, 1)
}

fn language_tag(tag: &str) -> Result<LocaleId, String> {
    let trimmed = tag.strip_suffix('-').unwrap_or(tag);
    let subtags: Vec<&str> = trimmed.split('-').collect();
    let n = subtags.len();
    let mut i = 1;
    if subtags.is_empty() || !language_subtag(subtags[0]) {
        return Err(INCORRECT_LOCALE.into());
    }
    let mut id = LocaleId {
        language: subtags[0].to_ascii_lowercase(),
        ..Default::default()
    };
    if i < n && script_subtag(subtags[i]) {
        let mut s = subtags[i].to_ascii_lowercase();
        if let Some(first) = s.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        id.script = s;
        i += 1;
    }
    if i < n && region_subtag(subtags[i]) {
        id.region = subtags[i].to_ascii_uppercase();
        i += 1;
    }
    let variants = i;
    while i < n && !is_singleton(subtags[i]) {
        if !variant_subtag(subtags[i]) {
            return Err(INCORRECT_LOCALE.into());
        }
        i += 1;
    }
    let invalid = || format!("Invalid language tag: {}", ascii_lower_bytes(tag));
    if tag.ends_with('-') {
        return Err(invalid());
    }
    let mut seen = BTreeSet::new();
    for &v in &subtags[variants..i] {
        let lv = v.to_ascii_lowercase();
        if !seen.insert(lv) {
            return Err(invalid());
        }
    }
    while i < n {
        let s = subtags[i].to_ascii_lowercase();
        if !is_singleton(&s) || !seen.insert(s.clone()) {
            return Err(invalid());
        }
        let Some(next) = parse_extension(&s, &subtags, i + 1) else {
            return Err(invalid());
        };
        i = next;
    }
    Ok(canonical_locale(id))
}

fn parse_extension(singleton: &str, subtags: &[&str], start: usize) -> Option<usize> {
    let n = subtags.len();
    let mut i = start;
    let take_values = |idx: &mut usize, least: usize| {
        while *idx < n && alnum_of(subtags[*idx], least, 8) {
            *idx += 1;
        }
    };
    match singleton {
        "x" => {
            take_values(&mut i, 1);
            return (i > start && i == n).then_some(i);
        }
        "u" => {
            take_values(&mut i, 3);
            while i < n
                && subtags[i].len() == 2
                && is_alnum(&subtags[i][..1])
                && is_alpha(&subtags[i][1..])
            {
                i += 1;
                take_values(&mut i, 3);
            }
        }
        "t" => {
            if i < n && language_subtag(subtags[i]) {
                i += 1;
                if i < n && script_subtag(subtags[i]) {
                    i += 1;
                }
                if i < n && region_subtag(subtags[i]) {
                    i += 1;
                }
                while i < n && variant_subtag(subtags[i]) {
                    i += 1;
                }
            }
            while i < n
                && subtags[i].len() == 2
                && is_alpha(&subtags[i][..1])
                && is_digits(&subtags[i][1..])
            {
                i += 1;
                let field = i;
                take_values(&mut i, 3);
                if i == field {
                    return None;
                }
            }
        }
        _ => take_values(&mut i, 2),
    }
    (i > start).then_some(i)
}

fn ascii_lower_bytes(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        s.bytes().map(char::from).collect()
    }
}

fn locale_aliases() -> &'static BTreeMap<&'static str, &'static str> {
    static ALIASES: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
    ALIASES.get_or_init(|| {
        let raw = go_intl_raw();
        let mut out = BTreeMap::new();
        for src in [raw.language_alias, raw.region_alias] {
            for entry in src.split(';') {
                if let Some((from, to)) = entry.split_once('>') {
                    out.insert(from, to);
                }
            }
        }
        out
    })
}

fn canonical_locale(mut id: LocaleId) -> LocaleId {
    let aliases = locale_aliases();
    if let Some(&to) = aliases.get(id.language.as_str()) {
        let parts: Vec<&str> = to.split('-').collect();
        id.language = parts[0].to_owned();
        for &p in &parts[1..] {
            if script_subtag(p) && id.script.is_empty() {
                id.script = p.to_owned();
            } else if region_subtag(p) && id.region.is_empty() {
                id.region = p.to_owned();
            }
        }
    }
    if !id.region.is_empty()
        && let Some(&to) = aliases.get(id.region.as_str())
    {
        id.region = to.to_owned();
    }
    id
}

#[derive(Clone)]
struct IntlLocale {
    patterns: Vec<&'static str>,
    months: Vec<&'static str>,
    months_wide: Vec<&'static str>,
    days: Vec<&'static str>,
    days_wide: Vec<&'static str>,
    period: Vec<&'static str>,
    zones: Vec<&'static str>,
}

fn intl_locales() -> &'static BTreeMap<&'static str, IntlLocale> {
    static TABLE: OnceLock<BTreeMap<&'static str, IntlLocale>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out: BTreeMap<&'static str, IntlLocale> = BTreeMap::new();
        for line in go_intl_raw().intl_data.split('\n') {
            let Some((tag, data)) = line.split_once('=') else {
                continue;
            };
            if let Some(other) = data.strip_prefix('@') {
                if let Some(cloned) = out.get(other).cloned() {
                    out.insert(tag, cloned);
                }
                continue;
            }
            let f: Vec<&str> = data.split('|').collect();
            if f.len() < 7 {
                continue;
            }
            let items = |s: &'static str| s.split('~').collect();
            out.insert(
                tag,
                IntlLocale {
                    patterns: items(f[0]),
                    months: items(f[1]),
                    months_wide: items(f[2]),
                    days: items(f[3]),
                    days_wide: items(f[4]),
                    period: items(f[5]),
                    zones: items(f[6]),
                },
            );
        }
        out
    })
}

fn lookup_locale<'a, V>(
    id: &LocaleId,
    map: &'a BTreeMap<&'static str, V>,
) -> Option<(&'static str, &'a V)> {
    let join = |a: &str, b: &str, c: &str| {
        let mut out = a.to_owned();
        if !b.is_empty() {
            out.push('-');
            out.push_str(b);
        }
        if !c.is_empty() {
            out.push('-');
            out.push_str(c);
        }
        out
    };
    for cand in [
        join(&id.language, &id.script, &id.region),
        join(&id.language, &id.script, ""),
        id.language.clone(),
    ] {
        if let Some((&k, v)) = map.get_key_value(cand.as_str()) {
            return Some((k, v));
        }
    }
    None
}

fn default_locale() -> &'static str {
    #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = env::var(name)
            && !v.is_empty()
        {
            let v = v.split('.').next().unwrap_or(&v);
            let v = v.split('@').next().unwrap_or(v);
            if let Ok(id) = language_tag(&v.replace('_', "-"))
                && let Some((tag, _)) = lookup_locale(&id, intl_locales())
            {
                return tag;
            }
            break;
        }
    }
    "en-US"
}

fn zone_groups() -> &'static BTreeMap<&'static str, usize> {
    static GROUPS: OnceLock<BTreeMap<&'static str, usize>> = OnceLock::new();
    GROUPS.get_or_init(|| {
        let mut out = BTreeMap::new();
        for (i, group) in go_intl_raw().zone_group_data.split(';').enumerate() {
            for z in group.split(',') {
                out.insert(z, i);
            }
        }
        out
    })
}

fn zone_name_lines(data: &'static str) -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut out: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for line in data.split('\n') {
        let Some((tag, names)) = line.split_once('=') else {
            continue;
        };
        if let Some(other) = names.strip_prefix('@') {
            if let Some(cloned) = out.get(other).cloned() {
                out.insert(tag, cloned);
            }
            continue;
        }
        out.insert(tag, names.split('|').collect());
    }
    out
}

fn metazone_name(tag: &str, zone: &TimeZoneData, offset: f64, long: bool) -> &'static str {
    static SHORT: OnceLock<BTreeMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    static LONG: OnceLock<BTreeMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    let TimeZoneData::Named { id, .. } = zone else {
        return "";
    };
    let Some(&group) = zone_groups().get(id.as_str()) else {
        return "";
    };
    let table = if long {
        LONG.get_or_init(|| zone_name_lines(go_intl_raw().long_zone_data))
    } else {
        SHORT.get_or_init(|| zone_name_lines(go_intl_raw().short_zone_data))
    };
    let Some(entry) = table.get(tag).and_then(|row| row.get(group)).copied() else {
        return "";
    };
    let (winter, summer) = entry.split_once('~').unwrap_or((entry, entry));
    let w_probe = days_from_civil(2024, 1, 15) * 86_400 + 12 * 3600;
    let s_probe = days_from_civil(2024, 7, 15) * 86_400 + 12 * 3600;
    let w_min = zone_offset_seconds(zone, w_probe) / 60;
    let s_min = zone_offset_seconds(zone, s_probe) / 60;
    #[expect(clippy::cast_possible_truncation, reason = "offset minutes fit i32")]
    let target = (offset / 60_000.0).round() as i32;
    if target == w_min {
        winter
    } else if target == s_min {
        summer
    } else {
        ""
    }
}

pub(super) struct IntlFormat {
    tag: &'static str,
    locale: &'static IntlLocale,
    pattern: &'static str,
}

const STYLES: &[&str] = &["full", "long", "medium", "short"];

pub(super) fn new_intl_format(
    locale_opt: Option<&str>,
    date_style: Option<&str>,
    time_style: Option<&str>,
) -> Result<IntlFormat, String> {
    let parse_style = |opt: Option<&str>| -> Result<usize, String> {
        let Some(s) = opt else {
            return Ok(0);
        };
        STYLES
            .iter()
            .position(|&k| k == s)
            .map(|i| i + 1)
            .ok_or_else(|| format!("invalid style {}", json_string(s)))
    };
    let date = parse_style(date_style)?;
    let time = parse_style(time_style)?;
    let locales = intl_locales();
    let mut tag = default_locale();
    if let Some(l) = locale_opt {
        let id = language_tag(l)?;
        if let Some((found, _)) = lookup_locale(&id, locales) {
            tag = found;
        }
    }
    let locale = &locales[tag];
    Ok(IntlFormat {
        tag,
        locale,
        pattern: locale.patterns[date * 5 + time],
    })
}

impl IntlFormat {
    pub(super) fn format(&self, ms: f64, zone: Option<&TimeZoneData>) -> Result<String, String> {
        let t = time_clip(ms);
        if t.is_nan() {
            return Err("Invalid time value".into());
        }
        let utc = match zone {
            None => true,
            Some(TimeZoneData::Named { id, .. }) => id == "UTC",
            Some(TimeZoneData::Offset(_)) => false,
        };
        let offset = match zone {
            None => 0.0,
            Some(z) => dt_zoned_offset(&DateTimeData {
                ms: t,
                zone: Some(z.clone()),
            })?,
        };
        let wall = JsDate::new(t + offset);
        let mut out = String::new();
        let mut pat = self.pattern;
        while !pat.is_empty() {
            let b = pat.as_bytes()[0];
            if b == b'\'' {
                let mut i = 1;
                let bytes = pat.as_bytes();
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if i + 1 == bytes.len() || bytes[i + 1] != b'\'' {
                            break;
                        }
                        i += 1;
                    }
                    let ch = pat[i..].chars().next().unwrap_or('\0');
                    out.push(ch);
                    i += ch.len_utf8();
                }
                pat = &pat[(i + 1).min(pat.len())..];
            } else if b.is_ascii_alphabetic() {
                let n = pat.bytes().take_while(|&c| c == b).count();
                let tok = &pat[..n];
                if b == b'z' {
                    out.push_str(&self.zone_name(zone, utc, offset, n == 4));
                } else {
                    out.push_str(&self.locale.field(tok, wall));
                }
                pat = &pat[n..];
            } else {
                let ch = pat.chars().next().unwrap_or('\0');
                out.push(ch);
                pat = &pat[ch.len_utf8()..];
            }
        }
        Ok(out)
    }

    #[expect(clippy::cast_possible_truncation, reason = "offset minutes fit i32")]
    fn zone_name(&self, zone: Option<&TimeZoneData>, utc: bool, offset: f64, long: bool) -> String {
        if !utc && let Some(z @ TimeZoneData::Named { .. }) = zone {
            let name = metazone_name(self.tag, z, offset, long);
            if !name.is_empty() {
                return name.to_owned();
            }
        }
        let l = self.locale;
        let b_long = usize::from(long);
        let minutes = (offset / 60_000.0).round() as i32;
        if utc {
            return l.zones[b_long].to_owned();
        }
        if minutes == 0 {
            return l.zones[2 + b_long].to_owned();
        }
        let abs = minutes.abs();
        let (h, m) = (abs / 60, abs % 60);
        let template = if long {
            l.zones[8 + usize::from(minutes < 0)]
        } else {
            l.zones[4 + 2 * usize::from(minutes < 0) + usize::from(m != 0)]
        };
        template
            .replace("{HH}", &format!("{h:02}"))
            .replace("{H}", &h.to_string())
            .replace("{MM}", &format!("{m:02}"))
    }
}

impl IntlLocale {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded calendar fields"
    )]
    fn field(&self, token: &str, wall: JsDate) -> String {
        let year = wall.year() as i32;
        let month = wall.month() as usize;
        let day = wall.date() as i32;
        let hour = wall.hours() as usize;
        let h12 = hour % 12;
        match token {
            "y" => year.to_string(),
            "yy" => format!("{:02}", year.rem_euclid(100)),
            "M" => (month + 1).to_string(),
            "MM" => format!("{:02}", month + 1),
            "MMM" => self.months[month].to_owned(),
            "MMMM" => self.months_wide[month].to_owned(),
            "d" => day.to_string(),
            "dd" => format!("{day:02}"),
            "EEE" => self.days[wall.week_day() as usize].to_owned(),
            "EEEE" => self.days_wide[wall.week_day() as usize].to_owned(),
            "a" => self.period[hour].to_owned(),
            "h" | "hh" | "K" | "KK" | "H" | "HH" | "k" | "kk" => {
                let first = token.as_bytes()[0];
                let mut n = match first {
                    b'h' | b'K' => h12,
                    _ => hour,
                };
                if n == 0 && first == b'h' {
                    n = 12;
                } else if n == 0 && first == b'k' {
                    n = 24;
                }
                if token.len() == 2 {
                    format!("{n:02}")
                } else {
                    n.to_string()
                }
            }
            "m" => (wall.minutes() as i32).to_string(),
            "mm" => format!("{:02}", wall.minutes() as i32),
            "s" => (wall.seconds() as i32).to_string(),
            "ss" => format!("{:02}", wall.seconds() as i32),
            _ => String::new(),
        }
    }
}

pub(super) fn format_dt(
    mode: FormatMode,
    locale: Option<&str>,
    date_style: Option<&str>,
    time_style: Option<&str>,
    d: &DateTimeData,
) -> Result<String, String> {
    let fmt = new_intl_format(locale, date_style, time_style)?;
    match mode {
        FormatMode::Zone => {
            if let Some(TimeZoneData::Offset(off)) = &d.zone {
                let s = offset_to_string(*off);
                if s.len() > 6 || &s[1..3] >= "24" {
                    let wall = super::datetime_tzif::dt_to_date(d)?;
                    return fmt.format(wall.t, None);
                }
            }
            fmt.format(d.ms, d.zone.as_ref())
        }
        FormatMode::Local | FormatMode::Intl => {
            let lz = local_zone();
            fmt.format(d.ms, Some(&lz))
        }
        FormatMode::Utc => fmt.format(d.ms, None),
    }
}

#[derive(Clone, Copy)]
pub(super) enum FormatMode {
    Zone,
    Local,
    Utc,
    Intl,
}
