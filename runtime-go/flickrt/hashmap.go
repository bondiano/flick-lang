package flickrt

// The functions of `HashMap`: `set` and `union` take the new key and
// value, and callbacks get the value first, then the key.

func HashMapWith(w Value) Value             { return mapOf(empty(w)) }
func HashMapEmpty(fb *Fiber, d Value) Value { return HashMapWith(EqHashCanonical(fb, d)) }

func HashMapInsert(fb *Fiber, k, v, m Value) Value {
	t := hmap(m)
	return mapOf(t.set(fb, k, v, t.w.of(fb, k), false))
}

func HashMapRemove(fb *Fiber, k, m Value) Value { return mapOf(hmap(m).remove(fb, k)) }

func HashMapGet(fb *Fiber, k, m Value) Value {
	t := hmap(m)
	if v, ok := t.get(fb, k, t.w.of(fb, k)); ok {
		return Some(v)
	}
	return None
}

func HashMapSize(m Value) Value    { return hmap(m).size }
func HashMapIsEmpty(m Value) Value { return hmap(m).size == 0 }

func HashMapToList(m Value) Value {
	t := hmap(m)
	out := make([]Value, t.size)
	for i, e := range t.all() {
		out[i] = []Value{e.key, e.value}
	}
	return out
}

func fromEntries(fb *Fiber, t *table, entries Value) *table {
	for _, kv := range List(entries) {
		pair := kv.([]Value)
		t = t.set(fb, pair[0], pair[1], t.w.of(fb, pair[0]), false)
	}
	return t
}

// HashMapUnion is `a` with the entries of `b`, which win.
func HashMapUnion(fb *Fiber, a, b Value) Value {
	return mapOf(fromEntries(fb, hmap(a), HashMapToList(b)))
}

func HashMapRehash(fb *Fiber, w, m Value) Value {
	return mapOf(fromEntries(fb, empty(w), HashMapToList(m)))
}
func HashMapFromList(fb *Fiber, w, entries Value) Value {
	return mapOf(fromEntries(fb, empty(w), entries))
}

func HashMapHas(fb *Fiber, k, m Value) Value {
	t := hmap(m)
	_, ok := t.get(fb, k, t.w.of(fb, k))
	return ok
}

// HashMapGetHash and HashMapHasHash look `k` up by the hash given.
func HashMapGetHash(fb *Fiber, k, h, m Value) Value {
	if v, ok := hmap(m).get(fb, k, h.(int32)); ok {
		return Some(v)
	}
	return None
}

func HashMapHasHash(fb *Fiber, k, h, m Value) Value {
	_, ok := hmap(m).get(fb, k, h.(int32))
	return ok
}

func HashMapHasBy(fb *Fiber, f, m Value) Value { return HashMapSome(fb, f, m) }

func HashMapKeys(m Value) Value {
	t := hmap(m)
	out := make([]Value, t.size)
	for i, e := range t.all() {
		out[i] = e.key
	}
	return out
}

func HashMapValues(m Value) Value {
	t := hmap(m)
	out := make([]Value, t.size)
	for i, e := range t.all() {
		out[i] = e.value
	}
	return out
}

func modifyAt(fb *Fiber, t *table, k Value, h int32, f Value) *table {
	before := None
	if v, ok := t.get(fb, k, h); ok {
		before = Some(v)
	}
	after := Apply(fb, f, before).(*Data)
	switch {
	case after.Tag == "Some":
		return t.set(fb, k, after.Fields()[0], h, false)
	case before != None:
		return t.remove(fb, k)
	}
	return t
}

// HashMapModifyAt sets or removes `k` by what `f` makes of its value.
func HashMapModifyAt(fb *Fiber, k, f, m Value) Value {
	t := hmap(m)
	return mapOf(modifyAt(fb, t, k, t.w.of(fb, k), f))
}

func HashMapModifyHash(fb *Fiber, k, h, f, m Value) Value {
	return mapOf(modifyAt(fb, hmap(m), k, h.(int32), f))
}

func HashMapModify(fb *Fiber, k, f, m Value) Value {
	t := hmap(m)
	h := t.w.of(fb, k)
	if v, ok := t.get(fb, k, h); ok {
		return mapOf(t.set(fb, k, Apply(fb, f, v), h, false))
	}
	return m
}

func HashMapRemoveMany(fb *Fiber, ks, m Value) Value {
	t := hmap(m)
	for _, k := range List(ks) {
		t = t.remove(fb, k)
	}
	return mapOf(t)
}

func HashMapSetMany(fb *Fiber, entries, m Value) Value {
	return mapOf(fromEntries(fb, hmap(m), entries))
}

// mapped is `t` with each entry's value as `f` makes it, dropping those it
// makes nothing of.
func mapped(t *table, f func(e entry) (Value, bool)) *table {
	return t.kept(func(e entry) (entry, bool) {
		v, ok := f(e)
		return entry{e.key, v, e.hash}, ok
	})
}

func HashMapMap(fb *Fiber, f, m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) { return Apply2(fb, f, e.value, e.key), true }))
}

// HashMapFlatMap is the union of what `f` makes of each entry, in order.
func HashMapFlatMap(fb *Fiber, f, m Value) Value {
	t := hmap(m)
	out := empty(t.w)
	for _, e := range t.all() {
		out = fromEntries(fb, out, HashMapToList(Apply2(fb, f, e.value, e.key)))
	}
	return mapOf(out)
}

func HashMapReduce(fb *Fiber, zero, f, m Value) Value {
	acc := zero
	for _, e := range hmap(m).all() {
		acc = Apply(fb, Apply2(fb, f, acc, e.value), e.key)
	}
	return acc
}

func HashMapFilter(fb *Fiber, f, m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) {
		return e.value, Bool(Apply2(fb, f, e.value, e.key))
	}))
}

func HashMapCompact(m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) {
		o := e.value.(*Data)
		if o.Tag == "Some" {
			return o.Fields()[0], true
		}
		return nil, false
	}))
}

func HashMapFilterMap(fb *Fiber, f, m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) {
		r := Apply2(fb, f, e.value, e.key).(*Data)
		if r.Tag == "Success" {
			return r.Fields()[0], true
		}
		return nil, false
	}))
}

func HashMapFindFirst(fb *Fiber, f, m Value) Value {
	for _, e := range hmap(m).all() {
		if Bool(Apply2(fb, f, e.value, e.key)) {
			return Some([]Value{e.key, e.value})
		}
	}
	return None
}

func HashMapSome(fb *Fiber, f, m Value) Value {
	for _, e := range hmap(m).all() {
		if Bool(Apply2(fb, f, e.value, e.key)) {
			return true
		}
	}
	return false
}

func HashMapEvery(fb *Fiber, f, m Value) Value {
	for _, e := range hmap(m).all() {
		if !Bool(Apply2(fb, f, e.value, e.key)) {
			return false
		}
	}
	return true
}
