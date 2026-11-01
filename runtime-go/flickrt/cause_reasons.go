package flickrt

import "strings"

// The rest of Effect's `Cause` over Flick's opaque `Failure e`: its
// reasons, what a filter finds in it, and the stack frames annotated on it.

func reasonOf(r Value) *Cause { return r.(*Cause) }

// CauseFromReasons is Effect's `Cause.fromReasons`: repeats are kept.
func CauseFromReasons(reasons Value) Value {
	var c *Cause
	for _, r := range List(reasons) {
		c = concat(c, reasonOf(r))
	}
	return failureOf(c)
}

func CauseMakeFailReason(err Value) Value { return &Cause{kind: causeFail, err: err} }
func CauseMakeDieReason(message Value) Value {
	return &Cause{kind: causeDie, defect: message.(string)}
}
func CauseMakeInterruptReason(id Value) Value { return interruption(id) }
func CauseIsFailReason(r Value) Value         { return reasonOf(r).kind == causeFail }
func CauseIsDieReason(r Value) Value          { return reasonOf(r).kind == causeDie }
func CauseIsInterruptReason(r Value) Value    { return reasonOf(r).kind == causeInterrupt }

// findReason is Effect's filter of the first reason of `kind`: `Ok` of
// what `found` makes of it, else `Err` of the failure as it is.
func findReason(f Value, kind causeKind, found func(*Cause) Value) Value {
	if r := causeOfFailure(f).firstOf(kind); r != nil {
		return Ok(found(r))
	}
	return Err(f)
}

func reasonItself(r *Cause) Value { return r }

func CauseFindFail(f Value) Value { return findReason(f, causeFail, reasonItself) }
func CauseFindError(f Value) Value {
	return findReason(f, causeFail, func(r *Cause) Value { return r.err })
}
func CauseFindDie(f Value) Value { return findReason(f, causeDie, reasonItself) }
func CauseFindDefect(f Value) Value {
	return findReason(f, causeDie, func(r *Cause) Value { return r.message() })
}
func CauseFindInterrupt(f Value) Value { return findReason(f, causeInterrupt, reasonItself) }

// CauseFilterInterruptors is the ids of the interrupting fibers, when the
// failure has an interruption.
func CauseFilterInterruptors(f Value) Value {
	if !causeOfFailure(f).has(causeInterrupt) {
		return Err(f)
	}
	return Ok(CauseInterruptors(f))
}

func CausePretty(f Value) Value { return causePretty(causeOfFailure(f)) }

// CausePrettyErrors is `name: message` of each of Effect's pretty errors.
func CausePrettyErrors(f Value) Value {
	errs := prettyErrors(causeOfFailure(f))
	out := make([]Value, len(errs))
	for i, e := range errs {
		out[i] = e.name + ": " + e.message
	}
	return out
}

// annotated is Effect's `Cause.annotate` of a stack trace: each reason
// with `frame`, unless it has one and `overwrite` is false.
func (c *Cause) annotated(frame *stackFrame, overwrite bool) *Cause {
	var out *Cause
	changed := false
	for _, r := range c.reasons() {
		if r.trace == nil || overwrite {
			annotated := *r
			annotated.trace = frame
			r, changed = &annotated, true
		}
		out = concat(out, r)
	}
	if !changed {
		return c
	}
	return out
}

// frameOption is a stack frame as an Option.
func frameOption(frame *stackFrame) Value {
	if frame == nil {
		return None
	}
	return Some(frame)
}

// CauseStackTrace is the stack frame of the failure: Effect merges the
// annotations of its reasons, the last one winning.
func CauseStackTrace(f Value) Value {
	var frame *stackFrame
	for _, r := range causeOfFailure(f).reasons() {
		if r.trace != nil {
			frame = r.trace
		}
	}
	return frameOption(frame)
}

// CauseInterruptorStackTrace is the interrupting fiber's frame.
//
// shortcut: always None, as an interruption on the go target carries the interrupting fiber's id only; record its frame if a program reads it
func CauseInterruptorStackTrace(Value) Value { return None }

func CauseReasonStackTrace(r Value) Value { return frameOption(reasonOf(r).trace) }

func CauseAnnotate(overwrite, frame, f Value) Value {
	return failureOf(causeOfFailure(f).annotated(frame.(*stackFrame), overwrite.(bool)))
}

func CauseFrameName(frame Value) Value   { return frame.(*stackFrame).name }
func CauseFrameParent(frame Value) Value { return frame.(*stackFrame).parent }

func CauseFrameStack(frame Value) Value {
	if s := frame.(*stackFrame).stack; s != "" {
		return Some(s)
	}
	return None
}

// flickLocation is the Flick line the runtime was called from, as
// `at file.fl:line`, or "" for none.
func flickLocation() string {
	line, _, _ := strings.Cut(flickFrames(), "\n")
	return strings.TrimSpace(line)
}
