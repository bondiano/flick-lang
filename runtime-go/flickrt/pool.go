package flickrt

import (
	"math"
	"slices"
	"sync"
)

// Pool is items acquired ahead and lent to tasks, at most `concurrency`
// tasks per item, released when the owner of the pool's region closes.
//
// shortcut: a resize acquires its items one after another and the call that starts it waits for it to end, where the JS target acquires them concurrently and returns once its fiber suspends; acquire them in fibers if slow acquisitions need it
type pool struct {
	mu                sync.Mutex
	acquire           Value
	env               *env // the services of the call that made it
	clock             Clock
	concurrency       int
	isFixed           bool
	minSize, maxSize  int
	targetUtilization float64
	strategy          poolStrategy // nil for none

	shuttingDown   bool
	usage          int
	resizing       *Semaphore
	items          []*poolItem // in the order they were acquired
	availableHead  *poolItem
	availableTail  *poolItem
	invalidatedSet map[*poolItem]bool
	waiters        []*poolWaiter
	fibers         map[*child]struct{} // the resizes and the strategy, stopped by the shutdown
}

type poolItem struct {
	exit           *Exit
	scope          *Owner
	refCount       int
	idleSince      float64
	disableReclaim bool
	isAvailable    bool
	prev, next     *poolItem
	reservations   int        // the scopes `reserve` keeps it for
	shutdown       *Semaphore // the shutdown waiting for its release, if any
}

// A poolWaiter is a task waiting for an item; `failure` is an acquisition's
// failure handed to it.
type poolWaiter struct {
	ch      chan struct{}
	failure *Cause
}

// A poolStrategy releases the items beyond `min` after their time to live.
type poolStrategy interface {
	run(fb *Fiber, p *pool)
	onAcquire(fb *Fiber, item *poolItem)
}

func newPool(fb *Fiber, o *Record, min, max int, strategy poolStrategy) Value {
	p := &pool{
		acquire: o.Get("acquire"), env: fb.env, clock: clockOf(fb),
		concurrency: 1, isFixed: min == max, minSize: min, maxSize: max,
		targetUtilization: 1, strategy: strategy,
		resizing: newSemaphore(1), invalidatedSet: map[*poolItem]bool{}, fibers: map[*child]struct{}{},
	}
	if n, ok := some(o.Get("concurrency")); ok {
		p.concurrency = int(n.(int32))
	}
	if u, ok := some(o.Get("targetUtilization")); ok {
		p.targetUtilization = math.Min(math.Max(u.(float64), 0.1), 1)
	}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		p.stopFibers(fb)
		p.shutdown(fb)
		return Unit
	})
	return registering(fb, func(outer int) Value {
		if p.minSize > 0 {
			p.resizeAndWait(fb)
		}
		if strategy != nil {
			p.spawn(fb, func(fb *Fiber) { strategy.run(fb, p) })
		}
		return p
	})
}

func PoolMake(fb *Fiber, options Value) Value {
	o := record(options)
	size := int(o.Get("size").(int32))
	return newPool(fb, o, size, size, nil)
}

var poolCreation = Tag("Pool.TimeToLiveStrategy.Creation")

func PoolMakeWithTtl(fb *Fiber, options Value) Value {
	o := record(options)
	ttl := millisOf(dur(o.Get("timeToLive")))
	var strategy poolStrategy = &usageTTL{ttl: ttl}
	if s, ok := some(o.Get("timeToLiveStrategy")); ok && s.(*Data).Tag == poolCreation {
		strategy = &creationTTL{ttl: ttl, clock: clockOf(fb), wake: make(chan struct{})}
	}
	return newPool(fb, o, int(o.Get("min").(int32)), int(o.Get("max").(int32)), strategy)
}

func poolOf(v Value) *pool { return v.(*pool) }

// spawn runs `f` in a fiber of the pool, stopped by its shutdown.
func (p *pool) spawn(fb *Fiber, f func(fb *Fiber)) *child {
	t := *fb
	t.env = p.env
	p.mu.Lock()
	defer p.mu.Unlock()
	var c *child
	c = fork(&t, newOwner(), Eff(func(fb *Fiber) Value {
		return masked(fb, 0, func() Value { f(fb); return Unit })
	}), func(*child) {
		p.mu.Lock()
		delete(p.fibers, c)
		p.mu.Unlock()
	})
	p.fibers[c] = struct{}{}
	return c
}

func (p *pool) stopFibers(fb *Fiber) {
	p.mu.Lock()
	fibers := make([]*child, 0, len(p.fibers))
	for c := range p.fibers {
		fibers = append(fibers, c)
	}
	p.mu.Unlock()
	stop(fb, fibers...)
}

