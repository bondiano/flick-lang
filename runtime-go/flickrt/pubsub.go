package flickrt

import (
	"math"
	"slices"
	"sync"
)

// PubSub is Effect 4's (PubSub.ts): a backing hub, the subscriptions
// with takers waiting, a strategy for a full hub (back pressure, dropping
// or sliding) and an end. One lock guards the hub, its subscriptions and
// its strategy, as JS runs each of their operations at once.
type PubSub struct {
	mu           sync.Mutex
	atomic       hubAtomic
	subscribers  []*Subscription // with takers waiting, as Effect's Map orders them
	live         []*Subscription // the hub's scope: what shutting it down unsubscribes
	isShutdown   bool
	shutdownHook chan struct{} // closed once shut down
	strategy     hubStrategy
	publishers   []hubPublisher // waiting for room, under back pressure
	ended        Value
	hasEnded     bool
}

type hubStrategy int

const (
	hubBackPressure hubStrategy = iota
	hubDropping
	hubSliding
)

// Subscription is Effect's: a backing subscription, its takers, and the
// replay window it reads first.
type Subscription struct {
	hub        *PubSub
	backing    hubBacking
	pollers    []*hubSignal
	shutdown   bool
	closed     bool
	replay     *replayWindow
	subscribed bool // among the hub's subscribers
}

// hubSignal is a Deferred a taker or a publisher waits on: settled once,
// under the hub's lock, with a value or an interruption.
type hubSignal struct {
	value   Value
	cause   *Cause
	settled bool
	done    chan struct{}
}

func newHubSignal() *hubSignal { return &hubSignal{done: make(chan struct{})} }

func (d *hubSignal) settle(v Value, c *Cause) {
	if !d.settled {
		d.value, d.cause, d.settled = v, c, true
		close(d.done)
	}
}

// await waits for the signal, unlocking the hub first; an interrupted
// waiter calls `withdraw` under the lock.
func (d *hubSignal) await(fb *Fiber, h *PubSub, withdraw func()) Value {
	h.mu.Unlock()
	if !isClosed(d.done) {
		if _, c := catch(func() Value { wait(fb, d.done); return nil }); c != nil {
			h.mu.Lock()
			withdraw()
			h.mu.Unlock()
			panic(&Failure{c})
		}
	}
	if d.cause != nil {
		panic(&Failure{d.cause})
	}
	return d.value
}

type hubPublisher struct {
	value Value
	d     *hubSignal
	last  bool
}

func newPubSub(atomic hubAtomic, strategy hubStrategy) *PubSub {
	return &PubSub{atomic: atomic, strategy: strategy, shutdownHook: make(chan struct{})}
}

func pubSubOf(v Value) *PubSub { return v.(*PubSub) }

// interruptSelf is Effect's `Effect.interrupt`, a failure with the
// fiber's own interruption, after unlocking the hub.
func (h *PubSub) interruptSelf(fb *Fiber) Value {
	h.mu.Unlock()
	panic(&Failure{&Cause{kind: causeInterrupt, fiber: fb.state.id}})
}

func (h *PubSub) size() int {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.isShutdown {
		return 0
	}
	return h.atomic.size()
}

// shutdown is Effect's: the first call unsubscribes every subscriber, as
// closing the hub's scope does, and interrupts the waiting publishers.
func (h *PubSub) shutdown(fb *Fiber) {
	masked(fb, fb.state.mask+1, func() Value {
		h.mu.Lock()
		h.isShutdown = true
		if isClosed(h.shutdownHook) {
			h.mu.Unlock()
			return nil
		}
		close(h.shutdownHook)
		live := h.live
		h.live = nil
		h.mu.Unlock()
		for i := len(live) - 1; i >= 0; i-- {
			live[i].unsubscribe(fb)
		}
		h.mu.Lock()
		interrupt := &Cause{kind: causeInterrupt, fiber: fb.state.id}
		for _, p := range h.publishers {
			if p.last {
				p.d.settle(nil, interrupt)
			}
		}
		h.publishers = nil
		h.mu.Unlock()
		return nil
	})
}

