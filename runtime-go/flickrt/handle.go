package flickrt

import "iter"

// Handlers of effects: `try … with` with arms for operations, and `provide
// S impl`. Each effect is a service whose implementation is the arms, as
// `S.Impl` is: a `tail` arm gives the operation its value, run with the
// services and the owner of the place of the handler; an `abort` arm
// unwinds the body to the handler and gives it its value. Neither is a
// typed failure of the code in between: a `try` there does not see them.
// The body of a handler with `control` arms runs as a coroutine: an
// operation hands its arm to the handler, which runs it with `resume`, the
// rest of the body; an arm that never resumes discontinues the body, which
// unwinds and closes its scopes.

// A handlerSignal is where a handler unwinds to: the `abort` arm to run
// there, or the failure of a `tail` arm, which is the handler's, not its
// body's.
type handlerSignal struct {
	token       *handlerToken
	arm         Value
	cause       *Cause
	discontinue bool
}

type handlerToken struct{ _ byte }

// unwinds is whether `c` unwinds to a handler of effects.
func unwinds(c *Cause) bool {
	for _, r := range c.reasons() {
		if r.kind == causeFail {
			if _, ok := r.err.(*handlerSignal); ok {
				return true
			}
		}
	}
	return false
}

// unwindingTo is the signal of the handler `token` in `c`.
func unwindingTo(c *Cause, token *handlerToken) *handlerSignal {
	for _, r := range c.reasons() {
		if r.kind == causeFail {
			if s, ok := r.err.(*handlerSignal); ok && s.token == token {
				return s
			}
		}
	}
	return nil
}

func unwind(s *handlerSignal) Value { panic(&Failure{&Cause{kind: causeFail, err: s}}) }

// around is a function of the program whose task, once its arguments are
// all given, is passed to `k`; the dictionaries of a constrained operation
// come first.
func around(f Value, k func(Eff) Value) Value {
	switch g := f.(type) {
	case Poly:
		return Poly(func(fb *Fiber, dicts ...Value) Value { return around(g(fb, dicts...), k) })
	case Fn:
		return Fn(func(fb *Fiber, a Value) Value { return around(g(fb, a), k) })
	case Fn2:
		return Fn2(func(fb *Fiber, a, b Value) Value { return around(g(fb, a, b), k) })
	case Fn3:
		return Fn3(func(fb *Fiber, a, b, c Value) Value { return around(g(fb, a, b, c), k) })
	case Eff:
		return k(g)
	}
	return f
}

// Install is one effect a handler installs: its key, the arms by
// operation, the operations whose arms abort, and those whose arms
// control, with the number of arguments each takes before `resume`.
type Install struct {
	Key      string
	Arms     Value
	Aborts   []string
	Controls []Control
}

// Control is an operation whose arm controls, with its arity.
type Control struct {
	Name  string
	Arity int
}

// A request is what the body asks of its handler: to run an arm, applied
// to the operation's arguments, with `resume`; or, for a handler around,
// to perform there what the body performed.
type request struct {
	arm      Value
	forward  func(*Fiber) Value
	response Value
}

// A coroutine is the body of a `control` handler: what it yields to, and
// the coroutine the handler runs in.
type coroutine struct {
	parent *coroutine
	token  *handlerToken
	yield  func(*request) bool
}

// controller is a handler with `control` arms while it runs.
type controller struct {
	token *handlerToken
	state *fiberState
	co    *coroutine
}

// perform hands `req` to the handler from the body: at once when the body
// runs in the handler's coroutine, through each coroutine between when it
// runs in one nested in it, and a defect from another fiber.
func (h *controller) perform(performer *Fiber, req *request) Value {
	cur := performer.state.coro
	if performer.state != h.state || cur == nil {
		panic(Defect{"a `control` operation ran in a fiber other than its handler's"})
	}
	if cur != h.co {
		inner := req
		req = &request{forward: func(driver *Fiber) Value { return h.perform(driver, inner) }}
	}
	if !cur.yield(req) {
		return unwind(&handlerSignal{token: cur.token, discontinue: true})
	}
	return req.response
}

// controlled is a function of the program that, given `arity` arguments,
// dictionaries first, is passed to `k`: a `control` arm before `resume`.
func controlled(f Value, arity int, k func(Value) Value) Value {
	if arity == 0 {
		return k(f)
	}
	if g, ok := f.(Poly); ok {
		return Poly(func(fb *Fiber, dicts ...Value) Value { return controlled(g(fb, dicts...), arity, k) })
	}
	return Fn(func(fb *Fiber, a Value) Value { return controlled(Apply(fb, f, a), arity-1, k) })
}

