package flickrt

import (
	"math"
	"slices"
)

// TxPubSub and TxSubscriptionRef as Effect 4's: a TxRef of the subscribers'
// queues, each a TxQueue of the hub's strategy, and a TxRef of whether the
// hub is shut down; a TxSubscriptionRef publishes each value it is set to.

// TxPubSub is Effect's TxPubSub.
type TxPubSub struct {
	subscribers *TxRef // a []*TxQueue, never mutated
	shutdown    *TxRef // a bool
	strategy    queueStrategy
	capacity    int
}

func txPubSubOf(v Value) *TxPubSub { return v.(*TxPubSub) }

func newTxPubSub(strategy queueStrategy, capacity int) *TxPubSub {
	return &TxPubSub{newTxRef([]*TxQueue(nil)), newTxRef(false), strategy, capacity}
}

func TxPubSubBounded(n Value) Value  { return newTxPubSub(queueSuspend, int(n.(int32))) }
func TxPubSubDropping(n Value) Value { return newTxPubSub(queueDropping, int(n.(int32))) }
func TxPubSubSliding(n Value) Value  { return newTxPubSub(queueSliding, int(n.(int32))) }
func TxPubSubUnbounded() Value       { return newTxPubSub(queueSuspend, unboundedCapacity) }

// TxPubSubCapacity is Infinity for an unbounded hub, as on JS.
func TxPubSubCapacity(p Value) Value {
	if c := txPubSubOf(p).capacity; c != unboundedCapacity {
		return int32(c)
	}
	return math.Inf(1)
}

func (p *TxPubSub) subscribersIn(fb *Fiber, t *tx) []*TxQueue {
	return t.get(fb, p.subscribers).([]*TxQueue)
}

// sizeIn is the size of the fullest subscriber's queue.
func (p *TxPubSub) sizeIn(fb *Fiber, t *tx) int {
	most := 0
	for _, q := range p.subscribersIn(fb, t) {
		most = max(most, len(q.itemsIn(fb, t)))
	}
	return most
}

func TxPubSubSize(fb *Fiber, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return int32(txPubSubOf(p).sizeIn(fb, t)) })
}

func TxPubSubIsEmpty(fb *Fiber, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return txPubSubOf(p).sizeIn(fb, t) == 0 })
}

func TxPubSubIsNonEmpty(fb *Fiber, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return txPubSubOf(p).sizeIn(fb, t) > 0 })
}

// TxPubSubIsFull is whether a subscriber's queue is full.
func TxPubSubIsFull(fb *Fiber, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		h := txPubSubOf(p)
		if h.capacity == unboundedCapacity {
			return false
		}
		return slices.ContainsFunc(h.subscribersIn(fb, t), func(q *TxQueue) bool { return q.fullIn(fb, t) })
	})
}

func TxPubSubIsShutdown(fb *Fiber, p Value) Value {
	return txRead(fb, txPubSubOf(p).shutdown, Identity)
}

// publishIn offers `x` to every subscriber; whether each accepted it.
func (p *TxPubSub) publishIn(fb *Fiber, t *tx, x Value) bool {
	if t.get(fb, p.shutdown).(bool) {
		return false
	}
	all := true
	for _, q := range p.subscribersIn(fb, t) {
		if !q.offerIn(fb, t, x) {
			all = false
		}
	}
	return all
}

func TxPubSubPublish(fb *Fiber, x, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return txPubSubOf(p).publishIn(fb, t, x) })
}

func TxPubSubPublishAll(fb *Fiber, xs, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		h := txPubSubOf(p)
		if t.get(fb, h.shutdown).(bool) {
			return false
		}
		all := true
		for _, x := range List(xs) {
			if !h.publishIn(fb, t, x) {
				all = false
			}
		}
		return all
	})
}

// acquireSubscriber adds a queue of the hub's strategy to its subscribers.
func (p *TxPubSub) acquireSubscriber(fb *Fiber, t *tx) *TxQueue {
	q := newTxQueue(p.strategy, p.capacity)
	t.update(fb, p.subscribers, func(v Value) Value {
		return append(slices.Clip(v.([]*TxQueue)), q)
	})
	return q
}

