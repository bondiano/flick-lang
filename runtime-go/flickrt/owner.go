package flickrt

import "sync"

// Lexical owners. Each `scope` has one: the fibers spawned into it, the
// foreign calls they left running, hooks that stop its fiber groups and
// the finalizers of what was acquired into it. Closing it is two-phase
// (docs/formal/runtime.md §3.3): admission of children ends, the hooks
// run, children are interrupted and each one awaited, supervisors
// included, the foreign calls end, and only then the finalizers run, last
// first, each seeing how the body ended; their failures join the body's.
type Owner struct {
	mu   sync.Mutex
	open bool // admits children; finalizers are admitted until `finalizing`
	// finalizing is set when the finalizers start: one added later runs at once, seeing `exit`
	finalizing bool
	exit       *Cause
	children   map[*child]struct{}
	watchers   sync.WaitGroup
	pending    []<-chan struct{} // foreign calls still running after an interruption
	hooks      []func(fb *Fiber)
	finalizers []finalizer
	defects    *defects // where failures of supervisors go; the root's when nil
	trace      int64    // its id in the trace; 0 when it is not traced (trace.go)
	fins       int64    // the finalizers added through Flick so far, for the trace
	calls      []int64  // the trace ids of `pending`, when traced
}

// defects collects failures of supervisors: the root's fail the program,
// a test's fail the test.
type defects struct {
	mu     sync.Mutex
	causes []*Cause
}

var rootDefects = &defects{}

func (d *defects) add(c *Cause) {
	d.mu.Lock()
	d.causes = append(d.causes, c)
	d.mu.Unlock()
}

func (d *defects) all() *Cause {
	d.mu.Lock()
	defer d.mu.Unlock()
	var out *Cause
	for _, c := range d.causes {
		out = Sequential(out, c)
	}
	return out
}

// report sends the failure of a supervisor to the owner's root.
func (o *Owner) report(c *Cause) {
	if o == nil || o.defects == nil {
		rootDefects.add(c)
		return
	}
	o.defects.add(c)
}

// track makes the owner wait, before it releases resources, for a foreign
// call that ends when `done` is closed.
func (o *Owner) track(done <-chan struct{}) {
	o.mu.Lock()
	o.pending = append(o.pending, done)
	if tracing && o.trace != 0 {
		o.calls = append(o.calls, traceID())
		traceEvent("track", "o", o.trace, "t", o.calls[len(o.calls)-1])
	}
	o.mu.Unlock()
}

// settle waits for the foreign calls still running, and those they leave.
func (o *Owner) settle() {
	for {
		o.mu.Lock()
		pending, calls := o.pending, o.calls
		o.pending, o.calls = nil, nil
		o.mu.Unlock()
		if len(pending) == 0 {
			return
		}
		for i, done := range pending {
			<-done
			if i < len(calls) {
				traceEvent("settle", "o", o.trace, "t", calls[i])
			}
		}
	}
}

