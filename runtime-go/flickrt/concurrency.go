package flickrt

import (
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"time"
)

// Each fiber is a goroutine. A child ends with its value or the cause it
// failed with; interruption is cooperative, at checkpoints and blocking
// waits.
type child struct {
	state     *fiberState
	done      chan struct{}
	value     Value
	cause     *Cause
	mu        sync.Mutex
	ended     bool
	observers []func(*child) // run when it ends, before anyone awaiting it resumes
}

// observe runs `f` when the child ends, or now if it has.
func (c *child) observe(f func(*child)) {
	c.mu.Lock()
	if !c.ended {
		c.observers = append(c.observers, f)
		c.mu.Unlock()
		return
	}
	c.mu.Unlock()
	f(c)
}

// fork runs `task` as a child of `fb` in `owner`, calling `ended` when it
// ends.
// concurrent tells whether a fiber has run beside main.
var concurrent atomic.Bool

func fork(fb *Fiber, owner *Owner, task Value, ended func(*child)) *child {
	concurrent.Store(true)
	c := &child{state: newState(), done: make(chan struct{})}
	inner := *fb
	inner.state, inner.owner = c.state, owner
	tc := counted(fb)
	if tc != nil {
		c.state.clock = tc
		tc.running.Add(1)
	}
	recorded := fiberMetrics(&inner)
	go func() {
		if tc != nil {
			defer tc.running.Add(-1)
		}
		c.value, c.cause = catch(func() Value { return Run(&inner, task) })
		if recorded != nil {
			recorded(c.cause)
		}
		c.mu.Lock()
		c.ended = true
		observers := c.observers
		c.mu.Unlock()
		for _, f := range observers {
			f(c)
		}
		close(c.done)
		if ended != nil {
			ended(c)
		}
	}()
	return c
}

// stop interrupts children and awaits them.
func stop(fb *Fiber, children ...*child) {
	for _, c := range children {
		c.state.interrupt(fb.state.id)
	}
	if tc := parking(fb); tc != nil {
		defer tc.park(func() bool {
			for _, c := range children {
				if !isClosed(c.done) {
					return false
				}
			}
			return true
		})()
	}
	for _, c := range children {
		<-c.done
	}
}

// nextEnded is the next child to end, nil when `signal` comes first.
func nextEnded(fb *Fiber, ended chan *child, signal <-chan struct{}) *child {
	if tc := parking(fb); tc != nil {
		defer tc.park(func() bool { return len(ended) > 0 || isClosed(signal) })()
	}
	select {
	case c := <-ended:
		return c
	case <-signal:
		return nil
	}
}

// firstClosed is the index of the first of the channels, which are only ever
// closed, to close.
func firstClosed(fb *Fiber, a, b, c <-chan struct{}) int {
	if tc := parking(fb); tc != nil {
		defer tc.park(func() bool { return isClosed(a) || isClosed(b) || isClosed(c) })()
	}
	select {
	case <-a:
		return 0
	case <-b:
		return 1
	case <-c:
		return 2
	}
}

func (c *child) result() Value {
	if c.cause != nil {
		panic(&Failure{c.cause})
	}
	return c.value
}

// Supervised is the `Failure` a supervisor sees: the cause of a child, and
// whether its owner's closing interrupted it.
type Supervised struct {
	cause    *Cause
	shutdown bool
}

// FailurePrimary is the first typed error of a failure without defects.
func FailurePrimary(f Value) Value {
	c := f.(*Supervised).cause
	if c.has(causeDie) {
		return None
	}
	for _, r := range c.reasons() {
		if r.kind == causeFail {
			return Some(r.err)
		}
	}
	return None
}

// FiberSpawn is `Fiber.spawn owner supervisor task`: runs `task` as a child
// of `owner`. A failure of the child, its interruption by a closing owner
// included, goes to `supervisor`; a failure of the supervisor goes to the
// root reporter.
func FiberSpawn(fb *Fiber, owner, supervisor, task Value) Value {
	o := owner.(*Owner)
	o.mu.Lock()
	defer o.mu.Unlock()
	if !o.open {
		panic(Defect{"Error: the owner is closing: it admits no new children"})
	}
	o.watchers.Add(1)
	watcher := *fb
	watcher.state, watcher.owner = newState(), o
	watcher.state.mask = 1
	var c *child
	c = fork(fb, o, task, func(c *child) {
		defer o.watchers.Done()
		o.mu.Lock()
		delete(o.children, c)
		shutdown := !o.open && !c.cause.has(causeFail) && !c.cause.has(causeDie)
		o.mu.Unlock()
		if c.cause == nil {
			return
		}
		_, failed := catch(func() Value { return Run(&watcher, Apply(&watcher, supervisor, &Supervised{c.cause, shutdown})) })
		if failed != nil {
			o.report(failed)
		}
	})
	o.children[c] = struct{}{}
	return c
}

