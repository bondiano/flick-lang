package flickrt

// TxHashMap and TxHashSet: a TxRef of a HashMap or HashSet (hash.go),
// whose witness compares the keys; each change puts a new map or set in
// the ref.

// TxHashMap is a TxRef of a HashMap.
type TxHashMap struct{ ref *TxRef }

func txMapOf(v Value) *TxRef     { return v.(*TxHashMap).ref }
func newTxHashMap(m Value) Value { return &TxHashMap{newTxRef(m)} }

func TxHashMapEmpty(w Value) Value { return newTxHashMap(HashMapWith(w)) }

func TxHashMapMake(fb *Fiber, w, entries Value) Value {
	return newTxHashMap(HashMapFromList(fb, w, entries))
}

func TxHashMapGet(fb *Fiber, k, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapGet(fb, k, v) })
}

func TxHashMapSet(fb *Fiber, k, x, m Value) Value {
	return txUpdate(fb, txMapOf(m), func(v Value) Value { return HashMapInsert(fb, k, x, v) })
}

func TxHashMapHas(fb *Fiber, k, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapHas(fb, k, v) })
}

// TxHashMapRemove removes `k`; whether it was there.
func TxHashMapRemove(fb *Fiber, k, m Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		r := txMapOf(m)
		current := t.get(fb, r)
		existed := HashMapHas(fb, k, current)
		if Bool(existed) {
			t.set(fb, r, HashMapRemove(fb, k, current))
		}
		return existed
	})
}

func TxHashMapClear(fb *Fiber, m Value) Value {
	return txUpdate(fb, txMapOf(m), func(v Value) Value { return HashMapWith(hmap(v).w) })
}

func TxHashMapSize(fb *Fiber, m Value) Value { return txRead(fb, txMapOf(m), HashMapSize) }

func TxHashMapIsEmpty(fb *Fiber, m Value) Value { return txRead(fb, txMapOf(m), HashMapIsEmpty) }

func TxHashMapIsNonEmpty(fb *Fiber, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return hmap(v).size > 0 })
}

// TxHashMapModify sets `k` to `f` of its value, if it has one; gives the
// value it had.
func TxHashMapModify(fb *Fiber, k, f, m Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		r := txMapOf(m)
		current := t.get(fb, r)
		old := HashMapGet(fb, k, current)
		if x, ok := some(old); ok {
			t.set(fb, r, HashMapInsert(fb, k, Apply(fb, f, x), current))
		}
		return old
	})
}

// TxHashMapModifyAt sets or removes `k` by what `f` makes of its value.
func TxHashMapModifyAt(fb *Fiber, k, f, m Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		r := txMapOf(m)
		current := t.get(fb, r)
		old := HashMapGet(fb, k, current)
		if x, ok := some(Apply(fb, f, old)); ok {
			t.set(fb, r, HashMapInsert(fb, k, x, current))
		} else if _, had := some(old); had {
			t.set(fb, r, HashMapRemove(fb, k, current))
		}
		return Unit
	})
}

func TxHashMapKeys(fb *Fiber, m Value) Value     { return txRead(fb, txMapOf(m), HashMapKeys) }
func TxHashMapValues(fb *Fiber, m Value) Value   { return txRead(fb, txMapOf(m), HashMapValues) }
func TxHashMapEntries(fb *Fiber, m Value) Value  { return txRead(fb, txMapOf(m), HashMapToList) }
func TxHashMapSnapshot(fb *Fiber, m Value) Value { return txRead(fb, txMapOf(m), Identity) }

func TxHashMapUnion(fb *Fiber, other, m Value) Value {
	return txUpdate(fb, txMapOf(m), func(v Value) Value { return HashMapUnion(fb, v, other) })
}

func TxHashMapRemoveMany(fb *Fiber, ks, m Value) Value {
	return txUpdate(fb, txMapOf(m), func(v Value) Value { return HashMapRemoveMany(fb, ks, v) })
}

func TxHashMapSetMany(fb *Fiber, entries, m Value) Value {
	return txUpdate(fb, txMapOf(m), func(v Value) Value { return HashMapSetMany(fb, entries, v) })
}

func TxHashMapGetHash(fb *Fiber, k, h, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapGetHash(fb, k, h, v) })
}

func TxHashMapHasHash(fb *Fiber, k, h, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapHasHash(fb, k, h, v) })
}

// derivedMap is a new TxHashMap of what `f` makes of the map of `m`.
func derivedMap(fb *Fiber, m Value, f func(Value) Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return newTxHashMap(f(v)) })
}

func TxHashMapMap(fb *Fiber, f, m Value) Value {
	return derivedMap(fb, m, func(v Value) Value { return HashMapMap(fb, f, v) })
}

func TxHashMapFilter(fb *Fiber, f, m Value) Value {
	return derivedMap(fb, m, func(v Value) Value { return HashMapFilter(fb, f, v) })
}

func TxHashMapFilterMap(fb *Fiber, f, m Value) Value {
	return derivedMap(fb, m, func(v Value) Value { return HashMapFilterMap(fb, f, v) })
}

func TxHashMapCompact(fb *Fiber, m Value) Value { return derivedMap(fb, m, HashMapCompact) }

