//! Pure metered `Arbitrary` shrinking and bounds primitives (Stage 4D).

use std::cmp::Ordering;
use std::collections::HashSet;

use flick_core::ir::Lit;
use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::arbitrary_ops::{
    MAX_SAFE, alloc_utf16, expect_num, index_number, number_index, semantic_magnitudes,
};
use super::bigint_ops::parse_operand;
use super::decimal_round::{Dec, dec_compare, pow10};
use super::{Data, Error, Fuel, Heap, Value, charge};

fn opt_num(heap: &Heap, val: &Value) -> Result<Option<f64>, Error> {
    let Value::Object(tok) = val else {
        return Ok(None);
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Ok(None);
    };
    if tag == "Prelude.Option.Some" && !fields.is_empty() {
        Ok(Some(expect_num(&fields[0])?))
    } else {
        Ok(None)
    }
}

fn shrink_integer(current: f64, target: f64, asap: bool) -> Vec<(f64, Option<f64>)> {
    let mut out = Vec::new();
    let gap = current - target;
    let mut prev = target;
    let mut has = !asap;
    let mut to_remove = if asap { gap } else { (gap / 2.0).trunc() };
    while to_remove != 0.0 {
        #[expect(clippy::float_cmp, reason = "exact JS shrink equality check")]
        let v = if to_remove == gap {
            target
        } else {
            current - to_remove
        };
        out.push((v, has.then_some(prev)));
        prev = v;
        has = true;
        to_remove = (to_remove / 2.0).trunc();
    }
    out
}

fn shrink_float(current: f64, target: f64, asap: bool) -> Vec<(f64, Option<f64>)> {
    if current.is_nan() {
        return vec![(target, None)];
    }
    let at = BigInt::from(number_index(current));
    let gap = &at - BigInt::from(number_index(target));
    let mut prev = target;
    let mut has = !asap;
    let two = BigInt::from(2);
    let mut to_remove = if asap { gap.clone() } else { &gap / &two };
    let mut out = Vec::new();
    while !to_remove.is_zero() {
        let v = if to_remove == gap {
            target
        } else {
            index_number((&at - &to_remove).to_i64().unwrap_or(0))
        };
        out.push((v, has.then_some(prev)));
        prev = v;
        has = true;
        to_remove /= &two;
    }
    out
}

