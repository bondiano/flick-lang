//! Counted built-in `Check.*` specification constructor primitive.

use flick_core::ir::Lit;
use flick_core::opt::{js_number, json_string};
use regex_automata::meta::Regex;

use super::decimal_ops::format_decimal;
use super::{Data, Error, Heap, Value, charge};

pub(super) const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

pub(super) fn number_of(val: &Value) -> Option<f64> {
    match *val {
        Value::Float(f) => Some(f),
        Value::Int(i) => Some(f64::from(i)),
        _ => None,
    }
}

fn cardinality(val: &Value) -> Result<i32, Error> {
    match *val {
        Value::Int(i) => Ok(i.max(0)),
        #[expect(clippy::cast_possible_truncation, reason = "bounded cardinality")]
        Value::Float(f) => Ok((f.floor().max(0.0) as i64).min(i64::from(i32::MAX)) as i32),
        _ => Err(Error::ExpectedInt),
    }
}

fn finite_bound(val: &Value) -> Result<f64, Error> {
    let x = number_of(val).ok_or(Error::ExpectedFloat)?;
    if !x.is_finite() {
        return Err(Error::RangeError(format!(
            "RangeError: Expected a finite number, got {}",
            js_number(x)
        )));
    }
    Ok(x)
}

pub(super) fn rec_field(heap: &Heap, rec: &Value, name: &str) -> Result<Value, Error> {
    let Value::Object(tok) = rec else {
        return Err(Error::ExpectedRecord);
    };
    let Data::Record(fields) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedRecord);
    };
    fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.representation())
        .ok_or_else(|| Error::MissingNamedField(name.into()))
}

pub(super) fn pair_fields(heap: &Heap, val: &Value) -> Result<(Value, Value), Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("tuple"));
    };
    let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("tuple"));
    };
    Ok((fields[0].representation(), fields[1].representation()))
}

fn regexp_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if r"/\^$*+?.()|[]{}".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn regexp_source(pattern: &str) -> String {
    pattern
        .replace('\n', r"\n")
        .replace('\r', r"\r")
        .replace('\u{2028}', r"\u2028")
        .replace('\u{2029}', r"\u2029")
}

fn build_meta_exact(heap: &mut Heap, desc: &str, kws: Vec<(&str, Value)>) -> Result<Value, Error> {
    let dv = heap.literal(&Lit::String(desc.to_owned()))?;
    let fields: Vec<(String, Value)> = kws.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    let rec = match heap.record(fields) {
        Ok(r) => r,
        Err(err) => {
            heap.drop_value(dv)?;
            return Err(err);
        }
    };
    heap.construct("Check.$Meta.$Exact", vec![dv, rec])
}

fn build_meta_approx(heap: &mut Heap, desc: &str) -> Result<Value, Error> {
    let dv = heap.literal(&Lit::String(desc.to_owned()))?;
    heap.construct("Check.$Meta.$Approx", vec![dv])
}

fn build_meta_none(heap: &mut Heap, desc: &str) -> Result<Value, Error> {
    let dv = heap.literal(&Lit::String(desc.to_owned()))?;
    heap.construct("Check.$Meta.$None", vec![dv])
}

fn build_meta_length(
    heap: &mut Heap,
    desc: &str,
    min: Option<i32>,
    max: Option<i32>,
) -> Result<Value, Error> {
    let dv = heap.literal(&Lit::String(desc.to_owned()))?;
    let min_v = match min {
        Some(n) => heap.construct("Prelude.Option.Some", vec![Value::Int(n)])?,
        None => heap.construct("Prelude.Option.None", vec![])?,
    };
    let max_v = match max {
        Some(n) => heap.construct("Prelude.Option.Some", vec![Value::Int(n)])?,
        None => heap.construct("Prelude.Option.None", vec![])?,
    };
    heap.construct("Check.$Meta.$Length", vec![dv, min_v, max_v])
}

fn make_spec_dup(heap: &mut Heap, kind: &str, param: &Value, meta: Value) -> Result<Value, Error> {
    let dup = match heap.dup(param) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(meta)?;
            return Err(err);
        }
    };
    make_spec(heap, kind, dup, meta)
}

fn make_spec(heap: &mut Heap, kind: &str, param: Value, meta: Value) -> Result<Value, Error> {
    let kv = match heap.literal(&Lit::String(kind.to_owned())) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(param)?;
            heap.drop_value(meta)?;
            return Err(err);
        }
    };
    heap.construct("Check.$Spec.$Spec", vec![kv, param, meta])
}

