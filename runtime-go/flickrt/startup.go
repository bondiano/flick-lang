package flickrt

import "sync"

// Turns. Effect runs a forked fiber after the fibers already scheduled,
// each up to its next suspension, so what those do before they suspend
// happens first: the requests fibers make "at the same time" meet in one
// batch. Go runs fibers at once; where that order matters, a fiber takes a
// turn: it waits until each fiber forked before the turn was queued has
// suspended once (a blocking wait, a yield, its end) and each turn queued
// before it has suspended, as Effect's queue of tasks would have it.
//
// A forked fiber belongs to the cohort of the fibers forked after the last
// turn until it first suspends; a turn is a cohort of the one fiber that
// takes it, from the moment it may run until it suspends.

// A cohort is fibers before their first suspension, or a turn.
type cohort struct {
	n       int           // its fibers that have not suspended
	turn    chan struct{} // a turn's, closed when it may run
	granted bool
	taken   bool // by its fiber, which runs in it
	next    *cohort
}

// startup is the cohorts in the order they were queued; forks join the tail.
var startup = struct {
	mu         sync.Mutex
	head, tail *cohort
}{head: firstCohort, tail: firstCohort}

var firstCohort = &cohort{}

// forked counts a new fiber in the tail cohort until it first suspends.
func forked(s *fiberState) {
	startup.mu.Lock()
	startup.tail.n++
	s.cohort = startup.tail
	startup.mu.Unlock()
}

// suspended ends the fiber's run in its cohort, if it is in one.
func suspended(s *fiberState) {
	c := s.cohort
	if c == nil {
		return
	}
	s.cohort = nil
	startup.mu.Lock()
	c.n--
	advance()
	startup.mu.Unlock()
}

// advance drops the cohorts at the head that are done and lets the turn
// after them run. The caller holds the lock.
func advance() {
	for h := startup.head; h.n == 0 && h != startup.tail; h = startup.head {
		startup.head = h.next
		if t := startup.head; t.turn != nil && !t.granted {
			t.granted = true
			close(t.turn)
		}
	}
}

// queueTurn queues a turn after the fibers forked so far and the turns
// queued before; the fibers forked from now on come after it.
func queueTurn() *cohort {
	t := &cohort{n: 1, turn: make(chan struct{})}
	t.next = &cohort{}
	startup.mu.Lock()
	startup.tail.next = t
	startup.tail = t.next
	advance()
	startup.mu.Unlock()
	return t
}

// awaitTurn waits for turn `t`, the fiber suspended meanwhile; it then runs
// in the turn until it next suspends. An interruption gives the turn up.
func awaitTurn(fb *Fiber, t *cohort) {
	if _, cause := catch(func() Value { wait(fb, t.turn); return Unit }); cause != nil {
		giveUp(t)
		panic(&Failure{cause})
	}
	startup.mu.Lock()
	t.taken = true
	startup.mu.Unlock()
	fb.state.cohort = t
}

// giveUp lets the turns after `t` run if its fiber never took it.
func giveUp(t *cohort) {
	startup.mu.Lock()
	if !t.taken {
		t.n = 0
		advance()
	}
	startup.mu.Unlock()
}

// forkInTurn forks `task` to run in a turn queued now; a fiber that ends
// without taking it, interrupted before it began, gives it up.
func forkInTurn(fb *Fiber, owner *Owner, task func(fb *Fiber)) *child {
	t := queueTurn()
	return fork(fb, owner, Eff(func(fb *Fiber) Value {
		awaitTurn(fb, t)
		task(fb)
		return Unit
	}), func(*child) { giveUp(t) })
}

// yieldTurn is Effect's `yieldNow` where the order matters: the fiber runs
// again after the fibers scheduled before it.
func yieldTurn(fb *Fiber) { awaitTurn(fb, queueTurn()) }
