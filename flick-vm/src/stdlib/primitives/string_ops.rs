//! Counted `String` primitive implementations (`packages/core/src/String.fl`).

use flick_core::ir::Lit;
use flick_core::ir::memory::StringIntrinsic;
use unicode_normalization::UnicodeNormalization;

use super::string_case;
use super::{Data, Error, Fuel, Heap, Value, charge};

pub(super) fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}')
}

pub(super) fn run(
    heap: &mut Heap,
    op: StringIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        StringIntrinsic::Trim
        | StringIntrinsic::TrimStart
        | StringIntrinsic::TrimEnd
        | StringIntrinsic::ToUpper
        | StringIntrinsic::ToLower
        | StringIntrinsic::Capitalize
        | StringIntrinsic::Uncapitalize
        | StringIntrinsic::Lines
        | StringIntrinsic::LinesWithSeparators
        | StringIntrinsic::StripMargin
        | StringIntrinsic::SnakeToCamel
        | StringIntrinsic::SnakeToPascal
        | StringIntrinsic::SnakeToKebab
        | StringIntrinsic::CamelToSnake
        | StringIntrinsic::PascalToSnake
        | StringIntrinsic::KebabToSnake
        | StringIntrinsic::NoCase
        | StringIntrinsic::PascalCase
        | StringIntrinsic::CamelCase
        | StringIntrinsic::ConstantCase
        | StringIntrinsic::ConfigCase
        | StringIntrinsic::KebabCase
        | StringIntrinsic::SnakeCase
        | StringIntrinsic::Chars => run_unary(heap, op, &args[0], fuel),
        StringIntrinsic::Split
        | StringIntrinsic::EndsWith
        | StringIntrinsic::IndexOf
        | StringIntrinsic::LastIndexOf
        | StringIntrinsic::ToLocaleUpper
        | StringIntrinsic::ToLocaleLower
        | StringIntrinsic::TakeLeft
        | StringIntrinsic::TakeRight
        | StringIntrinsic::At
        | StringIntrinsic::CharAt
        | StringIntrinsic::CharCodeAt
        | StringIntrinsic::CodePointAt
        | StringIntrinsic::Normalize
        | StringIntrinsic::Repeat
        | StringIntrinsic::StripMarginWith => run_binary(heap, op, &args[0], &args[1], fuel),
        StringIntrinsic::Slice
        | StringIntrinsic::Substring
        | StringIntrinsic::PadStart
        | StringIntrinsic::PadEnd
        | StringIntrinsic::ReplaceAll => run_ternary(heap, op, &args[0], &args[1], &args[2], fuel),
    }
}

fn run_unary(
    heap: &mut Heap,
    op: StringIntrinsic,
    arg: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    if op == StringIntrinsic::Chars {
        let units = heap.utf16_units(arg)?;
        charge(fuel, units.len())?;
        return utf16_chars_list(heap, &units);
    }
    if matches!(
        op,
        StringIntrinsic::Capitalize | StringIntrinsic::Uncapitalize
    ) {
        let units = heap.utf16_units(arg)?;
        charge(fuel, units.len())?;
        return capitalize_utf16(heap, &units, op == StringIntrinsic::Capitalize);
    }
    let text = heap.string_lossy(arg)?;
    charge(fuel, text.len())?;
    let out = match op {
        StringIntrinsic::Trim => js_trim(&text).to_owned(),
        StringIntrinsic::TrimStart => text
            .trim_start_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}')
            .to_owned(),
        StringIntrinsic::TrimEnd => text
            .trim_end_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}')
            .to_owned(),
        StringIntrinsic::ToUpper => text.to_uppercase(),
        StringIntrinsic::ToLower => text.to_lowercase(),
        StringIntrinsic::Lines => return string_list(heap, lines_of(&text, false)),
        StringIntrinsic::LinesWithSeparators => return string_list(heap, lines_of(&text, true)),
        StringIntrinsic::StripMargin => strip_margin(&text, "|"),
        StringIntrinsic::SnakeToCamel => string_case::snake_to(&text, false),
        StringIntrinsic::SnakeToPascal => string_case::snake_to(&text, true),
        StringIntrinsic::SnakeToKebab => text.replace('_', "-"),
        StringIntrinsic::CamelToSnake => string_case::camel_to_snake(&text),
        StringIntrinsic::PascalToSnake => string_case::pascal_to_snake(&text),
        StringIntrinsic::KebabToSnake => text.replace('-', "_"),
        StringIntrinsic::NoCase => string_case::no_case(&text),
        StringIntrinsic::PascalCase => string_case::pascal_case(&text),
        StringIntrinsic::CamelCase => string_case::camel_case(&text),
        StringIntrinsic::ConstantCase => string_case::constant_case(&text),
        StringIntrinsic::ConfigCase => string_case::config_case(&text),
        StringIntrinsic::KebabCase => string_case::kebab_case(&text),
        StringIntrinsic::SnakeCase => string_case::snake_case(&text),
        _ => unreachable!("non-unary string op"),
    };
    heap.literal(&Lit::String(out))
}