#[expect(clippy::too_many_lines, reason = "complete Check.* catalog")]
pub(super) fn make_check(
    heap: &mut Heap,
    kind_val: &Value,
    param: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let kind = heap.string_lossy(kind_val)?;
    charge(fuel, kind.len())?;
    match kind.as_str() {
        "min_length" => {
            let n = cardinality(param)?;
            let meta = build_meta_length(
                heap,
                &format!("a value with a length of at least {n}"),
                Some(n),
                None,
            )?;
            make_spec(heap, "min_length", Value::Int(n), meta)
        }
        "max_length" => {
            let n = cardinality(param)?;
            let meta = build_meta_length(
                heap,
                &format!("a value with a length of at most {n}"),
                None,
                Some(n),
            )?;
            make_spec(heap, "max_length", Value::Int(n), meta)
        }
        "length_between" => {
            let (a, b) = pair_fields(heap, param)?;
            let (lo, hi) = (cardinality(&a)?, cardinality(&b)?);
            let desc = if lo == hi {
                format!("a value with a length of {lo}")
            } else {
                format!("a value with a length between {lo} and {hi}")
            };
            let meta = build_meta_length(heap, &desc, Some(lo), Some(hi))?;
            let pair = heap.construct("$tuple", vec![Value::Int(lo), Value::Int(hi)])?;
            make_spec(heap, "length_between", pair, meta)
        }
        "min_code_points" => {
            let n = cardinality(param)?;
            let meta = build_meta_exact(
                heap,
                &format!("a string with at least {n} code points"),
                vec![("minLength", Value::Float(f64::from(n)))],
            )?;
            make_spec(heap, "min_code_points", Value::Int(n), meta)
        }
        "max_code_points" => {
            let n = cardinality(param)?;
            let meta = build_meta_exact(
                heap,
                &format!("a string with at most {n} code points"),
                vec![("maxLength", Value::Float(f64::from(n)))],
            )?;
            make_spec(heap, "max_code_points", Value::Int(n), meta)
        }
        "code_points_between" => {
            let (a, b) = pair_fields(heap, param)?;
            let (lo, hi) = (cardinality(&a)?, cardinality(&b)?);
            let desc = if lo == hi {
                format!("a string with {lo} code points")
            } else {
                format!("a string with between {lo} and {hi} code points")
            };
            let r1 = heap.record(vec![("minLength".into(), Value::Float(f64::from(lo)))])?;
            let r2 = heap.record(vec![("maxLength".into(), Value::Float(f64::from(hi)))])?;
            let all_of = heap.construct("$list", vec![r1, r2])?;
            let meta = build_meta_exact(heap, &desc, vec![("allOf", all_of)])?;
            let pair = heap.construct("$tuple", vec![Value::Int(lo), Value::Int(hi)])?;
            make_spec(heap, "code_points_between", pair, meta)
        }
        "pattern" => {
            let src = heap.string_lossy(param)?;
            if let Err(err) = Regex::new(&src) {
                return Err(Error::RangeError(format!(
                    "SyntaxError: Invalid regular expression: /{src}/: {err}"
                )));
            }
            let meta = build_meta_approx(heap, &format!("a string matching the RegExp {src}"))?;
            make_spec_dup(heap, "pattern", param, meta)
        }
        "starts_with" | "ends_with" | "includes" => {
            let lit = heap.string_lossy(param)?;
            let what = match kind.as_str() {
                "starts_with" => "starting with",
                "ends_with" => "ending with",
                _ => "including",
            };
            let desc = format!("a string {what} {}", json_string(&lit));
            let meta = if lit.is_empty() {
                build_meta_exact(heap, &desc, vec![])?
            } else {
                let esc = regexp_escape(&lit);
                let pat = regexp_source(&match kind.as_str() {
                    "starts_with" => format!("^{esc}"),
                    "ends_with" => format!("{esc}$"),
                    _ => esc,
                });
                let pv = heap.literal(&Lit::String(pat))?;
                build_meta_exact(heap, &desc, vec![("pattern", pv)])?
            };
            make_spec_dup(heap, &kind, param, meta)
        }
        "trimmed" => {
            let pat = heap.literal(&Lit::String(r"^\S[\s\S]*\S$|^\S$|^$".into()))?;
            let meta = build_meta_exact(
                heap,
                "a string with no leading or trailing whitespace",
                vec![("pattern", pat)],
            )?;
            make_spec(heap, "trimmed", Value::Unit, meta)
        }
        "lowercased" | "uppercased" | "capitalized" | "uncapitalized" => {
            let desc = match kind.as_str() {
                "lowercased" => "a string with all characters in lowercase",
                "uppercased" => "a string with all characters in uppercase",
                "capitalized" => "a string with the first character in uppercase",
                _ => "a string with the first character in lowercase",
            };
            let meta = build_meta_approx(heap, desc)?;
            make_spec(heap, &kind, Value::Unit, meta)
        }
        "uuid" => make_uuid_check(heap, param),
        "ulid" | "guid" | "base64" | "base64url" | "string_finite" | "string_bigint" => {
            make_named_pattern_check(heap, &kind)
        }
        "gt" | "gte" | "lt" | "lte" => {
            let b = finite_bound(param)?;
            let (word, kw) = match kind.as_str() {
                "gt" => ("a value greater than", "exclusiveMinimum"),
                "gte" => ("a value greater than or equal to", "minimum"),
                "lt" => ("a value less than", "exclusiveMaximum"),
                _ => ("a value less than or equal to", "maximum"),
            };
            let meta = build_meta_exact(
                heap,
                &format!("{word} {}", js_number(b)),
                vec![(kw, Value::Float(b))],
            )?;
            make_spec(heap, &kind, Value::Float(b), meta)
        }
        "between" => make_between_num_check(heap, param),
        "multiple_of" => {
            let d = number_of(param).ok_or(Error::ExpectedFloat)?;
            if !d.is_finite() || d == 0.0 {
                return Err(Error::RangeError(format!(
                    "RangeError: Expected a finite non-zero number, got {}",
                    js_number(d)
                )));
            }
            let abs_d = d.abs();
            let meta = build_meta_exact(
                heap,
                &format!("a value that is a multiple of {}", js_number(abs_d)),
                vec![("multipleOf", Value::Float(abs_d))],
            )?;
            make_spec(heap, "multiple_of", Value::Float(abs_d), meta)
        }
        "int" => {
            let ty = heap.literal(&Lit::String("integer".into()))?;
            let meta = build_meta_exact(
                heap,
                "an integer",
                vec![
                    ("type", ty),
                    ("minimum", Value::Float(-MAX_SAFE)),
                    ("maximum", Value::Float(MAX_SAFE)),
                ],
            )?;
            make_spec(heap, "int", Value::Unit, meta)
        }
        "finite" => {
            let ty = heap.literal(&Lit::String("number".into()))?;
            let meta = build_meta_exact(heap, "a finite number", vec![("type", ty)])?;
            make_spec(heap, "finite", Value::Unit, meta)
        }
        "gt_bigint" | "gte_bigint" | "lt_bigint" | "lte_bigint" => {
            let b = heap.bigint(param)?;
            let word = order_word(&kind);
            let meta = build_meta_approx(heap, &format!("{word} {b}n"))?;
            make_spec_dup(heap, &kind, param, meta)
        }
        "between_bigint" => {
            let lo = heap.bigint(&rec_field(heap, param, "minimum")?)?.to_owned();
            let hi = heap.bigint(&rec_field(heap, param, "maximum")?)?.to_owned();
            let desc = between_desc(
                &format!("{lo}n"),
                &format!("{hi}n"),
                matches!(
                    rec_field(heap, param, "exclusiveMinimum")?,
                    Value::Bool(true)
                ),
                matches!(
                    rec_field(heap, param, "exclusiveMaximum")?,
                    Value::Bool(true)
                ),
            );
            let meta = build_meta_approx(heap, &desc)?;
            make_spec_dup(heap, "between_bigint", param, meta)
        }
        "gt_decimal" | "gte_decimal" | "lt_decimal" | "lte_decimal" => {
            let ds = format_dec_val(heap, param)?;
            let meta = build_meta_none(heap, &format!("{} {ds}", order_word(&kind)))?;
            make_spec_dup(heap, &kind, param, meta)
        }
        "between_decimal" => {
            let lo = format_dec_val(heap, &rec_field(heap, param, "minimum")?)?;
            let hi = format_dec_val(heap, &rec_field(heap, param, "maximum")?)?;
            let desc = between_desc(
                &lo,
                &hi,
                matches!(
                    rec_field(heap, param, "exclusiveMinimum")?,
                    Value::Bool(true)
                ),
                matches!(
                    rec_field(heap, param, "exclusiveMaximum")?,
                    Value::Bool(true)
                ),
            );
            let meta = build_meta_none(heap, &desc)?;
            make_spec_dup(heap, "between_decimal", param, meta)
        }
        "unique" => {
            let meta = build_meta_exact(
                heap,
                "an array with unique items",
                vec![("uniqueItems", Value::Bool(true))],
            )?;
            make_spec(heap, "unique", Value::Unit, meta)
        }
        "unique_keys" => {
            let meta = build_meta_none(heap, "an array with unique keys")?;
            make_spec(heap, "unique_keys", Value::Unit, meta)
        }
        _ => Err(Error::RangeError(format!("unknown Check kind: {kind}"))),
    }
}

