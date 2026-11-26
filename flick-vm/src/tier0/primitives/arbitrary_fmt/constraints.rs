//! `Arbitrary` constraint extraction and merging primitives (Stage 4D).

use std::cmp::Ordering;

use flick_core::ir::Lit;
use num_bigint::BigInt;

use super::super::arbitrary_ops::expect_num;
use super::super::bigint_ops::parse_operand;
use super::super::check_ops::{pair_fields, rec_field};
use super::super::list_ops::slice_reps;
use super::super::{Data, Error, Heap, Value};

// Constraint representation: Tagged("Arbitrary.$Constraint.$Constraint", 11 fields):
// [0] order: String ("number" | "bigint" | "")
// [1] minimum: Option (Float or BigInt)
// [2] maximum: Option (Float or BigInt)
// [3] exMin: Bool
// [4] exMax: Bool
// [5] minLen: Option Float
// [6] maxLen: Option Float
// [7] minCp: Option Float
// [8] maxCp: Option Float
// [9] patterns: List String
// [10] number: String ("integer" | "finite" | "")

#[derive(Default)]
struct RawConstraint {
    order: String,
    minimum: Option<Value>,
    maximum: Option<Value>,
    ex_min: bool,
    ex_max: bool,
    min_len: Option<f64>,
    max_len: Option<f64>,
    min_cp: Option<f64>,
    max_cp: Option<f64>,
    patterns: Vec<String>,
    number: String,
}