fn char_at_unit(
    heap: &mut Heap,
    units: &[u16],
    index: i32,
    allow_negative: bool,
) -> Result<Value, Error> {
    let len = i64::try_from(units.len()).unwrap_or(i64::MAX);
    let resolved = if allow_negative && index < 0 {
        len + i64::from(index)
    } else {
        i64::from(index)
    };
    if resolved >= 0
        && let Ok(idx) = usize::try_from(resolved)
        && let Some(&unit) = units.get(idx)
    {
        let text = heap.alloc_utf16(vec![unit]);
        heap.construct("Prelude.Option.Some", vec![text])
    } else {
        heap.construct("Prelude.Option.None", vec![])
    }
}

fn run_binary(
    heap: &mut Heap,
    op: StringIntrinsic,
    first: &Value,
    second: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let units = heap.utf16_units(second)?;
    charge(fuel, units.len())?;
    match op {
        StringIntrinsic::Split => {
            let sep = heap.utf16_units(first)?;
            charge(fuel, sep.len())?;
            utf16_split_list(heap, &units, &sep)
        }
        StringIntrinsic::EndsWith => {
            let suffix = heap.utf16_units(first)?;
            charge(fuel, suffix.len())?;
            Ok(Value::Bool(units.ends_with(&suffix)))
        }
        StringIntrinsic::IndexOf | StringIntrinsic::LastIndexOf => {
            let part = heap.utf16_units(first)?;
            charge(fuel, part.len())?;
            let found = utf16_index_of(&units, &part, op == StringIntrinsic::LastIndexOf);
            option_int(heap, found)
        }
        StringIntrinsic::ToLocaleUpper => {
            let locale = heap.string(first)?;
            let subject = heap.string_lossy(second)?;
            let out = string_case::to_locale_upper(locale, &subject)?;
            heap.literal(&Lit::String(out))
        }
        StringIntrinsic::ToLocaleLower => {
            let locale = heap.string(first)?;
            let subject = heap.string_lossy(second)?;
            let out = string_case::to_locale_lower(locale, &subject)?;
            heap.literal(&Lit::String(out))
        }
        StringIntrinsic::TakeLeft => {
            let count = usize::try_from(expect_int(first)?.max(0))
                .unwrap_or(0)
                .min(units.len());
            Ok(heap.alloc_utf16(units[..count].to_vec()))
        }
        StringIntrinsic::TakeRight => {
            let count = usize::try_from(expect_int(first)?.max(0))
                .unwrap_or(0)
                .min(units.len());
            Ok(heap.alloc_utf16(units[units.len() - count..].to_vec()))
        }
        StringIntrinsic::At | StringIntrinsic::CharAt => {
            char_at_unit(heap, &units, expect_int(first)?, op == StringIntrinsic::At)
        }
        StringIntrinsic::CharCodeAt => {
            let found = usize::try_from(expect_int(first)?)
                .ok()
                .and_then(|idx| units.get(idx).copied())
                .map(i32::from);
            option_int(heap, found)
        }
        StringIntrinsic::CodePointAt => {
            let found = usize::try_from(expect_int(first)?)
                .ok()
                .and_then(|idx| utf16_code_point_at(&units, idx));
            option_int(heap, found)
        }
        StringIntrinsic::Normalize => {
            let subject = heap.string_lossy(second)?;
            let out = normalize_unicode(heap, first, &subject)?;
            heap.literal(&Lit::String(out))
        }
        StringIntrinsic::Repeat => {
            let count = usize::try_from(expect_int(first)?.max(0)).unwrap_or(0);
            charge(fuel, count.saturating_mul(units.len().max(1)))?;
            let mut out = Vec::with_capacity(units.len().saturating_mul(count));
            for _ in 0..count {
                out.extend_from_slice(&units);
            }
            Ok(heap.alloc_utf16(out))
        }
        StringIntrinsic::StripMarginWith => {
            let margin = heap.string(first)?;
            let subject = heap.string_lossy(second)?;
            charge(fuel, margin.len())?;
            heap.literal(&Lit::String(strip_margin(&subject, margin)))
        }
        _ => unreachable!("non-binary string op"),
    }
}

