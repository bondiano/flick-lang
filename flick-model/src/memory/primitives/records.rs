fn list_range(
    heap: &mut Heap,
    from: &Value,
    until: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (Value::Int(from), Value::Int(until)) = (from, until) else {
        return Err(Error::ExpectedInt);
    };
    if *from >= *until {
        super::charge(fuel, 1)?;
        return heap.construct("$list", Vec::new());
    }
    let count =
        usize::try_from(i64::from(*until) - i64::from(*from)).map_err(|_| Error::OutOfFuel)?;
    super::charge(fuel, count.saturating_add(1))?;
    let mut fields = Vec::with_capacity(count);
    let mut current = i64::from(*from);
    let limit = i64::from(*until);
    while current < limit {
        fields.push(Value::Int(i32::try_from(current).expect("in i32 range")));
        current += 1;
    }
    heap.construct("$list", fields)
}

fn record_get(
    heap: &mut Heap,
    label: &Value,
    record: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let name = heap.label_str(label)?.to_owned();
    super::charge(fuel, name.len().saturating_add(1))?;
    heap.project_named(record, &name)
}

fn record_extend(
    heap: &mut Heap,
    label: &Value,
    value: &Value,
    record: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let name = heap.label_str(label)?.to_owned();
    let Value::Object(token) = record else {
        return Err(Error::ExpectedRecord);
    };
    let existing: Vec<(String, Value)> = {
        let Data::Record(fields) = &heap.object(token.id)?.data else {
            return Err(Error::ExpectedRecord);
        };
        if fields.iter().any(|(k, _)| k == &name) {
            return Err(Error::DuplicateField(name));
        }
        super::charge(fuel, fields.len().saturating_add(2))?;
        fields
            .iter()
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect()
    };
    let mut out = Vec::with_capacity(existing.len() + 1);
    for (k, v) in existing {
        match heap.dup(&v) {
            Ok(owned) => out.push((k, owned)),
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    match heap.dup(value) {
        Ok(owned) => out.push((name, owned)),
        Err(err) => {
            heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
            return Err(err);
        }
    }
    heap.record(out)
}

fn record_remove(
    heap: &mut Heap,
    label: &Value,
    record: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let name = heap.label_str(label)?.to_owned();
    let Value::Object(token) = record else {
        return Err(Error::ExpectedRecord);
    };
    let kept: Vec<(String, Value)> = {
        let Data::Record(fields) = &heap.object(token.id)?.data else {
            return Err(Error::ExpectedRecord);
        };
        super::charge(fuel, fields.len().saturating_add(1))?;
        fields
            .iter()
            .filter(|(k, _)| k != &name)
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect()
    };
    let mut out = Vec::with_capacity(kept.len());
    for (k, v) in kept {
        match heap.dup(&v) {
            Ok(owned) => out.push((k, owned)),
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    heap.record(out)
}

fn record_project(
    heap: &mut Heap,
    labels_list: &Value,
    record: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (backing, offset, len) = heap.list_view(labels_list)?;
    super::charge(fuel, len.saturating_add(1))?;
    let mut names = Vec::with_capacity(len);
    for idx in offset..offset + len {
        let Data::Tagged { fields, .. } = &heap.object(backing)?.data else {
            return Err(Error::PatternKind("list"));
        };
        names.push(heap.string(&fields[idx])?.to_owned());
    }
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        match heap.project_named(record, &name) {
            Ok(val) => out.push((name, val)),
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    heap.record(out)
}

fn record_merge_disjoint(
    heap: &mut Heap,
    left: &Value,
    right: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (Value::Object(l_tok), Value::Object(r_tok)) = (left, right) else {
        return Err(Error::ExpectedRecord);
    };
    let pairs: Vec<(String, Value)> = {
        let (Data::Record(lf), Data::Record(rf)) =
            (&heap.object(l_tok.id)?.data, &heap.object(r_tok.id)?.data)
        else {
            return Err(Error::ExpectedRecord);
        };
        super::charge(fuel, lf.len().saturating_add(rf.len()).saturating_add(1))?;
        let mut seen = BTreeSet::new();
        let mut collected = Vec::with_capacity(lf.len() + rf.len());
        for (k, v) in lf.iter().chain(rf.iter()) {
            if !seen.insert(k.as_str()) {
                return Err(Error::DuplicateField(k.clone()));
            }
            collected.push((k.clone(), v.representation()));
        }
        collected
    };
    let mut out = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        match heap.dup(&v) {
            Ok(owned) => out.push((k, owned)),
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    heap.record(out)
}

fn record_labels(heap: &mut Heap, record: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let Value::Object(token) = record else {
        return Err(Error::ExpectedRecord);
    };
    let mut labels: Vec<String> = {
        let Data::Record(fields) = &heap.object(token.id)?.data else {
            return Err(Error::ExpectedRecord);
        };
        super::charge(fuel, fields.len().saturating_add(1))?;
        fields.iter().map(|(k, _)| k.clone()).collect()
    };
    labels.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    let mut values = Vec::with_capacity(labels.len());
    for label in labels {
        match heap.literal(&Lit::String(label)) {
            Ok(v) => values.push(v),
            Err(err) => {
                heap.drop_values(values)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", values)
}

fn exit_predicate(
    heap: &Heap,
    kind: Intrinsic,
    exit: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    super::charge(fuel, 1)?;
    let Value::Object(token) = exit else {
        return Err(Error::ExpectedExit);
    };
    let Data::Exit(data) = &heap.object(token.id)?.data else {
        return Err(Error::ExpectedExit);
    };
    let result = match (kind, data) {
        (Intrinsic::ExitIsSuccess, ExitData::Success(_))
        | (Intrinsic::ExitIsFailure, ExitData::Failure(_)) => true,
        (Intrinsic::ExitIsSuccess, ExitData::Failure(_))
        | (Intrinsic::ExitIsFailure | Intrinsic::ExitHasInterrupts, ExitData::Success(_)) => false,
        (Intrinsic::ExitHasInterrupts, ExitData::Failure(cause)) => {
            let Value::Object(cause_tok) = cause else {
                return Err(Error::ExpectedCause);
            };
            let Data::Cause(data) = &heap.object(cause_tok.id)?.data else {
                return Err(Error::ExpectedCause);
            };
            data.reasons
                .iter()
                .any(|r| matches!(r, super::causes::StoredReason::Interrupt { .. }))
        }
        _ => unreachable!("exit predicate dispatch"),
    };
    Ok(Value::Bool(result))
}