// targetSize and activeSize are with the lock.
func (p *pool) targetSize() int {
	switch {
	case p.shuttingDown:
		return 0
	case p.isFixed:
		return p.minSize
	}
	target := int(math.Ceil(float64(p.usage) / p.targetUtilization / float64(p.concurrency)))
	return min(max(p.minSize, target), p.maxSize)
}

func (p *pool) activeSize() int { return len(p.items) - len(p.invalidatedSet) }

func (p *pool) has(item *poolItem) bool { return slices.Contains(p.items, item) }

func (p *pool) dropItem(item *poolItem) {
	if i := slices.Index(p.items, item); i >= 0 {
		p.items = slices.Delete(p.items, i, i+1)
	}
	p.removeAvailable(item)
	delete(p.invalidatedSet, item)
}

func (p *pool) addAvailable(item *poolItem) {
	if item.isAvailable || item.reservations > 0 {
		return
	}
	item.isAvailable, item.prev, item.next = true, p.availableTail, nil
	if p.availableTail != nil {
		p.availableTail.next = item
	} else {
		p.availableHead = item
	}
	p.availableTail = item
}

func (p *pool) addAvailableFront(item *poolItem) {
	if item.isAvailable || item.reservations > 0 {
		return
	}
	item.isAvailable, item.prev, item.next = true, nil, p.availableHead
	if p.availableHead != nil {
		p.availableHead.prev = item
	} else {
		p.availableTail = item
	}
	p.availableHead = item
}

func (p *pool) removeAvailable(item *poolItem) {
	if !item.isAvailable {
		return
	}
	item.isAvailable = false
	if item.prev != nil {
		item.prev.next = item.next
	} else {
		p.availableHead = item.next
	}
	if item.next != nil {
		item.next.prev = item.prev
	} else {
		p.availableTail = item.prev
	}
	item.prev, item.next = nil, nil
}

func (p *pool) firstHealthy() *poolItem {
	item := p.availableHead
	for item != nil && item.exit.cause != nil {
		item = item.next
	}
	return item
}

// wake wakes the first `n` waiters. With the lock.
func (p *pool) wake(n int) {
	for ; n > 0 && len(p.waiters) > 0; n-- {
		close(p.waiters[0].ch)
		p.waiters = p.waiters[1:]
	}
}

// lend lends `item` to the caller, its release registered by `hold`.
func (p *pool) lend(item *poolItem) {
	item.refCount++
	if item.refCount >= p.concurrency {
		p.removeAvailable(item)
	}
}

func (p *pool) release(fb *Fiber, item *poolItem) {
	p.mu.Lock()
	item.refCount--
	p.usage--
	if item.refCount == 0 {
		item.idleSince = p.clock.millis()
	}
	if p.invalidatedSet[item] {
		p.mu.Unlock()
		p.invalidateItem(fb, item)
		return
	}
	if item.refCount < p.concurrency {
		p.addAvailableFront(item)
		p.wake(1)
	}
	p.mu.Unlock()
}

// take is an item lent to the caller: the head of the available ones if
// the pool needs no more, else the first healthy one, waiting for one at
// the interruptibility `outer`.
func (p *pool) take(fb *Fiber, outer int) *poolItem {
	p.mu.Lock()
	if p.shuttingDown {
		p.mu.Unlock()
		panic(interruptNow(fb))
	}
	p.usage++
	if h := p.availableHead; h != nil && h.exit.cause == nil && (p.isFixed || p.targetSize() <= p.activeSize()) {
		p.lend(h)
		p.mu.Unlock()
		return h
	}
	p.mu.Unlock()
	for {
		p.mu.Lock()
		if p.shuttingDown {
			p.usage--
			p.mu.Unlock()
			panic(interruptNow(fb))
		}
		if p.targetSize() > p.activeSize() && p.resizing.tryTake(1) {
			p.resizing.release(1)
			p.mu.Unlock()
			p.spawn(fb, p.resize)
			p.mu.Lock()
		}
		item := p.firstHealthy()
		if item == nil && p.targetSize() <= p.activeSize() {
			item = p.availableHead
		}
		switch {
		case item == nil:
			p.mu.Unlock()
			p.waitForItem(fb, outer)
		case item.exit.cause != nil:
			p.dropItem(item)
			p.mu.Unlock()
		default:
			p.lend(item)
			p.mu.Unlock()
			return item
		}
	}
}

