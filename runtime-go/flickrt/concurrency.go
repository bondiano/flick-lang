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

// concurrent tells whether a fiber has run beside main.
var concurrent atomic.Bool

// fork runs `task` as a child of `fb` in `owner`, calling `ended` when it
// ends.
func fork(fb *Fiber, owner *Owner, task Value, ended func(*child)) *child {
	return spawn(fb, owner, task, nil, ended)
}

// spawn is fork that closes `starting`, when given, once the child first
// blocks. A virtual clock counts the child as running until it ends.
func spawn(fb *Fiber, owner *Owner, task Value, starting chan struct{}, ended func(*child)) *child {
	concurrent.Store(true)
	c := &child{state: newState(), done: make(chan struct{})}
	inner := *fb
	inner.state, inner.owner = c.state, owner
	c.state.starting = starting
	tc := fb.state.clock
	if tc != nil {
		c.state.clock = tc
		tc.count(1)
	}
	go func() {
		c.value, c.cause = catch(func() Value { return Run(&inner, task) })
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
		if tc != nil {
			tc.count(-1)
		}
	}()
	return c
}

// stop interrupts children and awaits them.
func stop(fb *Fiber, children ...*child) {
	park(fb)
	defer unpark(fb)
	for _, c := range children {
		c.state.interrupt(fb.state.id)
	}
	for _, c := range children {
		<-c.done
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

var (
	reportedMu sync.Mutex
	reported   []*Cause // failures of supervisors, which fail the program
)

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
	watcher.state = newState()
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
		_, failed := catch(func() Value { return Run(&watcher, Apply(supervisor, &Supervised{c.cause, shutdown})) })
		if failed != nil {
			reportedMu.Lock()
			reported = append(reported, failed)
			reportedMu.Unlock()
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
				park(fb)
				select {
				case permits <- struct{}{}:
					unpark(fb)
				case <-fb.state.signal:
					unpark(fb)
					panic(fb.state.stopped())
				}
				defer func() { <-permits }()
				return Run(fb, task)
			})
		}
		children[i] = fork(fb, fb.owner, t, func(c *child) { ended <- c })
	}
	for range tasks {
		c := nextEnded(fb, ended)
		if c == nil || c.cause != nil {
			failure := fb.state.stopped()
			if c != nil {
				failure = &Failure{c.cause}
			}
			stop(fb, children...)
			panic(failure)
		}
	}
	out := make([]Value, len(tasks))
	for i, c := range children {
		out[i] = c.value
	}
	return out
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

// nextEnded is the next child to end, or nil when the fiber is interrupted
// first.
func nextEnded(fb *Fiber, ended <-chan *child) *child {
	park(fb)
	defer unpark(fb)
	if fb.state.mask > 0 {
		return <-ended
	}
	select {
	case c := <-ended:
		return c
	case <-fb.state.signal:
		return nil
	}
}

// raceAll runs the tasks: the first to succeed wins, or with `first` the
// first to end however it ends; the others are interrupted and awaited.
// When every one fails, their causes combine. As Effect runs a branch at
// once until it suspends, each starts once the one before has blocked or
// ended, and none starts after a winner.
func raceAll(fb *Fiber, tasks []Value, first bool) Value {
	ended := make(chan *child, len(tasks))
	children := make([]*child, 0, len(tasks))
	var failed *Cause
	pending := 0
	// whether `c`, which ended, won: it ends the race
	won := func(c *child) bool {
		pending--
		if c.cause == nil || first {
			stop(fb, children...)
			return true
		}
		failed = Parallel(failed, c.cause)
		return false
	}
	for _, t := range tasks {
		starting := make(chan struct{})
		c := spawn(fb, fb.owner, t, starting, func(c *child) { ended <- c })
		children = append(children, c)
		pending++
		for started := false; !started; {
			park(fb)
			var e *child
			select {
			case <-starting:
				started = true
			case e = <-ended:
				started = e == c
			}
			unpark(fb)
			if e != nil && won(e) {
				return e.result()
			}
		}
	}
	for pending > 0 {
		c := nextEnded(fb, ended)
		if c == nil {
			stop(fb, children...)
			panic(fb.state.stopped())
		}
		if won(c) {
			return c.result()
		}
	}
	panic(&Failure{failed})
}

