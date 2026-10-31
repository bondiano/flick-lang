package flickrt

import (
	"sync"
	"time"
)

// Layers, as Effect's: a layer is a value with an identity that builds
// services into a scope, and a memo map builds each layer it is asked for
// once, sharing its services while any scope that asked still holds them.
// `provide layer { body }` builds the layer into the owner of the body.

// Services is a `Context`: built services by key. It is never mutated.
type Services map[string]Value

// with is `s` and `t`, the services of `t` replacing those of `s`.
func (s Services) with(t Services) Services {
	out := make(Services, len(s)+len(t))
	for k, v := range s {
		out[k] = v
	}
	for k, v := range t {
		out[k] = v
	}
	return out
}

// provided is `fb` with the services of `s` added to its environment.
func provided(fb *Fiber, s Services) *Fiber {
	inner := *fb
	for k, v := range s {
		inner.env = inner.env.with(k, v)
	}
	return &inner
}

// A Layer builds its services in a fiber that has its inputs, into a scope.
type Layer struct {
	build func(fb *Fiber, memo *MemoMap, scope *Owner) Services
}

func layerOf(v Value) *Layer { return v.(*Layer) }

// fromBuild is a layer that builds into a scope of its own, forked from the
// one it is built into and closed at once if the build fails.
func fromBuild(build func(fb *Fiber, memo *MemoMap, scope *Owner) Services) *Layer {
	return &Layer{func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		inner := forkScope(fb, scope)
		out, cause := catch(func() Value { return build(fb, memo, inner) })
		if cause != nil {
			cause = inner.close(fb, cause)
			panic(&Failure{cause})
		}
		return out.(Services)
	}}
}

// memoized is [fromBuild] whose build the memo map shares.
func memoized(build func(fb *Fiber, memo *MemoMap, scope *Owner) Services) *Layer {
	self := &Layer{}
	*self = *fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		return memo.getOrBuild(fb, self, scope, build)
	})
	return self
}

// forkScope is a scope closed when `parent` closes, with how it ended.
func forkScope(fb *Fiber, parent *Owner) *Owner {
	child := newOwner()
	parent.add(fb, func(fb *Fiber, exit *Cause) Value {
		if cause := child.closeFinalizers(fb, exit); cause != nil {
			panic(&Failure{cause})
		}
		return Unit
	})
	return child
}

// A MemoMap is what one build shares: an entry per layer, whose scope
// closes when the last scope observing it does. A forked map reads its
// parent's entries and builds into its own.
type MemoMap struct {
	mu      sync.Mutex
	parent  *MemoMap
	entries map[*Layer]*memoEntry
	idle    time.Duration // how long an entry no scope holds stays built; 0 for not at all
}

type memoEntry struct {
	observers int
	scope     *Owner
	done      chan struct{}
	services  Services
	cause     *Cause
}

func newMemoMap(parent *MemoMap) *MemoMap {
	return &MemoMap{parent: parent, entries: map[*Layer]*memoEntry{}}
}

// lookup is the entry of `l` here or in a parent, and the map holding it.
func (m *MemoMap) lookup(l *Layer) (*memoEntry, *MemoMap) {
	for at := m; at != nil; at = at.parent {
		if at != m {
			at.mu.Lock()
		}
		e := at.entries[l]
		if at != m {
			at.mu.Unlock()
		}
		if e != nil {
			return e, at
		}
	}
	return nil, nil
}

// observe makes `scope` hold `e` until it closes; false when it has closed.
func (m *MemoMap) observe(fb *Fiber, l *Layer, e *memoEntry, scope *Owner) bool {
	return scope.addIfOpen(fb, func(fb *Fiber, exit *Cause) Value {
		m.mu.Lock()
		e.observers--
		last := e.observers == 0
		if last && m.idle > 0 {
			// idle: released later, unless a scope holds it again by then
			m.mu.Unlock()
			idle := *fb
			idle.state = newState()
			time.AfterFunc(m.idle, func() { m.release(&idle, l, e) })
			return Unit
		}
		if last && m.entries[l] == e {
			delete(m.entries, l)
		}
		m.mu.Unlock()
		if last {
			if cause := e.scope.closeFinalizers(fb, exit); cause != nil {
				panic(&Failure{cause})
			}
		}
		return Unit
	}, func() { e.observers++ })
}

