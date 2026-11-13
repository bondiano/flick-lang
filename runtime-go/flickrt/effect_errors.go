package flickrt

// Effect's `Effect` on whole failures, the part `Effect.Errors`: a cause
// reaches Flick as an opaque `Failure`, an exit as an `Exit` and a defect
// as text.

// runCaught runs `task`: its value, or nil and its cause.
func runCaught(fb *Fiber, task Value) (Value, *Cause) {
	return catch(func() Value { return Run(fb, task) })
}

func EffectCatchCause(fb *Fiber, f, task Value) Value {
	v, c := runCaught(fb, task)
	if c == nil {
		return v
	}
	return Run(fb, Apply(f, failureOf(c)))
}

func EffectCatchCauseIf(fb *Fiber, predicate, f, task Value) Value {
	v, c := runCaught(fb, task)
	if c == nil {
		return v
	}
	if !Bool(Apply(predicate, failureOf(c))) {
		panic(&Failure{c})
	}
	return Run(fb, Apply(f, failureOf(c)))
}

// EffectCatchCauseFilter recovers with `f` from a failure the filter
// passes; one it refuses fails with what the filter gives.
func EffectCatchCauseFilter(fb *Fiber, filter, f, task Value) Value {
	v, c := runCaught(fb, task)
	if c == nil {
		return v
	}
	r := Apply(filter, failureOf(c))
	if !isOk(r) {
		return failWith(causeOfFailure(value(r)))
	}
	return Run(fb, apply2(f, value(r), failureOf(c)))
}

func EffectCatchDefect(fb *Fiber, f, task Value) Value {
	v, c := runCaught(fb, task)
	if c == nil {
		return v
	}
	r := c.firstOf(causeDie)
	if r == nil {
		panic(&Failure{c})
	}
	return Run(fb, Apply(f, defectText(r)))
}

// tapped runs `task`; when it fails, `f` of its cause, if `f` gives a
// task, before failing again.
func tapped(fb *Fiber, task Value, f func(*Cause) Value) Value {
	v, c := runCaught(fb, task)
	if c == nil {
		return v
	}
	if t := f(c); t != nil {
		Run(fb, t)
	}
	panic(&Failure{c})
}

func EffectTapCause(fb *Fiber, f, task Value) Value {
	return tapped(fb, task, func(c *Cause) Value { return Apply(f, failureOf(c)) })
}

func EffectTapCauseIf(fb *Fiber, predicate, f, task Value) Value {
	return tapped(fb, task, func(c *Cause) Value {
		if !Bool(Apply(predicate, failureOf(c))) {
			return nil
		}
		return Apply(f, failureOf(c))
	})
}

func EffectTapCauseFilter(fb *Fiber, filter, f, task Value) Value {
	return tapped(fb, task, func(c *Cause) Value {
		if r := Apply(filter, failureOf(c)); isOk(r) {
			return apply2(f, value(r), failureOf(c))
		}
		return nil
	})
}

func EffectTapDefect(fb *Fiber, f, task Value) Value {
	return tapped(fb, task, func(c *Cause) Value {
		if r := c.firstOf(causeDie); r != nil {
			return Apply(f, defectText(r))
		}
		return nil
	})
}

// matched is `onFailure` of the failure or `onSuccess` of the value, as
// `run` makes either a value.
func matched(fb *Fiber, onFailure, onSuccess, task Value, run func(Value) Value) Value {
	v, c := runCaught(fb, task)
	if c != nil {
		return run(Apply(onFailure, failureOf(c)))
	}
	return run(Apply(onSuccess, v))
}

func EffectMatchCause(fb *Fiber, onFailure, onSuccess, task Value) Value {
	return matched(fb, onFailure, onSuccess, task, Identity)
}

func EffectMatchCauseEffect(fb *Fiber, onFailure, onSuccess, task Value) Value {
	return matched(fb, onFailure, onSuccess, task, func(t Value) Value { return Run(fb, t) })
}

// EffectOrDie turns a typed failure into a defect of the error.
func EffectOrDie(fb *Fiber, task Value) Value {
	v, c := runCaught(fb, task)
	if c == nil {
		return v
	}
	if r := c.firstOf(causeFail); r != nil {
		panic(&Failure{&Cause{kind: causeDie, defect: describe(r.err)}})
	}
	panic(&Failure{c})
}

// EffectIgnore drops a typed failure; with a level Effect logs it, which
// the go target does not.
//
// shortcut: no log at the level given, as Effect.log has no go form yet; log the cause with it
func EffectIgnore(fb *Fiber, _, task Value) Value {
	if _, c := runCaught(fb, task); c != nil && c.firstOf(causeFail) == nil {
		panic(&Failure{c})
	}
	return Unit
}

// EffectIgnoreCause drops any failure.
//
// shortcut: no log at the level given, as for EffectIgnore
func EffectIgnoreCause(fb *Fiber, _, task Value) Value {
	runCaught(fb, task)
	return Unit
}

// afterFailure runs `task`, then, when it failed, `f` of its cause
// uninterruptibly, if `f` gives a task.
func afterFailure(fb *Fiber, task Value, f func(*Cause) Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if e.cause == nil {
			return nil
		}
		return f(e.cause)
	})
}

func EffectOnError(fb *Fiber, f, task Value) Value {
	return afterFailure(fb, task, func(c *Cause) Value { return Apply(f, failureOf(c)) })
}

func EffectOnErrorIf(fb *Fiber, predicate, f, task Value) Value {
	return afterFailure(fb, task, func(c *Cause) Value {
		if !Bool(Apply(predicate, failureOf(c))) {
			return nil
		}
		return Apply(f, failureOf(c))
	})
}

func EffectOnErrorFilter(fb *Fiber, filter, f, task Value) Value {
	return afterFailure(fb, task, func(c *Cause) Value {
		if r := Apply(filter, failureOf(c)); isOk(r) {
			return apply2(f, value(r), failureOf(c))
		}
		return nil
	})
}

func EffectOnExitIf(fb *Fiber, predicate, f, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if !Bool(Apply(predicate, e)) {
			return nil
		}
		return Apply(f, e)
	})
}

func EffectOnExitFilter(fb *Fiber, filter, f, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if r := Apply(filter, e); isOk(r) {
			return apply2(f, value(r), e)
		}
		return nil
	})
}

// EffectOnExitPrimitive runs the task `f` gives of the exit, if any;
// interruptibly when asked, as Effect's primitive can.
func EffectOnExitPrimitive(fb *Fiber, interruptible, f, task Value) Value {
	e := exitOf(fb, task)
	if o := Apply(f, e).(*Data); o.Tag == "Some" {
		run := Uninterruptible
		if interruptible.(bool) {
			run = Interruptible
		}
		_, c := catch(func() Value { return run(fb, o.Fields[0]) })
		e = &Exit{e.value, Sequential(e.cause, c)}
	}
	if e.cause != nil {
		panic(&Failure{e.cause})
	}
	return e.value
}

// EffectWithErrorReporting gives a failure, or only one with a defect,
// to the error reporters of the fiber before failing again.
func EffectWithErrorReporting(fb *Fiber, defectsOnly, task Value) Value {
	return afterFailure(fb, task, func(c *Cause) Value {
		if !defectsOnly.(bool) || c.has(causeDie) {
			ErrorReporterReport(fb, failureOf(c))
		}
		return nil
	})
}
