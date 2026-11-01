package flickrt

import (
	"slices"
	"strings"
)

// Exit is how a computation ended: its value, or the cause it failed with.
type Exit struct {
	value Value
	cause *Cause
}

// exitOf runs `task` to its exit, which itself never fails.
func exitOf(fb *Fiber, task Value) *Exit {
	v, c := catch(func() Value { return Run(fb, task) })
	return &Exit{v, c}
}

func ex(v Value) *Exit { return v.(*Exit) }

// failureOf is the `Failure` of a cause; one without reasons is the empty
// cause, which still fails, as Effect's `Cause.empty`.
func failureOf(c *Cause) Value {
	if c == nil {
		c = emptyCause
	}
	return &Supervised{cause: c}
}
func causeOfFailure(f Value) *Cause { return f.(*Supervised).cause }

// outcome is the exit as a task that ends the same way.
func (e *Exit) outcome() Value {
	if e.cause != nil {
		return failing(e.cause)
	}
	return Succeed(e.value)
}

// interruption is the interruption by fiber `id`; 0 is no fiber.
func interruption(id Value) *Cause {
	var fiber int64
	if o := id.(*Data); o.Tag == "Some" {
		fiber = int64(o.Fields[0].(float64))
	}
	return &Cause{kind: causeInterrupt, fiber: fiber}
}

func ExitValue(e Value) Value {
	if x := ex(e); x.cause == nil {
		return Some(x.value)
	}
	return None
}

func ExitFailure(e Value) Value {
	if x := ex(e); x.cause != nil {
		return Some(failureOf(x.cause))
	}
	return None
}

func ExitSucceed(v Value) Value   { return &Exit{value: v} }
func ExitFail(err Value) Value    { return &Exit{cause: &Cause{kind: causeFail, err: err}} }
func ExitFailCause(f Value) Value { return &Exit{cause: causeOfFailure(f)} }
func ExitDie(message Value) Value {
	return &Exit{cause: &Cause{kind: causeDie, defect: message.(string)}}
}
func ExitInterrupt(id Value) Value    { return &Exit{cause: interruption(id)} }
func ExitIsSuccess(e Value) Value     { return ex(e).cause == nil }
func ExitIsFailure(e Value) Value     { return ex(e).cause != nil }
func ExitHasFails(e Value) Value      { return ex(e).cause.has(causeFail) }
func ExitHasDies(e Value) Value       { return ex(e).cause.has(causeDie) }
func ExitHasInterrupts(e Value) Value { return ex(e).cause.has(causeInterrupt) }

var ExitVoid Value = &Exit{value: Unit}

// pass is Effect's filter result: `Ok` what passes, `Err` the rest.
func pass(ok bool, yes, no Value) Value {
	if ok {
		return Ok(yes)
	}
	return Err(no)
}

func ExitFilterSuccess(e Value) Value { return pass(ex(e).cause == nil, e, e) }
func ExitFilterFailure(e Value) Value { return pass(ex(e).cause != nil, e, e) }

func ExitFilterValue(e Value) Value {
	x := ex(e)
	return pass(x.cause == nil, x.value, e)
}

func ExitFilterCause(e Value) Value {
	x := ex(e)
	if x.cause == nil {
		return Err(e)
	}
	return Ok(failureOf(x.cause))
}

// firstOf is the first reason of `kind` in a cause.
func (c *Cause) firstOf(kind causeKind) *Cause {
	for _, r := range c.reasons() {
		if r.kind == kind {
			return r
		}
	}
	return nil
}

func ExitFindError(e Value) Value {
	if r := ex(e).cause.firstOf(causeFail); r != nil {
		return Ok(r.err)
	}
	return Err(e)
}

func ExitFindErrorOption(e Value) Value {
	if r := ex(e).cause.firstOf(causeFail); r != nil {
		return Some(r.err)
	}
	return None
}

