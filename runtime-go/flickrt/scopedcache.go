package flickrt

import (
	"math"
	"sync"
)

// ScopedCache is Effect's, each value kept with a lease count as
// runtime/src/effect/ScopedCache.ts does: a value is used only under a
// lease, and dropping its entry releases it once the last lease ends. Each
// entry has an owner of its own, which holds the value's holder.
type scopedCache struct {
	mu       sync.Mutex
	closed   bool
	entries  ordMap[*scopedEntry]
	capacity int
	env      *env // the services of the call that made it
	lookup   Value
	ttl      timeToLive // of the exit of the value, not of its holder
}

type scopedEntry struct {
	expiresAt float64 // `+Inf` for never
	scope     *Owner
	done      chan struct{} // closed once `holder` or `cause` is set
	holder    *holder
	cause     *Cause
	awaiters  int
	fiber     *child // the lookup's, nil for a set value
}

func (e *scopedEntry) finish(h *holder, cause *Cause) {
	e.holder, e.cause = h, cause
	close(e.done)
}

func (e *scopedEntry) result() *holder {
	if e.cause != nil {
		panic(&Failure{e.cause})
	}
	return e.holder
}

func scopedOf(v Value) *scopedCache { return v.(*scopedCache) }

func newScopedCache(fb *Fiber, options Value, curried bool) Value {
	o := record(options)
	c := &scopedCache{capacity: int(o.Get("capacity").(int32)), env: fb.env, lookup: o.Get("lookup"), ttl: entryTTL(o.Get("timeToLive"), curried)}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		c.mu.Lock()
		c.closed = true
		entries := c.entries.vals
		c.entries.clear()
		c.mu.Unlock()
		closeEntries(fb, entries)
		return Unit
	})
	return c
}

func ScopedCacheMake(fb *Fiber, options Value) Value     { return newScopedCache(fb, options, false) }
func ScopedCacheMakeWith(fb *Fiber, options Value) Value { return newScopedCache(fb, options, true) }

// closeEntries closes the owners of entries, in order.
func closeEntries(fb *Fiber, entries []*scopedEntry) {
	var out *Cause
	for _, e := range entries {
		out = Sequential(out, e.scope.close(fb, nil))
	}
	if out != nil {
		panic(&Failure{out})
	}
}

// closeEvicted closes the owner of an entry the capacity evicted.
//
// shortcut: a failing finalizer is dropped, where Effect logs it at UnhandledLogLevel; log it once Logger's references are on the go target
func closeEvicted(fb *Fiber, entries []*scopedEntry) {
	for _, e := range entries {
		e.scope.close(fb, nil)
	}
}

// evict drops the oldest entries beyond the capacity. With the lock.
func (c *scopedCache) evict() []*scopedEntry {
	diff := len(c.entries.keys) - c.capacity
	if diff <= 0 {
		return nil
	}
	out := c.entries.vals[:diff:diff]
	c.entries.keys, c.entries.vals = c.entries.keys[diff:], c.entries.vals[diff:]
	return out
}

// expiry is when an entry of the value of `h`, or of the failure, expires
// by `clock`.
func (c *scopedCache) expiry(fb *Fiber, clock Clock, h *holder, cause *Cause, key Value) float64 {
	exit := &Exit{cause: cause}
	if h != nil {
		exit.value = h.value
	}
	return expiry(clock, c.ttl(fb, exit, key))
}

// lookupHolder is the lookup of `key` into the entry's owner: the holder is
// registered under the mask it was acquired in, as `acquireRelease` does.
func (c *scopedCache) lookupHolder(fb *Fiber, key Value) *holder {
	return registering(fb, func(outer int) Value {
		h := acquireHolder(fb, Apply(fb, c.lookup, key), outer)
		fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { h.retire(fb); return Unit })
		return h
	}).(*holder)
}

func (c *scopedCache) removeEntry(key Value, e *scopedEntry) {
	if c.closed {
		return
	}
	if i := c.entries.find(key); i >= 0 && c.entries.vals[i] == e {
		c.entries.removeAt(i)
	}
}

