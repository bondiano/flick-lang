/// Program execution meters unbounded arithmetic while retaining both inputs.
pub(crate) fn binary_metered(
    heap: &mut Heap,
    op: BinOp,
    left: &Value,
    right: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    heap.admit_ordinary(left)?;
    heap.admit_ordinary(right)?;
    if let BinOp::Arith(op, Num::BigInt) = op {
        bigint_binary(heap, op, left, right, |work| charge(fuel, work))
    } else {
        binary(heap, op, left, right)
    }
}

fn bigint_binary(
    heap: &mut Heap,
    op: Arith,
    left: &Value,
    right: &Value,
    mut charge: impl FnMut(usize) -> Result<(), Error>,
) -> Result<Value, Error> {
    let text = bigint::arithmetic(op, heap.bigint(left)?, heap.bigint(right)?, &mut charge)?;
    charge(1)?;
    Ok(heap.allocate(Data::BigInt(text)))
}

pub(super) fn binary(
    heap: &mut Heap,
    op: BinOp,
    left: &Value,
    right: &Value,
) -> Result<Value, Error> {
    heap.admit_ordinary(left)?;
    heap.admit_ordinary(right)?;
    Ok(match op {
        BinOp::Arith(op, num) => return arith_binary(heap, op, num, left, right),
        BinOp::Eq | BinOp::NotEq => Value::Bool(equal(heap, left, right)? == (op == BinOp::Eq)),
        BinOp::Same | BinOp::NotSame => {
            Value::Bool(same_primitive(heap, left, right)? == (op == BinOp::Same))
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ordered = order(heap, left, right)?;
            Value::Bool(match (op, ordered) {
                (_, None) => false,
                (BinOp::Lt, Some(order)) => order == Ordering::Less,
                (BinOp::Le, Some(order)) => order != Ordering::Greater,
                (BinOp::Gt, Some(order)) => order == Ordering::Greater,
                (_, Some(order)) => order != Ordering::Less,
            })
        }
        BinOp::And | BinOp::Or => {
            let (Value::Bool(a), Value::Bool(b)) = (left, right) else {
                return Err(Error::ExpectedBoolean);
            };
            Value::Bool(if op == BinOp::And { *a && *b } else { *a || *b })
        }
        BinOp::Concat => {
            let mut units = heap.utf16_units(left)?;
            units.extend(heap.utf16_units(right)?);
            heap.alloc_utf16(units)
        }
        BinOp::ListConcat => return list_concat(heap, left, right),
    })
}

#[expect(
    clippy::float_cmp,
    reason = "primitive strict equality uses IEEE float comparison"
)]
fn same_primitive(heap: &Heap, left: &Value, right: &Value) -> Result<bool, Error> {
    match (left, right) {
        (Value::Float(a), Value::Float(b)) => Ok(a == b),
        (Value::Float32(a), Value::Float32(b)) => Ok(a == b),
        (Value::Object(a), Value::Object(b)) => {
            match (&heap.object(a.id)?.data, &heap.object(b.id)?.data) {
                (Data::ServiceTag { .. }, Data::ServiceTag { .. }) => Ok(a.id == b.id),
                (
                    Data::ForeignHandle {
                        execution: ae,
                        host_id: ah,
                    },
                    Data::ForeignHandle {
                        execution: be,
                        host_id: bh,
                    },
                ) => Ok(ae == be && ah == bh),
                (Data::String(a), Data::String(b)) => Ok(a == b),
                (Data::Utf16(a), Data::Utf16(b)) => Ok(a == b),
                (Data::Bytes(a), Data::Bytes(b)) => Ok(a == b),
                (Data::JsonNull, Data::JsonNull) => Ok(true),
                (Data::Duration(a), Data::Duration(b)) => {
                    Ok(duration_ops::compare_raw(a, b) == Ordering::Equal)
                }
                (Data::DateTime(a), Data::DateTime(b)) => Ok(datetime_ops::dt_eq(a, b)),
                (Data::TimeZone(a), Data::TimeZone(b)) => Ok(datetime_tzif::tz_equals(a, b)),
                (Data::Cron(a), Data::Cron(b)) => Ok(cron_ops::cron_equals_raw(a, b)),
                (
                    Data::BigDecimal {
                        value: av,
                        scale: as_,
                    },
                    Data::BigDecimal {
                        value: bv,
                        scale: bs,
                    },
                ) => Ok(decimal_ops::compare_raw(av, *as_, bv, *bs) == Ok(Ordering::Equal)),
                (Data::String(_), Data::Utf16(_)) | (Data::Utf16(_), Data::String(_)) => Ok(false),
                (Data::BigInt(a), Data::BigInt(b)) => Ok(bigint_text(a)? == bigint_text(b)?),
                (
                    Data::Tagged {
                        tag: a,
                        ordinary_fields: an,
                        fields: x,
                        ..
                    },
                    Data::Tagged {
                        tag: b,
                        ordinary_fields: bn,
                        fields: y,
                        ..
                    },
                ) => {
                    tagged_boundary(a, *an, x.len())?;
                    tagged_boundary(b, *bn, y.len())?;
                    if *an != 0 || *bn != 0 {
                        return Err(unsupported("primitive equality of non-primitive objects"));
                    }
                    Ok(a == b)
                }
                _ => Err(unsupported("primitive equality of non-primitive objects")),
            }
        }
        (
            Value::Aggregate {
                tag: a,
                ordinary_fields: an,
                fields: x,
            },
            Value::Aggregate {
                tag: b,
                ordinary_fields: bn,
                fields: y,
            },
        ) => {
            tagged_boundary(a, *an, x.len())?;
            tagged_boundary(b, *bn, y.len())?;
            if *an != 0 || *bn != 0 {
                return Err(unsupported("primitive equality of non-primitive objects"));
            }
            Ok(a == b)
        }
        _ => equal(heap, left, right),
    }
}