fn opt_val(heap: &mut Heap, v: Option<Value>) -> Result<Value, Error> {
    match v {
        Some(x) => heap.construct("Prelude.Option.Some", vec![x]),
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn opt_float(heap: &mut Heap, v: Option<f64>) -> Result<Value, Error> {
    match v {
        Some(f) => heap.construct("Prelude.Option.Some", vec![Value::Float(f)]),
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn pack_constraint(heap: &mut Heap, c: RawConstraint) -> Result<Value, Error> {
    let order = heap.literal(&Lit::String(c.order))?;
    let min_v = opt_val(heap, c.minimum)?;
    let max_v = opt_val(heap, c.maximum)?;
    let min_len = opt_float(heap, c.min_len)?;
    let max_len = opt_float(heap, c.max_len)?;
    let min_cp = opt_float(heap, c.min_cp)?;
    let max_cp = opt_float(heap, c.max_cp)?;
    let mut pats = Vec::with_capacity(c.patterns.len());
    for p in c.patterns {
        pats.push(heap.literal(&Lit::String(p))?);
    }
    let plist = heap.construct("$list", pats)?;
    let number = heap.literal(&Lit::String(c.number))?;
    heap.construct(
        "Arbitrary.$Constraint.$Constraint",
        vec![
            order,
            min_v,
            max_v,
            Value::Bool(c.ex_min),
            Value::Bool(c.ex_max),
            min_len,
            max_len,
            min_cp,
            max_cp,
            plist,
            number,
        ],
    )
}

fn unwrap_opt(heap: &Heap, val: &Value) -> Result<Option<Value>, Error> {
    let Value::Object(tok) = val else {
        return Ok(None);
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Ok(None);
    };
    if tag == "Prelude.Option.Some" && !fields.is_empty() {
        Ok(Some(fields[0].representation()))
    } else {
        Ok(None)
    }
}

fn read_opt_f(heap: &Heap, val: &Value) -> Result<Option<f64>, Error> {
    match unwrap_opt(heap, val)? {
        Some(v) => Ok(Some(expect_num(&v)?)),
        None => Ok(None),
    }
}

fn read_constraint(heap: &Heap, val: &Value) -> Result<RawConstraint, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Constraint"));
    };
    let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Constraint"));
    };
    let mut patterns = Vec::new();
    for p in &slice_reps(heap, &fields[9])? {
        patterns.push(heap.string_lossy(p)?);
    }
    Ok(RawConstraint {
        order: heap.string_lossy(&fields[0])?,
        minimum: unwrap_opt(heap, &fields[1])?,
        maximum: unwrap_opt(heap, &fields[2])?,
        ex_min: matches!(fields[3], Value::Bool(true)),
        ex_max: matches!(fields[4], Value::Bool(true)),
        min_len: read_opt_f(heap, &fields[5])?,
        max_len: read_opt_f(heap, &fields[6])?,
        min_cp: read_opt_f(heap, &fields[7])?,
        max_cp: read_opt_f(heap, &fields[8])?,
        patterns,
        number: heap.string_lossy(&fields[10])?,
    })
}

pub(in super::super) fn extract_constraint(
    heap: &mut Heap,
    spec_opt: &Value,
) -> Result<Value, Error> {
    let Some(spec) = unwrap_opt(heap, spec_opt)? else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let Value::Object(tok) = spec else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let (kind, param) = {
        let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
            return heap.construct("Prelude.Option.None", vec![]);
        };
        (heap.string_lossy(&fields[0])?, fields[1].representation())
    };
    let mut c = RawConstraint::default();
    match kind.as_str() {
        "int" => c.number = "integer".into(),
        "finite" => c.number = "finite".into(),
        "gt" | "gte" | "lt" | "lte" => {
            c.order = "number".into();
            let b = expect_num(&param)?;
            match kind.as_str() {
                "gt" => {
                    c.minimum = Some(Value::Float(b));
                    c.ex_min = true;
                }
                "gte" => c.minimum = Some(Value::Float(b)),
                "lt" => {
                    c.maximum = Some(Value::Float(b));
                    c.ex_max = true;
                }
                _ => c.maximum = Some(Value::Float(b)),
            }
        }
        "between" => {
            c.order = "number".into();
            c.minimum = Some(Value::Float(expect_num(&rec_field(
                heap, &param, "minimum",
            )?)?));
            c.maximum = Some(Value::Float(expect_num(&rec_field(
                heap, &param, "maximum",
            )?)?));
            c.ex_min = matches!(
                rec_field(heap, &param, "exclusiveMinimum")?,
                Value::Bool(true)
            );
            c.ex_max = matches!(
                rec_field(heap, &param, "exclusiveMaximum")?,
                Value::Bool(true)
            );
        }
        "gt_bigint" | "gte_bigint" | "lt_bigint" | "lte_bigint" => {
            c.order = "bigint".into();
            let dup = heap.dup(&param)?;
            match kind.as_str() {
                "gt_bigint" => {
                    c.minimum = Some(dup);
                    c.ex_min = true;
                }
                "gte_bigint" => c.minimum = Some(dup),
                "lt_bigint" => {
                    c.maximum = Some(dup);
                    c.ex_max = true;
                }
                _ => c.maximum = Some(dup),
            }
        }
        "between_bigint" => {
            c.order = "bigint".into();
            let lo = rec_field(heap, &param, "minimum")?;
            let hi = rec_field(heap, &param, "maximum")?;
            c.minimum = Some(heap.dup(&lo)?);
            c.maximum = Some(heap.dup(&hi)?);
            c.ex_min = matches!(
                rec_field(heap, &param, "exclusiveMinimum")?,
                Value::Bool(true)
            );
            c.ex_max = matches!(
                rec_field(heap, &param, "exclusiveMaximum")?,
                Value::Bool(true)
            );
        }
        "min_length" => c.min_len = Some(expect_num(&param)?),
        "max_length" => c.max_len = Some(expect_num(&param)?),
        "length_between" => {
            let (a, b) = pair_fields(heap, &param)?;
            c.min_len = Some(expect_num(&a)?);
            c.max_len = Some(expect_num(&b)?);
        }
        "min_code_points" => c.min_cp = Some(expect_num(&param)?),
        "max_code_points" => c.max_cp = Some(expect_num(&param)?),
        "code_points_between" => {
            let (a, b) = pair_fields(heap, &param)?;
            c.min_cp = Some(expect_num(&a)?);
            c.max_cp = Some(expect_num(&b)?);
        }
        _ => return heap.construct("Prelude.Option.None", vec![]),
    }
    let packed = pack_constraint(heap, c)?;
    heap.construct("Prelude.Option.Some", vec![packed])
}

