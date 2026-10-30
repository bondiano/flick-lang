package flickrt

import "sync"

// Lexical owners. Each `scope` has one: the fibers spawned into it, the
// foreign calls they left running and the finalizers of what was acquired
// into it. Closing it is two-phase: admission ends, children are
// interrupted and each one awaited, supervisors included, the foreign
// calls end, and only then the finalizers run, last first,
// each seeing how the body ended; their failures join the body's.
type Owner struct {
	mu         sync.Mutex
	open       bool
	children   map[*child]struct{}
	watchers   sync.WaitGroup
	pending    []<-chan struct{} // foreign calls still running after an interruption
	finalizers []finalizer
}

// track makes the owner wait, before it releases resources, for a foreign
// call that ends when `done` is closed.
func (o *Owner) track(done <-chan struct{}) {
	o.mu.Lock()
	o.pending = append(o.pending, done)
	o.mu.Unlock()
}

// settle waits for the foreign calls still running, and those they leave.
func (o *Owner) settle() {
	for {
		o.mu.Lock()
		pending := o.pending
		o.pending = nil
		o.mu.Unlock()
		if len(pending) == 0 {
			return
		}
		for _, done := range pending {
			<-done
		}
	}
}

// A finalizer runs in the closing fiber, with the services of the fiber
// that added it.
type finalizer struct {
	added *Fiber
	run   func(fb *Fiber, exit *Cause) Value
}

func newOwner() *Owner { return &Owner{open: true, children: map[*child]struct{}{}} }

// catch is the result of `f`, or the cause it failed with.
func catch(f func() Value) (out Value, cause *Cause) {
	defer func() {
		if r := recover(); r != nil {
			cause = causeOf(r)
		}
	}()
	return f(), nil
}

// add runs `finalizer` in `fb` when the owner closes, or at once when it
// has closed, as Effect's scopes do.
func (o *Owner) add(fb *Fiber, run func(fb *Fiber, exit *Cause) Value) {
	f := finalizer{fb, run}
	o.mu.Lock()
	if o.open {
		o.finalizers = append(o.finalizers, f)
		o.mu.Unlock()
		return
	}
	o.mu.Unlock()
	if cause := f.call(fb, nil); cause != nil {
		panic(&Failure{cause})
	}
}

// addIfOpen is [Owner.add] only while the owner is open, `then` running
// with it still open; whether it was.
func (o *Owner) addIfOpen(fb *Fiber, run func(fb *Fiber, exit *Cause) Value, then func()) bool {
	o.mu.Lock()
	defer o.mu.Unlock()
	if !o.open {
		return false
	}
	then()
	o.finalizers = append(o.finalizers, finalizer{fb, run})
	return true
}

func (f finalizer) call(closer *Fiber, exit *Cause) *Cause {
	fb := *f.added
	fb.state = closer.state
	_, cause := catch(func() Value { return masked(&fb, closer.state.mask+1, func() Value { return f.run(&fb, exit) }) })
	return cause
}

// close closes the owner, uninterruptibly, after its body ended with `exit`
// (nil on success): the cause the region ends with.
func (o *Owner) close(fb *Fiber, exit *Cause) *Cause {
	return o.closeWithCause(fb, exit, exit)
}

// closeFinalizers closes a nested owner without returning its parent's body again.
func (o *Owner) closeFinalizers(fb *Fiber, exit *Cause) *Cause {
	return o.closeWithCause(fb, exit, nil)
}

// closeWithCause separates the exit finalizers observe from the result they extend.
func (o *Owner) closeWithCause(fb *Fiber, exit, cause *Cause) *Cause {
	o.mu.Lock()
	o.open = false
	children := make([]*child, 0, len(o.children))
	for c := range o.children {
		children = append(children, c)
	}
	finalizers := o.finalizers
	o.finalizers = nil
	o.mu.Unlock()
	for _, c := range children {
		c.state.interrupt(fb.state.id)
	}
	o.watchers.Wait()
	o.settle()
	for i := len(finalizers) - 1; i >= 0; i-- {
		cause = Sequential(cause, finalizers[i].call(fb, exit))
	}
	return cause
}

// owned runs `body` in a fiber whose owner is a fresh one, then closes it.
func owned(fb *Fiber, body func(fb *Fiber, owner *Owner) Value) Value {
	owner := newOwner()
	inner := *fb
	inner.owner = owner
	out, cause := catch(func() Value { return body(&inner, owner) })
	if cause = owner.close(fb, cause); cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// Scope is `scope [s] { body }`: `body` receives the owner `s` and the one
// around it.
func Scope(fb *Fiber, body func(fb *Fiber, owner, outer Value) Value) Value {
	return owned(fb, func(inner *Fiber, owner *Owner) Value { return body(inner, owner, fb.owner) })
}

// Within is `fb` acquiring into `owner`, an outer region's, not the
// innermost one's.
func Within(fb *Fiber, owner Value) *Fiber {
	inner := *fb
	inner.owner = owner.(*Owner)
	return &inner
}

// AcquireRelease is a resource released when the owner of the call closes.
func AcquireRelease(fb *Fiber, acquire, release Value) Value {
	v := masked(fb, fb.state.mask+1, func() Value { return Run(fb, acquire) })
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { return Run(fb, Apply(fb, release, v)) })
	return v
}

// Bracket runs `use` and then `release`, even when `use` fails.
func Bracket(fb *Fiber, acquire, use, release Value) Value {
	inside := fb.state.mask
	v := masked(fb, inside+1, func() Value { return Run(fb, acquire) })
	out, cause := catch(func() Value { return Run(fb, Apply(fb, use, v)) })
	_, released := catch(func() Value {
		return masked(fb, inside+1, func() Value { return Run(fb, Apply(fb, release, v)) })
	})
	if cause = Sequential(cause, released); cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// AddFinalizer is `Scope.add_finalizer`: `finalizer` runs when the
// innermost owner closes.
func AddFinalizer(fb *Fiber, finalizer Value) Value {
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { return Run(fb, finalizer) })
	return Unit
}

// AddFinalizerExit is `Scope.add_finalizer_exit`: the finalizer sees the
// cause of a failure.
func AddFinalizerExit(fb *Fiber, finalizer Value) Value {
	fb.owner.add(fb, func(fb *Fiber, exit *Cause) Value {
		var o Value = None
		if exit != nil {
			o = Some(exit)
		}
		return Run(fb, Apply(fb, finalizer, o))
	})
	return Unit
}