// TxHashMapFlatMap is a new map of the entries of the maps `f` makes of
// each entry, in order.
func TxHashMapFlatMap(fb *Fiber, f, m Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		current := t.get(fb, txMapOf(m))
		result := &TxHashMap{newTxRef(HashMapWith(hmap(current).w))}
		for _, e := range hmap(current).all() {
			entries := HashMapToList(t.get(fb, txMapOf(Run(fb, Apply2(fb, f, e.value, e.key)))))
			t.update(fb, result.ref, func(v Value) Value { return HashMapSetMany(fb, entries, v) })
		}
		return result
	})
}

func TxHashMapReduce(fb *Fiber, zero, f, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapReduce(fb, zero, f, v) })
}

func TxHashMapSome(fb *Fiber, f, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapSome(fb, f, v) })
}

func TxHashMapEvery(fb *Fiber, f, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapEvery(fb, f, v) })
}

func TxHashMapFindFirst(fb *Fiber, f, m Value) Value {
	return txRead(fb, txMapOf(m), func(v Value) Value { return HashMapFindFirst(fb, f, v) })
}

// TxHashMapForEach runs `f` of each entry, in order.
func TxHashMapForEach(fb *Fiber, f, m Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		for _, e := range hmap(t.get(fb, txMapOf(m))).all() {
			Run(fb, Apply2(fb, f, e.value, e.key))
		}
		return Unit
	})
}

// TxHashSet is a TxRef of a HashSet.
type TxHashSet struct{ ref *TxRef }

func txSetOf(v Value) *TxRef     { return v.(*TxHashSet).ref }
func newTxHashSet(s Value) Value { return &TxHashSet{newTxRef(s)} }

func TxHashSetEmpty(w Value) Value { return newTxHashSet(HashSetWith(w)) }

func TxHashSetMake(fb *Fiber, w, xs Value) Value { return newTxHashSet(HashSetFromList(fb, w, xs)) }

func TxHashSetFromHashSet(s Value) Value { return newTxHashSet(s) }

func TxHashSetAdd(fb *Fiber, x, s Value) Value {
	return txUpdate(fb, txSetOf(s), func(v Value) Value { return HashSetInsert(fb, x, v) })
}

// TxHashSetRemove removes `x`; whether it was there.
func TxHashSetRemove(fb *Fiber, x, s Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		r := txSetOf(s)
		current := t.get(fb, r)
		existed := HashSetMember(fb, x, current)
		if Bool(existed) {
			t.set(fb, r, HashSetRemove(fb, x, current))
		}
		return existed
	})
}

func TxHashSetHas(fb *Fiber, x, s Value) Value {
	return txRead(fb, txSetOf(s), func(v Value) Value { return HashSetMember(fb, x, v) })
}

func TxHashSetSize(fb *Fiber, s Value) Value    { return txRead(fb, txSetOf(s), HashSetSize) }
func TxHashSetIsEmpty(fb *Fiber, s Value) Value { return txRead(fb, txSetOf(s), HashSetIsEmpty) }

func TxHashSetIsNonEmpty(fb *Fiber, s Value) Value {
	return txRead(fb, txSetOf(s), func(v Value) Value { return set(v).size > 0 })
}

func TxHashSetClear(fb *Fiber, s Value) Value {
	return txUpdate(fb, txSetOf(s), func(v Value) Value { return HashSetWith(set(v).w) })
}

// bothSets is `f` of the sets of `self` and `that`, read in one
// transaction.
func bothSets(fb *Fiber, that, self Value, f func(a, b Value) Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		a := t.get(fb, txSetOf(self))
		return f(a, t.get(fb, txSetOf(that)))
	})
}

func TxHashSetUnion(fb *Fiber, that, self Value) Value {
	return bothSets(fb, that, self, func(a, b Value) Value { return newTxHashSet(HashSetUnion(fb, a, b)) })
}

func TxHashSetIntersection(fb *Fiber, that, self Value) Value {
	return bothSets(fb, that, self, func(a, b Value) Value {
		return newTxHashSet(HashSetIntersection(fb, b, a))
	})
}

func TxHashSetDifference(fb *Fiber, that, self Value) Value {
	return bothSets(fb, that, self, func(a, b Value) Value { return newTxHashSet(HashSetDifference(fb, b, a)) })
}

func TxHashSetIsSubset(fb *Fiber, that, self Value) Value {
	return bothSets(fb, that, self, func(a, b Value) Value { return HashSetIsSubset(fb, b, a) })
}

func TxHashSetSome(fb *Fiber, f, s Value) Value {
	return txRead(fb, txSetOf(s), func(v Value) Value { return HashSetSome(fb, f, v) })
}

func TxHashSetEvery(fb *Fiber, f, s Value) Value {
	return txRead(fb, txSetOf(s), func(v Value) Value { return HashSetEvery(fb, f, v) })
}

func TxHashSetMap(fb *Fiber, w, f, s Value) Value {
	return txRead(fb, txSetOf(s), func(v Value) Value { return newTxHashSet(HashSetMap(fb, w, f, v)) })
}

func TxHashSetFilter(fb *Fiber, f, s Value) Value {
	return txRead(fb, txSetOf(s), func(v Value) Value { return newTxHashSet(HashSetFilter(fb, f, v)) })
}

func TxHashSetReduce(fb *Fiber, zero, f, s Value) Value {
	return txRead(fb, txSetOf(s), func(v Value) Value { return HashSetReduce(fb, zero, f, v) })
}

func TxHashSetToHashSet(fb *Fiber, s Value) Value { return txRead(fb, txSetOf(s), Identity) }
