//! Counted non-callback `List` core primitive implementations and shared slice helpers.

use std::mem;

use flick_core::ir::memory::ListIntrinsic;

use super::list_ops_extra;
use super::{Data, Error, Fuel, Heap, ObjectId, ObjectToken, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: ListIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        ListIntrinsic::Length => {
            let (_, _, len) = heap.list_view(&args[0])?;
            Ok(Value::Int(i32::try_from(len).unwrap_or(i32::MAX)))
        }
        ListIntrinsic::Get => list_get(heap, &args[0], &args[1]),
        ListIntrinsic::Set => list_set(heap, &args[0], &args[1], &args[2], fuel),
        ListIntrinsic::Repeat => list_repeat(heap, &args[0], &args[1], false, fuel),
        ListIntrinsic::Reverse => list_reverse(heap, &args[0], fuel),
        ListIntrinsic::Concat => list_concat(heap, &args[0], fuel),
        ListIntrinsic::Take => list_take_drop(heap, &args[0], &args[1], true),
        ListIntrinsic::Drop => list_take_drop(heap, &args[0], &args[1], false),
        ListIntrinsic::Slice => list_slice(heap, &args[0], &args[1], &args[2]),
        ListIntrinsic::Zip => list_zip(heap, &args[0], &args[1], fuel),
        ListIntrinsic::Unzip => list_unzip(heap, &args[0], fuel),
        ListIntrinsic::TakeRight => list_take_drop_right(heap, &args[0], &args[1], true),
        ListIntrinsic::DropRight => list_take_drop_right(heap, &args[0], &args[1], false),
        _ => list_ops_extra::run(heap, op, args, fuel),
    }
}

pub(super) fn expect_int(value: &Value) -> Result<i32, Error> {
    match *value {
        Value::Int(i) => Ok(i),
        _ => Err(Error::ExpectedInt),
    }
}

pub(super) fn slice_reps(heap: &Heap, list: &Value) -> Result<Vec<Value>, Error> {
    let (backing, offset, len) = heap.list_view(list)?;
    let Data::Tagged { fields, .. } = &heap.object(backing)?.data else {
        return Err(Error::PatternKind("list"));
    };
    Ok(fields[offset..offset + len]
        .iter()
        .map(Value::representation)
        .collect())
}

