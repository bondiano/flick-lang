//! Value operations borrow their inputs; the machine owns and releases them.
use std::cmp::Ordering;
use std::collections::BTreeSet;

use flick_core::ir::memory::Intrinsic;
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num, int_arith};
use flick_core::opt::{js_number, json_string};

use super::{Data, Error, Heap, Value, tagged_boundary};

#[path = "numeric.rs"]
mod numeric;

#[path = "scalar.rs"]
mod scalar;

#[path = "bigint.rs"]
mod bigint;

#[path = "bigint_conversion.rs"]
mod bigint_conversion;

#[path = "host.rs"]
mod host;

pub(super) fn metered_primitive(
    heap: &mut Heap,
    kind: Intrinsic,
    arguments: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    heap.admit_values(arguments)?;
    match kind {
        Intrinsic::ToBigInt => bigint_conversion::run(heap, &arguments[0], fuel),
        Intrinsic::ArrayToList => array_to_list(heap, &arguments[0], fuel),
        Intrinsic::PrimitiveCompare => numeric::compare(heap, &arguments[0], &arguments[1], fuel),
        Intrinsic::PrimitiveHash => Ok(Value::Int(numeric::hash(heap, &arguments[0], fuel)?)),
        Intrinsic::NumberToFloat | Intrinsic::FloatMin | Intrinsic::FloatMax => {
            scalar::run(heap, kind, arguments, fuel)
        }
        Intrinsic::IntUnsafeDiv | Intrinsic::IntUnsafeRem => {
            unsafe_int(kind, &arguments[0], &arguments[1])
        }
        Intrinsic::StringLength => {
            let text = heap.string(&arguments[0])?;
            super::charge(fuel, text.len())?;
            Ok(Value::Int(
                text.encode_utf16().fold(0_i32, |n, _| n.wrapping_add(1)),
            ))
        }
        _ => Err(unsupported("not a numeric primitive")),
    }
}

/// Both native backends represent readonly tuples and lists as arrays. Keep
/// ordinary pattern kinds strict; bridge only this audited observer boundary.
pub(super) fn array_to_list(
    heap: &mut Heap,
    value: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    heap.admit_ordinary(value)?;
    let Value::Object(token) = value else {
        return Err(unsupported("array observer requires a list or tuple"));
    };
    let (list, count) = match &heap.object(token.id)?.data {
        Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } if tag == "$list" || tag == "$tuple" => {
            tagged_boundary(tag, *ordinary_fields, fields.len())?;
            (tag == "$list", fields.len())
        }
        _ => return Err(unsupported("array observer requires a list or tuple")),
    };
    if list {
        return heap.dup(value);
    }
    // Retaining and validating the new fields each visit the tuple's slots.
    // Pay before either pass; cleanup of accepted tokens never needs fuel.
    super::charge(fuel, count.saturating_mul(2))?;
    let mut fields = Vec::with_capacity(count);
    for index in 0..count {
        match heap.project(value, index) {
            Ok(field) => fields.push(field),
            Err(error) => {
                heap.drop_values(fields)?;
                return Err(error);
            }
        }
    }
    heap.construct("$list", fields)
}

#[cfg(test)]
mod array_observer_tests {
    use super::*;

    #[test]
    fn invalid_array_layout_rejects_before_charging_or_reading_fields() {
        for tag in ["$list", "$tuple"] {
            for ordinary_fields in [0, 2] {
                let mut heap = Heap::new();
                let field = heap
                    .literal(&Lit::String("owned physical field".into()))
                    .expect("field");
                // Private injection only: public construction rejects these layouts.
                let input = heap.allocate(Data::Tagged {
                    tag: tag.into(),
                    ordinary_fields,
                    fields: vec![field],
                });
                let expected = if ordinary_fields == 0 {
                    Error::ReservedConstructorEvidence(tag.into())
                } else {
                    Error::InvalidTaggedBoundary {
                        ordinary: 2,
                        physical: 1,
                    }
                };
                let mut fuel = 0;
                assert_eq!(array_to_list(&mut heap, &input, &mut fuel), Err(expected));
                assert_eq!(fuel, 0);
                heap.drop_value(input)
                    .expect("destruction releases all physical fields");
                let stats = heap.finish().expect("no hidden evidence leaks");
                assert_eq!(stats.allocations, stats.frees);
            }
        }
    }