fn order_word(kind: &str) -> &'static str {
    if kind.starts_with("gte_") {
        "a value greater than or equal to"
    } else if kind.starts_with("gt_") {
        "a value greater than"
    } else if kind.starts_with("lte_") {
        "a value less than or equal to"
    } else {
        "a value less than"
    }
}

fn between_desc(lo: &str, hi: &str, ex_lo: bool, ex_hi: bool) -> String {
    let sl = if ex_lo { " (excluded)" } else { "" };
    let sh = if ex_hi { " (excluded)" } else { "" };
    format!("a value between {lo}{sl} and {hi}{sh}")
}

fn make_between_num_check(heap: &mut Heap, param: &Value) -> Result<Value, Error> {
    let lo = number_of(&rec_field(heap, param, "minimum")?).ok_or(Error::ExpectedFloat)?;
    let hi = number_of(&rec_field(heap, param, "maximum")?).ok_or(Error::ExpectedFloat)?;
    let ex_lo = matches!(
        rec_field(heap, param, "exclusiveMinimum")?,
        Value::Bool(true)
    );
    let ex_hi = matches!(
        rec_field(heap, param, "exclusiveMaximum")?,
        Value::Bool(true)
    );
    let desc = between_desc(&js_number(lo), &js_number(hi), ex_lo, ex_hi);
    let k_lo = if ex_lo { "exclusiveMinimum" } else { "minimum" };
    let k_hi = if ex_hi { "exclusiveMaximum" } else { "maximum" };
    let meta = build_meta_exact(
        heap,
        &desc,
        vec![(k_lo, Value::Float(lo)), (k_hi, Value::Float(hi))],
    )?;
    make_spec_dup(heap, "between", param, meta)
}