// end is Effect's `endUnsafe`.
func (h *PubSub) end(v Value) bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.isShutdown || h.hasEnded {
		return false
	}
	h.ended, h.hasEnded = v, true
	if h.strategy == hubBackPressure {
		for _, p := range h.publishers {
			if p.last {
				p.d.settle(false, nil)
			}
		}
		h.publishers = nil
	}
	// a waiting subscriber has nothing buffered, so it gets the final message at once
	for _, s := range h.subscribers {
		for _, p := range s.pollers {
			p.settle(v, nil)
		}
		s.pollers, s.subscribed = nil, false
	}
	h.subscribers = nil
	return true
}

func (h *PubSub) publish(fb *Fiber, v Value) bool {
	h.mu.Lock()
	if h.isShutdown || h.hasEnded {
		h.mu.Unlock()
		return false
	}
	if h.atomic.publish(v) {
		h.completeSubscribers()
		h.mu.Unlock()
		return true
	}
	return h.handleSurplus(fb, []Value{v})
}

func (h *PubSub) publishAll(fb *Fiber, xs []Value) bool {
	h.mu.Lock()
	if h.isShutdown || h.hasEnded {
		h.mu.Unlock()
		return false
	}
	surplus := h.atomic.publishAll(xs)
	h.completeSubscribers()
	if len(surplus) == 0 {
		h.mu.Unlock()
		return true
	}
	return h.handleSurplus(fb, surplus)
}

// handleSurplus is the strategy's, with the lock held, which it releases.
func (h *PubSub) handleSurplus(fb *Fiber, xs []Value) bool {
	switch h.strategy {
	case hubDropping:
		h.mu.Unlock()
		return false
	case hubSliding:
		defer h.mu.Unlock()
		if h.hasEnded {
			return false
		}
		h.slidingPublish(xs)
		h.completeSubscribers()
		return true
	}
	// the publisher may have yielded since `publish` checked the end
	if h.hasEnded {
		h.mu.Unlock()
		return false
	}
	d := newHubSignal()
	for i, x := range xs {
		h.publishers = append(h.publishers, hubPublisher{x, d, i == len(xs)-1})
	}
	h.onEmptySpace()
	h.completeSubscribers()
	remove := func() {
		h.publishers = slices.DeleteFunc(h.publishers, func(p hubPublisher) bool { return p.d == d })
	}
	if h.isShutdown {
		remove()
		return h.interruptSelf(fb).(bool)
	}
	return d.await(fb, h, remove).(bool)
}

func (h *PubSub) slidingPublish(xs []Value) {
	if len(xs) == 0 || h.atomic.capacity() <= 0 {
		return
	}
	for i := 0; i < len(xs); {
		h.atomic.slide()
		if h.atomic.publish(xs[i]) {
			i++
		}
	}
}

// onEmptySpace is the strategy's `onPubSubEmptySpaceUnsafe`: under back
// pressure, the waiting publishers' messages move into the room there is.
func (h *PubSub) onEmptySpace() {
	if h.strategy != hubBackPressure {
		return
	}
	for !h.atomic.isFull() && len(h.publishers) > 0 {
		p := h.publishers[0]
		h.publishers = h.publishers[1:]
		published := h.atomic.publish(p.value)
		if published && p.last {
			p.d.settle(true, nil)
		} else if !published {
			h.publishers = append([]hubPublisher{p}, h.publishers...)
		}
		h.completeSubscribers()
	}
}

// completePollers is Effect's `strategyCompletePollersUnsafe`.
func (h *PubSub) completePollers(s *Subscription) {
	for !s.backing.isEmpty() {
		if len(s.pollers) == 0 {
			h.removeSubscriber(s)
			return
		}
		p := s.pollers[0]
		v, ok := s.backing.poll()
		if !ok {
			return
		}
		s.pollers = s.pollers[1:]
		p.settle(v, nil)
		h.onEmptySpace()
	}
}

func (h *PubSub) completeSubscribers() {
	for _, s := range slices.Clone(h.subscribers) {
		h.completePollers(s)
	}
}

func (h *PubSub) addSubscriber(s *Subscription) {
	if !s.subscribed {
		s.subscribed = true
		h.subscribers = append(h.subscribers, s)
	}
}

func (h *PubSub) removeSubscriber(s *Subscription) {
	if s.subscribed {
		s.subscribed = false
		h.subscribers = withoutOne(h.subscribers, s)
	}
}

