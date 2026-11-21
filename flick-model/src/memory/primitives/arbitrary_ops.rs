//! Pure metered `Arbitrary` RNG and value-generation primitives (Stage 4D).

use flick_core::ir::Lit;
use flick_core::ir::memory::ArbitraryIntrinsic;
use flick_core::opt::js_number;
use num_bigint::BigInt;
use num_traits::{One, ToPrimitive, Zero};

use super::arbitrary_fmt::{
    extract_constraint, format_spaced_op, is_non_negative_duration, js_string_inner, js_string_val,
    make_duration, make_zone, make_zoned_datetime, merge_constraint, quote_json_op,
};
use super::arbitrary_shrink::{
    decimal_midpoint, number_bounds, shrink_bigint, shrink_decimal_cands, shrink_number,
    shrink_string,
};
use super::{Data, Error, Heap, Value, charge};

#[path = "arbitrary_ops/draw.rs"]
mod draw;

pub(super) use draw::semantic_magnitudes;

pub(super) const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
const UINT32_VALUES: f64 = 4_294_967_296.0;
const DOUBLE_VALUES: f64 = 9_007_199_254_740_992.0;

#[derive(Clone, Copy)]
pub(super) struct Xoshiro {
    pub(super) s0: u32,
    pub(super) s1: u32,
    pub(super) s2: u32,
    pub(super) s3: u32,
}

impl Xoshiro {
    pub(super) fn next_u32(&mut self) -> u32 {
        let result = (self.s1.wrapping_mul(5)).rotate_left(7).wrapping_mul(9);
        let t = self.s1 << 9;
        self.s2 ^= self.s0;
        self.s3 ^= self.s1;
        self.s1 ^= self.s2;
        self.s0 ^= self.s3;
        self.s2 ^= t;
        self.s3 = self.s3.rotate_left(11);
        result
    }

    pub(super) fn next_double(&mut self) -> f64 {
        let high = f64::from(self.next_u32() >> 5);
        let low = f64::from(self.next_u32() >> 6);
        (high * 67_108_864.0 + low) / DOUBLE_VALUES
    }

    pub(super) fn below(&mut self, n: f64) -> f64 {
        let bucket = (UINT32_VALUES / n).floor();
        let accepted = bucket * n;
        let mut v = f64::from(self.next_u32());
        while v >= accepted {
            v = f64::from(self.next_u32());
        }
        (v / bucket).floor()
    }

    pub(super) fn index(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "index bounded by n"
        )]
        {
            self.below(n as f64) as usize
        }
    }

    pub(super) fn random_bool(&mut self) -> bool {
        (self.next_u32() & 1) == 1
    }

    #[expect(clippy::float_cmp, reason = "exact integer check from random_int")]
    pub(super) fn biased(&mut self, bias_factor: f64) -> bool {
        self.random_int(1.0, bias_factor) == 1.0
    }

    pub(super) fn random_length(&mut self, min_v: f64, max_v: f64, bias_factor: f64) -> f64 {
        #[expect(clippy::float_cmp, reason = "exact JS range equality check")]
        if min_v == max_v {
            return min_v;
        }
        if self.biased(bias_factor) {
            return if self.random_bool() { min_v } else { max_v };
        }
        self.random_int(min_v, max_v)
    }

    pub(super) fn random_int(&mut self, min_raw: f64, max_raw: f64) -> f64 {
        let min_v = min_raw.ceil();
        let max_v = max_raw.floor();
        #[expect(clippy::float_cmp, reason = "exact JS integer range equality check")]
        if min_v == max_v {
            return min_v;
        }
        let width = max_v - min_v + 1.0;
        if width <= UINT32_VALUES {
            return min_v + self.below(width);
        }
        if width <= DOUBLE_VALUES {
            let bucket = (DOUBLE_VALUES / width).floor();
            let accepted = bucket * width;
            loop {
                let v = (self.next_double() * DOUBLE_VALUES).floor();
                if v < accepted {
                    return min_v + (v / bucket).floor();
                }
            }
        }
        let lo = exact_f64_to_bigint(min_v);
        let hi = exact_f64_to_bigint(max_v);
        self.random_bigint(&lo, &hi).to_f64().unwrap_or(min_v)
    }

    pub(super) fn random_bigint(&mut self, min_v: &BigInt, max_v: &BigInt) -> BigInt {
        if min_v == max_v {
            return min_v.clone();
        }
        let width_m1 = max_v - min_v;
        let bits = usize::try_from(width_m1.bits()).unwrap_or(1).max(1);
        let width = &width_m1 + BigInt::one();
        let leading = (bits - 1) % 32 + 1;
        let lead_limit = f64::from(1u32 << (leading - 1)) * 2.0;
        loop {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "bounded by 2^32"
            )]
            let mut v = BigInt::from(self.below(lead_limit) as u64);
            let mut rest = bits - leading;
            while rest > 0 {
                v <<= 32usize;
                v |= BigInt::from(self.next_u32());
                rest -= 32;
            }
            if v < width {
                return v + min_v;
            }
        }
    }
}

