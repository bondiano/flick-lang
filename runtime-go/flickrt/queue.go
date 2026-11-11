package flickrt

import (
	"math"
	"sync"
)

// Queue is messages up to a capacity, a strategy for a full queue, and an
// end. Failing or ending it lets takers drain what is left, then fail as
// it was failed; a take is a Pull, which a `Done` ends.
//
// shortcut: takers are woken at once, where the JS runtime schedules their release on its dispatcher, so the chunks a taker gets while a producer is offering may differ from JS's; batch the release if a program depends on them
type Queue struct {
	mu       sync.Mutex
	capacity int
	strategy queueStrategy
	messages []Value
	state    queueState
	exit     *Cause // what takes fail with, once closing
	takers   []*queueTaker
	offers   []*queueOffer
	ended    chan struct{} // closed once done
}

type queueStrategy int

const (
	queueSuspend queueStrategy = iota
	queueDropping
	queueSliding
)

type queueState int

const (
	queueOpen queueState = iota
	queueClosing
	queueDone
)

const unboundedCapacity = math.MaxInt

// queueTaker waits until `ready`; waking closes `wake`.
type queueTaker struct {
	ready func() bool
	wake  chan struct{}
}

// queueOffer is an offer waiting for room: one message, or the messages
// of `remaining` from `offset` on. Settling it sets `accepted` or `left`
// and closes `wake`.
type queueOffer struct {
	single    bool
	message   Value
	remaining []Value
	offset    int
	accepted  bool
	left      []Value
	wake      chan struct{}
}

func newQueue(capacity int, strategy queueStrategy) *Queue {
	return &Queue{capacity: capacity, strategy: strategy, ended: make(chan struct{})}
}

func queueOf(v Value) *Queue { return v.(*Queue) }

// offerLocked offers a message; the caller holds the lock, as for every
// method ending in Locked and those it calls.
func (q *Queue) offerLocked(x Value) bool {
	if q.state != queueOpen {
		return false
	}
	if len(q.messages) >= q.capacity {
		switch {
		case q.strategy == queueSliding:
			if len(q.messages) > 0 {
				q.messages = q.messages[1:]
			}
			q.messages = append(q.messages, x)
			return true
		case q.capacity <= 0 && len(q.takers) > 0:
			q.messages = append(q.messages, x)
			q.releaseTakers()
			return true
		}
		return false
	}
	q.messages = append(q.messages, x)
	q.releaseTakers()
	return true
}

// offer adds the message, waiting for room in a full `Suspend` queue.
func (q *Queue) offer(fb *Fiber, x Value) bool {
	q.mu.Lock()
	if q.offerLocked(x) {
		q.mu.Unlock()
		return true
	}
	if q.state != queueOpen || q.strategy != queueSuspend {
		q.mu.Unlock()
		return false
	}
	o := &queueOffer{single: true, message: x, wake: make(chan struct{})}
	q.offers = append(q.offers, o)
	q.mu.Unlock()
	q.waitOffer(fb, o)
	return o.accepted
}

func (q *Queue) offerAllLocked(xs []Value) []Value {
	if q.state != queueOpen {
		return append([]Value{}, xs...)
	}
	if q.capacity == unboundedCapacity || q.strategy == queueSliding {
		q.messages = append(q.messages, xs...)
		if over := len(q.messages) - q.capacity; q.strategy == queueSliding && over > 0 {
			q.messages = q.messages[over:]
		}
		q.releaseTakers()
		return []Value{}
	}
	free := q.capacity - len(q.messages)
	if q.capacity <= 0 {
		free = len(q.takers)
	}
	if free == 0 {
		return append([]Value{}, xs...)
	}
	remaining := []Value{}
	for i, x := range xs {
		if i < free {
			q.messages = append(q.messages, x)
		} else {
			remaining = append(remaining, x)
		}
	}
	q.releaseTakers()
	return remaining
}

// offerAll adds the messages in order, waiting for room for the rest in a
// full `Suspend` queue; those not added.
func (q *Queue) offerAll(fb *Fiber, xs []Value) []Value {
	q.mu.Lock()
	remaining := q.offerAllLocked(xs)
	if len(remaining) == 0 || q.strategy == queueDropping || q.state != queueOpen {
		q.mu.Unlock()
		return remaining
	}
	o := &queueOffer{remaining: remaining, wake: make(chan struct{})}
	q.offers = append(q.offers, o)
	q.mu.Unlock()
	q.waitOffer(fb, o)
	return o.left
}

