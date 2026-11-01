package flickrt

import (
	"sync"
	"sync/atomic"
)

// A Fiber is what effectful code runs in: the generated code threads it
// through every effectful call. Its derivations (another environment of
// services, another owner) share the fiber's state.
type Fiber struct {
	state *fiberState
	env   *env   // the services provided, by key
	owner *Owner // the innermost scope's
}

// An env is the services and fiber-local settings in effect, innermost
// first; a derived fiber adds to it without copying.
type env struct {
	key  string
	v    Value
	next *env
}

func (e *env) get(key string) (Value, bool) {
	for ; e != nil; e = e.next {
		if e.key == key {
			return e.v, true
		}
	}
	return nil, false
}

func (e *env) with(key string, v Value) *env { return &env{key, v, e} }

type fiberState struct {
	id        int64
	requested atomic.Bool
	by        atomic.Int64  // the fiber that asked for the interruption
	signal    chan struct{} // closed when interruption is asked for
	once      sync.Once
	mask      int                    // depth of uninterruptible regions; only the fiber's goroutine touches it
	clock     *testClock             // the test clock that counts the fiber as running, if any
	cohort    *cohort                // a forked fiber's until it first suspends (startup.go)
	forks     atomic.Pointer[cohort] // the cohort its forks join until it suspends (startup.go)
}

var fibers atomic.Int64

func newState() *fiberState {
	return &fiberState{id: fibers.Add(1), signal: make(chan struct{})}
}

// interrupt asks the fiber to stop at its next checkpoint.
func (s *fiberState) interrupt(by int64) {
	s.once.Do(func() {
		s.by.Store(by)
		s.requested.Store(true)
		close(s.signal)
	})
}

func (s *fiberState) stopped() *Failure {
	return &Failure{&Cause{kind: causeInterrupt, fiber: s.by.Load()}}
}

// Eff is a task: a computation run each time it is awaited, in the fiber
// that awaits it.
type Eff func(*Fiber) Value

// Check is an interruption checkpoint: an interruptible fiber asked to stop
// fails with the interruption.
func Check(fb *Fiber) {
	if stressing {
		stress()
	}
	if s := fb.state; s.requested.Load() && s.mask == 0 {
		panic(s.stopped())
	}
}

// Run runs a task in a fiber, after a checkpoint.
func Run(fb *Fiber, task Value) Value {
	Check(fb)
	return task.(Eff)(fb)
}

// wait blocks until `ch` is closed, or fails with the fiber's interruption
// when it is interruptible.
func wait(fb *Fiber, ch <-chan struct{}) {
	signal := signalOf(fb)
	if c := parking(fb); c != nil {
		defer c.park(func() bool { return isClosed(ch) || isClosed(signal) })()
	}
	select {
	case <-ch:
	case <-signal:
		panic(fb.state.stopped())
	}
}

// signalOf is the channel closed when the fiber is asked to stop, nil
// while it is uninterruptible.
func signalOf(fb *Fiber) <-chan struct{} {
	if fb.state.mask > 0 {
		return nil
	}
	return fb.state.signal
}

// isClosed is whether a channel that is only ever closed is.
func isClosed(ch <-chan struct{}) bool {
	select {
	case <-ch:
		return true
	default:
		return false
	}
}

// parking is called as a fiber blocks: it flushes stdout, and gives the
// test clock that counts the fiber, nil without one. Under one, the
// blocking wait is bracketed by `defer c.park(ready)()`, `ready` telling
// whether what it waits for has come; the closure is only made then.
func parking(fb *Fiber) *testClock {
	idle()
	suspended(fb.state)
	if testClocks.Load() == 0 {
		return nil
	}
	return fb.state.clock
}

// park is `parking` for a wait on I/O, which a test clock never waits for:
// call the function it gives when the wait ends.
func park(fb *Fiber, ready func() bool) func() {
	if c := parking(fb); c != nil {
		return c.park(ready)
	}
	return func() {}
}

// masked runs `f` with the fiber's interruptibility set by `mask`.
func masked(fb *Fiber, mask int, f func() Value) Value {
	saved := fb.state.mask
	fb.state.mask = mask
	defer func() { fb.state.mask = saved }()
	return f()
}

// rootFiber is the fiber `main` runs in.
func rootFiber() *Fiber { return &Fiber{state: newState()} }

// LiftStage adapts a function of one stage where one of another is
// expected: effectful stages (Sync and Runtime) give a task, pure ones a
// value; `result`, when given, adapts what the function returns.
func LiftStage(fromEff, toEff bool, result func(Value) Value) func(Value) Value {
	lifted := func(v Value) Value {
		if result != nil {
			return result(v)
		}
		return v
	}
	return func(f Value) Value {
		return Fn(func(fb *Fiber, x Value) Value {
			switch {
			case fromEff && toEff && result != nil:
				return Eff(func(fb *Fiber) Value { return result(Run(fb, Apply(fb, f, x))) })
			case fromEff && toEff:
				return Apply(fb, f, x)
			case toEff:
				return Eff(func(fb *Fiber) Value { return lifted(Apply(fb, f, x)) })
			case fromEff:
				if fb == nil {
					fb = rootFiber()
				}
				return lifted(Run(fb, Apply(fb, f, x)))
			}
			return lifted(Apply(fb, f, x))
		})
	}
}

// LiftEff adapts the result of a task.
func LiftEff(inner func(Value) Value) func(Value) Value {
	return func(task Value) Value {
		return Eff(func(fb *Fiber) Value { return inner(Run(fb, task)) })
	}
}