// release releases the entry of `l` if no scope holds it.
func (m *MemoMap) release(fb *Fiber, l *Layer, e *memoEntry) {
	m.mu.Lock()
	idle := e.observers == 0 && m.entries[l] == e
	if idle {
		delete(m.entries, l)
	}
	m.mu.Unlock()
	if idle {
		e.scope.closeFinalizers(fb, nil)
	}
}

// releaseIdle releases the entries no scope holds, as the scope of what
// keeps them idle closes.
func (m *MemoMap) releaseIdle(fb *Fiber, exit *Cause) *Cause {
	m.mu.Lock()
	var idle []*memoEntry
	for l, e := range m.entries {
		if e.observers == 0 {
			idle = append(idle, e)
			delete(m.entries, l)
		}
	}
	m.mu.Unlock()
	var out *Cause
	for _, e := range idle {
		out = Sequential(out, e.scope.closeFinalizers(fb, exit))
	}
	return out
}

// getOrBuild is the services of `l`: those of its entry, or built into a
// new entry's scope by `build`.
func (m *MemoMap) getOrBuild(fb *Fiber, l *Layer, scope *Owner, build func(*Fiber, *MemoMap, *Owner) Services) Services {
	m.mu.Lock()
	if e, at := m.lookup(l); e != nil {
		if at != m {
			at.mu.Lock()
		}
		at.observe(fb, l, e, scope)
		if at != m {
			at.mu.Unlock()
		}
		m.mu.Unlock()
		return e.wait(fb)
	}
	e := &memoEntry{scope: newOwner(), done: make(chan struct{})}
	if !m.observe(fb, l, e, scope) {
		// a closed scope cannot hold a shared entry
		m.mu.Unlock()
		return build(fb, m, scope)
	}
	m.entries[l] = e
	m.mu.Unlock()
	out, cause := catch(func() Value { return build(fb, m, e.scope) })
	e.cause = cause
	if cause == nil {
		e.services = out.(Services)
	}
	close(e.done)
	if cause != nil {
		panic(&Failure{cause})
	}
	return e.services
}

// wait is the entry's services once built, as the build that made it ended.
func (e *memoEntry) wait(fb *Fiber) Services {
	wait(fb, e.done)
	if e.cause != nil {
		panic(&Failure{e.cause})
	}
	return e.services
}

const memoKey = "effect/Layer/CurrentMemoMap"

// currentMemo is the memo map a build in `fb` uses: forked from the one of
// the build `fb` runs in, or a new one.
func currentMemo(fb *Fiber) *MemoMap {
	if m, ok := fb.env.get(memoKey); ok {
		return newMemoMap(m.(*MemoMap))
	}
	return newMemoMap(nil)
}

// buildWith builds `l` into `scope` through `memo`, which the build and
// what runs with its services read as the current memo map.
func buildWith(fb *Fiber, l *Layer, memo *MemoMap, scope *Owner) Services {
	inner := *fb
	inner.env = fb.env.with(memoKey, memo)
	return l.build(&inner, memo, scope).with(Services{memoKey: memo})
}

// Provide runs `body` with the services `layer` builds added to the
// fiber's; the body's owner holds what the layer acquired.
func Provide(fb *Fiber, layer, body Value) Value {
	return owned(fb, func(inner *Fiber, owner *Owner) Value {
		s := buildWith(inner, layerOf(layer), currentMemo(inner), owner)
		return Run(provided(inner, s), body)
	})
}

// constant is a layer of services built without effects.
func constant(s Services) Value {
	return &Layer{func(*Fiber, *MemoMap, *Owner) Services { return s }}
}

func LayerSucceed(key, impl Value) Value { return constant(Services{key.(string): impl}) }

var LayerEmpty Value = constant(Services{})

func LayerSucceedContext(ctx Value) Value { return constant(ctx.(Services)) }

func LayerSync(key, make Value) Value {
	return memoized(func(fb *Fiber, _ *MemoMap, _ *Owner) Services {
		return Services{key.(string): Apply(fb, make, Unit)}
	})
}