// waitForItem waits until an item may be available, failing with an
// acquisition's failure handed to it.
func (p *pool) waitForItem(fb *Fiber, outer int) {
	p.mu.Lock()
	if p.firstHealthy() != nil || p.shuttingDown {
		p.mu.Unlock()
		return
	}
	w := &poolWaiter{ch: make(chan struct{})}
	p.waiters = append(p.waiters, w)
	p.mu.Unlock()
	_, cause := catch(func() Value { masked(fb, outer, func() Value { wait(fb, w.ch); return Unit }); return Unit })
	p.mu.Lock()
	if i := slices.Index(p.waiters, w); i >= 0 {
		p.waiters = slices.Delete(p.waiters, i, i+1)
	}
	if cause == nil {
		cause = w.failure
	}
	if cause != nil {
		p.usage--
	}
	p.mu.Unlock()
	if cause != nil {
		panic(&Failure{cause})
	}
}

// PoolGet is an item, held by the innermost scope until it closes.
func PoolGet(fb *Fiber, self Value) Value {
	p := poolOf(self)
	return registering(fb, func(outer int) Value {
		item := p.take(fb, outer)
		fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { p.release(fb, item); return Unit })
		return item.exit.value
	})
}

// PoolUse runs `f` with an item, given back when `f` ends.
func PoolUse(fb *Fiber, f, self Value) Value {
	p := poolOf(self)
	return registering(fb, func(outer int) Value {
		item := p.take(fb, outer)
		out, cause := catch(func() Value {
			return masked(fb, outer, func() Value { return Run(fb, Apply(fb, f, item.exit.value)) })
		})
		_, released := catch(func() Value { p.release(fb, item); return Unit })
		if cause = finalized(cause, released); cause != nil { // then the failure of the release
			panic(&Failure{cause})
		}
		return out
	})
}

// find is the item whose value is `value`, by identity. With the lock.
func (p *pool) find(value Value) *poolItem {
	for _, item := range p.items {
		if item.exit.cause == nil && identical(item.exit.value, value) {
			return item
		}
	}
	return nil
}

func PoolInvalidate(fb *Fiber, value, self Value) Value {
	p := poolOf(self)
	p.mu.Lock()
	if p.shuttingDown {
		p.mu.Unlock()
		return Unit
	}
	item := p.find(value)
	if item != nil {
		item.disableReclaim = true
	}
	p.mu.Unlock()
	if item != nil {
		registering(fb, func(int) Value { p.invalidateItem(fb, item); return Unit })
	}
	return Unit
}

// invalidateItem releases `item` once its tasks are done, and starts a
// resize.
func (p *pool) invalidateItem(fb *Fiber, item *poolItem) {
	p.mu.Lock()
	if !p.has(item) {
		p.mu.Unlock()
		return
	}
	if item.refCount == 0 {
		p.dropItem(item)
		p.mu.Unlock()
		p.finalize(fb, item)
	} else {
		p.invalidatedSet[item] = true
		p.removeAvailable(item)
		p.mu.Unlock()
	}
	p.resizeAndWait(fb)
}

// PoolReserve keeps `item` for this task alone until the innermost scope
// closes.
func PoolReserve(fb *Fiber, value, self Value) Value {
	p := poolOf(self)
	if p.concurrency == 1 {
		return Unit
	}
	p.mu.Lock()
	item := p.find(value)
	if item != nil {
		if item.reservations == 0 {
			p.usage += p.concurrency - 1
		}
		item.reservations++
		p.removeAvailable(item)
	}
	p.mu.Unlock()
	if item == nil {
		return Unit
	}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		p.mu.Lock()
		defer p.mu.Unlock()
		if item.reservations--; item.reservations > 0 {
			return Unit
		}
		p.usage -= p.concurrency - 1
		if !p.shuttingDown && p.has(item) && !p.invalidatedSet[item] && item.refCount < p.concurrency {
			p.addAvailableFront(item)
			p.wake(p.concurrency - item.refCount)
		}
		return Unit
	})
	return Unit
}

// shutdown releases the free items, and waits for the lent ones to be given
// back and released.
func (p *pool) shutdown(fb *Fiber) {
	p.mu.Lock()
	if p.shuttingDown {
		p.mu.Unlock()
		return
	}
	p.shuttingDown = true
	size := len(p.items)
	lent := newSemaphore(size)
	for _, item := range slices.Clone(p.items) {
		if item.refCount > 0 {
			item.shutdown = lent
			p.invalidatedSet[item] = true
			lent.tryTake(1)
			continue
		}
		p.dropItem(item)
		p.mu.Unlock()
		p.finalize(fb, item)
		p.mu.Lock()
	}
	p.wake(len(p.waiters))
	p.mu.Unlock()
	lent.take(fb, size)
}