// FiberJoin is the result of a child, or its failure with the original cause.
func FiberJoin(fb *Fiber, f Value) Value {
	c := f.(*child)
	wait(fb, c.done)
	return c.result()
}

// FiberInterrupt interrupts a child and awaits its end.
func FiberInterrupt(fb *Fiber, f Value) Value {
	stop(fb, f.(*child))
	return Unit
}

// all runs the tasks concurrently, at most `limit` at a time when it is
// positive: their results in order, or the first failure, the others
// interrupted and awaited.
func all(fb *Fiber, tasks []Value, limit int) []Value {
	var permits chan struct{}
	if limit > 0 {
		permits = make(chan struct{}, limit)
	}
	ended := make(chan *child, len(tasks))
	children := make([]*child, len(tasks))
	for i, t := range tasks {
		if permits != nil {
			task := t
			t = Eff(func(fb *Fiber) Value {
				takePermit(fb, permits)
				defer func() { <-permits }()
				return Run(fb, task)
			})
		}
		children[i] = fork(fb, fb.owner, t, func(c *child) { ended <- c })
	}
	signal := signalOf(fb)
	for pending := len(tasks); pending > 0; pending-- {
		c := nextEnded(fb, ended, signal)
		if c == nil {
			stop(fb, children...)
			panic(fb.state.stopped())
		}
		if c.cause == nil {
			continue
		}
		// as Effect's forEachConcurrent: the others are interrupted, and what
		// they fail with besides that interruption joins the first failure
		failed := c.cause
		for _, other := range children {
			other.state.interrupt(fb.state.id)
		}
		for pending--; pending > 0; pending-- {
			if rest := withoutInterrupts(nextEnded(fb, ended, nil).cause); rest != nil {
				failed = Sequential(failed, rest)
			}
		}
		panic(&Failure{failed})
	}
	out := make([]Value, len(tasks))
	for i, c := range children {
		out[i] = c.value
	}
	return out
}

// takePermit takes one of the permits, or fails with the fiber's interruption.
func takePermit(fb *Fiber, permits chan struct{}) {
	if tc := parking(fb); tc != nil {
		defer tc.park(func() bool { return len(permits) < cap(permits) || isClosed(fb.state.signal) })()
	}
	select {
	case permits <- struct{}{}:
	case <-fb.state.signal:
		panic(fb.state.stopped())
	}
}

// ParallelPair is `parallel a b`: both results, or the first failure.
func ParallelPair(fb *Fiber, a, b Value) Value { return all(fb, []Value{a, b}, 0) }

// ParallelAll is `parallel_all limit tasks`: at most `limit` at a time.
func ParallelAll(fb *Fiber, limit, tasks Value) Value {
	n := limit.(int32)
	if n <= 0 {
		panic(Defect{fmt.Sprintf("RangeError: parallel_all needs a positive finite integer limit, not %d", n)})
	}
	return all(fb, List(tasks), int(n))
}

// ParallelResults is each branch's typed failure as an `Err`; a defect or
// an interruption still stops the group.
func ParallelResults(fb *Fiber, tasks Value) Value {
	in := List(tasks)
	wrapped := make([]Value, len(in))
	for i, t := range in {
		wrapped[i] = Eff(func(fb *Fiber) Value {
			v, c := catch(func() Value { return Run(fb, t) })
			if c == nil {
				return Ok(v)
			}
			if !c.has(causeDie) {
				for _, r := range c.reasons() {
					if r.kind == causeFail {
						return Err(r.err)
					}
				}
			}
			panic(&Failure{c})
		})
	}
	return all(fb, wrapped, 0)
}

// race runs both tasks: the first to end wins, or with `success` the first
// to succeed, both failures when neither does. The loser is interrupted
// and awaited.
func race(fb *Fiber, a, b Value, success bool) Value {
	ended := make(chan *child, 2)
	children := []*child{
		fork(fb, fb.owner, a, func(c *child) { ended <- c }),
		fork(fb, fb.owner, b, func(c *child) { ended <- c }),
	}
	var failed *Cause
	signal := signalOf(fb)
	for range children {
		c := nextEnded(fb, ended, signal)
		if c == nil {
			stop(fb, children...)
			panic(fb.state.stopped())
		}
		if c.cause == nil || !success {
			stop(fb, children...)
			return c.result()
		}
		failed = concat(failed, c.cause)
	}
	panic(&Failure{failed})
}

