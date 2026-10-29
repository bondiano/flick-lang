package flickrt

// The functions of `Record`: Flick's row primitives and Effect's Record
// functions, in the order Flick passes the arguments (data-last, the
// record at the end). Callbacks get the value, then the key, except
// `collect` and `map_keys`.

// RecordLabels is the labels of a record, sorted.
func RecordLabels(_, r Value) Value {
	keys := sortedKeys(record(r))
	out := make([]Value, len(keys))
	for i, k := range keys {
		out[i] = k
	}
	return out
}

// RecordProject is the fields of `r` the dictionary's labels name, in
// that order.
func RecordProject(labels, r Value) Value {
	x, out := record(r), &Record{}
	for _, l := range List(labels) {
		out = out.set(l.(string), x.Get(l.(string)))
	}
	return out
}

func RecordEmpty() Value { return &Record{} }

func RecordIsEmpty(r Value) Value { return len(record(r).keys) == 0 }
func RecordSize(r Value) Value    { return int32(len(record(r).keys)) }

func RecordKeys(r Value) Value {
	x := record(r)
	out := make([]Value, len(x.keys))
	for i, k := range x.keys {
		out[i] = k
	}
	return out
}

func RecordValues(r Value) Value { return append([]Value{}, record(r).vals...) }

func RecordSingleton(k, v Value) Value { return (&Record{}).set(k.(string), v) }

func RecordFromKeys(keys, v Value) Value {
	out := &Record{}
	for _, k := range List(keys) {
		out = out.set(k.(string), v)
	}
	return out
}

func RecordFromIterableWith(f, xs Value) Value {
	out := &Record{}
	for _, x := range List(xs) {
		pair := Apply(f, x).([]Value)
		out = out.set(pair[0].(string), pair[1])
	}
	return out
}

func RecordFromIterableBy(f, xs Value) Value {
	out := &Record{}
	for _, x := range List(xs) {
		out = out.set(Apply(f, x).(string), x)
	}
	return out
}

func RecordCollect(f, r Value) Value {
	x := record(r)
	out := make([]Value, len(x.keys))
	for i, k := range x.keys {
		out[i] = apply2(f, k, x.vals[i])
	}
	return out
}

func RecordHas(k, r Value) Value { return record(r).index(k.(string)) >= 0 }

func RecordGetKey(k, r Value) Value {
	x := record(r)
	if i := x.index(k.(string)); i >= 0 {
		return Some(x.vals[i])
	}
	return None
}

func RecordModify(k, f, r Value) Value {
	x := record(r)
	i := x.index(k.(string))
	if i < 0 {
		return None
	}
	return Some(x.set(k.(string), Apply(f, x.vals[i])))
}

func RecordReplace(k, v, r Value) Value {
	return RecordModify(k, Fn(func(Value) Value { return v }), r)
}

func RecordSet(k, v, r Value) Value { return record(r).set(k.(string), v) }

// without is `x` without the field `k`.
func (x *Record) without(k string) *Record {
	out := &Record{}
	for i, key := range x.keys {
		if key != k {
			out.keys = append(out.keys, key)
			out.vals = append(out.vals, x.vals[i])
		}
	}
	return out
}

func RecordRemoveKey(k, r Value) Value { return record(r).without(k.(string)) }

func RecordPop(k, r Value) Value {
	x := record(r)
	i := x.index(k.(string))
	if i < 0 {
		return None
	}
	return Some([]Value{x.vals[i], x.without(k.(string))})
}

// entries builds a record from each field `f` makes something of.
func entries(r Value, f func(k string, v Value) (string, Value, bool)) *Record {
	x, out := record(r), &Record{}
	for i, k := range x.keys {
		if key, v, ok := f(k, x.vals[i]); ok {
			out = out.set(key, v)
		}
	}
	return out
}

func RecordMap(f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) { return k, apply2(f, v, k), true })
}