fn list_concat(heap: &mut Heap, left: &Value, right: &Value) -> Result<Value, Error> {
    let (_, _, left_count) = match heap.list_view(left) {
        Ok(view) => view,
        Err(Error::PatternKind(_)) => return Err(unsupported("list concat receiver")),
        Err(error) => return Err(error),
    };
    let (_, _, right_count) = match heap.list_view(right) {
        Ok(view) => view,
        Err(Error::PatternKind(_)) => return Err(unsupported("list concat receiver")),
        Err(error) => return Err(error),
    };
    let mut fields = Vec::with_capacity(left_count + right_count);
    for (list, count) in [(left, left_count), (right, right_count)] {
        for index in 0..count {
            match heap.project(list, index) {
                Ok(value) => fields.push(value),
                Err(error) => {
                    heap.drop_values(fields)?;
                    return Err(error);
                }
            }
        }
    }
    heap.construct("$list", fields)
}

fn order(heap: &Heap, left: &Value, right: &Value) -> Result<Option<Ordering>, Error> {
    Ok(match (left, right) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Int8(a), Value::Int8(b)) => Some(a.cmp(b)),
        (Value::Int16(a), Value::Int16(b)) => Some(a.cmp(b)),
        (Value::Int64(a), Value::Int64(b)) => Some(a.cmp(b)),
        (Value::UInt8(a), Value::UInt8(b)) => Some(a.cmp(b)),
        (Value::UInt16(a), Value::UInt16(b)) => Some(a.cmp(b)),
        (Value::UInt32(a), Value::UInt32(b)) => Some(a.cmp(b)),
        (Value::UInt64(a), Value::UInt64(b)) => Some(a.cmp(b)),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        (Value::Float32(a), Value::Float32(b)) => a.partial_cmp(b),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::Object(a), Value::Object(b)) => {
            match (&heap.object(a.id)?.data, &heap.object(b.id)?.data) {
                (Data::String(a), Data::String(b)) => Some(a.encode_utf16().cmp(b.encode_utf16())),
                (Data::Utf16(a), Data::Utf16(b)) => Some(a.cmp(b)),
                (Data::String(a), Data::Utf16(b)) => {
                    Some(a.encode_utf16().cmp(b.iter().copied()))
                }
                (Data::Utf16(a), Data::String(b)) => {
                    Some(a.iter().copied().cmp(b.encode_utf16()))
                }
                (Data::BigInt(a), Data::BigInt(b)) => {
                    Some(bigint_order(&bigint_text(a)?, &bigint_text(b)?))
                }
                _ => return Err(unsupported("ordering of non-primitive objects")),
            }
        }
        _ => return Err(unsupported("ordering type mismatch")),
    })
}