fn mix32(mut v: u32) -> u32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x85eb_ca6b);
    v ^= v >> 13;
    v = v.wrapping_mul(0xc2b2_ae35);
    v ^ (v >> 16)
}

pub(super) fn number_index(v: f64) -> i64 {
    let bits = v.to_bits();
    if (bits >> 63) == 0 {
        bits.cast_signed()
    } else {
        (!bits).wrapping_sub(1u64 << 63).cast_signed()
    }
}

pub(super) fn index_number(i: i64) -> f64 {
    let u = i.cast_unsigned().wrapping_add(1u64 << 63);
    if (u >> 63) == 0 {
        f64::from_bits(!u)
    } else {
        f64::from_bits(u ^ (1u64 << 63))
    }
}

fn exact_f64_to_bigint(n: f64) -> BigInt {
    if n == 0.0 || !n.is_finite() {
        return BigInt::zero();
    }
    let bits = n.to_bits();
    let neg = (bits >> 63) != 0;
    let raw_exp = i32::try_from((bits >> 52) & 0x7ff).unwrap_or(0);
    let exp = raw_exp - 1023 - 52;
    let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let mut big = BigInt::from(mantissa);
    if exp >= 0 {
        big <<= usize::try_from(exp).unwrap_or(0);
    } else {
        big >>= usize::try_from(-exp).unwrap_or(0);
    }
    if neg { -big } else { big }
}

pub(super) fn expect_num(val: &Value) -> Result<f64, Error> {
    match *val {
        Value::Float(f) => Ok(f),
        Value::Int(i) => Ok(f64::from(i)),
        _ => Err(Error::ExpectedFloat),
    }
}

pub(super) fn read_rng(heap: &Heap, val: &Value) -> Result<Xoshiro, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("rng4"));
    };
    let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("rng4"));
    };
    if fields.len() != 4 {
        return Err(Error::PatternKind("rng4"));
    }
    let get = |i: usize| match fields[i] {
        Value::Int(n) => Ok(n.cast_unsigned()),
        _ => Err(Error::ExpectedInt),
    };
    Ok(Xoshiro {
        s0: get(0)?,
        s1: get(1)?,
        s2: get(2)?,
        s3: get(3)?,
    })
}

pub(super) fn pack_rng(heap: &mut Heap, x: Xoshiro) -> Result<Value, Error> {
    heap.construct(
        "$tuple",
        vec![
            Value::Int(x.s0.cast_signed()),
            Value::Int(x.s1.cast_signed()),
            Value::Int(x.s2.cast_signed()),
            Value::Int(x.s3.cast_signed()),
        ],
    )
}

pub(super) fn pack_rng_pair(heap: &mut Heap, x: Xoshiro, val: Value) -> Result<Value, Error> {
    let r = match pack_rng(heap, x) {
        Ok(r) => r,
        Err(err) => {
            heap.drop_value(val)?;
            return Err(err);
        }
    };
    heap.construct("$tuple", vec![r, val])
}

pub(super) fn alloc_utf16(heap: &mut Heap, units: &[u16]) -> Result<Value, Error> {
    match String::from_utf16(units) {
        Ok(s) => heap.literal(&Lit::String(s)),
        Err(_) => Ok(heap.allocate(Data::Utf16(units.to_vec()))),
    }
}

