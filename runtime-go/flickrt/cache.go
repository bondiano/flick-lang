package flickrt

import (
	"math"
	"sync"
)

// An ordMap is a mutable map in insertion order whose keys compare by `==`,
// as Effect's MutableHashMap; setting a key it has keeps its place.
//
// shortcut: a key is found by a linear scan; index the keys by hash if caches grow large
type ordMap[V any] struct {
	keys []Value
	vals []V
}

func (m *ordMap[V]) find(k Value) int {
	for i, at := range m.keys {
		if Equals(at, k) {
			return i
		}
	}
	return -1
}

func (m *ordMap[V]) get(k Value) (v V, ok bool) {
	if i := m.find(k); i >= 0 {
		return m.vals[i], true
	}
	return v, false
}

func (m *ordMap[V]) set(k Value, v V) {
	if i := m.find(k); i >= 0 {
		m.vals[i] = v
		return
	}
	m.keys, m.vals = append(m.keys, k), append(m.vals, v)
}

func (m *ordMap[V]) removeAt(i int) {
	m.keys = append(m.keys[:i:i], m.keys[i+1:]...)
	m.vals = append(m.vals[:i:i], m.vals[i+1:]...)
}

func (m *ordMap[V]) remove(k Value) (v V, ok bool) {
	if i := m.find(k); i >= 0 {
		v = m.vals[i]
		m.removeAt(i)
		return v, true
	}
	return v, false
}

// touch moves the key to the end, as a removal and a new set do.
func (m *ordMap[V]) touch(i int) {
	k, v := m.keys[i], m.vals[i]
	m.removeAt(i)
	m.keys, m.vals = append(m.keys, k), append(m.vals, v)
}

func (m *ordMap[V]) clear() { m.keys, m.vals = nil, nil }

// A timeToLive of an entry, from how its lookup ended and its key; an
// infinite one keeps it until it is evicted.
type timeToLive func(fb *Fiber, exit *Exit, key Value) *Duration

// entryTTL is a time to live of `options`: `None` for an infinite one, a
// Duration, or with `curried` a Flick function of the exit and the key.
func entryTTL(o Value, curried bool) timeToLive {
	f, ok := some(o)
	switch {
	case !ok:
		return func(*Fiber, *Exit, Value) *Duration { return dur(DurationInfinity) }
	case curried:
		return func(fb *Fiber, exit *Exit, key Value) *Duration { return dur(Apply(fb, Apply(fb, f, exit), key)) }
	}
	return func(*Fiber, *Exit, Value) *Duration { return dur(f) }
}

// expiry is when an entry of time to live `ttl` expires from now, `+Inf`
// for never.
func expiry(clock Clock, ttl *Duration) float64 {
	if ttl.infinite() {
		return math.Inf(1)
	}
	return clock.millis() + millisOf(ttl)
}

// A lookupCache is Effect's Cache: each lookup runs in a fiber of its own,
// which the last caller waiting for it interrupts when it is interrupted.
type lookupCache struct {
	mu       sync.Mutex
	entries  ordMap[*cacheEntry]
	capacity int
	env      *env // the services of the call that made it
	lookup   Value
	ttl      timeToLive
}

type cacheEntry struct {
	expiresAt   float64 // `+Inf` for never
	awaiters    int
	fiber       *child
	onInterrupt func() // with the lock
}

func newCache(fb *Fiber, lookup Value, capacity Value, ttl timeToLive) *lookupCache {
	return &lookupCache{capacity: int(capacity.(int32)), env: fb.env, lookup: lookup, ttl: ttl}
}

func CacheMake(fb *Fiber, options Value) Value {
	o := record(options)
	return newCache(fb, o.Get("lookup"), o.Get("capacity"), entryTTL(o.Get("timeToLive"), false))
}

func CacheMakeWith(fb *Fiber, lookup, options Value) Value {
	o := record(options)
	return newCache(fb, lookup, o.Get("capacity"), entryTTL(o.Get("timeToLive"), true))
}

func cacheOf(v Value) *lookupCache { return v.(*lookupCache) }

func (e *cacheEntry) expired(now float64) bool { return now >= e.expiresAt }

// done is the exit of the entry's lookup if it has ended.
func (e *cacheEntry) done() *Exit {
	if !isClosed(e.fiber.done) {
		return nil
	}
	return e.fiber.exit()
}

