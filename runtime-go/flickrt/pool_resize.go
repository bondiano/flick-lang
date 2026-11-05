package flickrt

import (
	"sync"
	"time"
)

// The growth of a pool and the time to live of its items.

// resizeAndWait resizes the pool in a fiber of its own, which its shutdown
// stops, and waits for it.
func (p *pool) resizeAndWait(fb *Fiber) {
	c := p.spawn(fb, p.resize)
	wait(fb, c.done)
}

// resize acquires items until the pool has its target size, one resize at
// a time.
func (p *pool) resize(fb *Fiber) {
	withPermit(fb, p.resizing, func() Value {
		for {
			p.mu.Lock()
			toAcquire := p.targetSize() - p.activeSize()
			p.mu.Unlock()
			if toAcquire <= 0 {
				return Unit
			}
			failed := false
			for range toAcquire {
				failed = p.allocate(fb).exit.cause != nil || failed
			}
			p.mu.Lock()
			p.wake(len(p.waiters))
			p.mu.Unlock()
			if p.isFixed || failed {
				return Unit
			}
		}
	})
}

// allocate acquires an item into an owner of its own, with the services
// of the pool's maker. A failed acquisition goes to the first waiter if no
// item is free, else it stays as an item that the next taker drops; either
// way its owner is closed.
func (p *pool) allocate(fb *Fiber) *poolItem {
	scope := newOwner()
	t := *fb
	t.env, t.owner = p.env, scope
	v, cause := catch(func() Value { return Run(&t, p.acquire) })
	item := &poolItem{exit: &Exit{v, cause}, scope: scope, idleSince: p.clock.millis()}
	return masked(fb, fb.state.mask+1, func() Value {
		p.mu.Lock()
		if cause == nil {
			p.items = append(p.items, item)
			p.addAvailable(item)
			p.mu.Unlock()
			p.onAcquire(fb, item)
			return item
		}
		if p.firstHealthy() == nil && len(p.waiters) > 0 {
			w := p.waiters[0]
			p.waiters = p.waiters[1:]
			w.failure = cause
			close(w.ch)
		} else {
			p.items = append(p.items, item)
			p.addAvailable(item)
		}
		p.mu.Unlock()
		p.finalize(fb, item)
		p.onAcquire(fb, item)
		return item
	}).(*poolItem)
}

func (p *pool) onAcquire(fb *Fiber, item *poolItem) {
	p.mu.Lock()
	tracked := p.strategy != nil && p.has(item)
	p.mu.Unlock()
	if tracked {
		p.strategy.onAcquire(fb, item)
	}
}

// finalize closes the item's owner with how its acquisition ended; a
// finalizer that fails is logged as no one handles it.
func (p *pool) finalize(fb *Fiber, item *poolItem) {
	logUnhandled(fb, "Unhandled error in pool finalizer", item.scope.closeFinalizers(fb, item.exit.cause))
	if item.shutdown != nil {
		item.shutdown.release(1)
	}
}

// creationTTL releases each item `ttl` milliseconds after its acquisition,
// in the order they were acquired.
type creationTTL struct {
	ttl     float64
	clock   Clock
	mu      sync.Mutex
	queue   []*poolItem
	created map[*poolItem]float64
	wake    chan struct{}
}

func (s *creationTTL) onAcquire(_ *Fiber, item *poolItem) {
	s.mu.Lock()
	if s.created == nil {
		s.created = map[*poolItem]float64{}
	}
	s.created[item] = s.clock.millis()
	s.queue = append(s.queue, item)
	close(s.wake)
	s.wake = make(chan struct{})
	s.mu.Unlock()
}

func (s *creationTTL) run(fb *Fiber, p *pool) {
	for {
		s.mu.Lock()
		if len(s.queue) == 0 {
			wake := s.wake
			s.mu.Unlock()
			wait(fb, wake)
			continue
		}
		item := s.queue[0]
		s.queue = s.queue[1:]
		created := s.created[item]
		delete(s.created, item)
		s.mu.Unlock()
		for {
			p.mu.Lock()
			gone := !p.has(item) || p.invalidatedSet[item]
			p.mu.Unlock()
			if gone {
				break
			}
			remaining := s.ttl - (s.clock.millis() - created)
			if remaining <= 0 {
				p.invalidateItem(fb, item)
				break
			}
			sleepOn(fb, s.clock, remaining)
		}
	}
}

// sleepOn sleeps `millis` of `clock`.
func sleepOn(fb *Fiber, clock Clock, millis float64) {
	ch, cancel := clock.timer(time.Duration(millis * float64(time.Millisecond)))
	defer cancel()
	wait(fb, ch)
}

// usageTTL releases, every `ttl` milliseconds, the items beyond the target
// size that have not been used for that long, the oldest first.
type usageTTL struct{ ttl float64 }

func (*usageTTL) onAcquire(*Fiber, *poolItem) {}

func (s *usageTTL) run(fb *Fiber, p *pool) {
	for {
		sleepOn(fb, p.clock, s.ttl)
		for {
			p.mu.Lock()
			if p.activeSize() <= p.targetSize() {
				p.mu.Unlock()
				break
			}
			expired := p.clock.millis() - s.ttl
			var oldest *poolItem
			for _, item := range p.items {
				if item.refCount > 0 || item.idleSince > expired || p.invalidatedSet[item] || item.reservations > 0 {
					continue
				}
				if oldest == nil || item.idleSince < oldest.idleSince {
					oldest = item
				}
			}
			p.mu.Unlock()
			if oldest == nil {
				break
			}
			p.invalidateItem(fb, oldest)
		}
	}
}

// identical is Effect's `===` on Flick values: primitives by value, other
// values by reference.
func identical(a, b Value) bool {
	switch x := a.(type) {
	case float64, string, bool, int32, UnitType:
		return a == b
	case []Value:
		y, ok := b.([]Value)
		return ok && len(x) == len(y) && (len(x) == 0 || &x[0] == &y[0])
	case Fn, Poly:
		return false
	}
	return Same(a, b)
}