// Handle is the handler of `installs` around `body`: `ret`, when not nil,
// makes its value of the body's, and `errors`, when not nil, handles the
// body's typed errors as the arms of a `try` do; their values are the
// handler's.
func Handle(fb *Fiber, installs []Install, body Value, ret Value, errors func(*Fiber, Value) Value) Value {
	token := &handlerToken{}
	at := *fb
	env := fb.env
	var h *controller
	for _, in := range installs {
		arms := record(in.Arms)
		impl := newRecord(len(arms.keys))
	ops:
		for i, name := range arms.keys {
			for _, c := range in.Controls {
				if c.Name == name {
					if h == nil {
						h = &controller{token: token, state: fb.state}
					}
					impl.put(name, controlled(arms.vals[i], c.Arity, func(arm Value) Value {
						return Eff(func(performer *Fiber) Value { return h.perform(performer, &request{arm: arm}) })
					}))
					continue ops
				}
			}
			abort := false
			for _, a := range in.Aborts {
				abort = abort || a == name
			}
			impl.put(name, around(arms.vals[i], func(arm Eff) Value {
				if abort {
					return Eff(func(*Fiber) Value { return unwind(&handlerSignal{token: token, arm: arm}) })
				}
				return Eff(func(performer *Fiber) Value {
					here := &Fiber{state: performer.state, env: at.env, owner: at.owner}
					v, c := catch(func() Value { return Run(here, arm) })
					if c == nil {
						return v
					}
					if c.has(causeFail) && !c.has(causeDie) && !unwinds(c) {
						return unwind(&handlerSignal{token: token, cause: c})
					}
					panic(&Failure{c})
				})
			}))
		}
		env = env.with(in.Key, impl)
	}
	inner := &Fiber{state: fb.state, env: env, owner: fb.owner}
	if h != nil {
		return h.drive(fb, inner, body, func(v Value, c *Cause) Value { return handled(fb, token, v, c, ret, errors) })
	}
	v, c := catch(func() Value { return Run(inner, body) })
	return handled(fb, token, v, c, ret, errors)
}

// handled is the handler's value from how its body ended.
func handled(fb *Fiber, token *handlerToken, v Value, c *Cause, ret Value, errors func(*Fiber, Value) Value) Value {
	if c == nil {
		if ret != nil {
			return Run(fb, Apply(fb, ret, v))
		}
		return v
	}
	if s := unwindingTo(c, token); s != nil {
		if s.arm != nil {
			return Run(fb, s.arm)
		}
		panic(&Failure{s.cause})
	}
	if errors != nil && !unwinds(c) {
		return Try(fb, func(*Fiber) Value { panic(&Failure{c}) }, errors)
	}
	panic(&Failure{c})
}

// drive runs `body` in `inner` as a coroutine, each request it makes in
// `fb`, and `ended` of how it ends. A body still suspended when the
// handler ends is discontinued.
func (h *controller) drive(fb, inner *Fiber, body Value, ended func(Value, *Cause) Value) Value {
	co := &coroutine{parent: fb.state.coro, token: h.token}
	h.co = co
	var value Value
	var cause *Cause
	next, stop := iter.Pull(func(yield func(*request) bool) {
		co.yield = yield
		value, cause = catch(func() Value { return Run(inner, body) })
		if cause != nil {
			if s := unwindingTo(cause, h.token); s != nil && s.discontinue {
				cause = nil
			}
		}
	})
	defer stop()
	var step func() Value
	step = func() Value {
		saved := fb.state.coro
		fb.state.coro = co
		req, ok := next()
		fb.state.coro = saved
		if !ok {
			return ended(value, cause)
		}
		if req.forward != nil {
			req.response = req.forward(fb)
			return step()
		}
		used := false
		resume := Fn(func(_ *Fiber, v Value) Value {
			return Eff(func(*Fiber) Value {
				if used {
					panic(Defect{"`resume` ran twice: the rest of a computation resumes once"})
				}
				used = true
				req.response = v
				return step()
			})
		})
		out := Run(fb, Apply(fb, req.arm, resume))
		if !used {
			stop()
		}
		return out
	}
	return step()
}
