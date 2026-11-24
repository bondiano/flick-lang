package flickrt

// The functions of `Record`: Flick's row primitives and the functions on
// dictionaries, in the order Flick passes the arguments (data-last, the
// record at the end). Callbacks get the value, then the key, except
// `collect` and `map_keys`.

// RecordLabels is the labels of a record, sorted.
func RecordLabels(_, r Value) Value {
	keys := sortedViewKeys(AsRecord(r))
	out := make([]Value, len(keys))
	for i, k := range keys {
		out[i] = k
	}
	return out
}

// RecordLabel is `Record.label`, which the compiler writes as the label's
// string: the binding itself is JS's `undefined`.
var RecordLabel Value = Unit

// RecordGet is the field `label` of `r`; the dictionary is erased.
func RecordGet(_, label, r Value) Value { return AsRecord(r).Get(label.(string)) }

// RecordExtend is `r` with the field `label`, in canonical label order.
func RecordExtend(_, _, label, v, r Value) Value {
	x := AsRecord(r)
	out := newRecord(x.Len() + 1)
	for i, k := range x.Keys() {
		out.putCanonical(k, x.At(i))
	}
	out.putCanonical(label.(string), v)
	return out
}

// RecordRemove is `r` without the field `label`, in canonical label order.
func RecordRemove(_, label, r Value) Value {
	x := AsRecord(r)
	drop := label.(string)
	out := newRecord(x.Len())
	for i, k := range x.Keys() {
		if k != drop {
			out.putCanonical(k, x.At(i))
		}
	}
	return out
}

// RecordMergeDisjoint merges two disjoint structural records in canonical label order.
func RecordMergeDisjoint(_, a, b Value) Value {
	x, y := AsRecord(a), AsRecord(b)
	out := newRecord(x.Len() + y.Len())
	for i, k := range x.Keys() {
		out.putCanonical(k, x.At(i))
	}
	for i, k := range y.Keys() {
		out.putCanonical(k, y.At(i))
	}
	return out
}

// RecordProject is the fields of `r` the dictionary's labels name, in
// canonical label order.
func RecordProject(labels, r Value) Value {
	x := AsRecord(r)
	ls := List(labels)
	out := newRecord(len(ls))
	for _, l := range ls {
		name := l.(string)
		out.putCanonical(name, x.Get(name))
	}
	return out
}

func RecordEmpty() Value { return &Record{} }

func RecordIsEmpty(r Value) Value { return AsRecord(r).Len() == 0 }
func RecordSize(r Value) Value    { return int32(AsRecord(r).Len()) }

func RecordKeys(r Value) Value {
	x := AsRecord(r)
	keys := x.Keys()
	out := make([]Value, len(keys))
	for i, k := range keys {
		out[i] = k
	}
	return out
}

func RecordValues(r Value) Value {
	x := AsRecord(r)
	n := x.Len()
	out := make([]Value, n)
	for i := range n {
		out[i] = x.At(i)
	}
	return out
}

func RecordSingleton(k, v Value) Value { return (&Record{}).set(k.(string), v) }

func RecordFromKeys(keys, v Value) Value {
	out := &Record{}
	for _, k := range List(keys) {
		out = out.set(k.(string), v)
	}
	return out
}

func RecordFromIterableWith(fb *Fiber, f, xs Value) Value {
	out := &Record{}
	for _, x := range List(xs) {
		pair := Apply(fb, f, x).([]Value)
		out = out.set(pair[0].(string), pair[1])
	}
	return out
}

func RecordFromIterableBy(fb *Fiber, f, xs Value) Value {
	out := &Record{}
	for _, x := range List(xs) {
		out = out.set(Apply(fb, f, x).(string), x)
	}
	return out
}

func RecordCollect(fb *Fiber, f, r Value) Value {
	x := AsRecord(r)
	keys := x.Keys()
	out := make([]Value, len(keys))
	for i, k := range keys {
		out[i] = Apply2(fb, f, k, x.At(i))
	}
	return out
}

func RecordHas(k, r Value) Value {
	_, ok := AsRecord(r).Lookup(k.(string))
	return ok
}

func RecordGetKey(k, r Value) Value {
	if v, ok := AsRecord(r).Lookup(k.(string)); ok {
		return Some(v)
	}
	return None
}

func RecordModify(fb *Fiber, k, f, r Value) Value {
	x := AsRecord(r)
	key := k.(string)
	v, ok := x.Lookup(key)
	if !ok {
		return None
	}
	return Some(copiedView(r).set(key, Apply(fb, f, v)))
}

func RecordReplace(fb *Fiber, k, v, r Value) Value {
	return RecordModify(fb, k, Fn(func(fb *Fiber, _ Value) Value { return v }), r)
}

func RecordSet(k, v, r Value) Value { return copiedView(r).set(k.(string), v) }

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

func withoutView(x RecordView, k string) *Record {
	out := &Record{}
	for i, key := range x.Keys() {
		if key != k {
			out.keys = append(out.keys, key)
			out.vals = append(out.vals, x.At(i))
		}
	}
	return out
}

