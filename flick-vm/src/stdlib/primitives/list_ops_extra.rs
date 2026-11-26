//! Counted extended `Array` and `NonEmpty` primitive implementations.

use flick_core::ir::memory::ListIntrinsic;

use super::list_ops::{dup_reps, expect_int, list_repeat, list_set, make_slice, slice_reps};
use super::{Data, Error, Fuel, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: ListIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    match op {
        ListIntrinsic::SplitAt => list_split_at(heap, &args[0], &args[1], false),
        ListIntrinsic::Split => list_split_parts(heap, &args[0], &args[1], fuel),
        ListIntrinsic::ChunksOf => list_chunks_of(heap, &args[0], &args[1], fuel),
        ListIntrinsic::Window => list_window(heap, &args[0], &args[1], fuel),
        ListIntrinsic::Tail => list_tail_init(heap, &args[0], true, false),
        ListIntrinsic::Init => list_tail_init(heap, &args[0], false, false),
        ListIntrinsic::Pad => list_pad(heap, &args[0], &args[1], &args[2], fuel),
        ListIntrinsic::Rotate => list_rotate(heap, &args[0], &args[1], fuel),
        ListIntrinsic::InsertAt => list_insert_at(heap, &args[0], &args[1], &args[2], fuel),
        ListIntrinsic::Remove => list_remove(heap, &args[0], &args[1], fuel),
        ListIntrinsic::Replace => list_replace(heap, &args[0], &args[1], &args[2], fuel),
        ListIntrinsic::Intersperse => list_intersperse(heap, &args[0], &args[1], fuel),
        ListIntrinsic::AppendAll => list_append_all(heap, &args[1], &args[0], fuel),
        ListIntrinsic::Cartesian => list_cartesian(heap, &args[1], &args[0], fuel),
        ListIntrinsic::RepeatAll => list_repeat_all(heap, &args[0], &args[1], fuel),
        ListIntrinsic::NonEmptyFromList => {
            let (_, _, len) = heap.list_view(&args[0])?;
            if len > 0 {
                let copy = heap.dup(&args[0])?;
                heap.construct("Prelude.Option.Some", vec![copy])
            } else {
                heap.construct("Prelude.Option.None", vec![])
            }
        }
        ListIntrinsic::NonEmptyCons => list_cons(heap, &args[0], &args[1], true, fuel),
        ListIntrinsic::NonEmptyAppend => list_cons(heap, &args[0], &args[1], false, fuel),
        ListIntrinsic::NonEmptyHead => list_elem_unchecked(heap, &args[0], true),
        ListIntrinsic::NonEmptyLast => list_elem_unchecked(heap, &args[0], false),
        ListIntrinsic::NonEmptyTail => list_tail_init(heap, &args[0], true, true),
        ListIntrinsic::NonEmptyInit => list_tail_init(heap, &args[0], false, true),
        ListIntrinsic::NonEmptyRange => non_empty_range(heap, &args[0], &args[1], fuel),
        ListIntrinsic::NonEmptyReplicate => list_repeat(heap, &args[0], &args[1], true, fuel),
        ListIntrinsic::NonEmptySetHead => list_set_edge(heap, &args[0], &args[1], true, fuel),
        ListIntrinsic::NonEmptySetLast => list_set_edge(heap, &args[0], &args[1], false, fuel),
        ListIntrinsic::NonEmptySplitAt => list_split_at(heap, &args[0], &args[1], true),
        ListIntrinsic::NonEmptyUnappend => non_empty_unedge(heap, &args[0], false),
        ListIntrinsic::NonEmptyUnprepend => non_empty_unedge(heap, &args[0], true),
        _ => unreachable!("core list op routed in list_ops::run"),
    }
}

fn list_split_at(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    at_least_one: bool,
) -> Result<Value, Error> {
    let raw = expect_int(n_val)?;
    let (backing, offset, len) = heap.list_view(list)?;
    let min = usize::from(at_least_one);
    let cut = usize::try_from(raw.max(0)).unwrap_or(0).max(min).min(len);
    let left = make_slice(heap, backing, offset, cut)?;
    let right = match make_slice(heap, backing, offset + cut, len - cut) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(left)?;
            return Err(err);
        }
    };
    heap.construct("$tuple", vec![left, right])
}

