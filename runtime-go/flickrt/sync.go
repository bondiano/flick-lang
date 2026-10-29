package flickrt

import "sync"

// Ref is a mutable cell that fibers share: each operation is atomic, and
// `modify` runs its pure function under the lock.
type Ref struct {
	mu    sync.Mutex
	value Value
}

func RefMake(v Value) Value    { return &Ref{value: v} }
func RefSame(a, b Value) Value { return a.(*Ref) == b.(*Ref) }

func RefGet(r Value) Value {
	cell := r.(*Ref)
	cell.mu.Lock()
	defer cell.mu.Unlock()
	return cell.value
}

func RefSet(r, v Value) Value {
	cell := r.(*Ref)
	cell.mu.Lock()
	cell.value = v
	cell.mu.Unlock()
	return Unit
}

func RefModify(r, f Value) Value {
	cell := r.(*Ref)
	cell.mu.Lock()
	defer cell.mu.Unlock()
	pair := Apply(f, cell.value).([]Value)
	cell.value = pair[1]
	return pair[0]
}

// Deferred is a value set once that fibers wait for: its outcome is a task
// each await runs.
type Deferred struct {
	mu      sync.Mutex
	outcome Value
	done    chan struct{}
}

func DeferredMake(*Fiber) Value { return &Deferred{done: make(chan struct{})} }

// settle completes the deferred with `outcome` unless it is done.
func settle(d Value, outcome Value) Value {
	x := d.(*Deferred)
	x.mu.Lock()
	defer x.mu.Unlock()
	if x.outcome != nil {
		return false
	}
	x.outcome = outcome
	close(x.done)
	return true
}

func failing(c *Cause) Value { return Eff(func(*Fiber) Value { panic(&Failure{c}) }) }

func DeferredAwait(fb *Fiber, d Value) Value {
	x := d.(*Deferred)
	wait(fb, x.done)
	return Run(fb, x.outcome)
}

func DeferredIsDone(_ *Fiber, d Value) Value {
	select {
	case <-d.(*Deferred).done:
		return true
	default:
		return false
	}
}

func DeferredPoll(_ *Fiber, d Value) Value {
	x := d.(*Deferred)
	x.mu.Lock()
	defer x.mu.Unlock()
	if x.outcome == nil {
		return None
	}
	return Some(x.outcome)
}

func DeferredSucceed(_ *Fiber, v, d Value) Value         { return settle(d, Succeed(v)) }
func DeferredSync(_ *Fiber, f, d Value) Value            { return settle(d, Succeed(Apply(f, Unit))) }
func DeferredCompleteWith(_ *Fiber, task, d Value) Value { return settle(d, task) }

func DeferredFail(_ *Fiber, err, d Value) Value {
	return settle(d, failing(&Cause{kind: causeFail, err: err}))
}

func DeferredFailSync(_ *Fiber, f, d Value) Value {
	return settle(d, failing(&Cause{kind: causeFail, err: Apply(f, Unit)}))
}

func DeferredFailCause(_ *Fiber, f, d Value) Value { return settle(d, failing(f.(*Supervised).cause)) }

func DeferredFailCauseSync(_ *Fiber, f, d Value) Value {
	return settle(d, failing(Apply(f, Unit).(*Supervised).cause))
}

func DeferredInterrupt(fb *Fiber, d Value) Value {
	return settle(d, failing(&Cause{kind: causeInterrupt, fiber: fb.state.id}))
}

func DeferredInterruptWith(_ *Fiber, id, d Value) Value {
	return settle(d, failing(&Cause{kind: causeInterrupt, fiber: int64(id.(float64))}))
}

// DeferredInto runs `task` and completes the deferred with how it ended.
func DeferredInto(fb *Fiber, d, task Value) Value {
	return masked(fb, fb.state.mask+1, func() Value {
		v, c := catch(func() Value { return Run(fb, task) })
		if c != nil {
			return settle(d, failing(c))
		}
		return settle(d, Succeed(v))
	})
}

// DeferredComplete is `into` unless the deferred is done, without running
// the task then.
func DeferredComplete(fb *Fiber, task, d Value) Value {
	if Bool(DeferredIsDone(fb, d)) {
		return false
	}
	return DeferredInto(fb, d, task)
}

// Latch is a gate fibers wait at: open, it lets them through; `release`
// wakes those waiting without opening it.
type Latch struct {
	mu   sync.Mutex
	open bool
	wake chan struct{} // closed to wake the fibers waiting now
}

func LatchMake(_ *Fiber, open Value) Value {
	return &Latch{open: Bool(open), wake: make(chan struct{})}
}

// wakeAll wakes the waiters; the caller holds the lock.
func (l *Latch) wakeAll() {
	close(l.wake)
	l.wake = make(chan struct{})
}

func LatchOpen(_ *Fiber, latch Value) Value {
	l := latch.(*Latch)
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.open {
		return false
	}
	l.open = true
	l.wakeAll()
	return true
}

func LatchRelease(_ *Fiber, latch Value) Value {
	l := latch.(*Latch)
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.open {
		return false
	}
	l.wakeAll()
	return true
}

func LatchClose(_ *Fiber, latch Value) Value {
	l := latch.(*Latch)
	l.mu.Lock()
	defer l.mu.Unlock()
	was := l.open
	l.open = false
	return was
}

func LatchIsOpen(latch Value) Value {
	l := latch.(*Latch)
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.open
}

func LatchAwait(fb *Fiber, latch Value) Value {
	l := latch.(*Latch)
	l.mu.Lock()
	if l.open {
		l.mu.Unlock()
		return Unit
	}
	wake := l.wake
	l.mu.Unlock()
	wait(fb, wake)
	return Unit
}

func LatchWhenOpen(fb *Fiber, latch, task Value) Value {
	LatchAwait(fb, latch)
	return Run(fb, task)
}
