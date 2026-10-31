package flickrt

import "sync"

// RcRef is Effect's: a resource acquired in a fiber of its own on the first
// `get`, kept while some scope holds it, and released when the last one
// closes (after the idle time) or when the owner of the reference does.
type rcRef struct {
	mu        sync.Mutex
	closed    bool
	acquiring *rcAcquiring
	acquired  *rcAcquired
	acquire   Value
	env       *env      // the services of the call that made it
	idle      *Duration // nil for none
}

type rcAcquiring struct {
	fiber    *child
	awaiters int
	acquired *rcAcquired
}

type rcAcquired struct {
	value       Value
	scope       *Owner
	timer       *child // the idle time's, while it runs
	refCount    int
	invalidated bool
}

// closeOwner closes an owner a library made, failing as its finalizers do.
func closeOwner(fb *Fiber, o *Owner) {
	if cause := o.close(fb, nil); cause != nil {
		panic(&Failure{cause})
	}
}

func RcRefMake(fb *Fiber, options Value) Value {
	o := record(options)
	r := &rcRef{acquire: o.Get("acquire"), env: fb.env}
	if d, ok := some(o.Get("idleTimeToLive")); ok {
		r.idle = dur(d)
	}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		r.mu.Lock()
		a, q := r.acquired, r.acquiring
		r.closed, r.acquired, r.acquiring = true, nil, nil
		var timer *child
		if a != nil {
			timer = a.timer
		}
		r.mu.Unlock()
		if timer != nil {
			stop(fb, timer)
		}
		if a != nil {
			closeOwner(fb, a.scope)
		}
		if q != nil {
			stop(fb, q.fiber)
		}
		return Unit
	})
	return r
}

// state is the acquired resource, counting one more holder; `outer` is
// the interruptibility of the wait for an acquisition.
func (r *rcRef) state(fb *Fiber, outer int) *rcAcquired {
	r.mu.Lock()
	switch {
	case r.closed:
		r.mu.Unlock()
		panic(interruptNow(fb))
	case r.acquired != nil:
		a := r.acquired
		a.refCount++
		timer := a.timer
		r.mu.Unlock()
		if timer != nil {
			stop(fb, timer)
		}
		return a
	case r.acquiring != nil:
		q := r.acquiring
		q.awaiters++
		r.mu.Unlock()
		return r.await(fb, q, outer)
	}
	q := &rcAcquiring{awaiters: 1}
	r.acquiring = q
	scope := newOwner()
	t := *fb
	t.env = r.env
	q.fiber = fork(&t, scope, Eff(func(fb *Fiber) Value {
		v, cause := catch(func() Value { return Run(fb, r.acquire) })
		var a *rcAcquired
		r.mu.Lock()
		switch {
		case cause == nil && r.acquiring == q:
			a = &rcAcquired{value: v, scope: scope, refCount: q.awaiters}
			r.acquiring, r.acquired, q.acquired = nil, a, a
		case cause == nil:
			cause = &Cause{kind: causeInterrupt, fiber: fb.state.id}
			fallthrough
		default:
			if r.acquiring == q {
				r.acquiring = nil
			}
		}
		r.mu.Unlock()
		if cause != nil {
			panic(&Failure{masked(fb, fb.state.mask+1, func() Value { return scope.close(fb, cause) }).(*Cause)})
		}
		return a
	}), nil)
	r.mu.Unlock()
	return r.await(fb, q, outer)
}

// await is the acquisition's result; the last holder waiting for it that
// is interrupted interrupts it.
func (r *rcRef) await(fb *Fiber, q *rcAcquiring, outer int) *rcAcquired {
	v, cause := catch(func() Value { return masked(fb, outer, func() Value { return FiberJoin(fb, q.fiber) }) })
	r.mu.Lock()
	a := q.acquired
	abandon := false
	if a == nil {
		q.awaiters--
		abandon = q.awaiters == 0 && r.acquiring == q
		if abandon {
			r.acquiring = nil
		}
	}
	r.mu.Unlock()
	switch {
	case a != nil && cause != nil:
		r.release(fb, a)
	case abandon:
		stop(fb, q.fiber)
	}
	if cause != nil {
		panic(&Failure{cause})
	}
	return v.(*rcAcquired)
}

// release lets go of one holder; the last one releases the resource, at
// once or after the idle time.
func (r *rcRef) release(fb *Fiber, a *rcAcquired) {
	r.mu.Lock()
	a.refCount--
	switch {
	case a.refCount > 0:
	case r.idle == nil || a.invalidated:
		if r.acquired == a {
			r.acquired = nil
		}
		r.mu.Unlock()
		closeOwner(fb, a.scope)
		return
	case r.idle.infinite():
	default:
		t := *fb
		t.env = r.env
		var timer *child
		timer = fork(&t, newOwner(), Eff(func(fb *Fiber) Value {
			defer func() {
				r.mu.Lock()
				if a.timer == timer {
					a.timer = nil
				}
				r.mu.Unlock()
			}()
			sleepFor(fb, r.idle)
			return masked(fb, fb.state.mask+1, func() Value {
				r.mu.Lock()
				expired := r.acquired == a && a.refCount == 0
				if expired {
					r.acquired = nil
				}
				r.mu.Unlock()
				if expired {
					closeOwner(fb, a.scope)
				}
				return Unit
			})
		}), nil)
		a.timer = timer
	}
	r.mu.Unlock()
}

// RcRefGet is the resource, held by the innermost scope until it closes.
func RcRefGet(fb *Fiber, ref Value) Value {
	r := ref.(*rcRef)
	return registering(fb, func(outer int) Value {
		a := r.state(fb, outer)
		fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { r.release(fb, a); return Unit })
		return a.value
	})
}

func RcRefInvalidate(fb *Fiber, ref Value) Value {
	r := ref.(*rcRef)
	return registering(fb, func(int) Value {
		r.mu.Lock()
		a := r.acquired
		if a == nil {
			r.mu.Unlock()
			return Unit
		}
		r.acquired, a.invalidated = nil, true
		if a.refCount > 0 {
			r.mu.Unlock()
			return Unit
		}
		timer := a.timer
		r.mu.Unlock()
		if timer != nil {
			timer.state.interrupt(fb.state.id)
		}
		closeOwner(fb, a.scope)
		return Unit
	})
}
