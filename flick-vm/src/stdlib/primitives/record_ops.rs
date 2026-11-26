//! Counted non-callback `ReadonlyRecord` dictionary primitives (`packages/core/src/Record.fl`).

use std::mem;

use flick_core::ir::Lit;
use flick_core::ir::memory::RecordIntrinsic;

use super::{Data, Error, Fuel, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: RecordIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        RecordIntrinsic::Empty => heap.record(Vec::new()),
        RecordIntrinsic::IsEmpty => Ok(Value::Bool(record_fields(heap, &args[0])?.is_empty())),
        RecordIntrinsic::Size => {
            let len = record_fields(heap, &args[0])?.len();
            Ok(Value::Int(i32::try_from(len).unwrap_or(i32::MAX)))
        }
        RecordIntrinsic::FromEntries => from_entries(heap, &args[0], fuel),
        RecordIntrinsic::ToEntries => to_entries(heap, &args[0], fuel),
        RecordIntrinsic::Keys => record_keys(heap, &args[0], fuel),
        RecordIntrinsic::Values => record_values(heap, &args[0], fuel),
        RecordIntrinsic::FromKeys => from_keys(heap, &args[0], &args[1], fuel),
        RecordIntrinsic::Singleton => {
            let key = heap.string(&args[0])?.to_owned();
            let val = heap.dup(&args[1])?;
            heap.record(vec![(key, val)])
        }
        RecordIntrinsic::Has => {
            let key = heap.string(&args[0])?;
            let fields = record_fields(heap, &args[1])?;
            charge(fuel, fields.len())?;
            Ok(Value::Bool(fields.iter().any(|(k, _)| k == key)))
        }
        RecordIntrinsic::Get => {
            let key = heap.string(&args[0])?.to_owned();
            let found = {
                let fields = record_fields(heap, &args[1])?;
                charge(fuel, fields.len())?;
                fields
                    .iter()
                    .find(|(k, _)| k == &key)
                    .map(|(_, v)| v.representation())
            };
            match found {
                Some(rep) => {
                    let owned = heap.dup(&rep)?;
                    heap.construct("Prelude.Option.Some", vec![owned])
                }
                None => heap.construct("Prelude.Option.None", vec![]),
            }
        }
        RecordIntrinsic::Set => record_set(heap, &args[0], &args[1], &args[2], false, fuel),
        RecordIntrinsic::Replace => record_set(heap, &args[0], &args[1], &args[2], true, fuel),
        RecordIntrinsic::Remove => record_remove(heap, &args[0], &args[1], fuel),
        RecordIntrinsic::Pop => record_pop(heap, &args[0], &args[1], fuel),
        RecordIntrinsic::Difference => record_difference(heap, &args[1], &args[0], fuel),
    }
}

fn record_fields<'a>(heap: &'a Heap, value: &Value) -> Result<&'a [(String, Value)], Error> {
    let Value::Object(token) = value else {
        return Err(Error::ExpectedRecord);
    };
    let Data::Record(fields) = &heap.object(token.id)?.data else {
        return Err(Error::ExpectedRecord);
    };
    Ok(fields)
}

fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || !key.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    let n: u32 = key.parse().ok()?;
    if n == u32::MAX || n.to_string() != key {
        return None;
    }
    Some(n)
}

/// Inserts or updates `(key, value)` maintaining ECMAScript `OwnPropertyKeys` order:
/// canonical array-index keys ascending first, then string keys in insertion order.
pub(super) fn put_js_key(
    fields: &mut Vec<(String, Value)>,
    key: String,
    value: Value,
) -> Option<Value> {
    if let Some(pos) = fields.iter().position(|(k, _)| k == &key) {
        let old = mem::replace(&mut fields[pos].1, value);
        return Some(old);
    }
    let at = if let Some(n) = array_index(&key) {
        fields
            .iter()
            .position(|(k, _)| array_index(k).is_none_or(|m| m > n))
            .unwrap_or(fields.len())
    } else {
        fields.len()
    };
    fields.insert(at, (key, value));
    None
}

