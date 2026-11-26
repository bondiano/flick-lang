//! Pure metered `Config` and `ConfigProvider` primitives (Stage 4D).

use std::collections::{HashMap, HashSet};

use flick_core::ir::Lit;
use flick_core::ir::memory::ConfigIntrinsic;
use num_bigint::BigInt;
use num_traits::{Signed, Zero};

use super::config_url::parse_url_href;
use super::datetime_parse::parse_js_date;
use super::schema_leaves::is_finite_text;
use super::string_case::config_case;
use super::{Error, Fuel, Heap, Value, charge};

#[path = "config_ops/nodes.rs"]
mod nodes;

pub(super) fn run(
    heap: &mut Heap,
    op: ConfigIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        ConfigIntrinsic::ConfigCase => {
            let s = heap.string_lossy(&args[0])?;
            charge(fuel, s.len())?;
            heap.literal(&Lit::String(config_case(&s)))
        }
        ConfigIntrinsic::ParseDotEnv => parse_dot_env_op(heap, &args[0], &args[1], fuel),
        ConfigIntrinsic::ParseUrl => {
            let s = heap.string_lossy(&args[0])?;
            charge(fuel, s.len())?;
            match parse_url_href(&s) {
                Some(href) => {
                    let v = heap.literal(&Lit::String(href))?;
                    heap.construct("Prelude.Option.Some", vec![v])
                }
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        ConfigIntrinsic::ParseDate => {
            let s = heap.string_lossy(&args[0])?;
            charge(fuel, s.len())?;
            let ms = parse_js_date(&s);
            if ms.is_nan() {
                heap.construct("Prelude.Option.None", vec![])
            } else {
                heap.construct("Prelude.Option.Some", vec![Value::Float(ms)])
            }
        }
        ConfigIntrinsic::DecodeByteSize => decode_byte_size_op(heap, &args[0], fuel),
        ConfigIntrinsic::DecodeNumber => decode_number_op(heap, &args[0], fuel),
        ConfigIntrinsic::SegmentInfo => {
            if let Value::Int(i) = &args[0] {
                heap.construct("Prelude.Result.Ok", vec![Value::Int(*i)])
            } else {
                let s = heap.string_lossy(&args[0])?;
                let sv = heap.literal(&Lit::String(s))?;
                heap.construct("Prelude.Result.Err", vec![sv])
            }
        }
        ConfigIntrinsic::UnknownNode => {
            nodes::unknown_node_op(heap, &args[0], &args[1], &args[2], fuel)
        }
        ConfigIntrinsic::EnvNode => nodes::env_node_op(heap, &args[0], &args[1], &args[2], fuel),
    }
}

fn decode_number_op(heap: &mut Heap, text_val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let s = heap.string_lossy(text_val)?;
    charge(fuel, s.len())?;
    let parsed = if is_finite_text(&s) {
        s.parse::<f64>().ok()
    } else if s == "NaN" {
        Some(f64::NAN)
    } else if s == "Infinity" {
        Some(f64::INFINITY)
    } else if s == "-Infinity" {
        Some(f64::NEG_INFINITY)
    } else {
        None
    };
    match parsed {
        Some(f) => heap.construct("Prelude.Option.Some", vec![Value::Float(f)]),
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn byte_size_unit(unit: &str) -> Option<BigInt> {
    const DEC_P: [&str; 10] = ["k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];
    const DEC_W: [&str; 10] = [
        "kilo", "mega", "giga", "tera", "peta", "exa", "zetta", "yotta", "ronna", "quetta",
    ];
    const BIN_W: [&str; 8] = [
        "kibi", "mebi", "gibi", "tebi", "pebi", "exbi", "zebi", "yobi",
    ];
    if matches!(unit, "B" | "byte" | "bytes") {
        return Some(BigInt::from(1));
    }
    for i in 0..DEC_P.len() {
        let p = DEC_P[i];
        let w = DEC_W[i];
        if unit == format!("{p}B") || unit == format!("{w}byte") || unit == format!("{w}bytes") {
            return Some(BigInt::from(1000u32).pow(u32::try_from(i + 1).unwrap_or(1)));
        }
        if i < BIN_W.len() {
            let bw = BIN_W[i];
            let up = p.to_ascii_uppercase();
            if unit == format!("{up}iB")
                || unit == format!("{bw}byte")
                || unit == format!("{bw}bytes")
            {
                return Some(BigInt::from(1024u32).pow(u32::try_from(i + 1).unwrap_or(1)));
            }
        }
    }
    None
}

fn decode_byte_size_op(heap: &mut Heap, text_val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let raw = heap.string_lossy(text_val)?;
    charge(fuel, raw.len())?;
    let s = raw.trim();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let int_part = &s[..i];
    let mut frac_part = "";
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return heap.construct("Prelude.Option.None", vec![]);
        }
        frac_part = &s[start..i];
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let unit = &s[i..];
    if unit.is_empty() || !unit.bytes().all(|b| b.is_ascii_alphabetic()) {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let Some(mult) = byte_size_unit(unit) else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let combined = format!("{int_part}{frac_part}");
    let Some(mut n) = BigInt::parse_bytes(combined.as_bytes(), 10) else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    n *= mult;
    let scale = BigInt::from(10u32).pow(u32::try_from(frac_part.len()).unwrap_or(0));
    let rem = &n % &scale;
    if !rem.is_zero() || n.is_negative() {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let q = n / scale;
    let bi = heap.literal(&Lit::BigInt(q.to_str_radix(10)))?;
    heap.construct("Prelude.Option.Some", vec![bi])
}

fn is_word_byte(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphanumeric()
}

fn dot_env_interpolate(
    value: &str,
    env: &HashMap<String, String>,
    seen: &HashSet<String>,
) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' || (i > 0 && bytes[i - 1] == b'\\') {
            i += 1;
            continue;
        }
        let mut start = i + 1;
        let braced = bytes.get(start) == Some(&b'{');
        if braced {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len() && is_word_byte(bytes[end]) {
            end += 1;
        }
        if end == start {
            i += 1;
            continue;
        }
        let name = &value[start..end];
        let mut fallback = "";
        if value[end..].starts_with(":-") {
            end += 2;
            let from = end;
            let mut depth = 0usize;
            while end < bytes.len() {
                if value[end..].starts_with("${") {
                    depth += 1;
                    end += 2;
                } else if bytes[end] == b'}' {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    end += 1;
                } else {
                    end += 1;
                }
            }
            fallback = &value[from..end];
        }
        if bytes.get(end) == Some(&b'}') {
            end += 1;
        }
        let mut expanded = String::new();
        if let Some(v) = env.get(name)
            && !seen.contains(name)
        {
            let mut inner = seen.clone();
            inner.insert(name.to_owned());
            expanded = dot_env_interpolate(v, env, &inner);
        }
        if expanded.is_empty() && !fallback.is_empty() {
            expanded = dot_env_interpolate(fallback, env, seen);
        }
        out.push_str(&value[cursor..i]);
        out.push_str(&expanded);
        cursor = end;
        i = end;
    }
    out.push_str(&value[cursor..]);
    out
}

fn parse_dot_env_lines(text: &str) -> (Vec<String>, HashMap<String, String>) {
    let norm = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut env = HashMap::new();
    let mut names = Vec::new();
    for raw_line in norm.split('\n') {
        let mut line = raw_line.trim_start_matches([' ', '\t', '\x0C', '\x0B']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export")
            && rest.starts_with(|c: char| c.is_ascii_whitespace())
        {
            line = rest.trim_start();
        }
        let mut k_end = 0;
        for (idx, ch) in line.char_indices() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-') {
                k_end = idx + ch.len_utf8();
            } else {
                break;
            }
        }
        if k_end == 0 {
            continue;
        }
        let key = &line[..k_end];
        let rest = &line[k_end..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        let after_sep = if let Some(r) = trimmed.strip_prefix('=') {
            r.trim_start_matches([' ', '\t'])
        } else if let Some(r) = rest.strip_prefix(':')
            && r.starts_with([' ', '\t'])
        {
            r.trim_start_matches([' ', '\t'])
        } else {
            continue;
        };
        let mut val = if let Some(first) = after_sep.as_bytes().first().copied()
            && matches!(first, b'\'' | b'"' | b'`')
        {
            let mut end = 1;
            let b = after_sep.as_bytes();
            let mut found = None;
            while end < b.len() {
                if b[end] == b'\\' && end + 1 < b.len() {
                    end += 2;
                    continue;
                }
                if b[end] == first {
                    found = Some(end);
                    break;
                }
                end += 1;
            }
            if let Some(close) = found {
                let inner = &after_sep[1..close];
                if first == b'"' {
                    inner.replace("\\n", "\n").replace("\\r", "\r")
                } else {
                    inner.to_owned()
                }
            } else {
                after_sep.trim().to_owned()
            }
        } else {
            let cut = after_sep.find('#').unwrap_or(after_sep.len());
            after_sep[..cut].trim().to_owned()
        };
        let trimmed_len = val.trim_end_matches('\r').len();
        val.truncate(trimmed_len);
        if !env.contains_key(key) {
            names.push(key.to_owned());
        }
        env.insert(key.to_owned(), val);
    }
    (names, env)
}

fn parse_dot_env_op(
    heap: &mut Heap,
    expand_val: &Value,
    text_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let Value::Bool(expand) = *expand_val else {
        return Err(Error::ExpectedBoolean);
    };
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len())?;
    let (names, mut env) = parse_dot_env_lines(&text);
    if expand {
        let mut expanded = HashMap::with_capacity(env.len());
        for k in &names {
            let mut seen = HashSet::new();
            seen.insert(k.clone());
            let v = dot_env_interpolate(&env[k], &env, &seen).replace("\\$", "$");
            expanded.insert(k.clone(), v);
        }
        env = expanded;
    }
    let mut pairs = Vec::with_capacity(names.len());
    for k in names {
        let v = env.remove(&k).unwrap_or_default();
        let kv = heap.literal(&Lit::String(k))?;
        let vv = match heap.literal(&Lit::String(v)) {
            Ok(val) => val,
            Err(err) => {
                heap.drop_value(kv)?;
                heap.drop_values(pairs)?;
                return Err(err);
            }
        };
        match heap.construct("$tuple", vec![kv, vv]) {
            Ok(p) => pairs.push(p),
            Err(err) => {
                heap.drop_values(pairs)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", pairs)
}