// subscribe is Effect's: the subscription ends when the owner closes, or
// when the hub shuts down; at once on a hub already shut down.
func (h *PubSub) subscribe(fb *Fiber) *Subscription {
	h.mu.Lock()
	s := &Subscription{hub: h, backing: h.atomic.subscribe(), replay: newReplayWindow(h.atomic.replay())}
	closed := isClosed(h.shutdownHook)
	if !closed {
		h.live = append(h.live, s)
	}
	h.mu.Unlock()
	if closed {
		s.unsubscribe(fb)
		return s
	}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		s.unsubscribe(fb)
		return Unit
	})
	return s
}

// unsubscribe is Effect's: the first call interrupts the takers and gives
// the subscription's messages up.
func (s *Subscription) unsubscribe(fb *Fiber) {
	h := s.hub
	h.mu.Lock()
	defer h.mu.Unlock()
	s.shutdown = true
	if s.closed {
		return
	}
	s.closed = true
	interrupt := &Cause{kind: causeInterrupt, fiber: fb.state.id}
	for _, p := range s.pollers {
		p.settle(nil, interrupt)
	}
	s.pollers = nil
	h.removeSubscriber(s)
	s.backing.unsubscribe()
	s.replay.close()
	h.onEmptySpace()
	h.live = withoutOne(h.live, s)
}

// pollNow is a message without waiting when no taker waits before.
func (s *Subscription) pollNow() (Value, bool) {
	if len(s.pollers) > 0 {
		return nil, false
	}
	v, ok := s.backing.poll()
	if ok {
		s.hub.onEmptySpace()
	}
	return v, ok
}

// take is Effect's, with the hub locked, which it releases.
func (s *Subscription) take(fb *Fiber) Value {
	if s.shutdown {
		return s.hub.interruptSelf(fb)
	}
	if s.replay.remaining > 0 {
		v := s.replay.take()
		s.hub.mu.Unlock()
		return v
	}
	if v, ok := s.pollNow(); ok {
		s.hub.mu.Unlock()
		return v
	}
	return s.pollForItem(fb)
}

// pollForItem waits for the next message, with the hub locked, which it
// releases.
func (s *Subscription) pollForItem(fb *Fiber) Value {
	h := s.hub
	if s.shutdown {
		return h.interruptSelf(fb)
	}
	if h.hasEnded {
		// messages may have come after the empty check
		v, ok := s.pollNow()
		if !ok {
			v = h.ended
		}
		h.mu.Unlock()
		return v
	}
	d := newHubSignal()
	h.addSubscriber(s)
	s.pollers = append(s.pollers, d)
	h.completePollers(s)
	return d.await(fb, h, func() { s.pollers = withoutOne(s.pollers, d) })
}

func (s *Subscription) takeAll(fb *Fiber) []Value {
	var value []Value
	for {
		h := s.hub
		h.mu.Lock()
		if s.shutdown {
			return h.interruptSelf(fb).([]Value)
		}
		as := []Value{}
		if len(s.pollers) == 0 {
			as = s.backing.pollUpTo(math.MaxInt)
		}
		if value != nil {
			as = append(value, as...)
		}
		h.onEmptySpace()
		if s.replay.remaining > 0 {
			out := append(s.replay.takeAll(), as...)
			h.mu.Unlock()
			return out
		}
		if len(as) > 0 {
			h.mu.Unlock()
			return as
		}
		value = []Value{s.pollForItem(fb)}
	}
}

func (s *Subscription) takeUpTo(fb *Fiber, n int) []Value {
	h := s.hub
	h.mu.Lock()
	if s.shutdown {
		return h.interruptSelf(fb).([]Value)
	}
	defer h.mu.Unlock()
	n = max(n, 0)
	var replay []Value
	if s.replay.remaining >= n {
		return s.replay.takeN(n)
	} else if s.replay.remaining > 0 {
		replay = s.replay.takeAll()
		n -= len(replay)
	}
	as := []Value{}
	if len(s.pollers) == 0 {
		as = s.backing.pollUpTo(n)
	}
	h.onEmptySpace()
	if replay == nil {
		return as
	}
	return append(replay, as...)
}