func RaceSuccess(fb *Fiber, a, b Value) Value { return raceAll(fb, []Value{a, b}, false) }
func RaceFirst(fb *Fiber, a, b Value) Value   { return raceAll(fb, []Value{a, b}, true) }

// timeoutAfter is the task's result, or `late()` when `millis` pass first,
// the task interrupted and awaited.
func timeoutAfter(fb *Fiber, millis Value, task Value, done func(Value) Value, late func() Value) Value {
	return within(fb, time.Duration(millis.(float64)*float64(time.Millisecond)), task, done, late)
}

// within is the task's result, or `late()` when `d` passes first on the
// fiber's clock, the task interrupted and awaited.
func within(fb *Fiber, d time.Duration, task Value, done func(Value) Value, late func() Value) Value {
	c := fork(fb, fb.owner, task, nil)
	a := clockOf(fb).after(d)
	signal := fb.state.signal
	if fb.state.mask > 0 {
		signal = nil
	}
	park(fb)
	select {
	case <-c.done:
		unpark(fb)
		a.stop()
		return done(c.result())
	case <-a.C:
		unpark(fb)
		a.stop()
		stop(fb, c)
		return late()
	case <-signal:
		unpark(fb)
		a.stop()
		stop(fb, c)
		panic(fb.state.stopped())
	}
}

var timeoutError = NewData(Tag("Prelude.TimeoutError.TimeoutError"))

func Timeout(fb *Fiber, millis, task Value) Value {
	return timeoutAfter(fb, millis, task, func(v Value) Value { return v }, func() Value { return Fail(timeoutError) })
}

func TimeoutOption(fb *Fiber, millis, task Value) Value {
	return timeoutAfter(fb, millis, task, Some, func() Value { return None })
}

// sleep blocks for `d` on the fiber's clock, or forever when it is
// infinite.
func sleep(fb *Fiber, d time.Duration, forever bool) {
	if forever {
		wait(fb, nil)
		return
	}
	a := clockOf(fb).after(d)
	park(fb)
	if fb.state.mask > 0 {
		<-a.C
		unpark(fb)
		a.stop()
		return
	}
	select {
	case <-a.C:
		unpark(fb)
		a.stop()
	case <-fb.state.signal:
		unpark(fb)
		a.stop()
		panic(fb.state.stopped())
	}
}

// EffectSleep is `Effect.sleep duration`.
func EffectSleep(fb *Fiber, d Value) Value {
	x := dur(d)
	if x.negative() {
		return Unit
	}
	sleep(fb, toGoDuration(x), x.infinite())
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
	return masked(fb, fb.state.mask+1, func() Value { return Run(fb, task) })
}

func Interruptible(fb *Fiber, task Value) Value {
	return masked(fb, 0, func() Value { return Run(fb, task) })
}

// regionMask runs `f restore` with interruptibility `mask`; `restore` gives
// a task the interruptibility around the call.
func regionMask(fb *Fiber, mask int, f Value) Value {
	outer := fb.state.mask
	restore := Fn(func(task Value) Value {
		return Eff(func(fb *Fiber) Value { return masked(fb, outer, func() Value { return Run(fb, task) }) })
	})
	return masked(fb, mask, func() Value { return Run(fb, Apply(f, restore)) })
}

func UninterruptibleMask(fb *Fiber, f Value) Value { return regionMask(fb, fb.state.mask+1, f) }
func InterruptibleMask(fb *Fiber, f Value) Value   { return regionMask(fb, 0, f) }

// EffectEnsuring runs `finalizer`, uninterruptibly, however `task` ends.
func EffectEnsuring(fb *Fiber, finalizer, task Value) Value {
	v, c := catch(func() Value { return Run(fb, task) })
	_, f := catch(func() Value { return Uninterruptible(fb, finalizer) })
	if c = Sequential(c, f); c != nil {
		panic(&Failure{c})
	}
	return v
}
