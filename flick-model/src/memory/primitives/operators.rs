/// Program execution meters unbounded arithmetic while retaining both inputs.
pub(super) fn binary_metered(
    heap: &mut Heap,
    op: BinOp,
    arguments: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    heap.admit_values(arguments)?;
    if let BinOp::Arith(op, Num::BigInt) = op {
        bigint_binary(heap, op, &arguments[0], &arguments[1], |work| {
            super::charge(fuel, work)
        })
    } else {
        binary(heap, op, &arguments[0], &arguments[1])
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

#[expect(
    clippy::float_cmp,
    reason = "language primitive equality uses exact IEEE comparisons"
)]
pub(super) fn binary(
    heap: &mut Heap,
    op: BinOp,
    left: &Value,
    right: &Value,
) -> Result<Value, Error> {
    heap.admit_ordinary(left)?;
    heap.admit_ordinary(right)?;
    Ok(match op {
        BinOp::Arith(op, Num::Int) => {
            let (Value::Int(a), Value::Int(b)) = (left, right) else {
                return Err(unsupported("Int arithmetic type mismatch"));
            };
            Value::Int(int_arith(op, *a, *b))
        }
        BinOp::Arith(op, Num::Float) => {
            let (Value::Float(a), Value::Float(b)) = (left, right) else {
                return Err(unsupported("Float arithmetic type mismatch"));
            };
            Value::Float(match op {
                Arith::Add => a + b,
                Arith::Sub => a - b,
                Arith::Mul => a * b,
                Arith::Div => a / b,
                Arith::Rem => a % b,
                Arith::Pow if b.is_nan() || a.abs() == 1.0 && b.is_infinite() => f64::NAN,
                Arith::Pow => a.powf(*b),
            })
        }
        BinOp::Arith(op, Num::BigInt) => {
            // Standalone execute deliberately has no fuel or automatic cleanup.
            return bigint_binary(heap, op, left, right, |_| Ok(()));
        }
        BinOp::Eq | BinOp::NotEq => Value::Bool(equal(heap, left, right)? == (op == BinOp::Eq)),
        BinOp::Same | BinOp::NotSame => {
            let equal = match (left, right) {
                (Value::Float(a), Value::Float(b)) => a == b,
                (Value::Object(a), Value::Object(b)) => {
                    match (&heap.object(a.id)?.data, &heap.object(b.id)?.data) {
                        (Data::ServiceTag { .. }, Data::ServiceTag { .. }) => a.id == b.id,
                        (Data::String(a), Data::String(b)) => a == b,
                        (Data::BigInt(a), Data::BigInt(b)) => bigint_text(a)? == bigint_text(b)?,
                        (
                            Data::Tagged {
                                tag: a,
                                ordinary_fields: an,
                                fields: x,
                            },
                            Data::Tagged {
                                tag: b,
                                ordinary_fields: bn,
                                fields: y,
                            },
                        ) => {
                            tagged_boundary(a, *an, x.len())?;
                            tagged_boundary(b, *bn, y.len())?;
                            if *an != 0 || *bn != 0 {
                                return Err(unsupported(
                                    "primitive equality of non-primitive objects",
                                ));
                            }
                            a == b
                        }
                        _ => {
                            return Err(unsupported("primitive equality of non-primitive objects"));
                        }
                    }
                }
                _ => equal(heap, left, right)?,
            };
            Value::Bool(equal == (op == BinOp::Same))
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
            let text = format!("{}{}", heap.string(left)?, heap.string(right)?);
            heap.literal(&Lit::String(text))?
        }
        BinOp::ListConcat => return list_concat(heap, left, right),
    })
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
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::Object(a), Value::Object(b)) => {
            match (&heap.object(a.id)?.data, &heap.object(b.id)?.data) {
                (Data::String(a), Data::String(b)) => Some(a.encode_utf16().cmp(b.encode_utf16())),
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
    )
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
        (Data::Label(a), Data::Label(b)) | (Data::String(a), Data::String(b)) => Ok(a == b),
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
            },
            Data::Tagged {
                tag: b_tag,
                ordinary_fields: b_count,
                fields: b,
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
    reason = "structural Float equality is SameValueZero"
)]
fn equal(heap: &Heap, left: &Value, right: &Value) -> Result<bool, Error> {
    let mut pending = vec![(left, right)];
    let mut seen = BTreeSet::new();
    while let Some((left, right)) = pending.pop() {
        let same = match (left, right) {
            (Value::Unit, Value::Unit) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b || a.is_nan() && b.is_nan(),
            (Value::Object(a), Value::Object(b)) => {
                if is_opaque_data(&heap.object(a.id)?.data)
                    || is_opaque_data(&heap.object(b.id)?.data)
                {
                    return Err(unsupported("closure equality"));
                }
                if !seen.insert((a.id, b.id)) {
                    continue;
                }
                equal_objects(heap, left, right, a.id, b.id, &mut pending)?
            }
            (Value::Owner(_) | Value::Arena(_) | Value::ArenaView(_), _)
            | (_, Value::Owner(_) | Value::Arena(_) | Value::ArenaView(_)) => {
                return Err(unsupported("opaque arena capability equality"));
            }
            (Value::Cell(_), _) | (_, Value::Cell(_)) => {
                return Err(unsupported("cell equality requires Ref.same"));
            }
            _ => false,
        };
        if !same {
            return Ok(false);
        }
    }
    Ok(true)
}