func RecordMapKeys(f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) { return apply2(f, k, v).(string), v, true })
}

func RecordMapEntries(f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) {
		pair := apply2(f, v, k).([]Value)
		return pair[0].(string), pair[1], true
	})
}

func RecordFilter(f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) { return k, v, Bool(apply2(f, v, k)) })
}

func RecordFilterMap(f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) {
		res := apply2(f, v, k)
		if isOk(res) {
			return k, value(res), true
		}
		return "", nil, false
	})
}

func RecordPartition(f, r Value) Value {
	passes, fails := &Record{}, &Record{}
	x := record(r)
	for i, k := range x.keys {
		if res := apply2(f, x.vals[i], k); isOk(res) {
			passes = passes.set(k, value(res))
		} else {
			fails = fails.set(k, value(res))
		}
	}
	return []Value{passes, fails}
}

func RecordSeparate(r Value) Value {
	return RecordPartition(Curry2(func(v, _ Value) Value { return v }), r)
}

func RecordGetSomes(r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) {
		if isSome(v) {
			return k, value(v), true
		}
		return "", nil, false
	})
}

func recordResults(r Value, ok bool) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) {
		if isOk(v) == ok {
			return k, value(v), true
		}
		return "", nil, false
	})
}

func RecordGetFailures(r Value) Value  { return recordResults(r, false) }
func RecordGetSuccesses(r Value) Value { return recordResults(r, true) }

func RecordReduce(zero, f, r Value) Value {
	acc, x := zero, record(r)
	for i, k := range x.keys {
		acc = Apply(apply2(f, acc, x.vals[i]), k)
	}
	return acc
}

func RecordEvery(f, r Value) Value {
	x := record(r)
	for i, k := range x.keys {
		if !Bool(apply2(f, x.vals[i], k)) {
			return false
		}
	}
	return true
}

func RecordSome(f, r Value) Value {
	x := record(r)
	for i, k := range x.keys {
		if Bool(apply2(f, x.vals[i], k)) {
			return true
		}
	}
	return false
}

func RecordFindFirst(f, r Value) Value {
	x := record(r)
	for i, k := range x.keys {
		if Bool(apply2(f, x.vals[i], k)) {
			return Some([]Value{k, x.vals[i]})
		}
	}
	return None
}

// RecordUnion is `self`'s fields, combined with `that`'s by `f`, then the
// labels only `that` has.
func RecordUnion(that, f, self Value) Value {
	return comb(RecordReducerUnion(CombinerMake(f))).combine(self, that)
}

func RecordIntersection(that, f, self Value) Value {
	x, y := record(self), record(that)
	out := &Record{}
	for i, k := range x.keys {
		if j := y.index(k); j >= 0 {
			out = out.set(k, apply2(f, x.vals[i], y.vals[j]))
		}
	}
	return out
}

// RecordDifference is the labels only one of them has, `self`'s first.
func RecordDifference(that, self Value) Value {
	x, y := record(self), record(that)
	switch {
	case len(x.keys) == 0:
		return y
	case len(y.keys) == 0:
		return x
	}
	out := &Record{}
	for i, k := range x.keys {
		if y.index(k) < 0 {
			out = out.set(k, x.vals[i])
		}
	}
	for j, k := range y.keys {
		if x.index(k) < 0 {
			out = out.set(k, y.vals[j])
		}
	}
	return out
}

func subrecord(eq Value, self, that *Record) bool {
	for i, k := range self.keys {
		j := that.index(k)
		if j < 0 || !Bool(apply2(eq, self.vals[i], that.vals[j])) {
			return false
		}
	}
	return true
}

// RecordIsSubrecordBy is whether every field of `self` is in `that`.
func RecordIsSubrecordBy(eq, that, self Value) Value {
	return subrecord(eq, record(self), record(that))
}

func RecordEquivalence(eq, self, that Value) Value {
	return subrecord(eq, record(self), record(that)) && subrecord(eq, record(that), record(self))
}