// waitOffer waits for the offer to settle; an interrupted one is
// withdrawn.
func (q *Queue) waitOffer(fb *Fiber, o *queueOffer) {
	if _, c := catch(func() Value { wait(fb, o.wake); return nil }); c != nil {
		q.mu.Lock()
		if q.state != queueDone {
			q.offers = withoutOne(q.offers, o)
			if q.state == queueClosing && len(q.offers) == 0 && len(q.messages) == 0 {
				q.finalize(q.exit)
			}
		}
		q.mu.Unlock()
		panic(&Failure{c})
	}
}

func withoutOne[T comparable](xs []T, x T) []T {
	for i, y := range xs {
		if y == x {
			return append(xs[:i], xs[i+1:]...)
		}
	}
	return xs
}

// failCause ends the queue with the cause once it is drained; whether
// this call ended it.
func (q *Queue) failCause(c *Cause) bool {
	q.mu.Lock()
	defer q.mu.Unlock()
	if q.state != queueOpen {
		return false
	}
	if len(q.offers) == 0 && len(q.messages) == 0 {
		q.finalize(c)
		return true
	}
	q.state, q.exit = queueClosing, c
	q.releaseTakers()
	return true
}

func (q *Queue) end() bool { return q.failCause(doneCause(Unit)) }

// shutdown ends the queue at once, dropping its messages and offers.
func (q *Queue) shutdown() bool {
	q.mu.Lock()
	defer q.mu.Unlock()
	if q.state == queueDone {
		return false
	}
	q.messages = nil
	offers := q.offers
	exit := q.exit
	if q.state == queueOpen {
		exit = &Cause{kind: causeInterrupt}
	}
	q.finalize(exit)
	for _, o := range offers {
		o.unoffered()
	}
	return true
}

func (o *queueOffer) unoffered() {
	if !o.single {
		o.left = o.remaining[o.offset:]
	}
	close(o.wake)
}

// releaseTakers wakes the takers that are ready, in order, until the
// messages run out.
func (q *Queue) releaseTakers() {
	if q.state == queueDone {
		return
	}
	for i := 0; i < len(q.takers); {
		t := q.takers[i]
		if !t.ready() {
			i++
			continue
		}
		q.takers = append(q.takers[:i], q.takers[i+1:]...)
		close(t.wake)
		if len(q.messages) == 0 {
			break
		}
	}
}

func (q *Queue) canTake(lo int) bool {
	need := q.capacity
	if need == 0 {
		need = 1
	}
	need = min(lo, need)
	if q.state == queueClosing {
		need = 1
	}
	return len(q.messages) >= need || (q.capacity <= 0 && q.state != queueDone && len(q.offers) > 0)
}

// awaitTake waits until `ready`, or fails as the queue ended.
func (q *Queue) awaitTake(fb *Fiber, ready func() bool) {
	q.mu.Lock()
	if q.state == queueDone {
		c := q.exit
		q.mu.Unlock()
		panic(&Failure{c})
	}
	if ready() {
		q.mu.Unlock()
		return
	}
	t := &queueTaker{ready: ready, wake: make(chan struct{})}
	q.takers = append(q.takers, t)
	q.mu.Unlock()
	if _, c := catch(func() Value { wait(fb, t.wake); return nil }); c != nil {
		q.mu.Lock()
		if n := len(q.takers); q.state != queueDone {
			// a taker woken but gone passes its wake on
			if q.takers = withoutOne(q.takers, t); len(q.takers) == n {
				q.releaseTakers()
			}
		}
		q.mu.Unlock()
		panic(&Failure{c})
	}
}

// takeOffer takes the first message of the first offer.
func (q *Queue) takeOffer() Value {
	o := q.offers[0]
	if o.single {
		q.offers = q.offers[1:]
		o.accepted = true
		close(o.wake)
		return o.message
	}
	x := o.remaining[o.offset]
	o.offset++
	if o.offset == len(o.remaining) {
		q.offers = q.offers[1:]
		o.left = []Value{}
		close(o.wake)
	}
	return x
}

// releaseCapacity moves waiting offers into the room there is, and ends a
// closing queue once drained.
func (q *Queue) releaseCapacity() {
	if q.state == queueDone {
		return
	}
	if len(q.offers) == 0 {
		if q.state == queueClosing && len(q.messages) == 0 {
			q.finalize(q.exit)
		}
		return
	}
	for len(q.offers) > 0 {
		o := q.offers[0]
		n := q.capacity - len(q.messages)
		if n <= 0 {
			return
		}
		if o.single {
			q.messages = append(q.messages, o.message)
			o.accepted = true
		} else {
			for ; o.offset < len(o.remaining); o.offset++ {
				if n == 0 {
					return
				}
				q.messages = append(q.messages, o.remaining[o.offset])
				n--
			}
			o.left = []Value{}
		}
		q.offers = q.offers[1:]
		close(o.wake)
	}
}