pub(super) fn run(
    heap: &mut Heap,
    op: ArbitraryIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        ArbitraryIntrinsic::HashSeed => hash_seed_op(heap, &args[0], fuel),
        ArbitraryIntrinsic::AttemptRandom => attempt_random_op(heap, &args[0], &args[1], &args[2]),
        ArbitraryIntrinsic::XoshiroNext => {
            let mut rng = read_rng(heap, &args[0])?;
            let n = rng.next_u32();
            pack_rng_pair(heap, rng, Value::Float(f64::from(n)))
        }
        ArbitraryIntrinsic::XoshiroDouble => {
            let mut rng = read_rng(heap, &args[0])?;
            let d = rng.next_double();
            pack_rng_pair(heap, rng, Value::Float(d))
        }
        ArbitraryIntrinsic::RandomInt => {
            let mut rng = read_rng(heap, &args[0])?;
            let lo = expect_num(&args[1])?;
            let hi = expect_num(&args[2])?;
            let v = rng.random_int(lo, hi);
            pack_rng_pair(heap, rng, Value::Float(v))
        }
        ArbitraryIntrinsic::RandomNumber => draw::random_number_op(heap, args),
        ArbitraryIntrinsic::RandomBigIntBiased => draw::random_bigint_biased_op(heap, args, fuel),
        ArbitraryIntrinsic::RandomDecimal => draw::random_decimal_op(heap, &args[0]),
        ArbitraryIntrinsic::RandomString => draw::random_string_op(heap, args, fuel),
        ArbitraryIntrinsic::RandomCodePointString => draw::random_cp_string_op(heap, args, fuel),
        ArbitraryIntrinsic::ShrinkNumber => shrink_number(heap, args),
        ArbitraryIntrinsic::ShrinkBigInt => shrink_bigint(heap, &args[0], &args[1], &args[2], fuel),
        ArbitraryIntrinsic::ShrinkDecimalCands => shrink_decimal_cands(heap, args),
        ArbitraryIntrinsic::DecimalMidpoint => decimal_midpoint(heap, &args[0], &args[1], &args[2]),
        ArbitraryIntrinsic::ShrinkString => shrink_string(heap, &args[0], &args[1], fuel),
        ArbitraryIntrinsic::NumberBounds => number_bounds(heap, args),
        ArbitraryIntrinsic::ExtractConstraint => extract_constraint(heap, &args[0]),
        ArbitraryIntrinsic::MergeConstraint => merge_constraint(heap, &args[0], &args[1]),
        ArbitraryIntrinsic::FormatSpaced => format_spaced_op(heap, &args[0], fuel),
        ArbitraryIntrinsic::MakeDuration => make_duration(heap, &args[0], &args[1], fuel),
        ArbitraryIntrinsic::IsNonNegativeDuration => is_non_negative_duration(heap, &args[0]),
        ArbitraryIntrinsic::MakeZonedDateTime => make_zoned_datetime(heap, &args[0], &args[1]),
        ArbitraryIntrinsic::MakeZone => make_zone(heap, &args[0]),
        ArbitraryIntrinsic::JsString => js_string_val(heap, &args[0]),
        ArbitraryIntrinsic::QuoteJson => quote_json_op(heap, &args[0]),
    }
}

fn hash_seed_op(heap: &mut Heap, seed: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let text = match seed {
        Value::Float(f) => format!("number:{}", js_number(*f)),
        Value::Int(i) => format!("number:{i}"),
        _ => format!("string:{}", js_string_inner(heap, seed)?),
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    charge(fuel, units.len())?;
    let mut first: u32 = 0x811c_9dc5;
    let mut second: u32 = 0x9e37_79b9;
    for (i, &c) in units.iter().enumerate() {
        first = (first ^ u32::from(c)).wrapping_mul(0x0100_0193);
        let idx = u32::try_from(i).unwrap_or(0);
        second = (second ^ u32::from(c) ^ idx).wrapping_mul(0x85eb_ca6b);
    }
    let len_u32 = u32::try_from(units.len()).unwrap_or(0);
    let f = mix32(first).cast_signed();
    let s = mix32(second ^ len_u32).cast_signed();
    heap.construct("$tuple", vec![Value::Int(f), Value::Int(s)])
}

fn attempt_random_op(
    heap: &mut Heap,
    first_val: &Value,
    second_val: &Value,
    attempt_val: &Value,
) -> Result<Value, Error> {
    let (Value::Int(fi), Value::Int(si)) = (first_val, second_val) else {
        return Err(Error::ExpectedInt);
    };
    let attempt = expect_num(attempt_val)?;
    let (first, second) = ((*fi).cast_unsigned(), (*si).cast_unsigned());
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "non-negative attempt"
    )]
    let low = (attempt as u64) as u32;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "non-negative attempt"
    )]
    let high = ((attempt / UINT32_VALUES).floor() as u64) as u32;
    let mut x = Xoshiro {
        s0: mix32(first ^ low ^ high.wrapping_mul(0x9e37_79b9)),
        s1: mix32(second ^ high ^ low.wrapping_mul(0x85eb_ca6b)),
        s2: mix32(first ^ high ^ low.wrapping_mul(0xc2b2_ae35) ^ 0x243f_6a88),
        s3: mix32(second ^ low ^ high.wrapping_mul(0x27d4_eb2f) ^ 0xb7e1_5162),
    };
    if (x.s0 | x.s1 | x.s2 | x.s3) == 0 {
        x.s0 = 0x9e37_79b9;
    }
    pack_rng(heap, x)
}