fn dup_record_pairs(
    heap: &mut Heap,
    pairs: &[(String, Value)],
) -> Result<Vec<(String, Value)>, Error> {
    let mut out = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        match heap.dup(v) {
            Ok(owned) => out.push((k.clone(), owned)),
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    Ok(out)
}

fn from_entries(heap: &mut Heap, entries_list: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let (backing, offset, len) = heap.list_view(entries_list)?;
    charge(fuel, len)?;
    let mut raw_pairs = Vec::with_capacity(len);
    for idx in offset..offset + len {
        let Data::Tagged { fields, .. } = &heap.object(backing)?.data else {
            return Err(Error::PatternKind("list"));
        };
        let Value::Object(pair_tok) = &fields[idx] else {
            return Err(Error::PatternKind("tuple"));
        };
        let Data::Tagged {
            tag,
            fields: pfields,
            ..
        } = &heap.object(pair_tok.id)?.data
        else {
            return Err(Error::PatternKind("tuple"));
        };
        if tag != "$tuple" || pfields.len() != 2 {
            return Err(Error::PatternKind("tuple"));
        }
        let key = heap.string(&pfields[0])?.to_owned();
        raw_pairs.push((key, pfields[1].representation()));
    }
    let mut out = Vec::with_capacity(raw_pairs.len());
    for (k, v_rep) in raw_pairs {
        match heap.dup(&v_rep) {
            Ok(owned) => {
                if let Some(old) = put_js_key(&mut out, k, owned) {
                    heap.drop_value(old)?;
                }
            }
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    heap.record(out)
}

fn to_entries(heap: &mut Heap, record: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let pairs: Vec<(String, Value)> = {
        let fields = record_fields(heap, record)?;
        charge(fuel, fields.len())?;
        fields
            .iter()
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect()
    };
    let mut out = Vec::with_capacity(pairs.len());
    for (k, v_rep) in pairs {
        let key = match heap.literal(&Lit::String(k)) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        };
        let val = match heap.dup(&v_rep) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(key)?;
                heap.drop_values(out)?;
                return Err(err);
            }
        };
        match heap.construct("$tuple", vec![key, val]) {
            Ok(pair) => out.push(pair),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn record_keys(heap: &mut Heap, record: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let keys: Vec<String> = {
        let fields = record_fields(heap, record)?;
        charge(fuel, fields.len())?;
        fields.iter().map(|(k, _)| k.clone()).collect()
    };
    let mut out = Vec::with_capacity(keys.len());
    for k in keys {
        match heap.literal(&Lit::String(k)) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn record_values(heap: &mut Heap, record: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let reps: Vec<Value> = {
        let fields = record_fields(heap, record)?;
        charge(fuel, fields.len())?;
        fields.iter().map(|(_, v)| v.representation()).collect()
    };
    let mut out = Vec::with_capacity(reps.len());
    for rep in reps {
        match heap.dup(&rep) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn from_keys(
    heap: &mut Heap,
    keys_list: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let (backing, offset, len) = heap.list_view(keys_list)?;
    charge(fuel, len)?;
    let mut names = Vec::with_capacity(len);
    for idx in offset..offset + len {
        let Data::Tagged { fields, .. } = &heap.object(backing)?.data else {
            return Err(Error::PatternKind("list"));
        };
        names.push(heap.string(&fields[idx])?.to_owned());
    }
    let mut out = Vec::with_capacity(names.len());
    for k in names {
        match heap.dup(val) {
            Ok(owned) => {
                if let Some(old) = put_js_key(&mut out, k, owned) {
                    heap.drop_value(old)?;
                }
            }
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    heap.record(out)
}

fn record_set(
    heap: &mut Heap,
    key_val: &Value,
    val: &Value,
    record: &Value,
    only_existing: bool,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let key = heap.string(key_val)?.to_owned();
    let existing: Vec<(String, Value)> = {
        let fields = record_fields(heap, record)?;
        charge(fuel, fields.len().saturating_add(1))?;
        if only_existing && !fields.iter().any(|(k, _)| k == &key) {
            return heap.construct("Prelude.Option.None", vec![]);
        }
        fields
            .iter()
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect()
    };
    let mut out = dup_record_pairs(heap, &existing)?;
    match heap.dup(val) {
        Ok(owned) => {
            if let Some(old) = put_js_key(&mut out, key, owned) {
                heap.drop_value(old)?;
            }
        }
        Err(err) => {
            heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
            return Err(err);
        }
    }
    let rec = heap.record(out)?;
    if only_existing {
        heap.construct("Prelude.Option.Some", vec![rec])
    } else {
        Ok(rec)
    }
}

fn record_remove(
    heap: &mut Heap,
    key_val: &Value,
    record: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let key = heap.string(key_val)?.to_owned();
    let kept: Vec<(String, Value)> = {
        let fields = record_fields(heap, record)?;
        charge(fuel, fields.len())?;
        fields
            .iter()
            .filter(|(k, _)| k != &key)
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect()
    };
    let out = dup_record_pairs(heap, &kept)?;
    heap.record(out)
}

fn record_pop(
    heap: &mut Heap,
    key_val: &Value,
    record: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let key = heap.string(key_val)?.to_owned();
    let (popped_rep, kept): (Option<Value>, Vec<(String, Value)>) = {
        let fields = record_fields(heap, record)?;
        charge(fuel, fields.len())?;
        let mut found = None;
        let mut rest = Vec::with_capacity(fields.len());
        for (k, v) in fields {
            if k == &key {
                found = Some(v.representation());
            } else {
                rest.push((k.clone(), v.representation()));
            }
        }
        (found, rest)
    };
    let Some(rep) = popped_rep else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let popped = heap.dup(&rep)?;
    let out = match dup_record_pairs(heap, &kept) {
        Ok(pairs) => pairs,
        Err(err) => {
            heap.drop_value(popped)?;
            return Err(err);
        }
    };
    let rest_rec = match heap.record(out) {
        Ok(r) => r,
        Err(err) => {
            heap.drop_value(popped)?;
            return Err(err);
        }
    };
    let pair = heap.construct("$tuple", vec![popped, rest_rec])?;
    heap.construct("Prelude.Option.Some", vec![pair])
}

fn record_difference(
    heap: &mut Heap,
    self_rec: &Value,
    that_rec: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let pairs: Vec<(String, Value)> = {
        let lf = record_fields(heap, self_rec)?;
        let rf = record_fields(heap, that_rec)?;
        charge(fuel, lf.len().saturating_add(rf.len()))?;
        let mut collected = Vec::new();
        for (k, v) in lf {
            if !rf.iter().any(|(rk, _)| rk == k) {
                collected.push((k.clone(), v.representation()));
            }
        }
        for (k, v) in rf {
            if !lf.iter().any(|(lk, _)| lk == k) {
                collected.push((k.clone(), v.representation()));
            }
        }
        collected
    };
    let mut out = Vec::with_capacity(pairs.len());
    for (k, v_rep) in pairs {
        match heap.dup(&v_rep) {
            Ok(owned) => {
                if let Some(old) = put_js_key(&mut out, k, owned) {
                    heap.drop_value(old)?;
                }
            }
            Err(err) => {
                heap.drop_values(out.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        }
    }
    heap.record(out)
}
