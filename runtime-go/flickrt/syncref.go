package flickrt

import "sync"

// SyncRef is a SynchronizedRef, and a SubscriptionRef with the hub its
// changes are published to: an update holds the lock while its task runs,
// and a failing update leaves the value as it was.
type SyncRef struct {
	mu    sync.Mutex
	value Value
	hub   *PubSub // a SubscriptionRef's, nil for a SynchronizedRef
}

func SyncRefMake(_ *Fiber, v Value) Value { return &SyncRef{value: v} }

// update runs `f` of the value under the lock: what to return, the new
// value, and whether to set it.
func update(r Value, f func(Value) (Value, Value, bool)) Value {
	x := r.(*SyncRef)
	x.mu.Lock()
	defer x.mu.Unlock()
	out, next, set := f(x.value)
	if set {
		x.value = next
		if x.hub != nil {
			// an unbounded hub never waits, so no fiber is needed
			x.hub.publish(nil, next)
		}
	}
	return out
}

func some(o Value) (Value, bool) {
	if d := o.(*Data); d.Tag == "Some" {
		return d.Fields[0], true
	}
	return nil, false
}

func SyncRefGet(_ *Fiber, r Value) Value {
	return update(r, func(a Value) (Value, Value, bool) { return a, nil, false })
}

func SyncRefSet(_ *Fiber, v, r Value) Value {
	return update(r, func(Value) (Value, Value, bool) { return Unit, v, true })
}

func SyncRefGetAndSet(_ *Fiber, v, r Value) Value {
	return update(r, func(a Value) (Value, Value, bool) { return a, v, true })
}

func SyncRefSetAndGet(_ *Fiber, v, r Value) Value {
	return update(r, func(Value) (Value, Value, bool) { return v, v, true })
}

// The pure and effectful updates: `next` is the new value of `f` of the
// old, `keep` the result to give.
func updated(fb *Fiber, f, r Value, effectful bool, keep func(old, next Value) Value) Value {
	return update(r, func(a Value) (Value, Value, bool) {
		next := Apply(fb, f, a)
		if effectful {
			next = Run(fb, next)
		}
		return keep(a, next), next, true
	})
}

func updatedSome(fb *Fiber, f, r Value, effectful bool, keep func(old, next Value) Value) Value {
	return update(r, func(a Value) (Value, Value, bool) {
		o := Apply(fb, f, a)
		if effectful {
			o = Run(fb, o)
		}
		next, ok := some(o)
		if !ok {
			return keep(a, a), nil, false
		}
		return keep(a, next), next, true
	})
}

func old(a, _ Value) Value  { return a }
func next(_, b Value) Value { return b }
func none(_, _ Value) Value { return Unit }

func SyncRefGetAndUpdate(fb *Fiber, f, r Value) Value       { return updated(fb, f, r, false, old) }
func SyncRefGetAndUpdateEffect(fb *Fiber, f, r Value) Value { return updated(fb, f, r, true, old) }
func SyncRefUpdate(fb *Fiber, f, r Value) Value             { return updated(fb, f, r, false, none) }
func SyncRefUpdateEffect(fb *Fiber, f, r Value) Value       { return updated(fb, f, r, true, none) }
func SyncRefUpdateAndGet(fb *Fiber, f, r Value) Value       { return updated(fb, f, r, false, next) }
func SyncRefUpdateAndGetEffect(fb *Fiber, f, r Value) Value { return updated(fb, f, r, true, next) }

func SyncRefGetAndUpdateSome(fb *Fiber, f, r Value) Value { return updatedSome(fb, f, r, false, old) }
func SyncRefGetAndUpdateSomeEffect(fb *Fiber, f, r Value) Value {
	return updatedSome(fb, f, r, true, old)
}
func SyncRefUpdateSome(fb *Fiber, f, r Value) Value       { return updatedSome(fb, f, r, false, none) }
func SyncRefUpdateSomeEffect(fb *Fiber, f, r Value) Value { return updatedSome(fb, f, r, true, none) }
func SyncRefUpdateSomeAndGet(fb *Fiber, f, r Value) Value { return updatedSome(fb, f, r, false, next) }
func SyncRefUpdateSomeAndGetEffect(fb *Fiber, f, r Value) Value {
	return updatedSome(fb, f, r, true, next)
}

// modified is `modify`: `f` gives the result and the new value, or with
// `optional` an `Option` of the new value.
func modified(fb *Fiber, f, r Value, effectful, optional bool) Value {
	return update(r, func(a Value) (Value, Value, bool) {
		pair := Apply(fb, f, a)
		if effectful {
			pair = Run(fb, pair)
		}
		out, v := At(pair, 0), At(pair, 1)
		if !optional {
			return out, v, true
		}
		n, ok := some(v)
		return out, n, ok
	})
}

func SyncRefModify(fb *Fiber, f, r Value) Value           { return modified(fb, f, r, false, false) }
func SyncRefModifyEffect(fb *Fiber, f, r Value) Value     { return modified(fb, f, r, true, false) }
func SyncRefModifySome(fb *Fiber, f, r Value) Value       { return modified(fb, f, r, false, true) }
func SyncRefModifySomeEffect(fb *Fiber, f, r Value) Value { return modified(fb, f, r, true, true) }