func ExitFindDefect(e Value) Value {
	if r := ex(e).cause.firstOf(causeDie); r != nil {
		return Ok(r.message())
	}
	return Err(e)
}

// defectMessage is a defect as the JS target describes it: the message of
// an error, without its name and the Flick lines it unwound.
func defectMessage(defect string) string {
	text, _, _ := strings.Cut(defect, "\n    at ")
	text = strings.TrimSuffix(text, "\n")
	name, message, ok := strings.Cut(text, ": ")
	if ok && slices.Contains(jsErrorNames, name) {
		return message
	}
	return text
}

var jsErrorNames = []string{"Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError", "URIError", "EvalError", "AggregateError"}

func ExitMatch(fb *Fiber, onSuccess, onFailure, e Value) Value {
	if x := ex(e); x.cause != nil {
		return Apply(fb, onFailure, failureOf(x.cause))
	}
	return Apply(fb, onSuccess, ex(e).value)
}

func ExitMap(fb *Fiber, f, e Value) Value {
	if x := ex(e); x.cause == nil {
		return &Exit{value: Apply(fb, f, x.value)}
	}
	return e
}

// mapErrors is the cause with its first typed error mapped by `f`: only
// that one has an error of the row; a later one, which a narrowing `try`
// left behind, has no type to map from (docs/formal/runtime.md §2.4).
func (c *Cause) mapErrors(fb *Fiber, f Value) *Cause {
	var out *Cause
	head := true
	for _, r := range c.reasons() {
		if head && r.kind == causeFail {
			r, head = &Cause{kind: causeFail, err: Apply(fb, f, r.err)}, false
		}
		out = concat(out, r)
	}
	return out
}

func ExitMapError(fb *Fiber, f, e Value) Value {
	x := ex(e)
	return &Exit{x.value, x.cause.mapErrors(fb, f)}
}

func ExitMapBoth(fb *Fiber, onFailure, onSuccess, e Value) Value {
	return ExitMap(fb, onSuccess, ExitMapError(fb, onFailure, e))
}

func ExitAsVoid(e Value) Value {
	if ex(e).cause == nil {
		return ExitVoid
	}
	return e
}

func ExitAsVoidAll(exits Value) Value {
	var cause *Cause
	for _, e := range List(exits) {
		cause = concat(cause, ex(e).cause)
	}
	return &Exit{Unit, cause}
}

// The `Cause` module, on the cause of a `Failure`.

var CauseEmpty Value = failureOf(nil)

func CauseFail(err Value) Value { return failureOf(&Cause{kind: causeFail, err: err}) }
func CauseDie(message Value) Value {
	return failureOf(&Cause{kind: causeDie, defect: message.(string)})
}
func CauseInterrupt(id Value) Value    { return failureOf(interruption(id)) }
func CauseHasFails(f Value) Value      { return causeOfFailure(f).has(causeFail) }
func CauseHasDies(f Value) Value       { return causeOfFailure(f).has(causeDie) }
func CauseHasInterrupts(f Value) Value { return causeOfFailure(f).has(causeInterrupt) }
func CauseCombine(that, self Value) Value {
	return failureOf(Sequential(causeOfFailure(self), causeOfFailure(that)))
}
func CauseMap(fb *Fiber, f, self Value) Value {
	return failureOf(causeOfFailure(self).mapErrors(fb, f))
}

// CauseReasons is the reasons as `Failure e` types them: a typed failure
// after the first is a defect here.
func CauseReasons(f Value) Value {
	reasons := causeOfFailure(f).reasons()
	out := make([]Value, len(reasons))
	head := true
	for i, r := range reasons {
		if r.kind == causeFail {
			if !head {
				r = dieOf(r.err)
			}
			head = false
		}
		out[i] = r
	}
	return out
}

func CauseHasInterruptsOnly(f Value) Value {
	c := causeOfFailure(f)
	return c.has(causeInterrupt) && !c.has(causeFail) && !c.has(causeDie)
}

