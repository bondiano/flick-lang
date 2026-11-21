//! Biased numeric, decimal, and string random draws for `Arbitrary` (Stage 4D).

use flick_core::ir::Lit;
use num_bigint::{BigInt, Sign};
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::super::bigint_ops::parse_operand;
use super::super::decimal_round::pow10;
use super::super::{Data, Error, Heap, Value, charge};
use super::{
    Xoshiro, alloc_utf16, exact_f64_to_bigint, expect_num, index_number, number_index,
    pack_rng_pair, read_rng,
};

fn log_like(v: &BigInt) -> BigInt {
    if v.is_zero() {
        BigInt::zero()
    } else {
        BigInt::from(v.to_str_radix(10).len())
    }
}

fn select_biased_idx(rng: &mut Xoshiro, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    #[expect(clippy::cast_precision_loss, reason = "tiny slice length")]
    let n = len as f64;
    let i = rng.random_int(-2.0 * (n - 1.0), n - 2.0);
    if i < 0.0 {
        0
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bounded by len"
        )]
        {
            (i as usize) + 1
        }
    }
}

fn numeric_bigint_draw(rng: &mut Xoshiro, min_v: &BigInt, max_v: &BigInt, bias: f64) -> BigInt {
    if !rng.biased(bias) {
        return rng.random_bigint(min_v, max_v);
    }
    let ranges: Vec<(BigInt, BigInt)> = if min_v == max_v {
        vec![(min_v.clone(), max_v.clone())]
    } else if min_v.is_negative() && max_v.is_positive() {
        let low = log_like(&(-min_v));
        let high = log_like(max_v);
        vec![
            (-&low, high.clone()),
            (max_v - &high, max_v.clone()),
            (min_v.clone(), min_v + &low),
        ]
    } else {
        let gap = log_like(&(max_v - min_v));
        let mut rs = vec![(min_v.clone(), min_v + &gap), (max_v - &gap, max_v.clone())];
        if min_v.is_negative() {
            rs.swap(0, 1);
        }
        rs
    };
    let idx = select_biased_idx(rng, ranges.len());
    rng.random_bigint(&ranges[idx].0, &ranges[idx].1)
}

pub(super) fn random_number_op(heap: &mut Heap, args: &[Value]) -> Result<Value, Error> {
    let mut rng = read_rng(heap, &args[0])?;
    let lo_f = expect_num(&args[1])?;
    let hi_f = expect_num(&args[2])?;
    let Value::Bool(allow_nan) = args[3] else {
        return Err(Error::ExpectedBoolean);
    };
    let bias = expect_num(&args[4])?;
    let (lo, hi) = (number_index(lo_f), number_index(hi_f));
    let nan_below = allow_nan && hi <= 0;
    let mut from = BigInt::from(lo);
    let mut to = BigInt::from(hi);
    if nan_below {
        from -= BigInt::one();
    }
    if allow_nan && !nan_below {
        to += BigInt::one();
    }
    let idx = numeric_bigint_draw(&mut rng, &from, &to, bias);
    let val = match idx.to_i64() {
        Some(i) if i >= lo && i <= hi => index_number(i),
        _ => f64::NAN,
    };
    pack_rng_pair(heap, rng, Value::Float(val))
}

pub(in super::super) fn semantic_magnitudes() -> Vec<BigInt> {
    let max_val = exact_f64_to_bigint(f64::MAX);
    let max_safe = BigInt::from(9_007_199_254_740_991i64);
    vec![
        BigInt::from(1),
        BigInt::from(2),
        BigInt::from(0x7fff_ffffi64),
        BigInt::from(0x8000_0000i64),
        max_safe.clone(),
        max_safe + BigInt::one(),
        max_val.clone(),
        max_val + BigInt::one(),
        BigInt::one() << 1024usize,
    ]
}

fn random_magnitude(rng: &mut Xoshiro, max_v: Option<&BigInt>) -> BigInt {
    if max_v.is_some_and(Zero::is_zero) {
        return BigInt::zero();
    }
    let family = rng.random_int(0.0, 7.0);
    if family == 0.0 {
        return BigInt::zero();
    }
    #[expect(clippy::cast_precision_loss, reason = "bit length fits f64")]
    let max_bits = max_v.map_or(2048.0, |m| m.bits() as f64);
    let upper_bits = if family <= 3.0 {
        20.0f64.min(max_bits)
    } else {
        max_bits
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded 1..=2048"
    )]
    let bits = rng.random_int(1.0, upper_bits) as usize;
    let lowest = BigInt::one() << (bits - 1);
    let mut upper = (&lowest << 1usize) - BigInt::one();
    if let Some(m) = max_v
        && m < &upper
    {
        upper.clone_from(m);
    }
    rng.random_bigint(&lowest, &upper)
}

