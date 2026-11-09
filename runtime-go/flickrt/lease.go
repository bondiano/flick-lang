package flickrt

import "sync"

// Leases on the values of library owners (`ScopedRef`, `Resource`,
// `ScopedCache`), as packages/effect-ts/src/effect/lease.ts: a value is used only
// inside a lease, and a release the library starts (a replacement, a
// refresh, an eviction) waits until no lease holds the value, so a value is
// never released while it is in use.

// A holder is a value and the owner of what its acquisition registered.
type holder struct {
	value   Value
	scope   *Owner // nil for a value that needs no release
	mu      sync.Mutex
	leases  int
	retired bool
	closed  bool
}

// plain is a holder of a value that needs no release.
func plain(v Value) *holder { return &holder{value: v} }

// acquireHolder runs `acquire` with a fresh owner as its scope, at the
// interruptibility `outer`: what it acquires belongs to the holder. The
// caller runs it masked and registers the holder before the mask ends, so
// an interruption leaves nothing unreleased; on a failure the fresh owner
// is closed.
func acquireHolder(fb *Fiber, acquire Value, outer int) *holder {
	scope := newOwner()
	if fb.owner != nil {
		scope.defects = fb.owner.defects
	}
	inner := *fb
	inner.owner = scope
	v, cause := catch(func() Value { return masked(&inner, outer, func() Value { return Run(&inner, acquire) }) })
	if cause != nil {
		// lease.ts closes and then fails, outside `onExit`: no election
		panic(&Failure{Sequential(cause, scope.closeFinalizers(fb, cause))})
	}
	return &holder{value: v, scope: scope}
}

// release closes the holder's owner.
func (h *holder) release(fb *Fiber) {
	if h.scope == nil {
		return
	}
	if cause := h.scope.close(fb, nil); cause != nil {
		panic(&Failure{cause})
	}
}

// retire: the owner no longer offers the holder, released once no lease
// holds it.
func (h *holder) retire(fb *Fiber) {
	h.mu.Lock()
	now := !h.retired && h.leases == 0
	h.retired = true
	h.closed = h.closed || now
	h.mu.Unlock()
	if now {
		h.release(fb)
	}
}

// lease runs `use` with the value under a lease, or `closed` if the value
// is released already.
func (h *holder) lease(fb *Fiber, use func(*Fiber, Value) Value, closed func(*Fiber) Value) Value {
	h.mu.Lock()
	held := !h.closed
	if held {
		h.leases++
	}
	h.mu.Unlock()
	if !held {
		return closed(fb)
	}
	out, cause := catch(func() Value { return use(fb, h.value) })
	_, released := catch(func() Value {
		return masked(fb, fb.state.mask+1, func() Value {
			h.mu.Lock()
			h.leases--
			last := h.retired && h.leases == 0 && !h.closed
			h.closed = h.closed || last
			h.mu.Unlock()
			if last {
				h.release(fb)
			}
			return Unit
		})
	})
	if cause = finalized(cause, released); cause != nil { // lease.ts: acquireUseRelease
		panic(&Failure{cause})
	}
	return out
}

// leased runs `use` under a lease of the holder `get` gives; one released
// meanwhile is asked for again.
func leased(fb *Fiber, get func(*Fiber) *holder, use func(*Fiber, Value) Value) Value {
	return get(fb).lease(fb, use, func(fb *Fiber) Value { return leased(fb, get, use) })
}

// applying is `use` running the Flick callback `f`.
func applying(f Value) func(*Fiber, Value) Value {
	return func(fb *Fiber, v Value) Value { return Run(fb, Apply(fb, f, v)) }
}

// registering runs `f` masked, with the interruptibility around the call,
// then lets a pending interruption in.
func registering(fb *Fiber, f func(outer int) Value) Value {
	outer := fb.state.mask
	v := masked(fb, outer+1, func() Value { return f(outer) })
	Check(fb)
	return v
}

// ScopedRef: a reference to a resource that `set` replaces. Its value is
// used only under a lease; the previous resource is released once the
// last lease of it ends, and the last one when the owner of the
// reference's region closes.
type scopedRef struct {
	mu      sync.Mutex
	current *holder
	lock    *Semaphore
}