func (p *TxPubSub) releaseSubscriber(fb *Fiber, t *tx, q *TxQueue) {
	t.update(fb, p.subscribers, func(v Value) Value {
		subs := v.([]*TxQueue)
		out := make([]*TxQueue, 0, len(subs))
		for _, s := range subs {
			if s != q {
				out = append(out, s)
			}
		}
		return out
	})
	q.shutdownIn(fb, t)
}

// subscribed is Effect's acquireRelease of a subscriber made by `acquire`
// in a transaction, released as the scope closes.
func (p *TxPubSub) subscribed(fb *Fiber, acquire func(*Fiber, *tx) *TxQueue) Value {
	q := masked(fb, fb.state.mask+1, func() Value {
		return txDo(fb, func(fb *Fiber, t *tx) Value { return acquire(fb, t) })
	}).(*TxQueue)
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		return txDo(fb, func(fb *Fiber, t *tx) Value {
			p.releaseSubscriber(fb, t, q)
			return Unit
		})
	})
	return q
}

func TxPubSubSubscribe(fb *Fiber, p Value) Value {
	h := txPubSubOf(p)
	return h.subscribed(fb, h.acquireSubscriber)
}

// TxPubSubShutdown shuts the hub and every subscriber's queue down.
func TxPubSubShutdown(fb *Fiber, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		h := txPubSubOf(p)
		if t.get(fb, h.shutdown).(bool) {
			return Unit
		}
		t.set(fb, h.shutdown, true)
		for _, q := range h.subscribersIn(fb, t) {
			q.shutdownIn(fb, t)
		}
		return Unit
	})
}

// TxPubSubAwaitShutdown retries until the hub is shut down.
func TxPubSubAwaitShutdown(fb *Fiber, p Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		if !t.get(fb, txPubSubOf(p).shutdown).(bool) {
			return t.retryNow(fb)
		}
		return Unit
	})
}

// TxSubscriptionRef is Effect's TxSubscriptionRef.
type TxSubscriptionRef struct {
	ref    *TxRef
	pubsub *TxPubSub
}

func txSubRefOf(v Value) *TxSubscriptionRef { return v.(*TxSubscriptionRef) }

func TxSubscriptionRefMake(v Value) Value {
	return &TxSubscriptionRef{newTxRef(v), newTxPubSub(queueSuspend, unboundedCapacity)}
}

func TxSubscriptionRefGet(fb *Fiber, s Value) Value { return txRead(fb, txSubRefOf(s).ref, Identity) }

// subRefModify sets the second of `f` of the value and publishes it;
// gives the first.
func subRefModify(fb *Fiber, s Value, f func(Value) (Value, Value)) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		r := txSubRefOf(s)
		out, next := f(t.get(fb, r.ref))
		t.set(fb, r.ref, next)
		r.pubsub.publishIn(fb, t, next)
		return out
	})
}

func TxSubscriptionRefModify(fb *Fiber, f, s Value) Value { return subRefModify(fb, s, txPair(fb, f)) }

func TxSubscriptionRefSet(fb *Fiber, x, s Value) Value {
	return subRefModify(fb, s, func(Value) (Value, Value) { return Unit, x })
}

func TxSubscriptionRefUpdate(fb *Fiber, f, s Value) Value {
	return subRefModify(fb, s, func(v Value) (Value, Value) { return Unit, Apply(fb, f, v) })
}

func TxSubscriptionRefGetAndSet(fb *Fiber, x, s Value) Value {
	return subRefModify(fb, s, func(v Value) (Value, Value) { return v, x })
}

func TxSubscriptionRefGetAndUpdate(fb *Fiber, f, s Value) Value {
	return subRefModify(fb, s, func(v Value) (Value, Value) { return v, Apply(fb, f, v) })
}

func TxSubscriptionRefUpdateAndGet(fb *Fiber, f, s Value) Value {
	return subRefModify(fb, s, func(v Value) (Value, Value) {
		next := Apply(fb, f, v)
		return next, next
	})
}

// TxSubscriptionRefChanges subscribes until the scope closes: the queue
// holds the current value, then each new one.
func TxSubscriptionRefChanges(fb *Fiber, s Value) Value {
	r := txSubRefOf(s)
	return r.pubsub.subscribed(fb, func(fb *Fiber, t *tx) *TxQueue {
		q := r.pubsub.acquireSubscriber(fb, t)
		q.offerIn(fb, t, t.get(fb, r.ref))
		return q
	})
}