    #[test]
    fn failed_projection_releases_all_previously_retained_fields() {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let first = heap.dup(&outside).expect("valid first field");
        let dead = heap.construct("Dead", vec![]).expect("old object");
        let dead_id = dead.object_id().expect("old identity");
        let stale = dead.representation();
        heap.drop_value(dead).expect("retire old object");
        // A private corrupted heap exercises cleanup after the first successful dup.
        let input = heap.allocate(Data::Tagged {
            tag: "$tuple".into(),
            ordinary_fields: 2,
            fields: vec![first, stale],
        });
        assert_eq!(
            array_to_list(&mut heap, &input, &mut 100),
            Err(Error::InvalidObject(dead_id))
        );
        assert_eq!(heap.reference_count(&outside), Ok(2));
        let Data::Tagged { fields, .. } = &mut heap
            .object_mut(input.object_id().expect("input identity"))
            .expect("input object")
            .data
        else {
            panic!("tuple layout")
        };
        // Remove the intentionally unowned stale representation before destruction.
        fields[1] = Value::Unit;
        heap.drop_value(input).expect("valid tuple fields retire");
        assert_eq!(heap.string(&outside), Ok("outside"));
        assert_eq!(heap.reference_count(&outside), Ok(1));
        heap.drop_value(outside).expect("external release");
        let stats = heap.finish().expect("no retained projection tokens");
        assert_eq!(stats.allocations, stats.frees);
    }
}

/// Operands stay rooted by the caller until either result delivery or unwind.
fn unsafe_int(kind: Intrinsic, left: &Value, right: &Value) -> Result<Value, Error> {
    let (Value::Int(left), Value::Int(right)) = (left, right) else {
        return Err(unsupported("Int division/remainder type mismatch"));
    };
    let remainder = kind == Intrinsic::IntUnsafeRem;
    if *right == 0 {
        return Err(Error::IntDivisionByZero { remainder });
    }
    let op = if remainder { Arith::Rem } else { Arith::Div };
    Ok(Value::Int(int_arith(op, *left, *right)))
}

fn unsupported(what: impl Into<String>) -> Error {
    Error::UnsupportedPrimitive(what.into())
}

pub(super) fn not(value: &Value) -> Result<Value, Error> {
    match value {
        Value::Bool(value) => Ok(Value::Bool(!value)),
        _ => Err(Error::ExpectedBoolean),
    }
}

pub(super) fn neg(heap: &mut Heap, num: Num, value: &Value) -> Result<Value, Error> {
    heap.admit_ordinary(value)?;
    match (num, value) {
        (Num::Int, Value::Int(value)) => Ok(Value::Int(value.wrapping_neg())),
        (Num::Float, Value::Float(value)) => Ok(Value::Float(-value)),
        (Num::BigInt, _) => {
            let text = bigint_text(heap.bigint(value)?)?;
            let negative = if let Some(positive) = text.strip_prefix('-') {
                positive.to_owned()
            } else if text == "0" {
                text
            } else {
                format!("-{text}")
            };
            heap.literal(&Lit::BigInt(negative))
        }
        _ => Err(unsupported("numeric negation type mismatch")),
    }
}

/// Inputs remain rooted in the machine until success or error cleanup.
pub(super) fn string_predicate(
    heap: &Heap,
    kind: Intrinsic,
    needle: &Value,
    subject: &Value,
) -> Result<Value, Error> {
    heap.admit_ordinary(needle)?;
    heap.admit_ordinary(subject)?;
    let needle = heap.string(needle)?;
    let subject = heap.string(subject)?;
    // For representable (Unicode scalar) strings, exact UTF-8 substring/prefix
    // matches are equivalent to UTF-16 code-unit matches: a scalar's encoding
    // cannot match across scalar boundaries. No normalization is performed.
    // The heap cannot yet represent isolated UTF-16 surrogates; this does not
    // extend support to operations such as JS slice that can produce them.
    let matched = match kind {
        Intrinsic::StringStartsWith => subject.starts_with(needle),
        Intrinsic::StringIncludes => subject.contains(needle),
        _ => return Err(unsupported("not a string predicate")),
    };
    Ok(Value::Bool(matched))
}