pub(super) fn dup_reps(heap: &mut Heap, reps: &[Value]) -> Result<Vec<Value>, Error> {
    let mut out = Vec::with_capacity(reps.len());
    for rep in reps {
        match heap.dup(rep) {
            Ok(owned) => out.push(owned),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    Ok(out)
}

pub(super) fn make_slice(
    heap: &mut Heap,
    backing: ObjectId,
    offset: usize,
    len: usize,
) -> Result<Value, Error> {
    if len == 0 {
        return heap.construct("$list", Vec::new());
    }
    let backing_val = heap.dup(&Value::Object(ObjectToken { id: backing }))?;
    Ok(heap.allocate(Data::ListSlice {
        backing: backing_val,
        offset,
        len,
    }))
}

fn list_get(heap: &mut Heap, index: &Value, list: &Value) -> Result<Value, Error> {
    let idx = expect_int(index)?;
    let (backing, offset, len) = heap.list_view(list)?;
    if let Ok(i) = usize::try_from(idx)
        && i < len
    {
        let rep = {
            let Data::Tagged { fields, .. } = &heap.object(backing)?.data else {
                return Err(Error::PatternKind("list"));
            };
            fields[offset + i].representation()
        };
        let owned = heap.dup(&rep)?;
        heap.construct("Prelude.Option.Some", vec![owned])
    } else {
        heap.construct("Prelude.Option.None", vec![])
    }
}

pub(super) fn list_set(
    heap: &mut Heap,
    index: &Value,
    item: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let idx = expect_int(index)?;
    let reps = slice_reps(heap, list)?;
    charge(fuel, reps.len())?;
    let Ok(i) = usize::try_from(idx) else {
        return heap.dup(list);
    };
    if i >= reps.len() {
        return heap.dup(list);
    }
    let mut owned = dup_reps(heap, &reps)?;
    match heap.dup(item) {
        Ok(val) => {
            let old = mem::replace(&mut owned[i], val);
            heap.drop_value(old)?;
            heap.construct("$list", owned)
        }
        Err(err) => {
            heap.drop_values(owned)?;
            Err(err)
        }
    }
}

pub(super) fn list_repeat(
    heap: &mut Heap,
    count_val: &Value,
    item: &Value,
    at_least_one: bool,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let raw = expect_int(count_val)?;
    let min = usize::from(at_least_one);
    let count = usize::try_from(raw.max(0)).unwrap_or(0).max(min);
    charge(fuel, count)?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        match heap.dup(item) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn list_reverse(heap: &mut Heap, list: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let mut reps = slice_reps(heap, list)?;
    charge(fuel, reps.len())?;
    reps.reverse();
    let owned = dup_reps(heap, &reps)?;
    heap.construct("$list", owned)
}

fn list_concat(heap: &mut Heap, outer: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let outer_reps = slice_reps(heap, outer)?;
    charge(fuel, outer_reps.len())?;
    let mut flat = Vec::new();
    for inner in &outer_reps {
        let inner_reps = slice_reps(heap, inner)?;
        charge(fuel, inner_reps.len())?;
        flat.extend(inner_reps);
    }
    let owned = dup_reps(heap, &flat)?;
    heap.construct("$list", owned)
}

fn list_take_drop(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    take: bool,
) -> Result<Value, Error> {
    let n = usize::try_from(expect_int(n_val)?.max(0)).unwrap_or(0);
    let (backing, offset, len) = heap.list_view(list)?;
    let cut = n.min(len);
    if take {
        make_slice(heap, backing, offset, cut)
    } else {
        make_slice(heap, backing, offset + cut, len - cut)
    }
}

fn list_take_drop_right(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    take: bool,
) -> Result<Value, Error> {
    let n = usize::try_from(expect_int(n_val)?.max(0)).unwrap_or(0);
    let (backing, offset, len) = heap.list_view(list)?;
    let cut = n.min(len);
    if take {
        make_slice(heap, backing, offset + len - cut, cut)
    } else {
        make_slice(heap, backing, offset, len - cut)
    }
}

fn list_slice(heap: &mut Heap, start: &Value, end: &Value, list: &Value) -> Result<Value, Error> {
    let s_raw = expect_int(start)?;
    let e_raw = expect_int(end)?;
    let (backing, offset, len) = heap.list_view(list)?;
    let clamp = |raw: i32| {
        let len_i = i64::try_from(len).unwrap_or(i64::MAX);
        let idx = if raw < 0 {
            (len_i + i64::from(raw)).max(0)
        } else {
            i64::from(raw).min(len_i)
        };
        usize::try_from(idx).unwrap_or(len)
    };
    let s = clamp(s_raw);
    let e = clamp(e_raw);
    if s < e {
        make_slice(heap, backing, offset + s, e - s)
    } else {
        heap.construct("$list", Vec::new())
    }
}

fn list_zip(heap: &mut Heap, left: &Value, right: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let lr = slice_reps(heap, left)?;
    let rr = slice_reps(heap, right)?;
    let n = lr.len().min(rr.len());
    charge(fuel, n)?;
    let mut pairs = Vec::with_capacity(n);
    for i in 0..n {
        let a = match heap.dup(&lr[i]) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_values(pairs)?;
                return Err(err);
            }
        };
        let b = match heap.dup(&rr[i]) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(a)?;
                heap.drop_values(pairs)?;
                return Err(err);
            }
        };
        match heap.construct("$tuple", vec![a, b]) {
            Ok(pair) => pairs.push(pair),
            Err(err) => {
                heap.drop_values(pairs)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", pairs)
}

fn list_unzip(heap: &mut Heap, pairs_list: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let pairs = slice_reps(heap, pairs_list)?;
    charge(fuel, pairs.len())?;
    let mut firsts = Vec::with_capacity(pairs.len());
    let mut seconds = Vec::with_capacity(pairs.len());
    for pair in &pairs {
        let Value::Object(tok) = pair else {
            heap.drop_values(firsts)?;
            heap.drop_values(seconds)?;
            return Err(Error::PatternKind("tuple"));
        };
        let (a_rep, b_rep) = {
            let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
                heap.drop_values(firsts)?;
                heap.drop_values(seconds)?;
                return Err(Error::PatternKind("tuple"));
            };
            if tag != "$tuple" || fields.len() != 2 {
                heap.drop_values(firsts)?;
                heap.drop_values(seconds)?;
                return Err(Error::PatternKind("tuple"));
            }
            (fields[0].representation(), fields[1].representation())
        };
        match (heap.dup(&a_rep), heap.dup(&b_rep)) {
            (Ok(a), Ok(b)) => {
                firsts.push(a);
                seconds.push(b);
            }
            (Ok(a), Err(err)) | (Err(err), Ok(a)) => {
                heap.drop_value(a)?;
                heap.drop_values(firsts)?;
                heap.drop_values(seconds)?;
                return Err(err);
            }
            (Err(err), Err(_)) => {
                heap.drop_values(firsts)?;
                heap.drop_values(seconds)?;
                return Err(err);
            }
        }
    }
    let left_list = heap.construct("$list", firsts)?;
    let right_list = match heap.construct("$list", seconds) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(left_list)?;
            return Err(err);
        }
    };
    heap.construct("$tuple", vec![left_list, right_list])
}
