package flickrt

import (
	"fmt"
	"math"
	"sync"
	"time"
)

// RcMap is resources by key, each looked up in a fiber of its own and
// shared by reference count as an RcRef is.
type rcMap struct {
	mu       sync.Mutex
	closed   bool
	entries  ordMap[*rcEntry]
	lookup   Value
	env      *env // the services of the call that made it
	idle     func(fb *Fiber, key Value) *Duration
	capacity int
}

type rcEntry struct {
	done      chan struct{} // closed once `value` or `cause` is set
	value     Value
	cause     *Cause
	scope     *Owner
	idle      *Duration
	lookup    *child // the lookup's fiber
	timer     *child // the idle time's, while it runs
	expiresAt float64
	refCount  int
}

var rcExceeded = Tag("RcMap.ExceededCapacity.ExceededCapacity")

func RcMapMake(fb *Fiber, options Value) Value {
	o := record(options)
	m := &rcMap{lookup: o.Get("lookup"), env: fb.env, capacity: math.MaxInt}
	m.idle = func(*Fiber, Value) *Duration { return dur(DurationZero) }
	if f, ok := some(o.Get("idleTimeToLive")); ok {
		m.idle = func(fb *Fiber, key Value) *Duration { return dur(Apply(fb, f, key)) }
	}
	if n, ok := some(o.Get("capacity")); ok {
		m.capacity = max(int(n.(int32)), 0)
	}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		m.mu.Lock()
		if m.closed {
			m.mu.Unlock()
			return Unit
		}
		m.closed = true
		entries := m.entries.vals
		m.entries.clear()
		m.mu.Unlock()
		for _, e := range entries {
			// each one's failure is dropped
			catch(func() Value { m.closeEntry(fb, e); return Unit })
		}
		return Unit
	})
	return m
}

func rcMapOf(v Value) *rcMap { return v.(*rcMap) }

// closeEntry stops the entry's fibers and closes its owner.
func (m *rcMap) closeEntry(fb *Fiber, e *rcEntry) {
	m.mu.Lock()
	timer := e.timer
	m.mu.Unlock()
	if timer != nil {
		stop(fb, timer)
	}
	stop(fb, e.lookup)
	closeOwner(fb, e.scope)
}

// hold registers the release of `e` with the innermost scope and waits for
// its value at the interruptibility `outer`.
func (m *rcMap) hold(fb *Fiber, key Value, e *rcEntry, outer int) Value {
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { m.release(fb, key, e); return Unit })
	masked(fb, outer, func() Value { wait(fb, e.done); return Unit })
	if e.cause != nil {
		panic(&Failure{e.cause})
	}
	return e.value
}

// RcMapGet is the resource of `key`, looked up if no one holds it; a full
// map fails with the Flick error `RcMap.ExceededCapacity`.
func RcMapGet(fb *Fiber, key, self Value) Value {
	m := rcMapOf(self)
	return registering(fb, func(outer int) Value {
		m.mu.Lock()
		if m.closed {
			m.mu.Unlock()
			panic(interruptNow(fb))
		}
		if e, ok := m.entries.get(key); ok {
			e.refCount++
			m.mu.Unlock()
			return m.hold(fb, key, e, outer)
		}
		if len(m.entries.keys) >= m.capacity {
			m.mu.Unlock()
			return Fail(NewData(rcExceeded, fmt.Sprintf("RcMap attempted to exceed capacity of %d", m.capacity)))
		}
		e := &rcEntry{done: make(chan struct{}), scope: newOwner(), idle: m.idle(fb, key), refCount: 1}
		m.entries.set(key, e)
		t := *fb
		t.env = m.env
		e.lookup = fork(&t, e.scope, Eff(func(fb *Fiber) Value {
			v, cause := catch(func() Value { return Run(fb, Apply(fb, m.lookup, key)) })
			if cause.has(causeInterrupt) {
				m.removeInterrupted(fb, key, e)
			}
			e.value, e.cause = v, cause
			close(e.done)
			return Unit
		}), nil)
		m.mu.Unlock()
		return m.hold(fb, key, e, outer)
	})
}