func (q *Queue) finalize(exit *Cause) {
	if q.state == queueDone {
		return
	}
	q.state, q.exit = queueDone, exit
	for _, t := range q.takers {
		close(t.wake)
	}
	q.takers, q.offers = nil, nil
	close(q.ended)
}

// takeLocked is a message, the cause of a done queue, or false when a take
// must wait.
func (q *Queue) takeLocked() (Value, *Cause, bool) {
	if q.state == queueDone {
		return nil, q.exit, true
	}
	var x Value
	switch {
	case len(q.messages) > 0:
		x = q.messages[0]
		q.messages = q.messages[1:]
	case q.capacity <= 0 && len(q.offers) > 0:
		x = q.takeOffer()
	default:
		return nil, nil, false
	}
	q.releaseCapacity()
	return x, nil, true
}

// take is the queue's pull: the next message, waiting for one.
func (q *Queue) take(fb *Fiber) Value {
	for {
		q.mu.Lock()
		x, c, ok := q.takeLocked()
		q.mu.Unlock()
		if c != nil {
			panic(&Failure{c})
		}
		if ok {
			return x
		}
		q.awaitTake(fb, func() bool { return q.canTake(1) })
	}
}

// poll is the next message if there is one, without waiting.
func (q *Queue) poll() (Value, bool) {
	q.mu.Lock()
	defer q.mu.Unlock()
	x, c, ok := q.takeLocked()
	return x, ok && c == nil
}

func (q *Queue) takeBetweenLocked(lo, hi int) ([]Value, *Cause, bool) {
	if q.state == queueDone {
		return nil, q.exit, true
	}
	if hi <= 0 || lo <= 0 {
		return []Value{}, nil, true
	}
	if !q.canTake(lo) {
		return nil, nil, false
	}
	var xs []Value
	if len(q.messages) > 0 {
		n := min(hi, len(q.messages))
		xs = append([]Value{}, q.messages[:n]...)
		q.messages = q.messages[n:]
	} else {
		xs = []Value{q.takeOffer()}
	}
	q.releaseCapacity()
	return xs, nil, true
}

// takeBetween takes at least `lo` messages and at most `hi`, waiting
// for them; a closing queue gives what is left.
func (q *Queue) takeBetween(fb *Fiber, lo, hi int) []Value {
	lo, hi = max(lo, 0), max(hi, 0)
	for {
		q.mu.Lock()
		xs, c, ok := q.takeBetweenLocked(lo, hi)
		q.mu.Unlock()
		if c != nil {
			panic(&Failure{c})
		}
		if ok {
			return xs
		}
		q.awaitTake(fb, func() bool { return q.canTake(lo) })
	}
}

// takeAll is every message there is, waiting for one.
func (q *Queue) takeAll(fb *Fiber) Value { return q.takeBetween(fb, 1, unboundedCapacity) }

func (q *Queue) peek(fb *Fiber) Value {
	for {
		q.mu.Lock()
		if q.state == queueDone {
			c := q.exit
			q.mu.Unlock()
			panic(&Failure{c})
		}
		if len(q.messages) > 0 {
			x := q.messages[0]
			q.mu.Unlock()
			return x
		}
		q.mu.Unlock()
		q.awaitTake(fb, func() bool { return len(q.messages) > 0 })
	}
}

// clear takes every message there is without waiting; a failed queue
// fails.
func (q *Queue) clear() []Value {
	q.mu.Lock()
	defer q.mu.Unlock()
	if q.state == queueDone {
		if _, rest, ok := filterDone(q.exit); !ok {
			panic(&Failure{rest})
		}
		return []Value{}
	}
	var xs []Value
	switch {
	case len(q.messages) > 0:
		xs = q.messages
		q.messages = nil
	case len(q.offers) > 0:
		xs = []Value{q.takeOffer()}
	default:
		xs = []Value{}
	}
	q.releaseCapacity()
	return xs
}

// await waits until the queue is done, failing as it failed.
func (q *Queue) await(fb *Fiber) {
	wait(fb, q.ended)
	q.mu.Lock()
	c := q.exit
	q.mu.Unlock()
	if _, rest, ok := filterDone(c); !ok {
		panic(&Failure{rest})
	}
}

func (q *Queue) size() int {
	q.mu.Lock()
	defer q.mu.Unlock()
	if q.state == queueDone {
		return 0
	}
	return len(q.messages)
}