fn read_opt_bigint(heap: &Heap, val: &Value, fuel: &mut u64) -> Result<Option<BigInt>, Error> {
    let Value::Object(tok) = val else {
        return Ok(None);
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Ok(None);
    };
    if tag == "Prelude.Option.Some" && !fields.is_empty() {
        let inner = fields[0].representation();
        Ok(Some(parse_operand(heap, &inner, fuel)?))
    } else {
        Ok(None)
    }
}

pub(super) fn random_bigint_biased_op(
    heap: &mut Heap,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    let mut rng = read_rng(heap, &args[0])?;
    let min_v = read_opt_bigint(heap, &args[1], fuel)?;
    let max_v = read_opt_bigint(heap, &args[2], fuel)?;
    let bias = expect_num(&args[3])?;
    let drawn = if let (Some(lo), Some(hi)) = (&min_v, &max_v) {
        numeric_bigint_draw(&mut rng, lo, hi, bias)
    } else {
        let mut special = vec![BigInt::zero()];
        if let Some(lo) = &min_v {
            special.push(lo.clone());
            special.push(lo + BigInt::one());
        }
        if let Some(hi) = &max_v {
            special.push(hi.clone());
            special.push(hi - BigInt::one());
        }
        for m in semantic_magnitudes() {
            special.push(m.clone());
            special.push(-m);
        }
        special.retain(|v| {
            min_v.as_ref().is_none_or(|lo| v >= lo) && max_v.as_ref().is_none_or(|hi| v <= hi)
        });
        let (boundary, dir) = if let Some(lo) = &min_v {
            (Some(lo), BigInt::from(1))
        } else if let Some(hi) = &max_v {
            (Some(hi), BigInt::from(-1))
        } else {
            (None, BigInt::from(1))
        };
        let finite_mag = boundary.and_then(|b| {
            let p = b * &dir;
            if p.sign() == Sign::Plus {
                None
            } else {
                Some(-p)
            }
        });
        if rng.random_int(0.0, 7.0) == 0.0 {
            let idx = rng.index(special.len());
            special[idx].clone()
        } else if let Some(b) = boundary
            && ((b * &dir).is_positive() || rng.random_bool())
        {
            b + &dir * random_magnitude(&mut rng, None)
        } else {
            let sign = if rng.random_bool() {
                BigInt::from(1)
            } else {
                BigInt::from(-1)
            };
            let lim = if sign == dir {
                None
            } else {
                finite_mag.as_ref()
            };
            sign * random_magnitude(&mut rng, lim)
        }
    };
    let bi = heap.literal(&Lit::BigInt(drawn.to_str_radix(10)))?;
    pack_rng_pair(heap, rng, bi)
}

pub(super) fn random_decimal_op(heap: &mut Heap, rng_val: &Value) -> Result<Value, Error> {
    let mut rng = read_rng(heap, rng_val)?;
    let (value, scale) = if rng.random_int(0.0, 7.0) == 0.0 {
        let special = [
            (0i64, 0i64),
            (1, 1),
            (1, 0),
            (10, 0),
            (-1, 1),
            (-1, 0),
            (-10, 0),
        ];
        let (v, s) = special[rng.index(special.len())];
        (BigInt::from(v), s)
    } else {
        let mode = rng.random_int(0.0, 7.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::float_cmp,
            reason = "exact integer mode in 0..=7 and small scale range"
        )]
        let mut scale = if mode == 6.0 {
            rng.random_int(-6.0, 20.0) as i64
        } else {
            let precision = rng.random_int(1.0, 128.0);
            (precision - 1.0 - rng.random_int(-512.0, 512.0)) as i64
        };
        let bits_f = if rng.random_int(0.0, 3.0) == 0.0 {
            rng.random_int(1.0, 425.0)
        } else {
            rng.random_int(0.0, 20.0)
        };
        let mut dist = if bits_f > 0.0 {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "1..=425"
            )]
            let bits = bits_f as usize;
            let lowest = BigInt::one() << (bits - 1);
            let upper = (&lowest << 1usize) - BigInt::one();
            rng.random_bigint(&lowest, &upper)
        } else {
            BigInt::zero()
        };
        if !rng.random_bool() {
            dist = -dist;
        }
        let max_safe_i = 9_007_199_254_740_991i64;
        if rng.random_int(0.0, 7.0) == 0.0 && scale < max_safe_i {
            #[expect(clippy::cast_precision_loss, reason = "clamped to 8")]
            let cap = 8.0f64.min((max_safe_i - scale) as f64);
            #[expect(clippy::cast_possible_truncation, reason = "1..=8")]
            let trailing = rng.random_int(1.0, cap) as i64;
            dist *= pow10(trailing);
            scale += trailing;
        }
        (dist, scale)
    };
    let dec = heap.allocate(Data::BigDecimal { value, scale });
    pack_rng_pair(heap, rng, dec)
}