// get is the holder of `key`, looked up in a fiber of its own if it is
// missing or expired.
func (c *scopedCache) get(fb *Fiber, key Value) *holder {
	return registering(fb, func(outer int) Value {
		c.mu.Lock()
		if c.closed {
			c.mu.Unlock()
			panic(interruptNow(fb))
		}
		old, had := c.entries.get(key)
		if had && !(clockOf(fb).millis() >= old.expiresAt) {
			c.entries.touch(c.entries.find(key))
			c.mu.Unlock()
			return c.await(fb, key, old, outer)
		}
		e := &scopedEntry{expiresAt: math.Inf(1), scope: newOwner(), done: make(chan struct{})}
		c.entries.set(key, e)
		evicted := c.evict()
		c.mu.Unlock()
		closeEvicted(fb, evicted)
		if had {
			closeEvicted(fb, []*scopedEntry{old})
		}
		clock := clockOf(fb)
		look := *fb
		look.env = c.env
		c.mu.Lock()
		e.fiber = fork(&look, e.scope, Eff(func(fb *Fiber) Value {
			v, cause := catch(func() Value { return c.lookupHolder(fb, key) })
			masked(fb, fb.state.mask+1, func() Value {
				h, _ := v.(*holder)
				interrupted := cause.has(causeInterrupt)
				at := math.Inf(1)
				if !interrupted {
					at = c.expiry(fb, clock, h, cause, key)
				}
				c.mu.Lock()
				if interrupted {
					c.removeEntry(key, e)
				} else {
					e.expiresAt = at
				}
				c.mu.Unlock()
				e.finish(h, cause)
				if interrupted {
					cause = e.scope.close(fb, cause)
				}
				return Unit
			})
			if cause != nil {
				panic(&Failure{cause})
			}
			return v
		}), nil)
		c.mu.Unlock()
		return c.await(fb, key, e, outer)
	}).(*holder)
}

// interruptNow is the interruption of the fiber by itself.
func interruptNow(fb *Fiber) *Failure {
	return &Failure{&Cause{kind: causeInterrupt, fiber: fb.state.id}}
}

// await is the entry's holder at the interruptibility `outer`; the last
// caller waiting for it that is interrupted interrupts its lookup.
func (c *scopedCache) await(fb *Fiber, key Value, e *scopedEntry, outer int) Value {
	if isClosed(e.done) {
		return e.result()
	}
	c.mu.Lock()
	e.awaiters++
	c.mu.Unlock()
	_, cause := catch(func() Value { masked(fb, outer, func() Value { wait(fb, e.done); return Unit }); return Unit })
	c.mu.Lock()
	e.awaiters--
	lookup := e.fiber
	last := e.awaiters == 0 && lookup != nil && !isClosed(e.done)
	if last {
		c.removeEntry(key, e)
	}
	c.mu.Unlock()
	if last {
		stop(fb, lookup)
		if lookup.cause.has(causeDie) {
			cause = Sequential(cause, lookup.cause)
		}
	}
	if cause != nil {
		panic(&Failure{cause})
	}
	return e.result()
}

// find is the unexpired entry of `key`, moved to the end when `read`; an
// expired one is dropped and closed.
func (c *scopedCache) find(fb *Fiber, key Value, read bool) *scopedEntry {
	c.mu.Lock()
	if c.closed {
		c.mu.Unlock()
		panic(interruptNow(fb))
	}
	i := c.entries.find(key)
	if i < 0 {
		c.mu.Unlock()
		return nil
	}
	e := c.entries.vals[i]
	if clockOf(fb).millis() >= e.expiresAt {
		c.entries.removeAt(i)
		c.mu.Unlock()
		closeEvicted(fb, []*scopedEntry{e})
		return nil
	}
	if read {
		c.entries.touch(i)
	}
	c.mu.Unlock()
	return e
}

// optional runs `use` with the value an optional lookup found, under a lease.
func optional(fb *Fiber, get func(fb *Fiber) *holder, f Value) Value {
	h := get(fb)
	if h == nil {
		return Run(fb, Apply(fb, f, None))
	}
	return h.lease(fb, func(fb *Fiber, v Value) Value { return Run(fb, Apply(fb, f, Some(v))) },
		func(fb *Fiber) Value { return optional(fb, get, f) })
}

func ScopedCacheUse(fb *Fiber, key, self, f Value) Value {
	c := scopedOf(self)
	return leased(fb, func(fb *Fiber) *holder { return c.get(fb, key) }, applying(f))
}

func ScopedCacheUseOption(fb *Fiber, key, self, f Value) Value {
	c := scopedOf(self)
	return optional(fb, func(fb *Fiber) *holder {
		h, _ := registering(fb, func(outer int) Value {
			if e := c.find(fb, key, true); e != nil {
				return c.await(fb, key, e, outer)
			}
			return (*holder)(nil)
		}).(*holder)
		return h
	}, f)
}

func ScopedCacheUseSuccess(fb *Fiber, key, self, f Value) Value {
	c := scopedOf(self)
	return optional(fb, func(fb *Fiber) *holder {
		if e := c.find(fb, key, true); e != nil && isClosed(e.done) && e.cause == nil {
			return e.holder
		}
		return nil
	}, f)
}

