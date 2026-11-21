//! Primitive ordering and the canonical rt/Go hashes, borrowing staged roots.
use std::cmp::Ordering;

use flick_core::ir::memory::HashIntrinsic;

use super::super::{Data, Error, Heap, Value, charge, tagged_boundary};
use super::{bigint_order, normalize_bigint, unsupported};

pub(super) fn compare(
    heap: &mut Heap,
    left: &Value,
    right: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    let order = match (left, right) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => {
            if a.is_nan() || b.is_nan() {
                a.is_nan().cmp(&b.is_nan())
            } else {
                a.partial_cmp(b).expect("non-NaN floating comparison")
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            match (&heap.object(a.id)?.data, &heap.object(b.id)?.data) {
                (Data::String(a), Data::String(b)) => {
                    charge(fuel, a.len().saturating_add(b.len()))?;
                    a.encode_utf16().cmp(b.encode_utf16())
                }
                (Data::Utf16(a), Data::Utf16(b)) => {
                    charge(fuel, a.len().saturating_add(b.len()))?;
                    a.cmp(b)
                }
                (Data::String(a), Data::Utf16(b)) => {
                    charge(fuel, a.len().saturating_add(b.len()))?;
                    a.encode_utf16().cmp(b.iter().copied())
                }
                (Data::Utf16(a), Data::String(b)) => {
                    charge(fuel, a.len().saturating_add(b.len()))?;
                    a.iter().copied().cmp(b.encode_utf16())
                }
                (Data::BigInt(a), Data::BigInt(b)) => {
                    let a = normalize_bigint(a, |work| charge(fuel, work))?;
                    let b = normalize_bigint(b, |work| charge(fuel, work))?;
                    charge(fuel, a.len().saturating_add(b.len()))?;
                    bigint_order(&a, &b)
                }
                _ => return Err(unsupported("primitive comparison of non-primitive objects")),
            }
        }
        _ => return Err(unsupported("primitive comparison type mismatch")),
    };
    let name = match order {
        Ordering::Less => "LT",
        Ordering::Equal => "EQ",
        Ordering::Greater => "GT",
    };
    heap.construct(&format!("Prelude.Ordering.{name}"), vec![])
}

pub(super) fn hash(heap: &Heap, value: &Value, fuel: &mut u64) -> Result<i32, Error> {
    charge(fuel, 1)?;
    Ok(match value {
        Value::Int(value) => signed(optimize(unsigned(*value))),
        Value::Float(value) => hash_number(*value),
        Value::Bool(value) => signed(optimize(mix(hash_string(
            if *value { "true" } else { "false" },
            fuel,
        )?))),
        Value::Unit => signed(optimize(mix(hash_string("undefined", fuel)?))),
        Value::Object(token) => match &heap.object(token.id)?.data {
            Data::String(text) => signed(hash_string(text, fuel)?),
            Data::Utf16(units) => signed(hash_utf16(units, fuel)?),
            Data::BigInt(text) => {
                let decimal = normalize_bigint(text, |work| charge(fuel, work))?;
                signed(hash_string(&decimal, fuel)?)
            }
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
            } => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                if *ordinary_fields != 0
                    || !fields.is_empty()
                    || !matches!(
                        tag.as_str(),
                        "Prelude.Ordering.LT" | "Prelude.Ordering.EQ" | "Prelude.Ordering.GT"
                    )
                {
                    return Err(unsupported("primitive hash of non-primitive nominal data"));
                }
                let tag = native_tag(tag);
                let result = 0x3001 ^ combine(hash_string("_tag", fuel)?, hash_string(&tag, fuel)?);
                signed(optimize(result))
            }
            _ => {
                return Err(unsupported(
                    "primitive hash of executable/opaque or structural storage",
                ));
            }
        },
        _ => return Err(unsupported("primitive hash of a capability")),
    })
}

fn hash_string(text: &str, fuel: &mut u64) -> Result<u32, Error> {
    charge(fuel, text.len())?;
    let units: Vec<_> = text.encode_utf16().collect();
    hash_utf16(&units, fuel)
}

fn hash_utf16(units: &[u16], fuel: &mut u64) -> Result<u32, Error> {
    let mut hash = 5381_u32;
    for &unit in units.iter().rev() {
        charge(fuel, 1)?;
        hash = hash.wrapping_mul(33) ^ u32::from(unit);
    }
    Ok(optimize(hash))
}

fn hash_number(value: f64) -> i32 {
    // Decimal integer parsing recognizes exactly the signed-32-bit fast path,
    // including either zero, without a saturating float-to-integer conversion.
    if let Ok(integer) = value.to_string().parse::<i32>() {
        return signed(optimize(unsigned(integer)));
    }
    let bits = if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    };
    let bytes = bits.to_be_bytes();
    let high = u32::from_be_bytes(bytes[..4].try_into().expect("high word"));
    let low = u32::from_be_bytes(bytes[4..].try_into().expect("low word"));
    signed(optimize(combine(high, low)))
}

fn native_tag(id: &str) -> String {
    let hash = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!(
        "{}#{:012x}",
        id.rsplit('.').next().expect("constructor name"),
        hash >> 16
    )
}

fn unsigned(value: i32) -> u32 {
    u32::from_ne_bytes(value.to_ne_bytes())
}
fn signed(value: u32) -> i32 {
    i32::from_ne_bytes(value.to_ne_bytes())
}
fn optimize(value: u32) -> u32 {
    (value & 0xbfff_ffff) | ((value >> 1) & 0x4000_0000)
}
fn mix(value: u32) -> u32 {
    let mut value = value ^ (value >> 16);
    value = value.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 13;
    value = value.wrapping_mul(0xc2b2_ae35);
    value ^ (value >> 16)
}
fn combine(left: u32, right: u32) -> u32 {
    mix(left
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add(right.wrapping_mul(0x85eb_ca6b)))
}

fn hamt_walk_key(h: i32) -> u64 {
    let u = unsigned(h);
    let mut key = 0_u64;
    let mut shift = 0;
    while shift < 35 {
        key = (key << 5) | u64::from((u.checked_shr(shift).unwrap_or(0)) & 31);
        shift += 5;
    }
    key
}

pub(super) fn hash_op(
    heap: &mut Heap,
    op: HashIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        HashIntrinsic::Optimize => {
            let Value::Int(n) = args[0] else {
                return Err(Error::ExpectedInt);
            };
            Ok(Value::Int(signed(optimize(unsigned(n)))))
        }
        HashIntrinsic::Combine => {
            let (Value::Int(b), Value::Int(self_val)) = (&args[0], &args[1]) else {
                return Err(Error::ExpectedInt);
            };
            Ok(Value::Int(signed(combine(
                unsigned(*self_val),
                unsigned(*b),
            ))))
        }
        HashIntrinsic::WalkCompare => {
            let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) else {
                return Err(Error::ExpectedInt);
            };
            let tag = match hamt_walk_key(*a).cmp(&hamt_walk_key(*b)) {
                Ordering::Less => "Prelude.Ordering.LT",
                Ordering::Equal => "Prelude.Ordering.EQ",
                Ordering::Greater => "Prelude.Ordering.GT",
            };
            heap.construct(tag, vec![])
        }
    }
}
