package flickrt

import "sync"

// fiberGroup is a FiberSet, a FiberMap or a FiberHandle: children of the
// owner of its region, by key. A child that ends leaves the group; a
// failure other than an interruption by the group itself fails `join`.
// The owner of the region interrupts and awaits what is left when it
// begins to close, before its finalizers: a hook, as in the JS runtime.
type fiberGroup struct {
	mu      sync.Mutex
	owner   *Owner
	closed  bool
	entries []groupEntry
	failed  Value // a *Deferred completed by the first failure, or at close
}

type groupEntry struct {
	key Value
	c   *child
}

// internalFiber interrupts the children a group replaces or clears, so
// their interruption does not fail `join`.
const internalFiber = -1

func newGroup(fb *Fiber) Value {
	g := &fiberGroup{owner: fb.owner, failed: DeferredMake(fb)}
	fb.owner.onClose(fb, func(*Fiber) {
		g.mu.Lock()
		g.closed = true
		children := g.children()
		g.mu.Unlock()
		interruptAs(internalFiber, children)
		settle(g.failed, Succeed(Unit))
	})
	return g
}

func group(v Value) *fiberGroup { return v.(*fiberGroup) }

func (g *fiberGroup) children() []Value {
	out := make([]Value, len(g.entries))
	for i, e := range g.entries {
		out[i] = e.c
	}
	return out
}

func (g *fiberGroup) find(key Value) int {
	for i, e := range g.entries {
		if Bool(Equals(e.key, key)) {
			return i
		}
	}
	return -1
}

// add puts a child in the group under `key`, replacing (or with
// `onlyIfMissing` keeping) the one already there.
func (g *fiberGroup) add(key Value, c *child, options Value) {
	propagate, onlyIfMissing := false, false
	if r, ok := options.(*Record); ok {
		propagate = Bool(r.Get("propagateInterruption"))
		if r.index("onlyIfMissing") >= 0 {
			onlyIfMissing = Bool(r.Get("onlyIfMissing"))
		}
	} else if b, ok := options.(bool); ok {
		propagate = b
	}
	g.mu.Lock()
	if g.closed {
		g.mu.Unlock()
		c.state.interrupt(internalFiber)
		return
	}
	if i := g.find(key); i >= 0 {
		if g.entries[i].c == c {
			g.mu.Unlock()
			return
		}
		if onlyIfMissing {
			g.mu.Unlock()
			c.state.interrupt(internalFiber)
			return
		}
		g.entries[i].c.state.interrupt(internalFiber)
		g.entries[i].c = c
	} else {
		g.entries = append(g.entries, groupEntry{key, c})
	}
	g.mu.Unlock()
	c.observe(func(c *child) {
		g.mu.Lock()
		defer g.mu.Unlock()
		if g.closed {
			return
		}
		if i := g.find(key); i >= 0 && g.entries[i].c == c {
			g.entries = append(g.entries[:i:i], g.entries[i+1:]...)
		}
		if c.cause == nil {
			return
		}
		internal := false
		for _, r := range c.cause.reasons() {
			internal = internal || r.kind == causeInterrupt && r.fiber == internalFiber
		}
		onlyInterrupts := !c.cause.has(causeFail) && !c.cause.has(causeDie)
		if propagate && !internal || !propagate && !onlyInterrupts {
			settle(g.failed, failing(c.cause))
		}
	})
}

// run forks `task` in the group's owner and adds it; a closed group gives
// an interrupted child without running the task.
func (g *fiberGroup) run(fb *Fiber, key, options, task Value) Value {
	if g.isClosed() {
		return stoppedChild(internalFiber)
	}
	c := fork(fb, g.owner, task, nil)
	g.add(key, c, options)
	g.awaitIfClosed(c)
	return c
}

func (g *fiberGroup) isClosed() bool {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.closed
}