func RaceSuccess(fb *Fiber, a, b Value) Value { return race(fb, a, b, true) }
func RaceFirst(fb *Fiber, a, b Value) Value   { return race(fb, a, b, false) }

// timeoutAfter is the task's result, or `late()` when `millis` pass first,
// the task interrupted and awaited.
func timeoutAfter(fb *Fiber, millis Value, task Value, done func(Value) Value, late func() Value) Value {
	c := fork(fb, fb.owner, task, nil)
	timer, cancel := clockOf(fb).timer(time.Duration(millis.(float64) * float64(time.Millisecond)))
	defer cancel()
	switch firstClosed(fb, c.done, timer, signalOf(fb)) {
	case 0:
		return done(c.result())
	case 1:
		stop(fb, c)
		return late()
	}
	stop(fb, c)
	panic(fb.state.stopped())
}

var timeoutError = NewData(Tag("Prelude.TimeoutError.TimeoutError"))

func Timeout(fb *Fiber, millis, task Value) Value {
	return timeoutAfter(fb, millis, task, func(v Value) Value { return v }, func() Value { return Fail(timeoutError) })
}

func TimeoutOption(fb *Fiber, millis, task Value) Value {
	return timeoutAfter(fb, millis, task, Some, func() Value { return None })
}

// sleep blocks for `d` of the fiber's clock.
func sleep(fb *Fiber, d time.Duration) {
	ch, cancel := clockOf(fb).timer(d)
	defer cancel()
	wait(fb, ch)
}

// sleepFor is Effect's `sleep`: a duration that is not positive yields,
// an infinite one never ends.
func sleepFor(fb *Fiber, d *Duration) {
	switch {
	case !DurationIsPositive(d).(bool):
		EffectYieldNow(fb)
	case d.infinite():
		wait(fb, nil)
	default:
		sleep(fb, idleDuration(d))
	}
}

// EffectSleep is `Effect.sleep duration`.
func EffectSleep(fb *Fiber, d Value) Value {
	sleepFor(fb, dur(d))
	return Unit
}

func EffectNever(fb *Fiber) Value {
	wait(fb, nil)
	return Unit
}

func EffectYieldNow(fb *Fiber) Value {
	runtime.Gosched()
	Check(fb)
	return Unit
}

// EffectInterrupt interrupts the fiber itself.
func EffectInterrupt(fb *Fiber) Value {
	fb.state.interrupt(fb.state.id)
	panic(fb.state.stopped())
}

func Uninterruptible(fb *Fiber, task Value) Value {
	v := uninterruptible(fb, task)
	// a request that came in while masked interrupts as the mask ends, as in Effect
	Check(fb)
	return v
}

// uninterruptible runs `task` masked, leaving a pending interruption to the
// next checkpoint.
func uninterruptible(fb *Fiber, task Value) Value {
	return masked(fb, fb.state.mask+1, func() Value { return Run(fb, task) })
}

func Interruptible(fb *Fiber, task Value) Value {
	return masked(fb, 0, func() Value { return Run(fb, task) })
}

// regionMask runs `f restore` with interruptibility `mask`; `restore` gives
// a task the interruptibility around the call.
func regionMask(fb *Fiber, mask int, f Value) Value {
	outer := fb.state.mask
	restore := Fn(func(fb *Fiber, task Value) Value {
		return Eff(func(fb *Fiber) Value { return masked(fb, outer, func() Value { return Run(fb, task) }) })
	})
	v := masked(fb, mask, func() Value { return Run(fb, Apply(fb, f, restore)) })
	Check(fb)
	return v
}

func UninterruptibleMask(fb *Fiber, f Value) Value { return regionMask(fb, fb.state.mask+1, f) }
func InterruptibleMask(fb *Fiber, f Value) Value   { return regionMask(fb, 0, f) }

// EffectEnsuring runs `finalizer`, uninterruptibly, however `task` ends.
func EffectEnsuring(fb *Fiber, finalizer, task Value) Value {
	v, c := catch(func() Value { return Run(fb, task) })
	_, f := catch(func() Value { return uninterruptible(fb, finalizer) })
	if c = Sequential(c, f); c != nil {
		panic(&Failure{c})
	}
	return v
}