// await is the result of the entry's lookup; the last caller waiting for
// it that is interrupted interrupts it.
func (c *lookupCache) await(fb *Fiber, e *cacheEntry) Value {
	if exit := e.done(); exit != nil {
		return e.fiber.result()
	}
	c.mu.Lock()
	e.awaiters++
	c.mu.Unlock()
	_, cause := catch(func() Value { wait(fb, e.fiber.done); return Unit })
	c.mu.Lock()
	e.awaiters--
	last := e.awaiters == 0 && !isClosed(e.fiber.done)
	if last && e.onInterrupt != nil {
		e.onInterrupt()
	}
	c.mu.Unlock()
	if cause != nil {
		if last {
			masked(fb, fb.state.mask+1, func() Value { stop(fb, e.fiber); return Unit })
		}
		panic(&Failure{cause})
	}
	return e.fiber.result()
}

// start forks the lookup of `key` with the services of the cache's maker;
// as it ends, `ended` runs with the lock, after the caller's bookkeeping,
// and the clock of the caller.
func (c *lookupCache) start(fb *Fiber, key Value, ended func(clock Clock, ttl *Duration)) *cacheEntry {
	e := &cacheEntry{expiresAt: math.Inf(1)}
	clock := clockOf(fb)
	look := *fb
	look.env = c.env
	e.fiber = fork(&look, newOwner(), Eff(func(fb *Fiber) Value {
		v, cause := catch(func() Value { return Run(fb, Apply(fb, c.lookup, key)) })
		var ttl *Duration
		if !cause.has(causeInterrupt) {
			ttl = c.ttl(fb, &Exit{v, cause}, key)
		}
		c.mu.Lock()
		e.onInterrupt = nil
		ended(clock, ttl)
		c.mu.Unlock()
		if cause != nil {
			panic(&Failure{cause})
		}
		return v
	}), nil)
	return e
}

// removeEntry drops `key` if `e` is still its entry. With the lock.
func (c *lookupCache) removeEntry(key Value, e *cacheEntry) {
	if i := c.entries.find(key); i >= 0 && c.entries.vals[i] == e {
		c.entries.removeAt(i)
	}
}

// settle records how a lookup of `key` ended: an interrupted or zero-lived
// one is dropped. With the lock.
func (c *lookupCache) settle(clock Clock, key Value, e *cacheEntry, ttl *Duration, keep func()) {
	switch {
	case ttl == nil:
		c.removeEntry(key, e)
	case DurationIsZero(ttl).(bool):
		c.removeEntry(key, e)
	default:
		e.expiresAt = expiry(clock, ttl)
		keep()
	}
}

func (c *lookupCache) checkCapacity() {
	if diff := len(c.entries.keys) - c.capacity; diff > 0 {
		c.entries.keys, c.entries.vals = c.entries.keys[diff:], c.entries.vals[diff:]
	}
}

func CacheGet(fb *Fiber, key, self Value) Value {
	c := cacheOf(self)
	c.mu.Lock()
	if i := c.entries.find(key); i >= 0 {
		e := c.entries.vals[i]
		if !e.expired(clockOf(fb).millis()) {
			c.entries.touch(i)
			c.mu.Unlock()
			return c.await(fb, e)
		}
		c.entries.removeAt(i)
	}
	var e *cacheEntry
	e = c.start(fb, key, func(clock Clock, ttl *Duration) { c.settle(clock, key, e, ttl, func() {}) })
	e.onInterrupt = func() { c.removeEntry(key, e) }
	c.entries.set(key, e)
	c.checkCapacity()
	c.mu.Unlock()
	return c.await(fb, e)
}

// find is the unexpired entry of `key`, moved to the end when `read`; an
// expired one is dropped. With the lock.
func (c *lookupCache) find(fb *Fiber, key Value, read bool) *cacheEntry {
	i := c.entries.find(key)
	if i < 0 {
		return nil
	}
	e := c.entries.vals[i]
	if e.expired(clockOf(fb).millis()) {
		c.entries.removeAt(i)
		return nil
	}
	if read {
		c.entries.touch(i)
	}
	return e
}

func (c *lookupCache) lookupEntry(fb *Fiber, key Value, read bool) *cacheEntry {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.find(fb, key, read)
}

func CacheGetOption(fb *Fiber, key, self Value) Value {
	c := cacheOf(self)
	if e := c.lookupEntry(fb, key, true); e != nil {
		return Some(c.await(fb, e))
	}
	return None
}