// A finalizer runs in the closing fiber, with the services of the fiber
// that added it.
type finalizer struct {
	added *Fiber
	run   func(fb *Fiber, exit *Cause) Value
	owner *Owner // and its place among the owner's: traced when set
	place int64
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

// add runs `finalizer` in `fb` when the owner closes, or at once when its
// finalizers have started, as Effect's scopes do. A child still running
// while the owner closes adds to the stack, released after it ended.
func (o *Owner) add(fb *Fiber, run func(fb *Fiber, exit *Cause) Value) {
	o.addTraced(fb, run, false)
}

// addTraced is [Owner.add] that writes the finalizer to the trace when
// `traced`: one Flick adds, as the JS runtime sees them.
func (o *Owner) addTraced(fb *Fiber, run func(fb *Fiber, exit *Cause) Value, traced bool) {
	f := finalizer{added: fb, run: run}
	o.mu.Lock()
	if traced && tracing && o.trace != 0 {
		o.fins++
		f.owner, f.place = o, o.fins
		event := "fin"
		if o.finalizing {
			event = "now"
		}
		traceEvent(event, "o", o.trace, "i", f.place)
	}
	if !o.finalizing {
		o.finalizers = append(o.finalizers, f)
		o.mu.Unlock()
		return
	}
	exit := o.exit
	o.mu.Unlock()
	if cause := f.call(fb, exit); cause != nil {
		panic(&Failure{cause})
	}
}

// addIfOpen is [Owner.add] only while the owner admits finalizers, `then`
// running with it still admitting them; whether it did.
func (o *Owner) addIfOpen(fb *Fiber, run func(fb *Fiber, exit *Cause) Value, then func()) bool {
	o.mu.Lock()
	defer o.mu.Unlock()
	if o.finalizing {
		return false
	}
	then()
	o.finalizers = append(o.finalizers, finalizer{added: fb, run: run})
	return true
}

func (f finalizer) call(closer *Fiber, exit *Cause) *Cause {
	if f.owner != nil {
		traceEvent("run", "o", f.owner.trace, "i", f.place)
	}
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

// onClose adds a hook the owner runs when it begins to close, before it
// interrupts its children; at once when its finalizers have started.
func (o *Owner) onClose(fb *Fiber, hook func(fb *Fiber)) {
	o.mu.Lock()
	if !o.finalizing {
		o.hooks = append(o.hooks, hook)
		o.mu.Unlock()
		return
	}
	o.mu.Unlock()
	hook(fb)
}

// runHooks runs the hooks added so far, and those they add, uninterruptibly.
func (o *Owner) runHooks(fb *Fiber) (cause *Cause) {
	for {
		o.mu.Lock()
		hooks := o.hooks
		o.hooks = nil
		o.mu.Unlock()
		if len(hooks) == 0 {
			return cause
		}
		for _, hook := range hooks {
			_, c := catch(func() Value { return masked(fb, fb.state.mask+1, func() Value { hook(fb); return Unit }) })
			cause = Sequential(cause, c)
		}
	}
}

// closeWithCause separates the exit finalizers observe from the result they extend.
func (o *Owner) closeWithCause(fb *Fiber, exit, cause *Cause) *Cause {
	o.mu.Lock()
	o.open = false
	o.tracePhase("C0")
	o.mu.Unlock()
	o.tracePhase("C1")
	cause = Sequential(cause, o.runHooks(fb))
	o.mu.Lock()
	children := make([]*child, 0, len(o.children))
	for c := range o.children {
		children = append(children, c)
	}
	o.mu.Unlock()
	o.tracePhase("C2")
	for _, c := range children {
		if tracing && o.trace != 0 {
			traceEvent("interrupt", "o", o.trace, "f", traceChild(c))
		}
		c.state.interrupt(fb.state.id)
	}
	suspended(fb.state)
	o.watchers.Wait()
	o.tracePhase("C3")
	// a supervisor may have made a fiber group while the children ended
	cause = Sequential(cause, o.runHooks(fb))
	o.settle()
	o.tracePhase("C4")
	o.mu.Lock()
	o.finalizing, o.exit = true, exit
	o.tracePhase("C5")
	finalizers := o.finalizers
	o.finalizers = nil
	o.mu.Unlock()
	var released *Cause
	for i := len(finalizers) - 1; i >= 0; i-- {
		released = concat(released, finalizers[i].call(fb, exit))
	}
	o.tracePhase("C6")
	return Sequential(cause, released)
}

// owned runs `body` in a fiber whose owner is a fresh one, then closes it.
func owned(fb *Fiber, body func(fb *Fiber, owner *Owner) Value) Value {
	owner := newOwner()
	if fb.owner != nil {
		owner.defects = fb.owner.defects
	}
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
	return owned(fb, func(inner *Fiber, owner *Owner) Value {
		if tracing {
			traceOpen(owner, fb.owner, "scope")
		}
		return body(inner, owner, fb.owner)
	})
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
	fb.owner.addTraced(fb, func(fb *Fiber, _ *Cause) Value { return Run(fb, Apply(fb, release, v)) }, true)
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
	fb.owner.addTraced(fb, func(fb *Fiber, _ *Cause) Value { return Run(fb, finalizer) }, true)
	return Unit
}

// AddFinalizerExit is `Scope.add_finalizer_exit`: the finalizer sees the
// cause of a failure.
func AddFinalizerExit(fb *Fiber, finalizer Value) Value {
	fb.owner.addTraced(fb, func(fb *Fiber, exit *Cause) Value {
		var o Value = None
		if exit != nil {
			o = Some(exit)
		}
		return Run(fb, Apply(fb, finalizer, o))
	}, true)
	return Unit
}
