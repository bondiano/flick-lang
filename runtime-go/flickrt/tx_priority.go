package flickrt

import "slices"

// TxPriorityQueue is Effect 4's: a TxRef of a Chunk kept sorted by an
// order, equal elements in the order they came.
type TxPriorityQueue struct {
	ref   *TxRef
	order func(*Fiber, Value, Value) int
}

func txPriorityOf(v Value) *TxPriorityQueue { return v.(*TxPriorityQueue) }

func newTxPriorityQueue(fb *Fiber, ord Value) *TxPriorityQueue {
	return &TxPriorityQueue{order: orderOf(Method(fb, ord, "compare"))}
}

func TxPriorityQueueEmpty(fb *Fiber, ord Value) Value {
	q := newTxPriorityQueue(fb, ord)
	q.ref = newTxRef(ChunkEmpty)
	return q
}

// TxPriorityQueueFromIterable sorts the elements, stably, as JS's sort.
func TxPriorityQueueFromIterable(fb *Fiber, ord, xs Value) Value {
	q := newTxPriorityQueue(fb, ord)
	q.ref = newTxRef(&Chunk{q.sorted(fb, List(xs))})
	return q
}

func (q *TxPriorityQueue) sorted(fb *Fiber, xs []Value) []Value {
	out := slices.Clone(xs)
	slices.SortStableFunc(out, func(a, b Value) int { return q.order(fb, a, b) })
	return out
}

// mergeSorted is Effect's: the sorted `values` merged into the sorted
// `arr`, each after the elements equal to it, by galloping then binary
// search from where the previous one went.
func (q *TxPriorityQueue) mergeSorted(fb *Fiber, arr, values []Value) Value {
	out := make([]Value, 0, len(arr)+len(values))
	i := 0
	for _, value := range values {
		lo, hi := i, i
		for step := 1; hi < len(arr) && q.order(fb, arr[hi], value) <= 0; step *= 2 {
			lo = hi + 1
			hi += step
		}
		hi = min(hi, len(arr))
		for lo < hi {
			mid := int(uint(lo+hi) >> 1)
			if q.order(fb, arr[mid], value) <= 0 {
				lo = mid + 1
			} else {
				hi = mid
			}
		}
		out = append(append(out, arr[i:lo]...), value)
		i = lo
	}
	return &Chunk{append(out, arr[i:]...)}
}

func txPriorityRead(fb *Fiber, q Value, f func([]Value) Value) Value {
	return txRead(fb, txPriorityOf(q).ref, func(v Value) Value { return f(items(v)) })
}

func TxPriorityQueueSize(fb *Fiber, q Value) Value {
	return txPriorityRead(fb, q, func(xs []Value) Value { return int32(len(xs)) })
}

func TxPriorityQueueIsEmpty(fb *Fiber, q Value) Value {
	return txPriorityRead(fb, q, func(xs []Value) Value { return len(xs) == 0 })
}

func TxPriorityQueueIsNonEmpty(fb *Fiber, q Value) Value {
	return txPriorityRead(fb, q, func(xs []Value) Value { return len(xs) > 0 })
}

func TxPriorityQueuePeekOption(fb *Fiber, q Value) Value {
	return txPriorityRead(fb, q, func(xs []Value) Value { return ListGet(int32(0), xs) })
}

// TxPriorityQueuePeek is the least element, retrying while there is none.
func TxPriorityQueuePeek(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		xs := items(t.get(fb, txPriorityOf(q).ref))
		if len(xs) == 0 {
			return t.retryNow(fb)
		}
		return xs[0]
	})
}

func TxPriorityQueueOffer(fb *Fiber, x, q Value) Value {
	p := txPriorityOf(q)
	return txUpdate(fb, p.ref, func(v Value) Value { return p.mergeSorted(fb, items(v), []Value{x}) })
}

func TxPriorityQueueOfferAll(fb *Fiber, xs, q Value) Value {
	p := txPriorityOf(q)
	sorted := p.sorted(fb, List(xs))
	return txUpdate(fb, p.ref, func(v Value) Value { return p.mergeSorted(fb, items(v), sorted) })
}

// TxPriorityQueueTake takes the least element, retrying while there is
// none.
func TxPriorityQueueTake(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		r := txPriorityOf(q).ref
		xs := items(t.get(fb, r))
		if len(xs) == 0 {
			return t.retryNow(fb)
		}
		t.set(fb, r, &Chunk{xs[1:]})
		return xs[0]
	})
}

func TxPriorityQueueTakeAll(fb *Fiber, q Value) Value {
	return txModify(fb, txPriorityOf(q).ref, func(v Value) (Value, Value) { return items(v), ChunkEmpty })
}

func TxPriorityQueueTakeOption(fb *Fiber, q Value) Value {
	return txModify(fb, txPriorityOf(q).ref, func(v Value) (Value, Value) {
		if xs := items(v); len(xs) > 0 {
			return Some(xs[0]), &Chunk{xs[1:]}
		}
		return None, v
	})
}

func TxPriorityQueueTakeUpTo(fb *Fiber, n, q Value) Value {
	return txModify(fb, txPriorityOf(q).ref, func(v Value) (Value, Value) {
		return items(ChunkTake(n, v)), ChunkDrop(n, v)
	})
}

func TxPriorityQueueRemoveIf(fb *Fiber, f, q Value) Value {
	return txUpdate(fb, txPriorityOf(q).ref, func(v Value) Value {
		return chunkOf(ListFilter(fb, Fn(func(fb *Fiber, x Value) Value { return !Bool(Apply(fb, f, x)) }), items(v)))
	})
}

func TxPriorityQueueRetainIf(fb *Fiber, f, q Value) Value {
	return txUpdate(fb, txPriorityOf(q).ref, func(v Value) Value { return ChunkFilter(fb, f, v) })
}

func TxPriorityQueueToArray(fb *Fiber, q Value) Value {
	return txPriorityRead(fb, q, func(xs []Value) Value { return xs })
}