fn run_ternary(
    heap: &mut Heap,
    op: StringIntrinsic,
    a: &Value,
    b: &Value,
    c: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let units = heap.utf16_units(c)?;
    charge(fuel, units.len())?;
    match op {
        StringIntrinsic::Slice => {
            let s = clamp_slice_idx(expect_int(a)?, units.len());
            let e = clamp_slice_idx(expect_int(b)?, units.len());
            let out = if s < e { units[s..e].to_vec() } else { vec![] };
            Ok(heap.alloc_utf16(out))
        }
        StringIntrinsic::Substring => {
            let s = usize::try_from(expect_int(a)?.max(0))
                .unwrap_or(0)
                .min(units.len());
            let e = usize::try_from(expect_int(b)?.max(0))
                .unwrap_or(0)
                .min(units.len());
            let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
            Ok(heap.alloc_utf16(units[lo..hi].to_vec()))
        }
        StringIntrinsic::PadStart | StringIntrinsic::PadEnd => {
            let target_len = usize::try_from(expect_int(a)?.max(0)).unwrap_or(0);
            let fill = heap.utf16_units(b)?;
            if target_len <= units.len() || fill.is_empty() {
                return Ok(heap.alloc_utf16(units));
            }
            let needed = target_len - units.len();
            charge(fuel, needed)?;
            let mut pad = Vec::with_capacity(needed);
            while pad.len() < needed {
                let take = (needed - pad.len()).min(fill.len());
                pad.extend_from_slice(&fill[..take]);
            }
            let out = if op == StringIntrinsic::PadStart {
                [pad.as_slice(), units.as_slice()].concat()
            } else {
                [units.as_slice(), pad.as_slice()].concat()
            };
            Ok(heap.alloc_utf16(out))
        }
        StringIntrinsic::ReplaceAll => {
            let pat = heap.utf16_units(a)?;
            let repl = heap.utf16_units(b)?;
            charge(fuel, pat.len().saturating_add(repl.len()))?;
            let out = utf16_replace_all(&units, &pat, &repl);
            Ok(heap.alloc_utf16(out))
        }
        _ => unreachable!("non-ternary string op"),
    }
}

fn expect_int(value: &Value) -> Result<i32, Error> {
    match *value {
        Value::Int(i) => Ok(i),
        _ => Err(Error::ExpectedInt),
    }
}