// removeInterrupted drops an entry whose lookup was interrupted, closing it
// in a fiber of its own when no one holds it.
func (m *rcMap) removeInterrupted(fb *Fiber, key Value, e *rcEntry) {
	m.mu.Lock()
	drop := !m.closed && m.current(key) == e
	if drop {
		m.entries.remove(key)
	}
	unheld := e.refCount == 0
	m.mu.Unlock()
	if drop && unheld {
		fork(fb, newOwner(), Eff(func(fb *Fiber) Value { m.closeEntry(fb, e); return Unit }), nil)
	}
}

// current is the entry of `key`, nil without one. With the lock.
func (m *rcMap) current(key Value) *rcEntry {
	e, _ := m.entries.get(key)
	return e
}

// release lets go of one holder of `e`; the last one releases it, at once
// or after its idle time.
func (m *rcMap) release(fb *Fiber, key Value, e *rcEntry) {
	m.mu.Lock()
	e.refCount--
	switch {
	case e.refCount > 0:
	case m.closed || m.current(key) != e:
		m.mu.Unlock()
		m.closeEntry(fb, e)
		return
	case DurationIsZero(e.idle).(bool):
		m.entries.remove(key)
		m.mu.Unlock()
		m.closeEntry(fb, e)
		return
	case e.idle.infinite():
	default:
		clock := clockOf(fb)
		e.expiresAt = clock.millis() + millisOf(e.idle)
		if e.timer == nil {
			var timer *child
			timer = fork(fb, newOwner(), Eff(func(fb *Fiber) Value {
				defer func() {
					m.mu.Lock()
					if e.timer == timer {
						e.timer = nil
					}
					m.mu.Unlock()
				}()
				return m.expire(fb, key, e, clock)
			}), nil)
			e.timer = timer
		}
	}
	m.mu.Unlock()
}

// expire waits until the entry's time is up and drops it if no one holds
// it, waiting again when `touch` moved the time on.
func (m *rcMap) expire(fb *Fiber, key Value, e *rcEntry, clock Clock) Value {
	for {
		m.mu.Lock()
		remaining := e.expiresAt - clock.millis()
		if remaining > 0 {
			m.mu.Unlock()
			ch, cancel := clock.timer(time.Duration(remaining * float64(time.Millisecond)))
			_, cause := catch(func() Value { wait(fb, ch); return Unit })
			cancel()
			if cause != nil {
				panic(&Failure{cause})
			}
			continue
		}
		drop := !m.closed && e.refCount == 0 && m.current(key) == e
		if drop {
			m.entries.remove(key)
		}
		m.mu.Unlock()
		if drop {
			masked(fb, fb.state.mask+1, func() Value { closeOwner(fb, e.scope); return Unit })
		}
		return Unit
	}
}

// RcMapGetOption is the resource of `key` if it is held now, held as `get`
// holds it.
func RcMapGetOption(fb *Fiber, key, self Value) Value {
	m := rcMapOf(self)
	return registering(fb, func(outer int) Value {
		m.mu.Lock()
		e := m.current(key)
		if m.closed || e == nil {
			m.mu.Unlock()
			return None
		}
		e.refCount++
		m.mu.Unlock()
		return Some(m.hold(fb, key, e, outer))
	})
}

func RcMapKeys(fb *Fiber, self Value) Value {
	m := rcMapOf(self)
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.closed {
		panic(interruptNow(fb))
	}
	return append([]Value{}, m.entries.keys...)
}

func RcMapInvalidate(fb *Fiber, key, self Value) Value {
	m := rcMapOf(self)
	return registering(fb, func(int) Value {
		m.mu.Lock()
		e := m.current(key)
		if m.closed || e == nil {
			m.mu.Unlock()
			return Unit
		}
		m.entries.remove(key)
		unheld := e.refCount == 0
		m.mu.Unlock()
		if unheld {
			m.closeEntry(fb, e)
		}
		return Unit
	})
}

func RcMapHas(_ *Fiber, key, self Value) Value {
	m := rcMapOf(self)
	m.mu.Lock()
	defer m.mu.Unlock()
	return !m.closed && m.current(key) != nil
}

// RcMapTouch starts the idle time of `key`'s resource anew.
func RcMapTouch(fb *Fiber, key, self Value) Value {
	m := rcMapOf(self)
	m.mu.Lock()
	defer m.mu.Unlock()
	if e := m.current(key); !m.closed && e != nil && !DurationIsZero(e.idle).(bool) {
		e.expiresAt = clockOf(fb).millis() + millisOf(e.idle)
	}
	return Unit
}
