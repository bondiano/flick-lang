package flickrt

import "sync"

// Lexical owners. Each `scope` has one, holding the finalizers of what was
// acquired into it. Closing it ends admission, then runs the finalizers
// last first, each seeing how the body ended, and combines their failures
// with the body's.
type Owner struct {
	mu         sync.Mutex
	open       bool
	finalizers []func(exit *Cause) *Cause
}

func newOwner() *Owner { return &Owner{open: true} }

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
func (o *Owner) add(fb *Fiber, finalizer func(fb *Fiber, exit *Cause) Value) {
	run := func(exit *Cause) *Cause {
		_, cause := catch(func() Value { return finalizer(fb, exit) })
		return cause
	}
	o.mu.Lock()
	if o.open {
		o.finalizers = append(o.finalizers, run)
		o.mu.Unlock()
		return
	}
	o.mu.Unlock()
	if cause := run(nil); cause != nil {
		panic(&Failure{cause})
	}
}

// close closes the owner after its body ended with `exit` (nil on success):
// the cause the region ends with.
func (o *Owner) close(exit *Cause) *Cause {
	o.mu.Lock()
	o.open = false
	finalizers := o.finalizers
	o.finalizers = nil
	o.mu.Unlock()
	cause := exit
	for i := len(finalizers) - 1; i >= 0; i-- {
		cause = Sequential(cause, finalizers[i](exit))
	}
	return cause
}

// owned runs `body` in a fiber whose owner is a fresh one, then closes it.
func owned(fb *Fiber, body func(fb *Fiber, owner *Owner) Value) Value {
	owner := newOwner()
	inner := *fb
	inner.owner = owner
	out, cause := catch(func() Value { return body(&inner, owner) })
	if cause = owner.close(cause); cause != nil {
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
	v := Run(fb, acquire)
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { return Run(fb, Apply(release, v)) })
	return v
}

// Bracket runs `use` and then `release`, even when `use` fails.
func Bracket(fb *Fiber, acquire, use, release Value) Value {
	v := Run(fb, acquire)
	out, cause := catch(func() Value { return Run(fb, Apply(use, v)) })
	_, released := catch(func() Value { return Run(fb, Apply(release, v)) })
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
		return Run(fb, Apply(finalizer, o))
	})
	return Unit
}