#[expect(clippy::float_cmp, reason = "exact JS shrink equality checks")]
pub(super) fn shrink_number(heap: &mut Heap, args: &[Value]) -> Result<Value, Error> {
    let value = expect_num(&args[0])?;
    let min_v = opt_num(heap, &args[1])?;
    let max_v = opt_num(heap, &args[2])?;
    let Value::Bool(integer) = args[3] else {
        return Err(Error::ExpectedBoolean);
    };
    let ctx = opt_num(heap, &args[4])?;

    let cands = match (integer, ctx) {
        (false, None) => {
            let mut target = 0.0;
            if let Some(lo) = min_v
                && lo > 0.0
            {
                target = lo;
            } else if let Some(hi) = max_v
                && hi < 0.0
            {
                target = hi;
            }
            shrink_float(value, target, true)
        }
        (false, Some(c)) => {
            let vi = number_index(value);
            let ci = number_index(c);
            if !value.is_nan() && (vi == ci + 1 || vi == ci - 1) {
                vec![(c, None)]
            } else {
                shrink_float(value, c, false)
            }
        }
        (true, None) => {
            let lo = min_v.unwrap_or(0.0);
            let hi = max_v.unwrap_or(0.0);
            shrink_integer(value, hi.min(lo.max(0.0)), true)
        }
        (true, Some(c)) => {
            let adj_pos = value > 0.0 && value == c + 1.0 && min_v.is_none_or(|lo| value > lo);
            let adj_neg = value < 0.0 && value == c - 1.0 && max_v.is_none_or(|hi| value < hi);
            if adj_pos || adj_neg {
                vec![(c, None)]
            } else {
                shrink_integer(value, c, false)
            }
        }
    };

    let mut out = Vec::with_capacity(cands.len());
    for (v, next_ctx) in cands {
        let opt = match next_ctx {
            Some(c) => heap.construct("Prelude.Option.Some", vec![Value::Float(c)])?,
            None => heap.construct("Prelude.Option.None", vec![])?,
        };
        match heap.construct("$tuple", vec![Value::Float(v), opt]) {
            Ok(pair) => out.push(pair),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

pub(super) fn shrink_bigint(
    heap: &mut Heap,
    val_v: &Value,
    target_v: &Value,
    ctx_v: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let value = parse_operand(heap, val_v, fuel)?;
    let target = parse_operand(heap, target_v, fuel)?;
    let ctx = match ctx_v {
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::Tagged { tag, fields, .. }
                if tag == "Prelude.Option.Some" && !fields.is_empty() =>
            {
                let inner = fields[0].representation();
                Some(parse_operand(heap, &inner, fuel)?)
            }
            _ => None,
        },
        _ => None,
    };
    let from = ctx.as_ref().unwrap_or(&target);
    let gap = &value - from;
    if gap.is_zero() {
        return heap.construct("$list", vec![]);
    }
    let one = BigInt::one();
    if let Some(c) = &ctx
        && gap.abs() == one
    {
        let bi = heap.literal(&Lit::BigInt(c.to_str_radix(10)))?;
        return heap.construct("$list", vec![bi]);
    }
    let dir = BigInt::from(if gap.is_positive() { 1 } else { -1 });
    let mut seen: HashSet<String> = HashSet::new();
    let mut cands: Vec<BigInt> = Vec::new();
    let put = |seen: &mut HashSet<String>, cands: &mut Vec<BigInt>, c: BigInt| {
        let k = c.to_str_radix(10);
        if seen.insert(k) {
            cands.push(c);
        }
    };
    let add = |seen: &mut HashSet<String>, cands: &mut Vec<BigInt>, c: BigInt| {
        if ((&c - from) * &dir).is_positive() && ((&value - &c) * &dir).is_positive() {
            put(seen, cands, c);
        }
    };
    let ones = |bits: usize| (BigInt::one() << bits) - BigInt::one();
    if let Some(c) = &ctx {
        let passing = usize::try_from(c.bits()).unwrap_or(0);
        let failing = usize::try_from(value.bits()).unwrap_or(0);
        let diff = failing.abs_diff(passing);
        if diff > 1 && (c.is_zero() || c.is_positive() == value.is_positive()) {
            add(
                &mut seen,
                &mut cands,
                &dir * ones(usize::midpoint(passing, failing)),
            );
        }
    } else {
        put(&mut seen, &mut cands, target.clone());
        for m in semantic_magnitudes() {
            add(&mut seen, &mut cands, &target + &dir * m);
        }
        let gap_bits = usize::try_from(gap.bits()).unwrap_or(0);
        let mut bits = 1usize;
        while bits < gap_bits {
            add(&mut seen, &mut cands, &target + &dir * ones(bits));
            bits *= 2;
        }
    }
    let two = BigInt::from(2);
    let mut to_remove = &gap / &two;
    let mut step = 0;
    while !to_remove.is_zero() && step < 256 {
        add(&mut seen, &mut cands, &value - &to_remove);
        to_remove /= &two;
        step += 1;
    }
    put(&mut seen, &mut cands, &value - &dir);
    let dir_neg = if dir.is_positive() {
        Ordering::Less
    } else {
        Ordering::Greater
    };
    cands.sort_by(|a, b| {
        if a.cmp(b) == dir_neg {
            Ordering::Less
        } else if a == b {
            Ordering::Equal
        } else {
            Ordering::Greater
        }
    });
    charge(fuel, cands.len())?;
    let mut out = Vec::with_capacity(cands.len());
    for c in cands {
        match heap.literal(&Lit::BigInt(c.to_str_radix(10))) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn read_dec(heap: &Heap, val: &Value) -> Result<Dec, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    let Data::BigDecimal { value, scale } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("BigDecimal"));
    };
    Ok(Dec {
        value: value.clone(),
        scale: *scale,
    })
}

fn project_exact(d: &Dec, scale: i64) -> Option<BigInt> {
    let diff = scale - d.scale;
    if !(0..=4096).contains(&diff) {
        return None;
    }
    Some(&d.value * pow10(diff))
}

fn cmp_sign(a: &Dec, b: &Dec) -> Result<i8, Error> {
    Ok(match dec_compare(a, b)? {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    })
}

pub(super) fn shrink_decimal_cands(heap: &mut Heap, args: &[Value]) -> Result<Value, Error> {
    const EXPS: [i64; 21] = [
        -512, -256, -128, -64, -32, -16, -8, -4, -2, -1, 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512,
    ];
    let value = read_dec(heap, &args[0])?;
    let target = read_dec(heap, &args[1])?;
    let ctx = match &args[2] {
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::Tagged { tag, fields, .. }
                if tag == "Prelude.Option.Some" && !fields.is_empty() =>
            {
                let inner = fields[0].representation();
                let Value::Object(ptok) = inner else {
                    return Err(Error::PatternKind("tuple"));
                };
                let Data::Tagged { fields: pf, .. } = &heap.object(ptok.id)?.data else {
                    return Err(Error::PatternKind("tuple"));
                };
                let pd = read_dec(heap, &pf[0])?;
                #[expect(clippy::cast_possible_truncation, reason = "refinement int")]
                let rf = expect_num(&pf[1])? as i64;
                Some((pd, rf))
            }
            _ => None,
        },
        _ => None,
    };
    #[expect(clippy::cast_possible_truncation, reason = "bounded max scale")]
    let max_scale = expect_num(&args[3])? as i64;
    let from = ctx.as_ref().map_or(&target, |(p, _)| p);
    let dir = cmp_sign(&value, from)?;
    if dir == 0 {
        return heap.construct("$list", vec![]);
    }
    let mut raw = Vec::new();
    if let Some((passing, refinement)) = &ctx {
        let norm = passing.normalize();
        let scale = max_scale.min(norm.scale + (refinement + 1).min(128));
        if let Some(p) = project_exact(&norm, scale) {
            raw.push(Dec {
                value: p + BigInt::from(dir),
                scale,
            });
        }
    } else {
        for e in EXPS {
            raw.push(Dec {
                value: BigInt::from(dir),
                scale: -e,
            });
        }
    }
    let mut cands = Vec::new();
    if ctx.is_none() {
        cands.push(target.clone());
    }
    for c in raw {
        if c.scale <= max_scale && cmp_sign(&c, from)? == dir && cmp_sign(&c, &value)? == -dir {
            cands.push(c);
        }
    }
    let vals: Vec<Value> = cands
        .into_iter()
        .map(|d| {
            heap.allocate(Data::BigDecimal {
                value: d.value,
                scale: d.scale,
            })
        })
        .collect();
    heap.construct("$list", vals)
}