func LayerSyncContext(make Value) Value {
	return memoized(func(fb *Fiber, _ *MemoMap, _ *Owner) Services { return Apply(fb, make, Unit).(Services) })
}

// scoped runs `task` acquiring into `scope`.
func scoped(fb *Fiber, scope *Owner, task Value) Value { return Run(Within(fb, scope), task) }

func LayerEffect(key, task Value) Value {
	return memoized(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		return Services{key.(string): scoped(fb, scope, task)}
	})
}

func LayerEffectContext(task Value) Value {
	return memoized(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		return scoped(fb, scope, task).(Services)
	})
}

func LayerEffectDiscard(task Value) Value {
	return memoized(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		scoped(fb, scope, task)
		return Services{}
	})
}

func LayerSuspend(make Value) Value {
	return memoized(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		return layerOf(Apply(fb, make, Unit)).build(fb, memo, scope)
	})
}

func LayerUnwrap(task Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		return layerOf(scoped(fb, scope, task)).build(fb, memo, scope)
	})
}

// LayerMergeAll builds the layers concurrently, the services of later ones
// replacing earlier ones'; the first failure interrupts the others' builds.
// Their scopes release in parallel when the merged layer's does, as
// Effect's do.
func LayerMergeAll(first, rest Value) Value {
	layers := append([]Value{first}, List(rest)...)
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		scopes := make([]*Owner, len(layers))
		for i := range scopes {
			scopes[i] = newOwner()
		}
		scope.add(fb, func(fb *Fiber, exit *Cause) Value {
			if cause := closeAll(fb, scopes, exit); cause != nil {
				panic(&Failure{cause})
			}
			return Unit
		})
		ended := make(chan *child, len(layers))
		children := make([]*child, len(layers))
		for i, l := range layers {
			build := Eff(func(fb *Fiber) Value { return layerOf(l).build(fb, memo, scopes[i]) })
			children[i] = fork(fb, newOwner(), build, func(c *child) { ended <- c })
		}
		var failed *Cause
		signal := fb.state.signal
		if fb.state.mask > 0 {
			signal = nil
		}
		stopAll := func() {
			for _, other := range children {
				other.state.interrupt(fb.state.id)
			}
		}
		idle()
		for left := len(layers); left > 0; {
			select {
			case c := <-ended:
				left--
				if c.cause != nil && failed == nil {
					failed = c.cause
					stopAll()
				}
			case <-signal:
				// interrupted: the builds stop, and their scopes release with the merged one's
				signal = nil
				stopAll()
				if failed == nil {
					failed = fb.state.stopped().Cause
				}
			}
		}
		if failed != nil {
			panic(&Failure{failed})
		}
		out := Services{}
		for _, c := range children {
			out = out.with(c.value.(Services))
		}
		return out
	})
}

// closeAll closes the scopes in parallel, with the causes of what failed.
func closeAll(fb *Fiber, scopes []*Owner, exit *Cause) *Cause {
	causes := make([]*Cause, len(scopes))
	var wg sync.WaitGroup
	for i, s := range scopes {
		wg.Add(1)
		inner := *fb
		inner.state = newState()
		go func() {
			defer wg.Done()
			causes[i] = s.closeFinalizers(&inner, exit)
		}()
	}
	wg.Wait()
	var out *Cause
	for _, c := range causes {
		out = Parallel(out, c)
	}
	return out
}

func LayerMerge(a, b Value) Value { return LayerMergeAll(a, []Value{b}) }

// LayerProvide is `Layer.provide_ that self`: `self` built with the
// services of `that`.
func LayerProvide(that, self Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		inputs := layerOf(that).build(fb, memo, scope)
		return layerOf(self).build(provided(fb, inputs), memo, scope)
	})
}

// LayerProvideMerge is [LayerProvide] keeping the services of `that`.
func LayerProvideMerge(that, self Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		inputs := layerOf(that).build(fb, memo, scope)
		return inputs.with(layerOf(self).build(provided(fb, inputs), memo, scope))
	})
}

func LayerFlatMap(f, self Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		s := layerOf(self).build(fb, memo, scope)
		return layerOf(Apply(fb, f, s)).build(fb, memo, scope)
	})
}

