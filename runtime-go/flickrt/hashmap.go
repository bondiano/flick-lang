package flickrt

// The functions of `HashMap`, as Effect's HashMap defines them: `set` and
// `union` take the new key and value, and callbacks get the value first,
// then the key.

func HashMapWith(w Value) Value  { return mapOf(empty(w)) }
func HashMapEmpty(d Value) Value { return HashMapWith(EqHashCanonical(d)) }

func HashMapInsert(k, v, m Value) Value {
	t := hmap(m)
	return mapOf(t.set(k, v, t.w.of(k), false))
}

func HashMapRemove(k, m Value) Value { return mapOf(hmap(m).remove(k)) }

func HashMapGet(k, m Value) Value {
	t := hmap(m)
	if v, ok := t.get(k, t.w.of(k)); ok {
		return Some(v)
	}
	return None
}

func HashMapSize(m Value) Value    { return int32(len(hmap(m).entries)) }
func HashMapIsEmpty(m Value) Value { return len(hmap(m).entries) == 0 }

func HashMapToList(m Value) Value {
	t := hmap(m)
	out := make([]Value, len(t.entries))
	for i, e := range t.entries {
		out[i] = []Value{e.key, e.value}
	}
	return out
}

func fromEntries(t *table, entries Value) *table {
	for _, kv := range List(entries) {
		pair := kv.([]Value)
		t = t.set(pair[0], pair[1], t.w.of(pair[0]), false)
	}
	return t
}

// HashMapUnion is `a` with the entries of `b`, which win.
func HashMapUnion(a, b Value) Value {
	return mapOf(fromEntries(hmap(a), HashMapToList(b)))
}

func HashMapRehash(w, m Value) Value { return mapOf(fromEntries(empty(w), HashMapToList(m))) }
func HashMapFromList(w, entries Value) Value {
	return mapOf(fromEntries(empty(w), entries))
}

func HashMapHas(k, m Value) Value {
	t := hmap(m)
	_, ok := t.get(k, t.w.of(k))
	return ok
}

// HashMapGetHash and HashMapHasHash look `k` up by the hash given.
func HashMapGetHash(k, h, m Value) Value {
	if v, ok := hmap(m).get(k, h.(int32)); ok {
		return Some(v)
	}
	return None
}

func HashMapHasHash(k, h, m Value) Value {
	_, ok := hmap(m).get(k, h.(int32))
	return ok
}

func HashMapHasBy(f, m Value) Value { return HashMapSome(f, m) }

func HashMapKeys(m Value) Value {
	t := hmap(m)
	out := make([]Value, len(t.entries))
	for i, e := range t.entries {
		out[i] = e.key
	}
	return out
}

func HashMapValues(m Value) Value {
	t := hmap(m)
	out := make([]Value, len(t.entries))
	for i, e := range t.entries {
		out[i] = e.value
	}
	return out
}

func modifyAt(t *table, k Value, h int32, f Value) *table {
	before := None
	if v, ok := t.get(k, h); ok {
		before = Some(v)
	}
	after := Apply(f, before).(*Data)
	switch {
	case after.Tag == "Some":
		return t.set(k, after.Fields[0], h, false)
	case before != None:
		return t.remove(k)
	}
	return t
}

// HashMapModifyAt sets or removes `k` by what `f` makes of its value.
func HashMapModifyAt(k, f, m Value) Value {
	t := hmap(m)
	return mapOf(modifyAt(t, k, t.w.of(k), f))
}

func HashMapModifyHash(k, h, f, m Value) Value {
	return mapOf(modifyAt(hmap(m), k, h.(int32), f))
}

func HashMapModify(k, f, m Value) Value {
	t := hmap(m)
	h := t.w.of(k)
	if v, ok := t.get(k, h); ok {
		return mapOf(t.set(k, Apply(f, v), h, false))
	}
	return m
}

func HashMapRemoveMany(ks, m Value) Value {
	t := hmap(m)
	for _, k := range List(ks) {
		t = t.remove(k)
	}
	return mapOf(t)
}

func HashMapSetMany(entries, m Value) Value { return mapOf(fromEntries(hmap(m), entries)) }

// mapped is `t` with each entry's value as `f` makes it, dropping those it
// makes nothing of.
func mapped(t *table, f func(e entry) (Value, bool)) *table {
	out := &table{w: t.w, next: t.next}
	for _, e := range t.entries {
		if v, ok := f(e); ok {
			out.entries = append(out.entries, entry{e.key, v, e.hash, e.order})
		}
	}
	return out
}

func HashMapMap(f, m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) { return apply2(f, e.value, e.key), true }))
}

// HashMapFlatMap is the union of what `f` makes of each entry, in order.
func HashMapFlatMap(f, m Value) Value {
	t := hmap(m)
	out := empty(t.w)
	for _, e := range t.entries {
		out = fromEntries(out, HashMapToList(apply2(f, e.value, e.key)))
	}
	return mapOf(out)
}

func HashMapReduce(zero, f, m Value) Value {
	acc := zero
	for _, e := range hmap(m).entries {
		acc = Apply(apply2(f, acc, e.value), e.key)
	}
	return acc
}

func HashMapFilter(f, m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) {
		return e.value, Bool(apply2(f, e.value, e.key))
	}))
}

func HashMapCompact(m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) {
		o := e.value.(*Data)
		if o.Tag == "Some" {
			return o.Fields[0], true
		}
		return nil, false
	}))
}

func HashMapFilterMap(f, m Value) Value {
	return mapOf(mapped(hmap(m), func(e entry) (Value, bool) {
		r := apply2(f, e.value, e.key).(*Data)
		if r.Tag == "Success" {
			return r.Fields[0], true
		}
		return nil, false
	}))
}

func HashMapFindFirst(f, m Value) Value {
	for _, e := range hmap(m).entries {
		if Bool(apply2(f, e.value, e.key)) {
			return Some([]Value{e.key, e.value})
		}
	}
	return None
}

func HashMapSome(f, m Value) Value {
	for _, e := range hmap(m).entries {
		if Bool(apply2(f, e.value, e.key)) {
			return true
		}
	}
	return false
}

func HashMapEvery(f, m Value) Value {
	for _, e := range hmap(m).entries {
		if !Bool(apply2(f, e.value, e.key)) {
			return false
		}
	}
	return true
}
