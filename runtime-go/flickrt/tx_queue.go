package flickrt

// TxQueue is a TxRef of the elements (a Chunk) and a TxRef of the state,
// open, closing with a cause, or done with it. The strategies and states
// are those of Queue (queue.go); unbounded is a suspending queue of
// unboundedCapacity.
type TxQueue struct {
	strategy queueStrategy
	capacity int
	items    *TxRef // a *Chunk
	state    *TxRef // a *txQueueState
}

// txQueueState is the state of a TxQueue; never mutated.
type txQueueState struct {
	tag   queueState
	cause *Cause
}

var txQueueOpen = &txQueueState{}

func txQueueOf(v Value) *TxQueue { return v.(*TxQueue) }

func newTxQueue(strategy queueStrategy, capacity int) *TxQueue {
	return &TxQueue{strategy, capacity, newTxRef(ChunkEmpty), newTxRef(txQueueOpen)}
}

func TxQueueBounded(n Value) Value  { return newTxQueue(queueSuspend, int(n.(int32))) }
func TxQueueUnbounded() Value       { return newTxQueue(queueSuspend, unboundedCapacity) }
func TxQueueDropping(n Value) Value { return newTxQueue(queueDropping, int(n.(int32))) }
func TxQueueSliding(n Value) Value  { return newTxQueue(queueSliding, int(n.(int32))) }

func (q *TxQueue) stateIn(fb *Fiber, t *tx) *txQueueState {
	return t.get(fb, q.state).(*txQueueState)
}

func (q *TxQueue) itemsIn(fb *Fiber, t *tx) []Value { return items(t.get(fb, q.items)) }

// end sets the state done with the cause of `s`.
func (q *TxQueue) end(fb *Fiber, t *tx, s *txQueueState) {
	t.set(fb, q.state, &txQueueState{queueDone, s.cause})
}

// rest keeps the elements of `xs` from `n` on, ending a closing queue
// they leave empty.
func (q *TxQueue) rest(fb *Fiber, t *tx, s *txQueueState, xs []Value, n int) {
	t.set(fb, q.items, &Chunk{xs[n:]})
	if s.tag == queueClosing && n == len(xs) {
		q.end(fb, t, s)
	}
}

// failIfDone fails as a done queue's cause.
func failIfDone(s *txQueueState) {
	if s.tag == queueDone {
		panic(&Failure{s.cause})
	}
}

func (q *TxQueue) offerIn(fb *Fiber, t *tx, x Value) bool {
	if q.stateIn(fb, t).tag != queueOpen {
		return false
	}
	xs := q.itemsIn(fb, t)
	switch {
	case len(xs) < q.capacity:
		t.set(fb, q.items, ChunkAppend(x, &Chunk{xs}))
	case q.strategy == queueDropping:
		return false
	case q.strategy == queueSliding:
		t.set(fb, q.items, ChunkAppend(x, ChunkDrop(int32(1), &Chunk{xs})))
	default:
		t.retryNow(fb)
	}
	return true
}

func TxQueueOffer(fb *Fiber, x, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return txQueueOf(q).offerIn(fb, t, x) })
}

// TxQueueOfferAll offers each element; those the queue rejected.
func TxQueueOfferAll(fb *Fiber, xs, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		rejected := []Value{}
		for _, x := range List(xs) {
			if !txQueueOf(q).offerIn(fb, t, x) {
				rejected = append(rejected, x)
			}
		}
		return rejected
	})
}

// TxQueueTake takes the first element, retrying while there is none.
func TxQueueTake(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		s := x.stateIn(fb, t)
		failIfDone(s)
		xs := x.itemsIn(fb, t)
		if len(xs) == 0 {
			return t.retryNow(fb)
		}
		x.rest(fb, t, s, xs, 1)
		return xs[0]
	})
}

func TxQueuePoll(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		s := x.stateIn(fb, t)
		if s.tag == queueDone {
			return None
		}
		xs := x.itemsIn(fb, t)
		if len(xs) == 0 {
			return None
		}
		x.rest(fb, t, s, xs, 1)
		return Some(xs[0])
	})
}

// TxQueueTakeAll takes every element, retrying while there is none.
func TxQueueTakeAll(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		s := x.stateIn(fb, t)
		failIfDone(s)
		xs := x.itemsIn(fb, t)
		if len(xs) == 0 {
			return t.retryNow(fb)
		}
		t.set(fb, x.items, ChunkEmpty)
		if s.tag == queueClosing {
			x.end(fb, t, s)
		}
		return xs
	})
}

// takeRange takes at least `least` elements and at most `most`, retrying
// for fewer unless the queue is closing, which ends it with what is left.
func (q *TxQueue) takeRange(fb *Fiber, t *tx, s *txQueueState, least, most int) Value {
	xs := q.itemsIn(fb, t)
	if len(xs) < least {
		if s.tag != queueClosing {
			return t.retryNow(fb)
		}
		if len(xs) > 0 {
			t.set(fb, q.items, ChunkEmpty)
		}
		q.end(fb, t, s)
		return xs
	}
	n := min(len(xs), most)
	q.rest(fb, t, s, xs, n)
	return xs[:n:n]
}