// awaitIfClosed waits for a child the group closed around while it was
// being added: the closing hook did not see it, so it ends before its
// caller does.
func (g *fiberGroup) awaitIfClosed(c *child) {
	if g.isClosed() {
		<-c.done
	}
}

// stoppedChild is a child that ended interrupted by `by` without running.
func stoppedChild(by int64) *child {
	c := &child{state: newState(), done: make(chan struct{}), ended: true, cause: &Cause{kind: causeInterrupt, fiber: by}}
	c.state.interrupt(by)
	close(c.done)
	return c
}

func (g *fiberGroup) get(key Value) Value {
	g.mu.Lock()
	defer g.mu.Unlock()
	if i := g.find(key); i >= 0 {
		return Some(g.entries[i].c)
	}
	return None
}

func (g *fiberGroup) remove(key Value) {
	g.mu.Lock()
	i := g.find(key)
	var c *child
	if i >= 0 {
		c = g.entries[i].c
		g.entries = append(g.entries[:i:i], g.entries[i+1:]...)
	}
	g.mu.Unlock()
	if c != nil {
		interruptAs(internalFiber, []Value{c})
	}
}

func groupClear(_ *Fiber, v Value) Value {
	g := group(v)
	g.mu.Lock()
	children := g.children()
	g.mu.Unlock()
	return interruptAs(internalFiber, children)
}

func groupSize(_ *Fiber, v Value) Value {
	g := group(v)
	g.mu.Lock()
	defer g.mu.Unlock()
	return int32(len(g.entries))
}

func groupJoin(fb *Fiber, v Value) Value { return DeferredAwait(fb, group(v).failed) }

func groupAwaitEmpty(fb *Fiber, v Value) Value {
	g := group(v)
	for {
		g.mu.Lock()
		if g.closed || len(g.entries) == 0 {
			g.mu.Unlock()
			return Unit
		}
		c := g.entries[0].c
		g.mu.Unlock()
		wait(fb, c.done)
	}
}

// handleKey is the one key of a FiberHandle.
var handleKey Value = Unit

func FiberGroupMake(fb *Fiber) Value                { return newGroup(fb) }
func FiberGroupClear(fb *Fiber, g Value) Value      { return groupClear(fb, g) }
func FiberGroupSize(fb *Fiber, g Value) Value       { return groupSize(fb, g) }
func FiberGroupJoin(fb *Fiber, g Value) Value       { return groupJoin(fb, g) }
func FiberGroupAwaitEmpty(fb *Fiber, g Value) Value { return groupAwaitEmpty(fb, g) }

// A FiberSet keys each child by itself.
func FiberSetRun(fb *Fiber, options, task, s Value) Value {
	g := group(s)
	if g.isClosed() {
		return stoppedChild(internalFiber)
	}
	c := fork(fb, g.owner, task, nil)
	g.add(c, c, options)
	g.awaitIfClosed(c)
	return c
}

func FiberSetAdd(_ *Fiber, propagate, f, s Value) Value {
	group(s).add(f, f.(*child), propagate)
	return Unit
}

func FiberMapRun(fb *Fiber, options, key, task, m Value) Value {
	return group(m).run(fb, key, options, task)
}

func FiberMapSet(_ *Fiber, options, key, f, m Value) Value {
	group(m).add(key, f.(*child), options)
	return Unit
}

func FiberMapGet(_ *Fiber, key, m Value) Value { return group(m).get(key) }
func FiberMapHas(_ *Fiber, key, m Value) Value { return isSome(group(m).get(key)) }

func FiberMapRemove(_ *Fiber, key, m Value) Value {
	group(m).remove(key)
	return Unit
}

func FiberHandleRun(fb *Fiber, options, task, h Value) Value {
	return group(h).run(fb, handleKey, options, task)
}

func FiberHandleSet(_ *Fiber, options, f, h Value) Value {
	group(h).add(handleKey, f.(*child), options)
	return Unit
}

func FiberHandleGet(_ *Fiber, h Value) Value { return group(h).get(handleKey) }