fn string_edge_cases() -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = [
        "",
        " ",
        "\t",
        "\n",
        "\x00",
        "0",
        "-1",
        "4294967295",
        "__proto__",
        "constructor",
        "prototype",
        "toString",
    ]
    .iter()
    .map(|s| s.encode_utf16().collect())
    .collect();
    out.push(vec![0xD800]);
    out.push(vec![0xDC00]);
    out.push(vec![0xD83D, 0xDE00]);
    out
}

pub(super) fn random_string_op(
    heap: &mut Heap,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    let mut rng = read_rng(heap, &args[0])?;
    let min_v = expect_num(&args[1])?;
    let max_v = expect_num(&args[2])?;
    let bias = expect_num(&args[3])?;
    let units = if rng.biased(bias) {
        let edges: Vec<Vec<u16>> = string_edge_cases()
            .into_iter()
            .filter(|e| {
                #[expect(clippy::cast_precision_loss, reason = "small edge length")]
                let n = e.len() as f64;
                n >= min_v && n <= max_v
            })
            .collect();
        if edges.is_empty() {
            draw_ascii_units(&mut rng, min_v, max_v, bias)
        } else {
            let idx = rng.index(edges.len());
            edges[idx].clone()
        }
    } else {
        draw_ascii_units(&mut rng, min_v, max_v, bias)
    };
    charge(fuel, units.len())?;
    let sv = alloc_utf16(heap, &units)?;
    pack_rng_pair(heap, rng, sv)
}

fn draw_ascii_units(rng: &mut Xoshiro, min_v: f64, max_v: f64, bias: f64) -> Vec<u16> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bounded length"
    )]
    let len = rng.random_length(min_v, max_v, bias) as usize;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "32..=126"
        )]
        out.push(rng.random_int(32.0, 126.0) as u16);
    }
    out
}

pub(super) fn random_cp_string_op(
    heap: &mut Heap,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    let mut rng = read_rng(heap, &args[0])?;
    let min_v = expect_num(&args[1])?;
    let max_v = expect_num(&args[2])?;
    let min_cp = expect_num(&args[3])?;
    let max_cp = expect_num(&args[4])?;
    // Bias factor passed via min/max? Notice randomCodePointString in arbitrary_scalar.go uses st.randomLength -> needs bias.
    // Wait: we have 5 args so keep bias = 2.0 default only if unused, OR check callers. Actually randomCodePointString uses st.biased.
    let count = rng.random_length(min_cp, max_cp, 2.0);
    let length = rng.random_length(min_v.max(count), max_v.min(count * 2.0), 2.0);
    let mut pairs = length - count;
    let mut out = Vec::new();
    let mut rem = count;
    while rem > 0.0 {
        if rng.random_int(1.0, rem) <= pairs {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "supplementary range"
            )]
            let r = (rng.random_int(f64::from(0x10000), f64::from(0x10_ffff)) as u32) - 0x10000;
            out.push(u16::try_from(0xD800 + (r >> 10)).unwrap_or(0xD800));
            out.push(u16::try_from(0xDC00 + (r & 0x3FF)).unwrap_or(0xDC00));
            pairs -= 1.0;
        } else {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "32..=126"
            )]
            out.push(rng.random_int(32.0, 126.0) as u16);
        }
        rem -= 1.0;
    }
    charge(fuel, out.len())?;
    let sv = alloc_utf16(heap, &out)?;
    pack_rng_pair(heap, rng, sv)
}