pub(super) fn intrinsic(
    heap: &mut Heap,
    kind: Intrinsic,
    value: &Value,
    stdout: &mut String,
) -> Result<Value, Error> {
    heap.admit_ordinary(value)?;
    match kind {
        Intrinsic::PrintLine => {
            stdout.push_str(heap.string(value)?);
            stdout.push('\n');
            Ok(Value::Unit)
        }
        Intrinsic::Show => {
            let text = match value {
                Value::Unit => "undefined".into(),
                Value::Bool(value) => value.to_string(),
                Value::Int(value) => value.to_string(),
                Value::Float(value) => js_number(*value),
                Value::Object(token) => match &heap.object(token.id)?.data {
                    Data::String(text) => json_string(text),
                    Data::BigInt(text) => format!("{}n", bigint_text(text)?),
                    Data::Tagged {
                        tag,
                        ordinary_fields,
                        fields,
                    } => {
                        tagged_boundary(tag, *ordinary_fields, fields.len())?;
                        if tag.starts_with('$') {
                            return Err(unsupported("primitive show of collection"));
                        }
                        tag.rsplit('.').next().unwrap_or(tag).to_owned()
                    }
                    Data::RecoveryResult { .. }
                    | Data::Cause(_)
                    | Data::Defect(_)
                    | Data::ServiceTag { .. }
                    | Data::Layer(_)
                    | Data::Record(_)
                    | Data::Dictionary(_)
                    | Data::Closure { .. }
                    | Data::Task { .. }
                    | Data::Cell { .. }
                    | Data::Arena { .. } => {
                        return Err(unsupported(
                            "primitive show of record or executable/opaque storage",
                        ));
                    }
                },
                Value::Cell(_) => return Err(unsupported("primitive show of cell")),
                Value::Owner(_) | Value::Arena(_) | Value::ArenaView(_) => {
                    return Err(unsupported("primitive show of arena capability"));
                }
            };
            heap.literal(&Lit::String(text))
        }
        Intrinsic::Host(primitive) => host::run(heap, primitive, value, stdout),
        Intrinsic::Try
        | Intrinsic::FromResult
        | Intrinsic::Raise
        | Intrinsic::MatchFail
        | Intrinsic::RefMake
        | Intrinsic::RefGet
        | Intrinsic::RefSet
        | Intrinsic::RefModify
        | Intrinsic::RefSame
        | Intrinsic::ScopeRun
        | Intrinsic::ScopeCurrent
        | Intrinsic::ArenaMake
        | Intrinsic::ArenaAlloc
        | Intrinsic::ArenaGet
        | Intrinsic::LayerSucceed
        | Intrinsic::LayerRun
        | Intrinsic::LayerMerge
        | Intrinsic::StringStartsWith
        | Intrinsic::StringIncludes
        | Intrinsic::PrimitiveCompare
        | Intrinsic::PrimitiveHash
        | Intrinsic::IntUnsafeDiv
        | Intrinsic::IntUnsafeRem
        | Intrinsic::NumberToFloat
        | Intrinsic::FloatMin
        | Intrinsic::FloatMax
        | Intrinsic::ArrayToList
        | Intrinsic::ToBigInt
        | Intrinsic::StringLength => Err(Error::NeedsProgram),
    }
}
include!("primitives/operators.rs");

/// Converts a checked radix literal without imposing a machine-word bound.
fn bigint_text(text: &str) -> Result<String, Error> {
    normalize_bigint(text, |_| Ok(()))
}

/// Pattern literals use numerical equality, with every normalization step
/// charged before it runs (including the quadratic radix conversion).
pub(super) fn bigint_pattern_eq(left: &str, right: &str, fuel: &mut u64) -> Result<bool, Error> {
    let left = normalize_bigint(left, |work| super::charge(fuel, work))?;
    let right = normalize_bigint(right, |work| super::charge(fuel, work))?;
    super::charge(fuel, left.len().saturating_add(right.len()))?;
    Ok(left == right)
}

