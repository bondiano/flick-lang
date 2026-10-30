package flickrt

import "strings"

// The `Cause` module beyond flags and combining, and `Failure`: reasons
// are the leaves of a cause, a failure's cause as a list.

func reasonOf(r Value) *Cause { return r.(*Cause) }

func CauseFromReasons(reasons Value) Value {
	var c *Cause
	for _, r := range List(reasons) {
		c = Sequential(c, reasonOf(r))
	}
	return failureOf(c)
}

func CauseReasons(f Value) Value {
	rs := causeOfFailure(f).reasons()
	out := make([]Value, len(rs))
	for i, r := range rs {
		out[i] = r
	}
	return out
}

func CauseMakeFailReason(err Value) Value { return &Cause{kind: causeFail, err: err} }
func CauseMakeDieReason(message Value) Value {
	return &Cause{kind: causeDie, defect: message.(string), plain: true}
}
func CauseMakeInterruptReason(id Value) Value { return interruption(id) }

func CauseIsFailReason(r Value) Value      { return reasonOf(r).kind == causeFail }
func CauseIsDieReason(r Value) Value       { return reasonOf(r).kind == causeDie }
func CauseIsInterruptReason(r Value) Value { return reasonOf(r).kind == causeInterrupt }

// findReason is Effect's filter result of a search of the failure's reasons:
// `Ok` what `f` gives of the first of `kind`, else `Err` the failure.
func findReason(f Value, kind causeKind, of func(*Cause) Value) Value {
	if r := causeOfFailure(f).firstOf(kind); r != nil {
		return Ok(of(r))
	}
	return Err(f)
}

func CauseFindFail(f Value) Value { return findReason(f, causeFail, func(r *Cause) Value { return r }) }
func CauseFindError(f Value) Value {
	return findReason(f, causeFail, func(r *Cause) Value { return r.err })
}
func CauseFindDie(f Value) Value { return findReason(f, causeDie, func(r *Cause) Value { return r }) }
func CauseFindDefect(f Value) Value {
	return findReason(f, causeDie, func(r *Cause) Value { return defectText(r) })
}
func CauseFindInterrupt(f Value) Value {
	return findReason(f, causeInterrupt, func(r *Cause) Value { return r })
}

func CauseFilterInterruptors(f Value) Value {
	if causeOfFailure(f).has(causeInterrupt) {
		return Ok(CauseInterruptors(f))
	}
	return Err(f)
}

func CausePretty(f Value) Value { return pretty(causeOfFailure(f)) }

// CausePrettyErrors is `name: message` of each failure and defect, or of
// the interruption when there is nothing else.
//
// shortcut: a typed error is named Error with its text, where Effect names a JS error by its class; follow it if a program shows these names
func CausePrettyErrors(f Value) Value {
	out := []Value{}
	for _, r := range causeOfFailure(f).reasons() {
		switch r.kind {
		case causeFail:
			out = append(out, "Error: "+describe(r.err))
		case causeDie:
			text, _, _ := strings.Cut(strings.TrimSuffix(r.defect, "\n"), "\n    at ")
			if r.plain || !strings.Contains(text, ": ") {
				text = "Error: " + text
			}
			out = append(out, text)
		}
	}
	if len(out) == 0 {
		out = append(out, "InterruptError: All fibers interrupted without error")
	}
	return out
}

// stackFrame is Effect's StackFrame: the span a failure happened in, its
// lines when captured, and the span around it.
type stackFrame struct {
	name   string
	stack  Value // an Option String
	parent *stackFrame
}

func optionFrame(f *stackFrame) Value {
	if f == nil {
		return None
	}
	return Some(f)
}

// frameOf is the stack frame the reasons of a cause carry, the last one
// winning as Effect merges their annotations.
func frameOf(c *Cause) *stackFrame {
	var out *stackFrame
	for _, r := range c.reasons() {
		if r.frame != nil {
			out = r.frame
		}
	}
	return out
}

// annotated is the cause with each reason given the frame: those without
// one, or all with `overwrite`.
func (c *Cause) annotated(f *stackFrame, overwrite bool) *Cause {
	if c == nil {
		return nil
	}
	switch c.kind {
	case causeSequential, causeParallel:
		return &Cause{kind: c.kind, left: c.left.annotated(f, overwrite), right: c.right.annotated(f, overwrite)}
	}
	if c.frame != nil && !overwrite {
		return c
	}
	out := *c
	out.frame = f
	return &out
}

func CauseStackTrace(f Value) Value { return optionFrame(frameOf(causeOfFailure(f))) }

// CauseInterruptorStackTrace is none: the go target does not annotate an
// interruption with the frame of the fiber that asked for it.
//
// shortcut: no interruptor frames; record the interrupting fiber's span when interrupting if a program needs it
func CauseInterruptorStackTrace(Value) Value { return None }

func CauseReasonStackTrace(r Value) Value { return optionFrame(reasonOf(r).frame) }

func CauseAnnotate(overwrite, frame, f Value) Value {
	return failureOf(causeOfFailure(f).annotated(frame.(*stackFrame), overwrite.(bool)))
}

func CauseFrameName(f Value) Value   { return f.(*stackFrame).name }
func CauseFrameStack(f Value) Value  { return f.(*stackFrame).stack }
func CauseFrameParent(f Value) Value { return optionFrame(f.(*stackFrame).parent) }

// withFrame runs `task` in a span's frame: a failure that leaves it
// without a frame gets this one, as Effect annotates a failure with the
// fiber's frame where it happens.
func withFrame(fb *Fiber, f *stackFrame, task Value) Value {
	v, c := catch(func() Value { return withEnv(fb, frameKey, f, task) })
	if c != nil {
		panic(&Failure{c.annotated(f, false)})
	}
	return v
}

const frameKey = "$stackFrame"

func currentFrame(fb *Fiber) *stackFrame {
	if f, ok := fb.env.get(frameKey); ok {
		return f.(*stackFrame)
	}
	return nil
}

// The `Failure` module beyond `primary`; a RawCause is the cause itself.

func FailureRawCause(f Value) Value           { return causeOfFailure(f) }
func FailureInspect(_ *Fiber, f Value) Value  { return causeOfFailure(f) }
func RawCauseInspect(_ *Fiber, c Value) Value { return c }
func FailureRethrow(_ *Fiber, f Value) Value  { return failWith(causeOfFailure(f)) }

// DescribeReport is `RawCause.describe`: the JS runner's one line of the
// reasons of a cause report.
func DescribeReport(c Value) Value {
	r, _ := c.(*Cause)
	return describeReasons(r.reasons())
}
