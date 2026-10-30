package flickrt

import "sync"

// Resource is Effect's: a value acquired into a scope of its own, which a
// refresh replaces, closing the old one; it ends with the owner it was
// made in. A failed acquisition is kept and given to each `get`.
type resource struct {
	mu      sync.Mutex
	acquire Value
	made    *Fiber // the services acquiring runs with
	exit    *Exit
	scope   *Owner
}

// acquired runs `acquire` uninterruptibly into a fresh scope.
func (r *resource) acquired(fb *Fiber) (*Exit, *Owner) {
	scope := newOwner()
	inner := *r.made
	inner.state, inner.owner = fb.state, scope
	e := masked(&inner, fb.state.mask+1, func() Value { return exitOf(&inner, r.acquire) }).(*Exit)
	return e, scope
}

func ResourceManual(fb *Fiber, acquire Value) Value {
	made := *fb
	r := &resource{acquire: acquire, made: &made}
	r.exit, r.scope = r.acquired(fb)
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		r.mu.Lock()
		scope := r.scope
		r.mu.Unlock()
		if c := scope.close(fb, nil); c != nil {
			panic(&Failure{c})
		}
		return Unit
	})
	return r
}

func ResourceGet(fb *Fiber, r Value) Value {
	x := r.(*resource)
	x.mu.Lock()
	e := x.exit
	x.mu.Unlock()
	return Run(fb, e.outcome())
}

// ResourceRefresh acquires anew; a failure keeps the value there was.
func ResourceRefresh(fb *Fiber, r Value) Value {
	x := r.(*resource)
	e, scope := x.acquired(fb)
	if e.cause != nil {
		if c := scope.close(fb, nil); c != nil {
			e.cause = Sequential(e.cause, c)
		}
		panic(&Failure{e.cause})
	}
	x.mu.Lock()
	old := x.scope
	x.exit, x.scope = e, scope
	x.mu.Unlock()
	if c := old.close(fb, nil); c != nil {
		panic(&Failure{c})
	}
	return Unit
}

// EffectResourceAuto is Effect's Resource.auto: a resource a fiber of the
// scope refreshes by the schedule.
//
// shortcut: the first refresh waits for the schedule's first delay, where Effect's runs once the caller yields, which a goroutine cannot wait for; refresh at once if a program needs it
func EffectResourceAuto(fb *Fiber, schedule, acquire Value) Value {
	r := ResourceManual(fb, acquire)
	refresh := Eff(func(fb *Fiber) Value { return ResourceRefresh(fb, r) })
	ignore := Fn(func(Value) Value { return EffectVoid })
	FiberSpawn(fb, fb.owner, ignore, Eff(func(fb *Fiber) Value { return EffectSchedule(fb, schedule, refresh) }))
	return r
}

// The `Tx` module, as far as a transaction's metadata.
//
// shortcut: a transaction runs once, without a journal, as the go target has no TxRef yet; port Effect's STM with TxRef and Tx.retry
const txKey = "$transaction"

func TxAtomically(fb *Fiber, task Value) Value {
	now := clockOf(fb).millis()
	meta := Rec("attempt", int32(1), "retryReason", None, "start", now, "now", now,
		"elapsed", 0.0, "elapsedSincePrevious", 0.0)
	return withEnv(fb, txKey, meta, task)
}

func TxCurrent(fb *Fiber) Value {
	if m, ok := fb.env.get(txKey); ok {
		return m
	}
	panic(Defect{"Error: Tx.current runs only in a transaction"})
}