func newSemaphore(permits int) *Semaphore {
	return &Semaphore{permits: permits, wake: make(chan struct{})}
}

// withPermit runs `f` holding the semaphore's permit.
func withPermit(fb *Fiber, s *Semaphore, f func() Value) Value {
	s.take(fb, 1)
	defer s.release(1)
	return f()
}

func (r *scopedRef) get(*Fiber) *holder {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.current
}

// ownedRef registers the reference with the owner of the call.
func ownedRef(fb *Fiber, h *holder) Value {
	r := &scopedRef{current: h, lock: newSemaphore(1)}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		r.get(fb).retire(fb)
		return Unit
	})
	return r
}

// ScopedRefFromAcquire is `ScopedRef.from_acquire acquire`: the holder is
// registered under the mask it was acquired in, as `acquireRelease` does.
func ScopedRefFromAcquire(fb *Fiber, acquire Value) Value {
	return registering(fb, func(outer int) Value { return ownedRef(fb, acquireHolder(fb, acquire, outer)) })
}

func ScopedRefMake(fb *Fiber, f Value) Value { return ownedRef(fb, plain(Apply(fb, f, Unit))) }

func ScopedRefUse(fb *Fiber, ref, f Value) Value {
	return leased(fb, ref.(*scopedRef).get, applying(f))
}

// ScopedRefSet is `ScopedRef.set acquire ref`: an interruption between the
// swap and the retirement would leave the previous resource unreleased.
func ScopedRefSet(fb *Fiber, acquire, ref Value) Value {
	r := ref.(*scopedRef)
	withPermit(fb, r.lock, func() Value {
		return registering(fb, func(outer int) Value {
			next := acquireHolder(fb, acquire, outer)
			r.mu.Lock()
			previous := r.current
			r.current = next
			r.mu.Unlock()
			previous.retire(fb)
			return Unit
		})
	})
	return Unit
}

// Resource: the last successful acquisition, or the first failure. Its
// value is used only under a lease; the previous value is released once
// the last lease of it ends, and the last one when the owner of the
// resource's region closes.
type resource struct {
	mu      sync.Mutex
	current *holder
	failed  *Cause // while no acquisition succeeded
	again   func(fb *Fiber, outer int) *holder
	lock    *Semaphore
}

// ResourceManual is `Resource.manual acquire`: the first value is
// registered under the mask it was acquired in.
func ResourceManual(fb *Fiber, acquire Value) Value {
	env := fb.env
	r := &resource{lock: newSemaphore(1), again: func(fb *Fiber, outer int) *holder {
		inner := *fb
		inner.env = env
		return acquireHolder(&inner, acquire, outer)
	}}
	return registering(fb, func(outer int) Value {
		var h *holder
		_, r.failed = catch(func() Value { h = r.again(fb, outer); return Unit })
		r.current = h
		fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
			r.mu.Lock()
			h := r.current
			r.mu.Unlock()
			if h != nil {
				h.retire(fb)
			}
			return Unit
		})
		return r
	})
}

// ResourceUse fails as the acquisition did if it never succeeded.
func ResourceUse(fb *Fiber, res, f Value) Value {
	r := res.(*resource)
	return leased(fb, func(*Fiber) *holder {
		r.mu.Lock()
		defer r.mu.Unlock()
		if r.current == nil {
			panic(&Failure{r.failed})
		}
		return r.current
	}, applying(f))
}

// ResourceRefresh: if the acquisition fails, the previous value stays.
func ResourceRefresh(fb *Fiber, res Value) Value {
	r := res.(*resource)
	withPermit(fb, r.lock, func() Value {
		return registering(fb, func(outer int) Value {
			var next *holder
			_, cause := catch(func() Value { next = r.again(fb, outer); return Unit })
			r.mu.Lock()
			previous := r.current
			if cause != nil {
				if previous == nil {
					r.failed = cause
				}
				r.mu.Unlock()
				panic(&Failure{cause})
			}
			r.current, r.failed = next, nil
			r.mu.Unlock()
			if previous != nil {
				previous.retire(fb)
			}
			return Unit
		})
	})
	return Unit
}
