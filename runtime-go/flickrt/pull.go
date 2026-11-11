package flickrt

// Pull is, at each call, the next element of what is pulled, or its end, a
// failure whose cause is a fail of a *done carrying the done value, or
// another failure. The end being a failure, combined causes, exits and
// finalizers see it as one. One fiber pulls a pull at a time; [serial]
// shares one.
type Pull func(fb *Fiber) Value

// done is the end of a pull; Flick never sees one.
type done struct{ value Value }

func doneCause(v Value) *Cause { return &Cause{kind: causeFail, err: &done{v}} }

// halt ends a pull with the done value.
func halt(v Value) Value { panic(&Failure{doneCause(v)}) }

// haltPull ends at once, as `Cause.done()`: the upstream of a channel run
// on its own.
func haltPull(*Fiber) Value { return halt(Unit) }

func failPull(c *Cause) Pull { return func(*Fiber) Value { panic(&Failure{c}) } }

func isDoneReason(r *Cause) bool {
	_, ok := r.err.(*done)
	return ok && r.kind == causeFail
}

func isDoneCause(c *Cause) bool {
	for _, r := range c.reasons() {
		if isDoneReason(r) {
			return true
		}
	}
	return false
}

// filterDone is the done value of a cause that ended a pull; when
// something besides interruptions failed too, the cause without its ends.
func filterDone(c *Cause) (Value, *Cause, bool) {
	var d *done
	failed := false
	for _, r := range c.reasons() {
		switch {
		case isDoneReason(r):
			if d == nil {
				d = r.err.(*done)
			}
		case r.kind != causeInterrupt:
			failed = true
		}
	}
	if d == nil {
		return nil, c, false
	}
	if !failed {
		return d.value, nil, true
	}
	var rest *Cause
	for _, r := range c.reasons() {
		if !isDoneReason(r) {
			rest = Sequential(rest, r)
		}
	}
	return nil, rest, false
}

// catchDone is what `p` pulls, or `f` of the done value once it ended;
// another failure goes on.
func catchDone(fb *Fiber, p Pull, f func(fb *Fiber, leftover Value) Value) Value {
	v, c := catch(func() Value { return p(fb) })
	if c == nil {
		return v
	}
	d, rest, ok := filterDone(c)
	if !ok {
		panic(&Failure{rest})
	}
	return f(fb, d)
}

// orDone is `Some` of what `p` gives, `None` once its source is done.
func orDone(fb *Fiber, p Pull) Value {
	return catchDone(fb, func(fb *Fiber) Value { return Some(p(fb)) }, func(*Fiber, Value) Value { return None })
}

// doneExit is `Pull.doneExitFromCause`: the exit of what ended with `c`, a
// success of the done value when it was done.
func doneExit(c *Cause) *Exit {
	d, rest, ok := filterDone(c)
	if ok {
		return &Exit{value: d}
	}
	return &Exit{cause: rest}
}

// closeScope closes an owner [forkScope] made, with the exit of what ran
// in it (nil: a success), failing as its finalizers do.
//
// shortcut: an owner closed early stays among its parent's finalizers until the parent closes; remove it then if a long-lived channel forks scopes without end
func closeScope(fb *Fiber, o *Owner, exit *Cause) {
	if c := o.closeFinalizers(fb, exit); c != nil {
		panic(&Failure{c})
	}
}

// empty is whether the owner holds nothing to release.
func (o *Owner) empty() bool {
	o.mu.Lock()
	defer o.mu.Unlock()
	return len(o.finalizers) == 0 && len(o.children) == 0
}

// forkIn runs `task` in a fiber of its own, with the services and owner of
// `fb`, that the closing of `scope` interrupts and awaits.
func forkIn(fb *Fiber, scope *Owner, task func(fb *Fiber) Value) *child {
	c := fork(fb, fb.owner, Eff(task), nil)
	scope.add(fb, func(fb *Fiber, _ *Cause) Value {
		stop(fb, c)
		return Unit
	})
	return c
}

// serial is the pull shared by fibers: one pulls at a time, the others
// wait their turn interruptibly, as a semaphore of one permit does.
func serial(p Pull) Pull {
	s := &Semaphore{permits: 1, wake: make(chan struct{})}
	return func(fb *Fiber) Value {
		s.take(fb, 1)
		defer s.release(1)
		return p(fb)
	}
}