fn normalize_bigint(
    text: &str,
    mut charge: impl FnMut(usize) -> Result<(), Error>,
) -> Result<String, Error> {
    charge(text.len())?;
    let (negative, unsigned) = text
        .strip_prefix('-')
        .map_or((false, text), |value| (true, value));
    let (base, digits) = if let Some(digits) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        (16, digits)
    } else if let Some(digits) = unsigned
        .strip_prefix("0o")
        .or_else(|| unsigned.strip_prefix("0O"))
    {
        (8, digits)
    } else if let Some(digits) = unsigned
        .strip_prefix("0b")
        .or_else(|| unsigned.strip_prefix("0B"))
    {
        (2, digits)
    } else {
        (10, unsigned)
    };
    if digits.is_empty() {
        return Err(Error::InvalidBigInt(text.into()));
    }
    let mut decimal = vec![0_u32];
    for digit in digits.chars() {
        charge(1)?;
        let digit = digit
            .to_digit(base)
            .ok_or_else(|| Error::InvalidBigInt(text.into()))?;
        let mut carry = digit;
        for place in &mut decimal {
            charge(1)?;
            let next = *place * base + carry;
            *place = next % 10;
            carry = next / 10;
        }
        while carry != 0 {
            charge(1)?;
            decimal.push(carry % 10);
            carry /= 10;
        }
    }
    while decimal.len() > 1 && decimal.last() == Some(&0) {
        charge(1)?;
        decimal.pop();
    }
    charge(decimal.len())?;
    let mut result = String::new();
    if negative && decimal != [0] {
        result.push('-');
    }
    result.extend(
        decimal
            .into_iter()
            .rev()
            .map(|digit| char::from_digit(digit, 10).expect("decimal digit")),
    );
    Ok(result)
}

fn bigint_order(left: &str, right: &str) -> Ordering {
    match (left.strip_prefix('-'), right.strip_prefix('-')) {
        (Some(a), Some(b)) => b.len().cmp(&a.len()).then_with(|| b.cmp(a)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.len().cmp(&right.len()).then_with(|| left.cmp(right)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigint_show_adds_n_without_changing_numeric_normalization_or_input_storage() {
        for (spelling, numeric, shown) in [
            ("0", "0", "0n"),
            ("-0", "0", "0n"),
            ("0x00", "0", "0n"),
            ("-0b0", "0", "0n"),
            ("123", "123", "123n"),
            ("-123", "-123", "-123n"),
            ("0xff", "255", "255n"),
            ("-0xff", "-255", "-255n"),
            ("0o17", "15", "15n"),
            ("-0o17", "-15", "-15n"),
            ("0b1010", "10", "10n"),
            ("-0b1010", "-10", "-10n"),
            (
                "0x100000000000000000000000000000000",
                "340282366920938463463374607431768211456",
                "340282366920938463463374607431768211456n",
            ),
        ] {
            assert_eq!(
                bigint_text(spelling),
                Ok(numeric.into()),
                "numeric normalization remains suffix-free"
            );
            let mut heap = Heap::new();
            let input = heap
                .literal(&Lit::BigInt(spelling.into()))
                .expect("BigInt input");
            let result = intrinsic(&mut heap, Intrinsic::Show, &input, &mut String::new())
                .expect("primitive Show BigInt");
            assert_eq!(heap.string(&result), Ok(shown));
            assert_eq!(
                heap.bigint(&input),
                Ok(spelling),
                "display must not mutate its borrowed input"
            );
            assert_eq!(
                heap.stats().live_objects,
                2,
                "input and displayed String have independent owned storage"
            );
            heap.drop_values(vec![input, result])
                .expect("release both owned roots");
            heap.finish().expect("BigInt and output String released");
        }
    }

    #[test]
    fn bigint_show_does_not_change_other_primitive_display_forms() {
        for (literal, expected) in [
            (Lit::Unit, "undefined"),
            (Lit::Bool(false), "false"),
            (Lit::Int(-7), "-7"),
            (Lit::Float(1.5), "1.5"),
            (Lit::String("quoted".into()), "\"quoted\""),
        ] {
            let mut heap = Heap::new();
            let input = heap.literal(&literal).expect("primitive input");
            let result = intrinsic(&mut heap, Intrinsic::Show, &input, &mut String::new())
                .expect("primitive Show");
            assert_eq!(heap.string(&result), Ok(expected));
            heap.drop_values(vec![input, result])
                .expect("primitive roots release");
            heap.finish().expect("ordinary primitive display balanced");
        }
    }
}