pub(super) fn decimal_midpoint(
    heap: &mut Heap,
    passing_v: &Value,
    failing_v: &Value,
    max_scale_v: &Value,
) -> Result<Value, Error> {
    let passing = read_dec(heap, passing_v)?;
    let failing = read_dec(heap, failing_v)?;
    #[expect(clippy::cast_possible_truncation, reason = "bounded max scale")]
    let max_scale = expect_num(max_scale_v)? as i64;
    let scale = max_scale.min(passing.scale.max(failing.scale) + 1);
    let (Some(left), Some(right)) = (
        project_exact(&passing, scale),
        project_exact(&failing, scale),
    ) else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let v = &left + (&right - &left) / BigInt::from(2);
    if v == left || v == right {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let dec = heap.allocate(Data::BigDecimal { value: v, scale });
    heap.construct("Prelude.Option.Some", vec![dec])
}

pub(super) fn shrink_string(
    heap: &mut Heap,
    str_val: &Value,
    min_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let units = heap.utf16_units(str_val)?;
    let min_len = match *min_val {
        Value::Int(i) => usize::try_from(i.max(0)).unwrap_or(0),
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bounded len"
        )]
        Value::Float(f) => f.max(0.0) as usize,
        _ => return Err(Error::ExpectedInt),
    };
    charge(fuel, units.len() + 1)?;
    let mut cands: Vec<Vec<u16>> = Vec::new();
    let n = units.len();
    if n > min_len {
        cands.push(units[..min_len].to_vec());
        cands.push(units[..min_len.max(n / 2)].to_vec());
        cands.push(units[..n - 1].to_vec());
    }
    for (i, &u) in units.iter().enumerate() {
        for (cv, _) in shrink_integer(f64::from(u), 0.0, true) {
            let mut next = units.clone();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "u16 code unit"
            )]
            {
                next[i] = cv as u16;
            }
            cands.push(next);
        }
    }
    let mut seen: HashSet<Vec<u16>> = HashSet::new();
    seen.insert(units);
    let mut out = Vec::new();
    for c in cands {
        if seen.insert(c.clone()) {
            match alloc_utf16(heap, &c) {
                Ok(v) => out.push(v),
                Err(err) => {
                    heap.drop_values(out)?;
                    return Err(err);
                }
            }
        }
    }
    heap.construct("$list", out)
}