fn order_cmp(heap: &Heap, order: &str, a: &Value, b: &Value) -> Result<i8, Error> {
    if order == "bigint" {
        let mut dummy = u64::MAX;
        let ba: BigInt = parse_operand(heap, a, &mut dummy)?;
        let bb: BigInt = parse_operand(heap, b, &mut dummy)?;
        Ok(match ba.cmp(&bb) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        })
    } else {
        let x = expect_num(a)?;
        let y = expect_num(b)?;
        Ok(if x < y { -1 } else { i8::from(x > y) })
    }
}

fn merge_bound(
    heap: &mut Heap,
    order: &str,
    a: Option<&Value>,
    a_ex: bool,
    b: Option<&Value>,
    b_ex: bool,
    take: i8,
) -> Result<(Option<Value>, bool), Error> {
    match (a, b) {
        (None, None) => Ok((None, a_ex || b_ex)),
        (Some(av), None) => Ok((Some(heap.dup(av)?), a_ex)),
        (None, Some(bv)) => Ok((Some(heap.dup(bv)?), b_ex)),
        (Some(av), Some(bv)) => match order_cmp(heap, order, av, bv)? {
            o if o == take => Ok((Some(heap.dup(bv)?), b_ex)),
            0 => Ok((Some(heap.dup(av)?), a_ex || b_ex)),
            _ => Ok((Some(heap.dup(av)?), a_ex)),
        },
    }
}

pub(in super::super) fn merge_constraint(
    heap: &mut Heap,
    self_opt: &Value,
    that_val: &Value,
) -> Result<Value, Error> {
    let a = match unwrap_opt(heap, self_opt)? {
        Some(v) => read_constraint(heap, &v)?,
        None => RawConstraint::default(),
    };
    let b = read_constraint(heap, that_val)?;
    if !a.order.is_empty() && !b.order.is_empty() && a.order != b.order {
        return Err(Error::RangeError(
            "Error: Cannot merge ordered arbitrary constraints with different Order instances"
                .into(),
        ));
    }
    let order = if b.order.is_empty() {
        a.order.clone()
    } else {
        b.order.clone()
    };
    let (minimum, ex_min) = if order.is_empty() {
        let chosen = b.minimum.as_ref().or(a.minimum.as_ref());
        let dup = match chosen {
            Some(v) => Some(heap.dup(v)?),
            None => None,
        };
        (dup, a.ex_min || b.ex_min)
    } else {
        merge_bound(
            heap,
            &order,
            a.minimum.as_ref(),
            a.ex_min,
            b.minimum.as_ref(),
            b.ex_min,
            -1,
        )?
    };
    let (maximum, ex_max) = if order.is_empty() {
        let chosen = b.maximum.as_ref().or(a.maximum.as_ref());
        let dup = match chosen {
            Some(v) => Some(heap.dup(v)?),
            None => None,
        };
        (dup, a.ex_max || b.ex_max)
    } else {
        merge_bound(
            heap,
            &order,
            a.maximum.as_ref(),
            a.ex_max,
            b.maximum.as_ref(),
            b.ex_max,
            1,
        )?
    };
    let min_len = match (a.min_len, b.min_len) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    };
    let max_len = match (a.max_len, b.max_len) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    let min_cp = match (a.min_cp, b.min_cp) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    };
    let max_cp = match (a.max_cp, b.max_cp) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    let mut patterns = a.patterns;
    patterns.extend(b.patterns);
    let number = if a.number == "integer" || b.number == "integer" {
        "integer".into()
    } else if a.number == "finite" || b.number == "finite" {
        "finite".into()
    } else {
        String::new()
    };
    pack_constraint(
        heap,
        RawConstraint {
            order,
            minimum,
            maximum,
            ex_min,
            ex_max,
            min_len,
            max_len,
            min_cp,
            max_cp,
            patterns,
            number,
        },
    )
}