fn is_opaque_data(data: &Data) -> bool {
    matches!(
        data,
        Data::Dictionary(_)
            | Data::Closure { .. }
            | Data::Task { .. }
            | Data::Continuation { .. }
            | Data::Cell { .. }
            | Data::Var { .. }
            | Data::Lazy { .. }
            | Data::Arena { .. }
            | Data::Seq(_)
            | Data::Iterator { .. }
            | Data::Graph(_)
            | Data::GraphWalker(_)
            | Data::Context(_)
            | Data::ContextRef { .. }
            | Data::FiberHandle { .. }
            | Data::Deferred { .. }
            | Data::MemoMap { .. }
            | Data::FiberGroup { .. }
            | Data::MaskRestore { .. }
            | Data::MaskedTask { .. }
            | Data::CachedRunTask { .. }
            | Data::CachedInvalidateTask { .. }
            | Data::ExitTask(_)
    )
}

pub(crate) fn values_equal(
    heap: &Heap,
    left: &Value,
    right: &Value,
    fuel: Fuel<'_>,
) -> Result<bool, Error> {
    charge(fuel, 1)?;
    equal(heap, left, right)
}

fn equal_objects<'a>(
    heap: &'a Heap,
    left: &'a Value,
    right: &'a Value,
    a_id: ObjectId,
    b_id: ObjectId,
    pending: &mut Vec<(&'a Value, &'a Value)>,
) -> Result<bool, Error> {
    let (a_data, b_data) = (&heap.object(a_id)?.data, &heap.object(b_id)?.data);
    match (a_data, b_data) {
        (Data::ServiceTag { .. }, Data::ServiceTag { .. }) => Ok(a_id == b_id),
        (
            Data::ForeignHandle {
                execution: ae,
                host_id: ah,
            },
            Data::ForeignHandle {
                execution: be,
                host_id: bh,
            },
        ) => Ok(ae == be && ah == bh),
        (Data::Label(a), Data::Label(b)) | (Data::String(a), Data::String(b)) => Ok(a == b),
        (Data::Utf16(a), Data::Utf16(b)) => Ok(a == b),
        (Data::Bytes(a), Data::Bytes(b)) => Ok(a == b),
        (Data::JsonNull, Data::JsonNull) => Ok(true),
        (Data::Duration(a), Data::Duration(b)) => {
            Ok(duration_ops::compare_raw(a, b) == Ordering::Equal)
        }
        (Data::DateTime(a), Data::DateTime(b)) => Ok(datetime_ops::dt_eq(a, b)),
        (Data::TimeZone(a), Data::TimeZone(b)) => Ok(datetime_tzif::tz_equals(a, b)),
        (Data::Cron(a), Data::Cron(b)) => Ok(cron_ops::cron_equals_raw(a, b)),
        (
            Data::BigDecimal {
                value: av,
                scale: as_,
            },
            Data::BigDecimal {
                value: bv,
                scale: bs,
            },
        ) => Ok(decimal_ops::compare_raw(av, *as_, bv, *bs)? == Ordering::Equal),
        (Data::BigInt(a), Data::BigInt(b)) => Ok(bigint_text(a)? == bigint_text(b)?),
        (Data::ListSlice { .. }, _) | (_, Data::ListSlice { .. }) => {
            let Ok((a_obj, a_off, a_len)) = heap.list_view(left) else {
                return Ok(false);
            };
            let Ok((b_obj, b_off, b_len)) = heap.list_view(right) else {
                return Ok(false);
            };
            if a_len == b_len {
                let Data::Tagged { fields: af, .. } = &heap.object(a_obj)?.data else {
                    return Ok(false);
                };
                let Data::Tagged { fields: bf, .. } = &heap.object(b_obj)?.data else {
                    return Ok(false);
                };
                pending.extend(
                    af[a_off..a_off + a_len]
                        .iter()
                        .zip(&bf[b_off..b_off + b_len]),
                );
                Ok(true)
            } else {
                Ok(false)
            }
        }
        (
            Data::Tagged {
                tag: a_tag,
                ordinary_fields: a_count,
                fields: a,
                ..
            },
            Data::Tagged {
                tag: b_tag,
                ordinary_fields: b_count,
                fields: b,
                ..
            },
        ) => {
            tagged_boundary(a_tag, *a_count, a.len())?;
            tagged_boundary(b_tag, *b_count, b.len())?;
            if a_tag == b_tag && a_count == b_count {
                pending.extend(a[..*a_count].iter().zip(&b[..*b_count]));
                Ok(true)
            } else {
                Ok(false)
            }
        }
        (Data::BoxedAggregate { value: a, .. }, Data::BoxedAggregate { value: b, .. }) => {
            pending.push((a, b));
            Ok(true)
        }
        (Data::Record(a), Data::Record(b)) if a.len() == b.len() => {
            for (name, left) in a {
                let Some((_, right)) = b.iter().find(|(other, _)| other == name) else {
                    return Ok(false);
                };
                pending.push((left, right));
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[expect(
    clippy::float_cmp,
    reason = "structural Float equality is SameValueZero when strict_zero is false"
)]
fn equal_with(heap: &Heap, left: &Value, right: &Value, strict_zero: bool) -> Result<bool, Error> {
    let mut pending = vec![(left, right)];
    let mut seen = BTreeSet::new();
    while let Some((left, right)) = pending.pop() {
        let same = match (left, right) {
            (Value::Unit, Value::Unit) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Int8(a), Value::Int8(b)) => a == b,
            (Value::Int16(a), Value::Int16(b)) => a == b,
            (Value::Int64(a), Value::Int64(b)) => a == b,
            (Value::UInt8(a), Value::UInt8(b)) => a == b,
            (Value::UInt16(a), Value::UInt16(b)) => a == b,
            (Value::UInt32(a), Value::UInt32(b)) => a == b,
            (Value::UInt64(a), Value::UInt64(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => {
                if strict_zero {
                    a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan()
                } else {
                    a == b || a.is_nan() && b.is_nan()
                }
            }
            (Value::Float32(a), Value::Float32(b)) => {
                if strict_zero {
                    a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan()
                } else {
                    a == b || a.is_nan() && b.is_nan()
                }
            }
            (
                Value::Aggregate {
                    tag: a_tag,
                    ordinary_fields: a_count,
                    fields: a,
                },
                Value::Aggregate {
                    tag: b_tag,
                    ordinary_fields: b_count,
                    fields: b,
                },
            ) => {
                tagged_boundary(a_tag, *a_count, a.len())?;
                tagged_boundary(b_tag, *b_count, b.len())?;
                if a_tag == b_tag && a_count == b_count {
                    pending.extend(a[..*a_count].iter().zip(&b[..*b_count]));
                    true
                } else {
                    false
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                let (a_data, b_data) = (&heap.object(a.id)?.data, &heap.object(b.id)?.data);
                if is_opaque_data(a_data) || is_opaque_data(b_data) {
                    if strict_zero {
                        a.id == b.id
                    } else {
                        return Err(unsupported("closure equality"));
                    }
                } else if !seen.insert((a.id, b.id)) {
                    continue;
                } else {
                    equal_objects(heap, left, right, a.id, b.id, &mut pending)?
                }
            }
            (Value::Owner(_) | Value::Arena(_) | Value::ArenaView(_), _)
            | (_, Value::Owner(_) | Value::Arena(_) | Value::ArenaView(_)) => {
                if strict_zero {
                    false
                } else {
                    return Err(unsupported("opaque arena capability equality"));
                }
            }
            (Value::Cell(a), Value::Cell(b)) if strict_zero => a == b,
            (Value::Cell(_), _) | (_, Value::Cell(_)) => {
                if strict_zero {
                    false
                } else {
                    return Err(unsupported("cell equality requires Ref.same"));
                }
            }
            _ => false,
        };
        if !same {
            return Ok(false);
        }
    }
    Ok(true)
}

fn equal(heap: &Heap, left: &Value, right: &Value) -> Result<bool, Error> {
    equal_with(heap, left, right, false)
}

/// Structural `SameValue` equivalence for `Filter.equals_strict`: distinguishes
/// `0.0` from `-0.0`, compares mutable cells and foreign handles by identity,
/// ignores GADT held dictionaries and written record field order, and compares
/// records, tuples, lists, and ADTs structurally.
pub(super) fn strict_same_value(heap: &Heap, left: &Value, right: &Value) -> Result<bool, Error> {
    equal_with(heap, left, right, true)
}