func TxQueueTakeN(fb *Fiber, n, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		s := x.stateIn(fb, t)
		failIfDone(s)
		k := min(countOf(n), x.capacity)
		return x.takeRange(fb, t, s, k, k)
	})
}

func TxQueueTakeBetween(fb *Fiber, lo, hi, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		s := x.stateIn(fb, t)
		failIfDone(s)
		least, most := countOf(lo), countOf(hi)
		if least <= 0 || most <= 0 || least > most {
			return []Value{}
		}
		return x.takeRange(fb, t, s, least, most)
	})
}

// TxQueuePeek is the first element, retrying while there is none.
func TxQueuePeek(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		failIfDone(x.stateIn(fb, t))
		xs := x.itemsIn(fb, t)
		if len(xs) == 0 {
			return t.retryNow(fb)
		}
		return xs[0]
	})
}

// The size is a read; see TxChunkSize.

func txQueueItems(fb *Fiber, q Value, f func([]Value) Value) Value {
	return txRead(fb, txQueueOf(q).items, func(v Value) Value { return f(items(v)) })
}

func TxQueueSize(fb *Fiber, q Value) Value {
	return txQueueItems(fb, q, func(xs []Value) Value { return int32(len(xs)) })
}

func TxQueueIsEmpty(fb *Fiber, q Value) Value {
	return txQueueItems(fb, q, func(xs []Value) Value { return len(xs) == 0 })
}

func TxQueueIsNonEmpty(fb *Fiber, q Value) Value {
	return txQueueItems(fb, q, func(xs []Value) Value { return len(xs) > 0 })
}

func (q *TxQueue) fullIn(fb *Fiber, t *tx) bool {
	return q.capacity != unboundedCapacity && len(q.itemsIn(fb, t)) >= q.capacity
}

func TxQueueIsFull(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return txQueueOf(q).fullIn(fb, t) })
}

// failCauseIn closes an open queue with `cause`: done when it is empty,
// else closing until it is drained; whether it was open.
func (q *TxQueue) failCauseIn(fb *Fiber, t *tx, cause *Cause) bool {
	if q.stateIn(fb, t).tag != queueOpen {
		return false
	}
	tag := queueClosing
	if len(q.itemsIn(fb, t)) == 0 {
		tag = queueDone
	}
	t.set(fb, q.state, &txQueueState{tag, cause})
	return true
}

func TxQueueInterrupt(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		return txQueueOf(q).failCauseIn(fb, t, &Cause{kind: causeInterrupt, fiber: fb.state.id})
	})
}

// TxQueueFail drops the elements and fails the queue with `err`.
func TxQueueFail(fb *Fiber, err, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		if x.stateIn(fb, t).tag != queueOpen {
			return false
		}
		t.set(fb, x.items, ChunkEmpty)
		t.set(fb, x.state, &txQueueState{queueDone, &Cause{kind: causeFail, err: err}})
		return true
	})
}

func TxQueueFailCause(fb *Fiber, failure, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		return txQueueOf(q).failCauseIn(fb, t, causeOfFailure(failure))
	})
}

// clearIn takes the elements of a queue that is not done, ending a
// closing one.
func (q *TxQueue) clearIn(fb *Fiber, t *tx, s *txQueueState) []Value {
	xs := q.itemsIn(fb, t)
	t.set(fb, q.items, ChunkEmpty)
	if s.tag == queueClosing {
		q.end(fb, t, s)
	}
	return xs
}

// TxQueueClear takes the elements; a done queue fails as it was failed,
// or gives none once ended.
func TxQueueClear(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		x := txQueueOf(q)
		s := x.stateIn(fb, t)
		if s.tag == queueDone {
			if isDoneCause(s.cause) {
				return []Value{}
			}
			panic(&Failure{s.cause})
		}
		return x.clearIn(fb, t, s)
	})
}

// shutdownIn shuts the queue down: `clear`, its failure ignored, then
// `interrupt`.
func (q *TxQueue) shutdownIn(fb *Fiber, t *tx) bool {
	if s := q.stateIn(fb, t); s.tag != queueDone {
		q.clearIn(fb, t, s)
	}
	return q.failCauseIn(fb, t, &Cause{kind: causeInterrupt, fiber: fb.state.id})
}

func TxQueueShutdown(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return txQueueOf(q).shutdownIn(fb, t) })
}

func txQueueIs(fb *Fiber, q Value, tag queueState) Value {
	return txRead(fb, txQueueOf(q).state, func(v Value) Value { return v.(*txQueueState).tag == tag })
}

func TxQueueIsOpen(fb *Fiber, q Value) Value    { return txQueueIs(fb, q, queueOpen) }
func TxQueueIsClosing(fb *Fiber, q Value) Value { return txQueueIs(fb, q, queueClosing) }
func TxQueueIsDone(fb *Fiber, q Value) Value    { return txQueueIs(fb, q, queueDone) }

// TxQueueAwaitCompletion retries until the queue is done.
func TxQueueAwaitCompletion(fb *Fiber, q Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		if txQueueOf(q).stateIn(fb, t).tag != queueDone {
			return t.retryNow(fb)
		}
		return Unit
	})
}