// takeBetween is Effect's `takeRemainderLoop`.
func (s *Subscription) takeBetween(fb *Fiber, lo, hi int) []Value {
	lo, hi = max(lo, 0), max(hi, 0)
	acc := []Value{}
	for hi >= lo {
		bs := s.takeUpTo(fb, hi)
		acc = append(acc, bs...)
		remaining := lo - len(bs)
		if remaining < 1 {
			break
		}
		s.hub.mu.Lock()
		acc = append(acc, s.take(fb))
		if remaining == 1 {
			break
		}
		lo, hi = remaining-1, hi-len(bs)-1
	}
	return acc
}

func (s *Subscription) remaining(fb *Fiber) int {
	h := s.hub
	h.mu.Lock()
	if s.shutdown {
		return h.interruptSelf(fb).(int)
	}
	defer h.mu.Unlock()
	return s.backing.size() + s.replay.remaining
}

// The bindings of `PubSub`.

// boundedHub is a hub of the `Bounds` options.
func boundedHub(options Value, strategy hubStrategy) Value {
	o := record(options)
	return newPubSub(makeAtomicBounded(int(o.Get("capacity").(int32)), hubReplay(o.Get("replay"))), strategy)
}

func PubSubBounded(options Value) Value  { return boundedHub(options, hubBackPressure) }
func PubSubDropping(options Value) Value { return boundedHub(options, hubDropping) }
func PubSubSliding(options Value) Value  { return boundedHub(options, hubSliding) }

func PubSubUnbounded(options Value) Value {
	return newPubSub(makeAtomicUnbounded(hubReplay(record(options).Get("replay"))), hubDropping)
}

// PubSubCapacity is the hub's capacity; an unbounded one's is the largest
// Int.
//
// shortcut: Effect's unbounded capacity is 2^53-1, beyond Int on Go; make Int wider if a program reads it
func PubSubCapacity(hub Value) Value {
	return int32(min(pubSubOf(hub).atomic.capacity(), math.MaxInt32))
}

func PubSubSize(hub Value) Value { return int32(pubSubOf(hub).size()) }

func PubSubIsFull(hub Value) Value {
	h := pubSubOf(hub)
	return h.size() == h.atomic.capacity()
}

func PubSubIsEmpty(hub Value) Value { return pubSubOf(hub).size() == 0 }

func PubSubShutdown(fb *Fiber, hub Value) Value {
	pubSubOf(hub).shutdown(fb)
	return Unit
}

func PubSubEnd(v, hub Value) Value { return pubSubOf(hub).end(v) }

func PubSubIsShutdown(hub Value) Value {
	h := pubSubOf(hub)
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.isShutdown
}

func PubSubAwaitShutdown(fb *Fiber, hub Value) Value {
	wait(fb, pubSubOf(hub).shutdownHook)
	return Unit
}

func PubSubPublish(fb *Fiber, v, hub Value) Value { return pubSubOf(hub).publish(fb, v) }

func PubSubPublishAll(fb *Fiber, xs, hub Value) Value { return pubSubOf(hub).publishAll(fb, List(xs)) }

func PubSubSubscribe(fb *Fiber, hub Value) Value { return pubSubOf(hub).subscribe(fb) }

func subscriptionOf(v Value) *Subscription { return v.(*Subscription) }

func PubSubTake(fb *Fiber, sub Value) Value {
	s := subscriptionOf(sub)
	s.hub.mu.Lock()
	return s.take(fb)
}

func PubSubTakeAll(fb *Fiber, sub Value) Value { return subscriptionOf(sub).takeAll(fb) }

func PubSubTakeUpTo(fb *Fiber, n, sub Value) Value {
	return subscriptionOf(sub).takeUpTo(fb, int(n.(int32)))
}

func PubSubTakeBetween(fb *Fiber, lo, hi, sub Value) Value {
	return subscriptionOf(sub).takeBetween(fb, int(lo.(int32)), int(hi.(int32)))
}

func PubSubRemaining(fb *Fiber, sub Value) Value { return int32(subscriptionOf(sub).remaining(fb)) }

// StreamFromSubscription is Effect's: the chunks of messages there are, as
// they come.
func StreamFromSubscription(sub Value) Value {
	return &Stream{channelFromSubscriptionArray(subscriptionOf(sub))}
}