func RecordRemoveKey(k, r Value) Value { return withoutView(AsRecord(r), k.(string)) }

func RecordPop(k, r Value) Value {
	x := AsRecord(r)
	key := k.(string)
	v, ok := x.Lookup(key)
	if !ok {
		return None
	}
	return Some([]Value{v, withoutView(x, key)})
}

// entries builds a record from each field `f` makes something of.
func entries(r Value, f func(k string, v Value) (string, Value, bool)) *Record {
	x, out := AsRecord(r), &Record{}
	for i, k := range x.Keys() {
		if key, v, ok := f(k, x.At(i)); ok {
			out = out.set(key, v)
		}
	}
	return out
}

func RecordMap(fb *Fiber, f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) { return k, Apply2(fb, f, v, k), true })
}

func RecordMapKeys(fb *Fiber, f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) { return Apply2(fb, f, k, v).(string), v, true })
}

func RecordMapEntries(fb *Fiber, f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) {
		pair := Apply2(fb, f, v, k).([]Value)
		return pair[0].(string), pair[1], true
	})
}

func RecordFilter(fb *Fiber, f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) { return k, v, Bool(Apply2(fb, f, v, k)) })
}

func RecordFilterMap(fb *Fiber, f, r Value) Value {
	return entries(r, func(k string, v Value) (string, Value, bool) {
		res := Apply2(fb, f, v, k)
		if isOk(res) {
			return k, value(res), true
		}
		return "", nil, false
	})
}

func RecordPartition(fb *Fiber, f, r Value) Value {
	passes, fails := &Record{}, &Record{}
	x := AsRecord(r)
	for i, k := range x.Keys() {
		if res := Apply2(fb, f, x.At(i), k); isOk(res) {
			passes = passes.set(k, value(res))
		} else {
			fails = fails.set(k, value(res))
		}
	}
	return []Value{passes, fails}
}

func RecordSeparate(fb *Fiber, r Value) Value {
	return RecordPartition(fb, Curry2(func(fb *Fiber, v, _ Value) Value { return v }), r)
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

func RecordReduce(fb *Fiber, zero, f, r Value) Value {
	acc, x := zero, AsRecord(r)
	for i, k := range x.Keys() {
		acc = Apply(fb, Apply2(fb, f, acc, x.At(i)), k)
	}
	return acc
}

func RecordEvery(fb *Fiber, f, r Value) Value {
	x := AsRecord(r)
	for i, k := range x.Keys() {
		if !Bool(Apply2(fb, f, x.At(i), k)) {
			return false
		}
	}
	return true
}

func RecordSome(fb *Fiber, f, r Value) Value {
	x := AsRecord(r)
	for i, k := range x.Keys() {
		if Bool(Apply2(fb, f, x.At(i), k)) {
			return true
		}
	}
	return false
}

func RecordFindFirst(fb *Fiber, f, r Value) Value {
	x := AsRecord(r)
	for i, k := range x.Keys() {
		v := x.At(i)
		if Bool(Apply2(fb, f, v, k)) {
			return Some([]Value{k, v})
		}
	}
	return None
}

// RecordUnion is `self`'s fields, combined with `that`'s by `f`, then the
// labels only `that` has.
func RecordUnion(fb *Fiber, that, f, self Value) Value {
	return comb(RecordReducerUnion(CombinerMake(f))).combine(fb, self, that)
}

func RecordIntersection(fb *Fiber, that, f, self Value) Value {
	x, y := AsRecord(self), AsRecord(that)
	out := &Record{}
	for i, k := range x.Keys() {
		if yv, ok := y.Lookup(k); ok {
			out = out.set(k, Apply2(fb, f, x.At(i), yv))
		}
	}
	return out
}

// RecordDifference is the labels only one of them has, `self`'s first.
func RecordDifference(that, self Value) Value {
	x, y := AsRecord(self), AsRecord(that)
	switch {
	case x.Len() == 0:
		return that
	case y.Len() == 0:
		return self
	}
	out := &Record{}
	for i, k := range x.Keys() {
		if _, ok := y.Lookup(k); !ok {
			out = out.set(k, x.At(i))
		}
	}
	for j, k := range y.Keys() {
		if _, ok := x.Lookup(k); !ok {
			out = out.set(k, y.At(j))
		}
	}
	return out
}

func subrecord(fb *Fiber, eq Value, self, that RecordView) bool {
	for i, k := range self.Keys() {
		yv, ok := that.Lookup(k)
		if !ok || !Bool(Apply2(fb, eq, self.At(i), yv)) {
			return false
		}
	}
	return true
}

// RecordIsSubrecordBy is whether every field of `self` is in `that`.
func RecordIsSubrecordBy(fb *Fiber, eq, that, self Value) Value {
	return subrecord(fb, eq, AsRecord(self), AsRecord(that))
}

func RecordEquivalence(fb *Fiber, eq, self, that Value) Value {
	a, b := AsRecord(self), AsRecord(that)
	return subrecord(fb, eq, a, b) && subrecord(fb, eq, b, a)
}