fn option_int(heap: &mut Heap, value: Option<i32>) -> Result<Value, Error> {
    match value {
        Some(i) => heap.construct("Prelude.Option.Some", vec![Value::Int(i)]),
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn string_list(heap: &mut Heap, parts: Vec<String>) -> Result<Value, Error> {
    let mut values = Vec::with_capacity(parts.len());
    for part in parts {
        match heap.literal(&Lit::String(part)) {
            Ok(val) => values.push(val),
            Err(err) => {
                heap.drop_values(values)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", values)
}

fn utf16_chars_list(heap: &mut Heap, units: &[u16]) -> Result<Value, Error> {
    let mut values = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let len = if (0xD800..=0xDBFF).contains(&units[i])
            && i + 1 < units.len()
            && (0xDC00..=0xDFFF).contains(&units[i + 1])
        {
            2
        } else {
            1
        };
        values.push(heap.alloc_utf16(units[i..i + len].to_vec()));
        i += len;
    }
    heap.construct("$list", values)
}

fn utf16_split_list(heap: &mut Heap, subject: &[u16], sep: &[u16]) -> Result<Value, Error> {
    let mut values = Vec::new();
    if sep.is_empty() {
        if subject.is_empty() {
            values.push(heap.alloc_utf16(vec![]));
        } else {
            for &unit in subject {
                values.push(heap.alloc_utf16(vec![unit]));
            }
        }
        return heap.construct("$list", values);
    }
    let mut start = 0;
    let mut i = 0;
    while i + sep.len() <= subject.len() {
        if &subject[i..i + sep.len()] == sep {
            values.push(heap.alloc_utf16(subject[start..i].to_vec()));
            i += sep.len();
            start = i;
        } else {
            i += 1;
        }
    }
    values.push(heap.alloc_utf16(subject[start..].to_vec()));
    heap.construct("$list", values)
}

fn utf16_replace_all(subject: &[u16], pat: &[u16], repl: &[u16]) -> Vec<u16> {
    if pat.is_empty() {
        let mut out = Vec::with_capacity(subject.len() + repl.len() * (subject.len() + 1));
        out.extend_from_slice(repl);
        for &u in subject {
            out.push(u);
            out.extend_from_slice(repl);
        }
        return out;
    }
    let mut out = Vec::with_capacity(subject.len());
    let mut i = 0;
    while i < subject.len() {
        if i + pat.len() <= subject.len() && &subject[i..i + pat.len()] == pat {
            out.extend_from_slice(repl);
            i += pat.len();
        } else {
            out.push(subject[i]);
            i += 1;
        }
    }
    out
}

fn capitalize_utf16(heap: &mut Heap, units: &[u16], upper: bool) -> Result<Value, Error> {
    let Some(&first) = units.first() else {
        return heap.literal(&Lit::String(String::new()));
    };
    let mut out = if (0xD800..=0xDFFF).contains(&first) {
        vec![first]
    } else if let Some(ch) = char::from_u32(u32::from(first)) {
        let mapped = if upper {
            ch.to_uppercase().to_string()
        } else {
            ch.to_lowercase().to_string()
        };
        mapped.encode_utf16().collect()
    } else {
        vec![first]
    };
    out.extend_from_slice(&units[1..]);
    Ok(heap.alloc_utf16(out))
}

fn utf16_index_of(subject: &[u16], part: &[u16], last: bool) -> Option<i32> {
    if part.is_empty() {
        let idx = if last { subject.len() } else { 0 };
        return i32::try_from(idx).ok();
    }
    if part.len() > subject.len() {
        return None;
    }
    let idx = if last {
        (0..=subject.len() - part.len())
            .rev()
            .find(|&i| &subject[i..i + part.len()] == part)?
    } else {
        (0..=subject.len() - part.len()).find(|&i| &subject[i..i + part.len()] == part)?
    };
    i32::try_from(idx).ok()
}

fn utf16_code_point_at(units: &[u16], idx: usize) -> Option<i32> {
    let first = u32::from(*units.get(idx)?);
    if (0xD800..=0xDBFF).contains(&first)
        && let Some(&next) = units.get(idx + 1)
    {
        let second = u32::from(next);
        if (0xDC00..=0xDFFF).contains(&second) {
            let cp = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            return i32::try_from(cp).ok();
        }
    }
    i32::try_from(first).ok()
}

fn clamp_slice_idx(raw: i32, len: usize) -> usize {
    let len_i64 = i64::try_from(len).unwrap_or(i64::MAX);
    let idx = if raw < 0 {
        (len_i64 + i64::from(raw)).max(0)
    } else {
        i64::from(raw).min(len_i64)
    };
    usize::try_from(idx).unwrap_or(len)
}

fn lines_of(s: &str, keep: bool) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i] != b'\r' && bytes[i] != b'\n' {
            i += 1;
        }
        let mut end = i;
        if i < bytes.len() {
            i += if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
            if keep {
                end = i;
            }
        }
        out.push(s[start..end].to_owned());
    }
    out
}

fn strip_margin(s: &str, margin: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in lines_of(s, true) {
        let trimmed = line.trim_start_matches(|c: char| c <= ' ');
        if let Some(rest) = trimmed.strip_prefix(margin) {
            out.push_str(rest);
        } else {
            out.push_str(&line);
        }
    }
    out
}

fn normalize_unicode(heap: &Heap, form: &Value, subject: &str) -> Result<String, Error> {
    let Value::Object(tok) = form else {
        return Err(Error::ExpectedObject);
    };
    let Data::Tagged { tag, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedObject);
    };
    match tag.rsplit('.').next().unwrap_or(tag) {
        "NFC" => Ok(subject.nfc().collect()),
        "NFD" => Ok(subject.nfd().collect()),
        "NFKC" => Ok(subject.nfkc().collect()),
        "NFKD" => Ok(subject.nfkd().collect()),
        _ => Err(Error::RangeError(format!(
            "Invalid normalization form: {tag}"
        ))),
    }
}