func CauseFindErrorOption(f Value) Value {
	if r := causeOfFailure(f).firstOf(causeFail); r != nil {
		return Some(r.err)
	}
	return None
}

// CauseInterruptors is the ids of the fibers that interrupted, each once.
func CauseInterruptors(f Value) Value {
	out, seen := []Value{}, map[int64]bool{}
	for _, r := range causeOfFailure(f).reasons() {
		if r.kind == causeInterrupt && r.fiber != 0 && !seen[r.fiber] {
			seen[r.fiber] = true
			out = append(out, float64(r.fiber))
		}
	}
	return out
}

// Awaiting fibers.

func (c *child) exit() *Exit { return &Exit{c.value, c.cause} }

func FiberAwait(fb *Fiber, f Value) Value {
	c := f.(*child)
	wait(fb, c.done)
	return c.exit()
}

func FiberAwaitAll(fb *Fiber, fibers Value) Value {
	in := List(fibers)
	out := make([]Value, len(in))
	for i, f := range in {
		out[i] = FiberAwait(fb, f)
	}
	return out
}

func FiberJoinAll(fb *Fiber, fibers Value) Value {
	in := List(fibers)
	out := make([]Value, len(in))
	for i, f := range in {
		out[i] = FiberJoin(fb, f)
	}
	return out
}

// interruptAs interrupts the fibers as fiber `by` and awaits them.
func interruptAs(by int64, fibers []Value) Value {
	children := make([]*child, len(fibers))
	for i, f := range fibers {
		children[i] = f.(*child)
		children[i].state.interrupt(by)
	}
	for _, c := range children {
		<-c.done
	}
	return Unit
}

func FiberInterruptAs(fb *Fiber, id, f Value) Value {
	by := interruption(id).fiber
	if by == 0 {
		by = fb.state.id
	}
	return interruptAs(by, []Value{f})
}

func FiberInterruptAll(fb *Fiber, fibers Value) Value { return interruptAs(fb.state.id, List(fibers)) }

func FiberInterruptAllAs(_ *Fiber, id, fibers Value) Value {
	return interruptAs(int64(id.(float64)), List(fibers))
}

// ParallelExits runs the tasks concurrently: the exit of each.
func ParallelExits(fb *Fiber, tasks Value) Value {
	in := List(tasks)
	wrapped := make([]Value, len(in))
	for i, t := range in {
		wrapped[i] = Eff(func(fb *Fiber) Value { return handledExit(fb, t) })
	}
	return all(fb, wrapped, 0)
}

// EffectExit is `Effect.exit`: the exit of a task.
func EffectExit(fb *Fiber, task Value) Value { return handledExit(fb, task) }

// handledExit is the exit of `task` for a handler that sees it, which a
// fiber asked to stop skips; `exitOf` is for a boundary that must run.
func handledExit(fb *Fiber, task Value) *Exit {
	v, c := caught(fb, func() Value { return Run(fb, task) })
	return &Exit{v, c}
}

// withExit runs `task`, then `after` of its exit uninterruptibly; a
// failure of `after` joins the task's.
func withExit(fb *Fiber, task Value, after func(*Exit) Value) Value {
	e := exitOf(fb, task)
	if task := after(e); task != nil {
		_, c := catch(func() Value { return uninterruptible(fb, task) })
		e = &Exit{e.value, Sequential(e.cause, c)}
	}
	if e.cause != nil {
		panic(&Failure{e.cause})
	}
	return e.value
}

func EffectOnExit(fb *Fiber, handler, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value { return Apply(fb, handler, e) })
}

func EffectOnInterrupt(fb *Fiber, handler, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if !e.cause.has(causeInterrupt) {
			return nil
		}
		return Apply(fb, handler, CauseInterruptors(failureOf(e.cause)))
	})
}

// DeferredDone completes the deferred with an exit.
func DeferredDone(_ *Fiber, e, d Value) Value { return settle(d, ex(e).outcome()) }