fn next_number(v: f64) -> f64 {
    if v.is_nan() || (v.is_infinite() && v.is_sign_positive()) {
        v
    } else if v == 0.0 {
        f64::from_bits(1)
    } else {
        index_number(number_index(v) + 1)
    }
}

fn previous_number(v: f64) -> f64 {
    if v.is_nan() || (v.is_infinite() && v.is_sign_negative()) {
        v
    } else if v == 0.0 {
        -f64::from_bits(1)
    } else {
        index_number(number_index(v) - 1)
    }
}

pub(super) fn number_bounds(heap: &mut Heap, args: &[Value]) -> Result<Value, Error> {
    let mut min_v = opt_num(heap, &args[0])?;
    let mut max_v = opt_num(heap, &args[1])?;
    let (&Value::Bool(ex_min), &Value::Bool(ex_max)) = (&args[2], &args[3]) else {
        return Err(Error::ExpectedBoolean);
    };
    let mode = heap.string_lossy(&args[4])?;
    let integer = mode == "integer";
    let finite = mode == "finite";
    let what = if integer {
        "integer constraints"
    } else {
        "number constraints"
    };
    let fail = |h: &mut Heap| -> Result<Value, Error> {
        let s = h.literal(&Lit::String(what.to_owned()))?;
        h.construct("Prelude.Result.Err", vec![s])
    };
    if min_v.is_some_and(f64::is_nan) || max_v.is_some_and(f64::is_nan) {
        return fail(heap);
    }
    if integer {
        if let Some(lo) = &mut min_v {
            *lo = if ex_min { lo.floor() + 1.0 } else { lo.ceil() };
        }
        if let Some(hi) = &mut max_v {
            *hi = if ex_max { hi.ceil() - 1.0 } else { hi.floor() };
        }
    } else {
        if min_v.is_some_and(|lo| ex_min && lo == f64::INFINITY)
            || max_v.is_some_and(|hi| ex_max && hi == f64::NEG_INFINITY)
        {
            return fail(heap);
        }
        if let Some(lo) = &mut min_v {
            if ex_min {
                *lo = next_number(*lo);
            } else if *lo == 0.0 {
                *lo = (-0.0f64).copysign(-1.0);
            }
        }
        if let Some(hi) = &mut max_v {
            if ex_max {
                *hi = previous_number(*hi);
            } else if *hi == 0.0 {
                *hi = 0.0;
            }
        }
    }
    if integer || finite {
        if min_v.is_some_and(|lo| lo == f64::INFINITY)
            || max_v.is_some_and(|hi| hi == f64::NEG_INFINITY)
        {
            return fail(heap);
        }
        if let Some(lo) = &mut min_v
            && *lo == f64::NEG_INFINITY
        {
            *lo = if integer { -MAX_SAFE } else { -f64::MAX };
        }
        if let Some(hi) = &mut max_v
            && *hi == f64::INFINITY
        {
            *hi = if integer { MAX_SAFE } else { f64::MAX };
        }
    }
    if integer {
        if min_v.is_some_and(|lo| lo > MAX_SAFE) || max_v.is_some_and(|hi| hi < -MAX_SAFE) {
            return fail(heap);
        }
        if let Some(lo) = &mut min_v {
            *lo = lo.max(-MAX_SAFE);
        }
        if let Some(hi) = &mut max_v {
            *hi = hi.min(MAX_SAFE);
        }
    }
    if let (Some(lo), Some(hi)) = (min_v, max_v)
        && ((integer && lo > hi) || (!integer && number_index(lo) > number_index(hi)))
    {
        return fail(heap);
    }
    let min_opt = match min_v {
        Some(v) => heap.construct("Prelude.Option.Some", vec![Value::Float(v)])?,
        None => heap.construct("Prelude.Option.None", vec![])?,
    };
    let max_opt = match max_v {
        Some(v) => heap.construct("Prelude.Option.Some", vec![Value::Float(v)])?,
        None => heap.construct("Prelude.Option.None", vec![])?,
    };
    let pair = heap.construct("$tuple", vec![min_opt, max_opt])?;
    heap.construct("Prelude.Result.Ok", vec![pair])
}