fn make_uuid_check(heap: &mut Heap, param: &Value) -> Result<Value, Error> {
    let ver = extract_opt_int(heap, param)?;
    let (pat, desc) = if ver == 0 {
        (
            r"^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|[fF]{8}-[fF]{4}-[fF]{4}-[fF]{4}-[fF]{12})$"
                .to_owned(),
            "a UUID".to_owned(),
        )
    } else {
        (
            format!(
                r"^([0-9a-fA-F]{{8}}-[0-9a-fA-F]{{4}}-{ver}[0-9a-fA-F]{{3}}-[89abAB][0-9a-fA-F]{{3}}-[0-9a-fA-F]{{12}})$"
            ),
            format!("a UUID v{ver}"),
        )
    };
    let pv = heap.literal(&Lit::String(pat.clone()))?;
    let fv = heap.literal(&Lit::String("uuid".into()))?;
    let meta = build_meta_exact(heap, &desc, vec![("pattern", pv), ("format", fv)])?;
    let p_str = heap.literal(&Lit::String(pat))?;
    let d_str = heap.literal(&Lit::String(desc))?;
    let pair = heap.construct("$tuple", vec![p_str, d_str])?;
    make_spec(heap, "regex_exact", pair, meta)
}

fn make_named_pattern_check(heap: &mut Heap, kind: &str) -> Result<Value, Error> {
    let (pat, desc) = match kind {
        "ulid" => (
            r"^[0-7][0-9A-HJKMNP-TV-Za-hjkmnp-tv-z]{25}$",
            r"a string matching the RegExp ^[0-7][0-9A-HJKMNP-TV-Za-hjkmnp-tv-z]{25}$",
        ),
        "guid" => (
            r"^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$",
            "a GUID",
        ),
        "base64" => (
            r"^([0-9a-zA-Z+/]{4})*(([0-9a-zA-Z+/]{2}==)|([0-9a-zA-Z+/]{3}=))?$",
            "a base64 encoded string",
        ),
        "base64url" => (
            r"^([0-9a-zA-Z-_]{4})*(([0-9a-zA-Z-_]{2}(==)?)|([0-9a-zA-Z-_]{3}(=)?))?$",
            "a base64url encoded string",
        ),
        "string_finite" => (
            r"^[+-]?\d*\.?\d+(?:[Ee][+-]?\d+)?$",
            "a string representing a finite number",
        ),
        _ => (r"^-?\d+$", "a string representing a bigint"),
    };
    let pv = heap.literal(&Lit::String(pat.into()))?;
    let meta = build_meta_exact(heap, desc, vec![("pattern", pv)])?;
    let p_str = heap.literal(&Lit::String(pat.into()))?;
    let d_str = heap.literal(&Lit::String(desc.into()))?;
    let pair = heap.construct("$tuple", vec![p_str, d_str])?;
    make_spec(heap, "regex_exact", pair, meta)
}

fn extract_opt_int(heap: &Heap, val: &Value) -> Result<i32, Error> {
    let Value::Object(tok) = val else {
        return Ok(0);
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Ok(0);
    };
    if tag == "Prelude.Option.Some"
        && let Some(Value::Int(n)) = fields.first()
    {
        return Ok(*n);
    }
    Ok(0)
}

fn format_dec_val(heap: &Heap, val: &Value) -> Result<String, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    let Data::BigDecimal { value, scale } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    Ok(format_decimal(value, *scale))
}