func LayerTap(f, self Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		s := layerOf(self).build(fb, memo, scope)
		scoped(fb, scope, Apply(fb, f, s))
		return s
	})
}

// onFailure is a layer that runs `handle` on the cause `self` fails with;
// what `handle` returns is built in its place, or nil to fail as it did.
func onFailure(self Value, handle func(fb *Fiber, scope *Owner, cause *Cause) Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		out, cause := catch(func() Value { return layerOf(self).build(fb, memo, scope) })
		if cause == nil {
			return out.(Services)
		}
		next := handle(fb, scope, cause)
		if next == nil {
			panic(&Failure{cause})
		}
		return layerOf(next).build(fb, memo, scope)
	})
}

// firstError is the error of the first failure of `c`, if it has one.
func firstError(c *Cause) (Value, bool) {
	for _, r := range c.reasons() {
		if r.kind == causeFail {
			return r.err, true
		}
	}
	return nil, false
}

func LayerTapError(f, self Value) Value {
	return onFailure(self, func(fb *Fiber, scope *Owner, cause *Cause) Value {
		if err, ok := firstError(cause); ok {
			scoped(fb, scope, Apply(fb, f, err))
		}
		return nil
	})
}

func LayerTapCause(f, self Value) Value {
	return onFailure(self, func(fb *Fiber, scope *Owner, cause *Cause) Value {
		scoped(fb, scope, Apply(fb, f, failureOf(cause)))
		return nil
	})
}

func LayerCatch(f, self Value) Value {
	return onFailure(self, func(fb *Fiber, _ *Owner, cause *Cause) Value {
		if err, ok := firstError(cause); ok {
			return Apply(fb, f, err)
		}
		return nil
	})
}

func LayerCatchCause(f, self Value) Value {
	return onFailure(self, func(fb *Fiber, _ *Owner, cause *Cause) Value {
		return Apply(fb, f, failureOf(cause))
	})
}

func LayerOrDie(self Value) Value {
	return &Layer{func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		out, cause := catch(func() Value { return layerOf(self).build(fb, memo, scope) })
		if cause != nil {
			panic(&Failure{cause.failsToDies()})
		}
		return out.(Services)
	}}
}

// LayerUpdateService is `self` built with the service of `key` its input
// has changed by `f`.
func LayerUpdateService(key, f, self Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		impl, ok := fb.env.get(key.(string))
		if !ok {
			panic(Defect{"Service not found: " + key.(string)})
		}
		return layerOf(self).build(provided(fb, Services{key.(string): Apply(fb, f, impl)}), memo, scope)
	})
}

func LayerFresh(self Value) Value {
	return &Layer{func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		return layerOf(self).build(fb, newMemoMap(nil), scope)
	}}
}

// LayerLaunch builds the layer and holds its services until interrupted.
func LayerLaunch(fb *Fiber, self Value) Value {
	return owned(fb, func(inner *Fiber, owner *Owner) Value {
		buildWith(inner, layerOf(self), currentMemo(inner), owner)
		return EffectNever(inner)
	})
}

func LayerBuild(fb *Fiber, self Value) Value {
	return buildWith(fb, layerOf(self), currentMemo(fb), fb.owner)
}

func LayerBuildWithMemoMap(fb *Fiber, memo, self Value) Value {
	return buildWith(fb, layerOf(self), memo.(*MemoMap), fb.owner)
}

func LayerMakeMemoMap(*Fiber, Value) Value          { return newMemoMap(nil) }
func LayerForkMemoMap(_ *Fiber, parent Value) Value { return newMemoMap(parent.(*MemoMap)) }
func LayerCurrentMemoMap(fb *Fiber, _ Value) Value  { return currentMemo(fb) }

// fromTask is the build of `build`, a function of the memo map to a task
// that acquires into the layer's scope.
func fromTask(build Value) func(*Fiber, *MemoMap, *Owner) Services {
	return func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		return scoped(fb, scope, Apply(fb, build, memo)).(Services)
	}
}

func LayerFromBuild(build Value) Value     { return fromBuild(fromTask(build)) }
func LayerFromBuildMemo(build Value) Value { return memoized(fromTask(build)) }