func CacheGetSuccess(fb *Fiber, key, self Value) Value {
	if e := cacheOf(self).lookupEntry(fb, key, true); e != nil {
		if exit := e.done(); exit != nil && exit.cause == nil {
			return Some(exit.value)
		}
	}
	return None
}

func CacheSet(fb *Fiber, key, value, self Value) Value {
	c := cacheOf(self)
	exit := &Exit{value: value}
	ttl := c.ttl(fb, exit, key)
	done := &child{done: closedChan, value: value, ended: true}
	c.mu.Lock()
	defer c.mu.Unlock()
	if DurationIsZero(ttl).(bool) {
		c.entries.remove(key)
		return Unit
	}
	c.entries.set(key, &cacheEntry{expiresAt: expiry(clockOf(fb), ttl), fiber: done})
	c.checkCapacity()
	return Unit
}

func CacheHas(fb *Fiber, key, self Value) Value {
	return cacheOf(self).lookupEntry(fb, key, false) != nil
}

func CacheInvalidate(_ *Fiber, key, self Value) Value {
	c := cacheOf(self)
	c.mu.Lock()
	c.entries.remove(key)
	c.mu.Unlock()
	return Unit
}

// CacheInvalidateWhen drops `key` if its value satisfies `f`; a failed
// lookup is never dropped.
func CacheInvalidateWhen(fb *Fiber, key, f, self Value) Value {
	c := cacheOf(self)
	e := c.lookupEntry(fb, key, false)
	if e == nil {
		return false
	}
	v, cause := catch(func() Value { return c.await(fb, e) })
	if cause != nil || !Apply(fb, f, v).(bool) {
		return false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if i := c.entries.find(key); i < 0 || c.entries.vals[i] != e {
		return false
	}
	c.entries.remove(key)
	return true
}

// CacheRefresh looks `key` up again; an existing entry is replaced once
// the lookup ends.
func CacheRefresh(fb *Fiber, key, self Value) Value {
	c := cacheOf(self)
	c.mu.Lock()
	existing := c.find(fb, key, false) != nil
	var e *cacheEntry
	e = c.start(fb, key, func(clock Clock, ttl *Duration) {
		if ttl != nil && DurationIsZero(ttl).(bool) && existing {
			c.entries.remove(key)
			return
		}
		c.settle(clock, key, e, ttl, func() {
			if existing {
				c.entries.set(key, e)
				c.checkCapacity()
			}
		})
	})
	e.onInterrupt = func() { c.removeEntry(key, e) }
	if !existing {
		c.entries.set(key, e)
		c.checkCapacity()
	}
	c.mu.Unlock()
	return c.await(fb, e)
}

func CacheInvalidateAll(_ *Fiber, self Value) Value {
	c := cacheOf(self)
	c.mu.Lock()
	c.entries.clear()
	c.mu.Unlock()
	return Unit
}

func CacheSize(_ *Fiber, self Value) Value {
	c := cacheOf(self)
	c.mu.Lock()
	defer c.mu.Unlock()
	return int32(len(c.entries.keys))
}

// live walks the unexpired entries, dropping the expired ones.
func (c *lookupCache) live(fb *Fiber, f func(key Value, e *cacheEntry)) {
	now := clockOf(fb).millis()
	c.mu.Lock()
	defer c.mu.Unlock()
	for i := 0; i < len(c.entries.keys); {
		if e := c.entries.vals[i]; e.expired(now) {
			c.entries.removeAt(i)
			continue
		}
		f(c.entries.keys[i], c.entries.vals[i])
		i++
	}
}

func CacheKeys(fb *Fiber, self Value) Value {
	out := []Value{}
	cacheOf(self).live(fb, func(k Value, _ *cacheEntry) { out = append(out, k) })
	return out
}

// successes are the keys and values of the successful lookups.
func successes(fb *Fiber, self Value, f func(k, v Value) Value) Value {
	out := []Value{}
	cacheOf(self).live(fb, func(k Value, e *cacheEntry) {
		if exit := e.done(); exit != nil && exit.cause == nil {
			out = append(out, f(k, exit.value))
		}
	})
	return out
}

func CacheValues(fb *Fiber, self Value) Value {
	return successes(fb, self, func(_, v Value) Value { return v })
}

func CacheEntries(fb *Fiber, self Value) Value {
	return successes(fb, self, func(k, v Value) Value { return []Value{k, v} })
}