fn list_chunks_of(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let size = usize::try_from(expect_int(n_val)?.max(0))
        .unwrap_or(0)
        .max(1);
    let (backing, offset, len) = heap.list_view(list)?;
    let chunks = len.div_ceil(size);
    charge(fuel, chunks)?;
    let mut out = Vec::with_capacity(chunks);
    let mut i = 0;
    while i < len {
        let take = (len - i).min(size);
        match make_slice(heap, backing, offset + i, take) {
            Ok(slice) => out.push(slice),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
        i += take;
    }
    heap.construct("$list", out)
}

fn list_split_parts(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let parts = usize::try_from(expect_int(n_val)?.max(0))
        .unwrap_or(0)
        .max(1);
    let (_, _, len) = heap.list_view(list)?;
    let chunk_size = i32::try_from(len.div_ceil(parts).max(1)).unwrap_or(i32::MAX);
    list_chunks_of(heap, &Value::Int(chunk_size), list, fuel)
}

fn list_window(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let size = usize::try_from(expect_int(n_val)?.max(0)).unwrap_or(0);
    let (backing, offset, len) = heap.list_view(list)?;
    if size == 0 || size > len {
        return heap.construct("$list", Vec::new());
    }
    let count = len - size + 1;
    charge(fuel, count)?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        match make_slice(heap, backing, offset + i, size) {
            Ok(slice) => out.push(slice),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn list_tail_init(
    heap: &mut Heap,
    list: &Value,
    is_tail: bool,
    non_empty: bool,
) -> Result<Value, Error> {
    let (backing, offset, len) = heap.list_view(list)?;
    if len == 0 {
        return if non_empty {
            heap.construct("$list", Vec::new())
        } else {
            heap.construct("Prelude.Option.None", vec![])
        };
    }
    let slice = if is_tail {
        make_slice(heap, backing, offset + 1, len - 1)?
    } else {
        make_slice(heap, backing, offset, len - 1)?
    };
    if non_empty {
        Ok(slice)
    } else {
        heap.construct("Prelude.Option.Some", vec![slice])
    }
}

fn list_pad(
    heap: &mut Heap,
    n_val: &Value,
    fill: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let target = usize::try_from(expect_int(n_val)?.max(0)).unwrap_or(0);
    let (backing, offset, len) = heap.list_view(list)?;
    if len >= target {
        return make_slice(heap, backing, offset, target);
    }
    charge(fuel, target)?;
    let reps = slice_reps(heap, list)?;
    let mut out = dup_reps(heap, &reps)?;
    for _ in len..target {
        match heap.dup(fill) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn list_rotate(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let n = expect_int(n_val)?;
    let mut reps = slice_reps(heap, list)?;
    charge(fuel, reps.len())?;
    let len = reps.len();
    if len == 0 {
        return heap.construct("$list", Vec::new());
    }
    let len_i = i64::try_from(len).unwrap_or(i64::MAX);
    let m = i64::from(n) % len_i;
    if m != 0 {
        let at = usize::try_from(if m < 0 { -m } else { len_i - m }).unwrap_or(0);
        reps.rotate_left(at);
    }
    let owned = dup_reps(heap, &reps)?;
    heap.construct("$list", owned)
}

fn list_insert_at(
    heap: &mut Heap,
    idx_val: &Value,
    item: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let reps = slice_reps(heap, list)?;
    let Ok(i) = usize::try_from(idx) else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    if i > reps.len() {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    charge(fuel, reps.len().saturating_add(1))?;
    let mut owned = dup_reps(heap, &reps)?;
    match heap.dup(item) {
        Ok(v) => owned.insert(i, v),
        Err(err) => {
            heap.drop_values(owned)?;
            return Err(err);
        }
    }
    let built = heap.construct("$list", owned)?;
    heap.construct("Prelude.Option.Some", vec![built])
}

fn list_remove(
    heap: &mut Heap,
    idx_val: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let mut reps = slice_reps(heap, list)?;
    charge(fuel, reps.len())?;
    let Ok(i) = usize::try_from(idx) else {
        return heap.dup(list);
    };
    if i >= reps.len() {
        return heap.dup(list);
    }
    reps.remove(i);
    let owned = dup_reps(heap, &reps)?;
    heap.construct("$list", owned)
}

fn list_replace(
    heap: &mut Heap,
    idx_val: &Value,
    item: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let (_, _, len) = heap.list_view(list)?;
    if usize::try_from(idx).ok().is_none_or(|i| i >= len) {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let updated = list_set(heap, idx_val, item, list, fuel)?;
    heap.construct("Prelude.Option.Some", vec![updated])
}

fn list_intersperse(
    heap: &mut Heap,
    mid: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, list)?;
    if reps.len() <= 1 {
        return heap.dup(list);
    }
    charge(fuel, reps.len().saturating_mul(2))?;
    let mut out = Vec::with_capacity(reps.len() * 2 - 1);
    for (i, rep) in reps.iter().enumerate() {
        if i > 0 {
            match heap.dup(mid) {
                Ok(v) => out.push(v),
                Err(err) => {
                    heap.drop_values(out)?;
                    return Err(err);
                }
            }
        }
        match heap.dup(rep) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn list_append_all(
    heap: &mut Heap,
    self_list: &Value,
    that_list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let mut reps = slice_reps(heap, self_list)?;
    reps.extend(slice_reps(heap, that_list)?);
    charge(fuel, reps.len())?;
    let owned = dup_reps(heap, &reps)?;
    heap.construct("$list", owned)
}

fn list_cartesian(
    heap: &mut Heap,
    self_list: &Value,
    that_list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let lr = slice_reps(heap, self_list)?;
    let rr = slice_reps(heap, that_list)?;
    let total = lr.len().saturating_mul(rr.len());
    charge(fuel, total)?;
    let mut out = Vec::with_capacity(total);
    for a_rep in &lr {
        for b_rep in &rr {
            let a = match heap.dup(a_rep) {
                Ok(v) => v,
                Err(err) => {
                    heap.drop_values(out)?;
                    return Err(err);
                }
            };
            let b = match heap.dup(b_rep) {
                Ok(v) => v,
                Err(err) => {
                    heap.drop_value(a)?;
                    heap.drop_values(out)?;
                    return Err(err);
                }
            };
            match heap.construct("$tuple", vec![a, b]) {
                Ok(pair) => out.push(pair),
                Err(err) => {
                    heap.drop_values(out)?;
                    return Err(err);
                }
            }
        }
    }
    heap.construct("$list", out)
}

fn list_repeat_all(
    heap: &mut Heap,
    n_val: &Value,
    list: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let times = usize::try_from(expect_int(n_val)?.max(0))
        .unwrap_or(0)
        .max(1);
    let reps = slice_reps(heap, list)?;
    let total = reps.len().saturating_mul(times);
    charge(fuel, total)?;
    let mut out = Vec::with_capacity(total);
    for _ in 0..times {
        for rep in &reps {
            match heap.dup(rep) {
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

fn list_cons(
    heap: &mut Heap,
    item: &Value,
    list: &Value,
    front: bool,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, list)?;
    charge(fuel, reps.len().saturating_add(1))?;
    let mut out = Vec::with_capacity(reps.len() + 1);
    if front {
        out.push(heap.dup(item)?);
    }
    for rep in &reps {
        match heap.dup(rep) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    if !front {
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

fn list_elem_unchecked(heap: &mut Heap, list: &Value, first: bool) -> Result<Value, Error> {
    let (backing, offset, len) = heap.list_view(list)?;
    if len == 0 {
        return Err(Error::NoMatch);
    }
    let idx = if first { offset } else { offset + len - 1 };
    let rep = {
        let Data::Tagged { fields, .. } = &heap.object(backing)?.data else {
            return Err(Error::PatternKind("list"));
        };
        fields[idx].representation()
    };
    heap.dup(&rep)
}

fn non_empty_range(
    heap: &mut Heap,
    start_val: &Value,
    end_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let start = expect_int(start_val)?;
    let end = expect_int(end_val)?;
    if end < start {
        return heap.construct("$list", vec![Value::Int(start)]);
    }
    let count =
        usize::try_from(i64::from(end) - i64::from(start) + 1).map_err(|_| Error::OutOfFuel)?;
    charge(fuel, count)?;
    let mut fields = Vec::with_capacity(count);
    let mut cur = i64::from(start);
    let lim = i64::from(end);
    while cur <= lim {
        fields.push(Value::Int(i32::try_from(cur).expect("in i32 range")));
        cur += 1;
    }
    heap.construct("$list", fields)
}

fn list_set_edge(
    heap: &mut Heap,
    item: &Value,
    list: &Value,
    first: bool,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let (_, _, len) = heap.list_view(list)?;
    if len == 0 {
        return heap.dup(list);
    }
    let idx = if first {
        0
    } else {
        i32::try_from(len - 1).unwrap_or(i32::MAX)
    };
    list_set(heap, &Value::Int(idx), item, list, fuel)
}

fn non_empty_unedge(heap: &mut Heap, list: &Value, front: bool) -> Result<Value, Error> {
    let elem = list_elem_unchecked(heap, list, front)?;
    let rest = match list_tail_init(heap, list, front, true) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(elem)?;
            return Err(err);
        }
    };
    if front {
        heap.construct("$tuple", vec![elem, rest])
    } else {
        heap.construct("$tuple", vec![rest, elem])
    }
}
