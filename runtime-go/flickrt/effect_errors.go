package flickrt

// The part `Effect.Errors` of Effect's `Effect`: a task's whole failure as
// an opaque `Failure`, its exit, and a defect as inert text. A filter
// gives `Ok` of what it found, or `Err` of what stays.

// onCause runs `task`; when it fails, `handle` of the cause ends it.
func onCause(fb *Fiber, task Value, handle func(*Cause) Value) Value {
	v, c := catch(func() Value { return Run(fb, task) })
	if c == nil {
		return v
	}
	return handle(c)
}

// failCause fails with the cause again.
func failCause(c *Cause) Value { panic(&Failure{c}) }

func EffectCatchCause(fb *Fiber, f, task Value) Value {
	return onCause(fb, task, func(c *Cause) Value { return Run(fb, Apply(fb, f, failureOf(c))) })
}

func EffectCatchCauseIf(fb *Fiber, p, f, task Value) Value {
	return onCause(fb, task, func(c *Cause) Value {
		if !Bool(Apply(fb, p, failureOf(c))) {
			return failCause(c)
		}
		return Run(fb, Apply(fb, f, failureOf(c)))
	})
}

// EffectCatchCauseFilter fails with what the filter leaves, when it
// refuses the failure.
func EffectCatchCauseFilter(fb *Fiber, filter, f, task Value) Value {
	return onCause(fb, task, func(c *Cause) Value {
		r := Apply(fb, filter, failureOf(c))
		if !isOk(r) {
			return failCause(causeOfFailure(value(r)))
		}
		return Run(fb, apply2(fb, f, value(r), failureOf(c)))
	})
}

func EffectCatchDefect(fb *Fiber, f, task Value) Value {
	return onCause(fb, task, func(c *Cause) Value {
		r := c.firstOf(causeDie)
		if r == nil {
			return failCause(c)
		}
		return Run(fb, Apply(fb, f, defectMessage(r.defect)))
	})
}

// tapping runs `task`; when it fails, the task `tap` gives of the cause,
// if any, runs before the task fails as it did.
func tapping(fb *Fiber, task Value, tap func(*Cause) Value) Value {
	return onCause(fb, task, func(c *Cause) Value {
		if t := tap(c); t != nil {
			Run(fb, t)
		}
		return failCause(c)
	})
}

func EffectTapCause(fb *Fiber, f, task Value) Value {
	return tapping(fb, task, func(c *Cause) Value { return Apply(fb, f, failureOf(c)) })
}

func EffectTapCauseIf(fb *Fiber, p, f, task Value) Value {
	return tapping(fb, task, func(c *Cause) Value {
		if !Bool(Apply(fb, p, failureOf(c))) {
			return nil
		}
		return Apply(fb, f, failureOf(c))
	})
}

func EffectTapCauseFilter(fb *Fiber, filter, f, task Value) Value {
	return tapping(fb, task, func(c *Cause) Value {
		r := Apply(fb, filter, failureOf(c))
		if !isOk(r) {
			return nil
		}
		return apply2(fb, f, value(r), failureOf(c))
	})
}

func EffectTapDefect(fb *Fiber, f, task Value) Value {
	return tapping(fb, task, func(c *Cause) Value {
		r := c.firstOf(causeDie)
		if r == nil {
			return nil
		}
		return Apply(fb, f, defectMessage(r.defect))
	})
}

// EffectMatchCause is `onFailure` of the cause or `onSuccess` of the
// value; the eager form is the same here, as a task has no exit to read
// before it runs.
func EffectMatchCause(fb *Fiber, onFailure, onSuccess, task Value) Value {
	v, c := catch(func() Value { return Run(fb, task) })
	if c != nil {
		return Apply(fb, onFailure, failureOf(c))
	}
	return Apply(fb, onSuccess, v)
}

func EffectMatchCauseEffect(fb *Fiber, onFailure, onSuccess, task Value) Value {
	v, c := catch(func() Value { return Run(fb, task) })
	if c != nil {
		return Run(fb, Apply(fb, onFailure, failureOf(c)))
	}
	return Run(fb, Apply(fb, onSuccess, v))
}

// EffectOrDie is Effect's `catch_(self, die)`: a cause with a typed
// failure becomes the defect of its first error.
func EffectOrDie(fb *Fiber, task Value) Value {
	return onCause(fb, task, func(c *Cause) Value {
		r := c.firstOf(causeFail)
		if r == nil {
			return failCause(c)
		}
		return failCause(&Cause{kind: causeDie, defect: describe(r.err)})
	})
}

// ignoreTask runs `task` for nothing: a failure `drop` holds for is logged
// at the level, if one is given; another fails as it did.
func ignoreTask(fb *Fiber, level, task Value, drop func(*Cause) bool) Value {
	onCause(fb, task, func(c *Cause) Value {
		if !drop(c) {
			return failCause(c)
		}
		if l, ok := some(level); ok {
			logWith(fb, l.(string), []Value{}, c)
		}
		return Unit
	})
	return Unit
}

func EffectIgnore(fb *Fiber, level, task Value) Value {
	return ignoreTask(fb, level, task, func(c *Cause) bool { return c.has(causeFail) })
}

func EffectIgnoreCause(fb *Fiber, level, task Value) Value {
	return ignoreTask(fb, level, task, func(*Cause) bool { return true })
}

func EffectOnError(fb *Fiber, f, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if e.cause == nil {
			return nil
		}
		return Apply(fb, f, failureOf(e.cause))
	})
}

func EffectOnErrorIf(fb *Fiber, p, f, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if e.cause == nil || !Bool(Apply(fb, p, failureOf(e.cause))) {
			return nil
		}
		return Apply(fb, f, failureOf(e.cause))
	})
}

func EffectOnErrorFilter(fb *Fiber, filter, f, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if e.cause == nil {
			return nil
		}
		r := Apply(fb, filter, failureOf(e.cause))
		if !isOk(r) {
			return nil
		}
		return apply2(fb, f, value(r), failureOf(e.cause))
	})
}

func EffectOnExitIf(fb *Fiber, p, f, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if !Bool(Apply(fb, p, e)) {
			return nil
		}
		return Apply(fb, f, e)
	})
}

func EffectOnExitFilter(fb *Fiber, filter, f, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		r := Apply(fb, filter, e)
		if !isOk(r) {
			return nil
		}
		return apply2(fb, f, value(r), e)
	})
}

// EffectOnExitPrimitive runs the finalizer `f` gives, if any; with
// `interruptible` it keeps the interruptibility around the call.
func EffectOnExitPrimitive(fb *Fiber, interruptible, f, task Value) Value {
	outer := fb.state.mask
	return withExit(fb, task, func(e *Exit) Value {
		t, ok := some(Apply(fb, f, e))
		if !ok {
			return nil
		}
		if !interruptible.(bool) {
			return t
		}
		return Eff(func(fb *Fiber) Value { return masked(fb, outer, func() Value { return Run(fb, t) }) })
	})
}

// EffectWithErrorReporting reports a failure of `task` to the fiber's
// error reporters (only one with a defect when `defectsOnly`).
func EffectWithErrorReporting(fb *Fiber, defectsOnly, task Value) Value {
	return withExit(fb, task, func(e *Exit) Value {
		if e.cause != nil && (!defectsOnly.(bool) || e.cause.has(causeDie)) {
			ErrorReporterReport(fb, failureOf(e.cause))
		}
		return nil
	})
}
