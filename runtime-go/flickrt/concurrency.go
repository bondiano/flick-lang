package flickrt

import (
	"fmt"
	"runtime"
	"sync"
	"time"
)

// Each fiber is a goroutine. A child ends with its value or the cause it
// failed with; interruption is cooperative, at checkpoints and blocking
// waits.
type child struct {
	state *fiberState
	done  chan struct{}
	value Value
	cause *Cause
}

// fork runs `task` as a child of `fb` in `owner`, calling `ended` when it
// ends.
func fork(fb *Fiber, owner *Owner, task Value, ended func(*child)) *child {
	c := &child{state: newState(), done: make(chan struct{})}
	inner := *fb
	inner.state, inner.owner = c.state, owner
	go func() {
		c.value, c.cause = catch(func() Value { return Run(&inner, task) })
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
	reported   *Cause
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
			reported = Sequential(reported, failed)
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
				select {
				case permits <- struct{}{}:
				case <-fb.state.signal:
					panic(fb.state.stopped())
				}
				defer func() { <-permits }()
				return Run(fb, task)
			})
		}
		children[i] = fork(fb, fb.owner, t, func(c *child) { ended <- c })
	}
	var failure *Failure
	for range tasks {
		var c *child
		if fb.state.mask > 0 {
			c = <-ended
		} else {
			select {
			case c = <-ended:
			case <-fb.state.signal:
				failure = fb.state.stopped()
			}
		}
		if failure == nil && c.cause != nil {
			failure = &Failure{c.cause}
		}
		if failure != nil {
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
	for range children {
		var c *child
		if fb.state.mask > 0 {
			c = <-ended
		} else {
			select {
			case c = <-ended:
			case <-fb.state.signal:
				stop(fb, children...)
				panic(fb.state.stopped())
			}
		}
		if c.cause == nil || !success {
			stop(fb, children...)
			return c.result()
		}
		failed = Parallel(failed, c.cause)
	}
	panic(&Failure{failed})
}

func RaceSuccess(fb *Fiber, a, b Value) Value { return race(fb, a, b, true) }
func RaceFirst(fb *Fiber, a, b Value) Value   { return race(fb, a, b, false) }

// timeoutAfter is the task's result, or `late()` when `millis` pass first,
// the task interrupted and awaited.
func timeoutAfter(fb *Fiber, millis Value, task Value, done func(Value) Value, late func() Value) Value {
	c := fork(fb, fb.owner, task, nil)
	timer := time.NewTimer(time.Duration(millis.(float64) * float64(time.Millisecond)))
	defer timer.Stop()
	signal := fb.state.signal
	if fb.state.mask > 0 {
		signal = nil
	}
	select {
	case <-c.done:
		return done(c.result())
	case <-timer.C:
		stop(fb, c)
		return late()
	case <-signal:
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

// sleep blocks for `d`, or forever when it is infinite.
func sleep(fb *Fiber, d time.Duration, forever bool) {
	if forever {
		wait(fb, nil)
		return
	}
	timer := time.NewTimer(d)
	defer timer.Stop()
	if fb.state.mask > 0 {
		<-timer.C
		return
	}
	select {
	case <-timer.C:
	case <-fb.state.signal:
		panic(fb.state.stopped())
	}
}

// EffectSleep is `Effect.sleep duration`.
func EffectSleep(fb *Fiber, d Value) Value {
	x := dur(d)
	if x.negative() {
		return Unit
	}
	if x.infinite() {
		sleep(fb, 0, true)
	} else {
		sleep(fb, time.Duration(x.toNanos().Int64()), false)
	}
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
