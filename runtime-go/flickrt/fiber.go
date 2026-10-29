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
	env   map[string]Value // the services provided, by key; never mutated
	owner *Owner           // the innermost scope's
}

type fiberState struct {
	id        int64
	requested atomic.Bool
	by        atomic.Int64  // the fiber that asked for the interruption
	signal    chan struct{} // closed when interruption is asked for
	once      sync.Once
	mask      int // depth of uninterruptible regions; only the fiber's goroutine touches it
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
	s := fb.state
	if s.mask > 0 {
		<-ch
		return
	}
	select {
	case <-ch:
	case <-s.signal:
		panic(s.stopped())
	}
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
		return Fn(func(x Value) Value {
			switch {
			case fromEff && toEff && result != nil:
				return Eff(func(fb *Fiber) Value { return result(Run(fb, Apply(f, x))) })
			case fromEff && toEff:
				return Apply(f, x)
			case toEff:
				return Eff(func(*Fiber) Value { return lifted(Apply(f, x)) })
			case fromEff:
				// shortcut: a task where a pure value is expected runs in a fiber of its own, without the caller's services; thread the caller's when a program needs them
				return lifted(Run(rootFiber(), Apply(f, x)))
			}
			return lifted(Apply(f, x))
		})
	}
}

// LiftEff adapts the result of a task.
func LiftEff(inner func(Value) Value) func(Value) Value {
	return func(task Value) Value {
		return Eff(func(fb *Fiber) Value { return inner(Run(fb, task)) })
	}
}