// ScopedCacheSet caches a value that needs no release.
func ScopedCacheSet(fb *Fiber, key, value, self Value) Value {
	c := scopedOf(self)
	h := plain(value)
	e := &scopedEntry{scope: newOwner(), done: closedChan, holder: h}
	e.expiresAt = c.expiry(fb, clockOf(fb), h, nil, key)
	c.mu.Lock()
	if c.closed {
		c.mu.Unlock()
		panic(interruptNow(fb))
	}
	old, had := c.entries.get(key)
	c.entries.set(key, e)
	evicted := c.evict()
	c.mu.Unlock()
	closeEvicted(fb, evicted)
	if had {
		closeEntries(fb, []*scopedEntry{old})
	}
	return Unit
}

func ScopedCacheHas(fb *Fiber, key, self Value) Value {
	return scopedOf(self).find(fb, key, false) != nil
}

func ScopedCacheInvalidate(fb *Fiber, key, self Value) Value {
	c := scopedOf(self)
	c.mu.Lock()
	if c.closed {
		c.mu.Unlock()
		panic(interruptNow(fb))
	}
	e, had := c.entries.remove(key)
	c.mu.Unlock()
	if had {
		closeEntries(fb, []*scopedEntry{e})
	}
	return Unit
}

// ScopedCacheInvalidateWhen drops `key` if its value satisfies `f`; a
// failed lookup is never dropped.
func ScopedCacheInvalidateWhen(fb *Fiber, key, f, self Value) Value {
	c := scopedOf(self)
	return registering(fb, func(outer int) Value {
		e := c.find(fb, key, false)
		if e == nil {
			return false
		}
		v, cause := catch(func() Value { return c.await(fb, key, e, outer) })
		if cause != nil {
			if cause.has(causeDie) || cause.has(causeInterrupt) {
				panic(&Failure{cause})
			}
			return false
		}
		c.mu.Lock()
		if c.closed || !Apply(fb, f, v.(*holder).value).(bool) {
			c.mu.Unlock()
			return false
		}
		if i := c.entries.find(key); i < 0 || c.entries.vals[i] != e {
			c.mu.Unlock()
			return false
		}
		c.entries.remove(key)
		c.mu.Unlock()
		closeEntries(fb, []*scopedEntry{e})
		return true
	})
}

// ScopedCacheRefresh looks `key` up again in the calling fiber; the
// previous value is released once no lease holds it.
func ScopedCacheRefresh(fb *Fiber, key, self Value) Value {
	c := scopedOf(self)
	registering(fb, func(outer int) Value {
		c.mu.Lock()
		closed, has := c.closed, c.entries.find(key) >= 0
		c.mu.Unlock()
		if closed {
			panic(interruptNow(fb))
		}
		if !has {
			return masked(fb, outer, func() Value { return c.get(fb, key) })
		}
		e := &scopedEntry{scope: newOwner(), done: make(chan struct{})}
		look := *fb
		look.env, look.owner = c.env, e.scope
		v, cause := catch(func() Value {
			return masked(&look, outer, func() Value { return c.lookupHolder(&look, key) })
		})
		h, _ := v.(*holder)
		e.finish(h, cause)
		if cause.has(causeInterrupt) {
			panic(&Failure{e.scope.close(fb, cause)})
		}
		c.mu.Lock()
		if c.closed {
			c.mu.Unlock()
			e.scope.close(fb, nil)
			panic(interruptNow(fb))
		}
		e.expiresAt = c.expiry(fb, clockOf(fb), h, cause, key)
		old, had := c.entries.get(key)
		c.entries.set(key, e)
		evicted := c.evict()
		c.mu.Unlock()
		closeEvicted(fb, evicted)
		if had {
			closeEntries(fb, []*scopedEntry{old})
		}
		return e.result()
	})
	return Unit
}

func ScopedCacheInvalidateAll(fb *Fiber, self Value) Value {
	c := scopedOf(self)
	c.mu.Lock()
	if c.closed {
		c.mu.Unlock()
		panic(interruptNow(fb))
	}
	entries := c.entries.vals
	c.entries.clear()
	c.mu.Unlock()
	closeEntries(fb, entries)
	return Unit
}

func ScopedCacheSize(_ *Fiber, self Value) Value {
	c := scopedOf(self)
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return int32(0)
	}
	return int32(len(c.entries.keys))
}

// ScopedCacheKeys are the unexpired keys; the expired entries are closed.
func ScopedCacheKeys(fb *Fiber, self Value) Value {
	c := scopedOf(self)
	now := clockOf(fb).millis()
	keys := []Value{}
	var expired []*scopedEntry
	c.mu.Lock()
	for i := 0; !c.closed && i < len(c.entries.keys); {
		if e := c.entries.vals[i]; now >= e.expiresAt {
			expired = append(expired, e)
			c.entries.removeAt(i)
			continue
		}
		keys = append(keys, c.entries.keys[i])
		i++
	}
	c.mu.Unlock()
	closeEntries(fb, expired)
	return keys
}
