package flickrt

import (
	"fmt"
	"slices"
	"strings"
)

// Cause is why a computation failed, as Effect's: typed failures, defects
// and interruptions, sequential or parallel. A failing computation panics
// with a *Failure carrying its cause.
type Cause struct {
	kind   causeKind
	err    Value  // a typed failure's error
	defect string // a defect's message, with the Flick lines it unwound
	fiber  int64  // the interrupting fiber
	left   *Cause
	right  *Cause
}

type causeKind int

const (
	causeFail causeKind = iota
	causeDie
	causeInterrupt
	causeSequential
	causeParallel
)

// Failure is the panic of a failing computation.
type Failure struct{ Cause *Cause }

func (f *Failure) Error() string { return pretty(f.Cause) }

// Fail fails with a typed error.
func Fail(err Value) Value { panic(&Failure{&Cause{kind: causeFail, err: err}}) }

// Sequential and Parallel combine two causes as Effect's `Cause.combine`
// does: the reasons of both, each once, so the kind of the node is never
// observed (docs/formal/runtime.md §2.1).
func Sequential(a, b *Cause) *Cause { return combineReasons(causeSequential, a, b) }
func Parallel(a, b *Cause) *Cause   { return combineReasons(causeParallel, a, b) }

// concat is the reasons of both causes, repeats kept, as Effect's
// `Cause.fromReasons` over both lists: races and a scope's finalizers.
func concat(a, b *Cause) *Cause { return combine2(causeSequential, a, b) }

func combineReasons(kind causeKind, a, b *Cause) *Cause {
	if a == nil || b == nil {
		return combine2(kind, a, b)
	}
	var out *Cause
	var kept []*Cause
	for _, r := range append(a.reasons(), b.reasons()...) {
		if !slices.ContainsFunc(kept, func(k *Cause) bool { return sameReason(k, r) }) {
			kept = append(kept, r)
			out = combine2(kind, out, r)
		}
	}
	return out
}

// sameReason is Effect's equality of reasons as far as Go can tell it: a
// defect is only its own text here, while two defects of one text are
// two different errors in JS.
func sameReason(a, b *Cause) bool {
	if a.kind != b.kind {
		return false
	}
	switch a.kind {
	case causeFail:
		return Equals(a.err, b.err)
	case causeInterrupt:
		return a.fiber == b.fiber
	}
	return a == b
}

func combine2(kind causeKind, a, b *Cause) *Cause {
	switch {
	case a == nil:
		return b
	case b == nil:
		return a
	}
	return &Cause{kind: kind, left: a, right: b}
}

// reasons is the failures, defects and interruptions of a cause, in order.
func (c *Cause) reasons() []*Cause {
	if c == nil {
		return nil
	}
	if c.kind == causeSequential || c.kind == causeParallel {
		return append(c.left.reasons(), c.right.reasons()...)
	}
	return []*Cause{c}
}

func (c *Cause) has(kind causeKind) bool {
	for _, r := range c.reasons() {
		if r.kind == kind {
			return true
		}
	}
	return false
}

// failsToDies is the cause with each typed failure a defect.
func (c *Cause) failsToDies() *Cause {
	var out *Cause
	for _, r := range c.reasons() {
		if r.kind == causeFail {
			r = &Cause{kind: causeDie, defect: describe(r.err)}
		}
		out = concat(out, r)
	}
	return out
}

// withoutInterrupts is the cause without its interruptions; nil when only
// they were left.
func withoutInterrupts(c *Cause) *Cause {
	var out *Cause
	for _, r := range c.reasons() {
		if r.kind != causeInterrupt {
			out = concat(out, r)
		}
	}
	return out
}

// causeOf is the cause a recovered panic carries; any other panic is a
// defect, its Flick lines read while the panic's frames are on the stack.
func causeOf(r any) *Cause {
	switch x := r.(type) {
	case *Failure:
		return x.Cause
	case Defect:
		return &Cause{kind: causeDie, defect: x.Message + "\n" + flickFrames()}
	case error:
		return &Cause{kind: causeDie, defect: x.Error() + "\n" + flickFrames()}
	}
	return &Cause{kind: causeDie, defect: fmt.Sprint(r) + "\n" + flickFrames()}
}

// Unmatched is what the handler of a `try` gives when no arm matches.
var Unmatched Value = &struct{ unmatched bool }{true}

// Try is `try body with arms`: the first typed failure of a cause without
// defects goes to `handle`, which gives Unmatched when no arm matches and
// the cause goes on. A cause with a defect turns its failures into defects;
// interruption alone has no typed error.
func Try(fb *Fiber, body Eff, handle func(*Fiber, Value) Value) Value {
	out, caught := catch(func() Value { return body(fb) })
	if caught == nil {
		return out
	}
	if caught.has(causeDie) {
		if caught.has(causeFail) {
			caught = caught.failsToDies()
		}
		panic(&Failure{caught})
	}
	for _, r := range caught.reasons() {
		if r.kind == causeFail {
			if v := handle(fb, r.err); v != Unmatched {
				return v
			}
			break
		}
	}
	panic(&Failure{caught})
}

var (
	appErrorTag = Tag("Prelude.AppError.AppError")
)

// FromResult is the value of an `Ok`; an `Err` fails with its error.
func FromResult(r Value) Value {
	if isOk(r) {
		return value(r)
	}
	return Fail(value(r))
}

// AppFail fails with an `AppError` of the message.
func AppFail(message Value) Value {
	return Fail(NewData(appErrorTag, Rec("message", message, "details", None)))
}

// describe is the text of a reason, as the JS runner writes one: a
// record's `message`, else JSON.
func describe(err Value) string {
	if r, ok := err.(*Record); ok && r.index("message") >= 0 {
		if m, ok := r.Get("message").(string); ok {
			return m
		}
	}
	if s, ok := err.(string); ok {
		return s
	}
	return Format(err)
}

// pretty is each failure and defect of a cause on its own line.
func pretty(c *Cause) string {
	var lines []string
	for _, r := range c.reasons() {
		switch r.kind {
		case causeFail:
			lines = append(lines, describe(r.err))
		case causeDie:
			lines = append(lines, strings.TrimSuffix(r.defect, "\n"))
		}
	}
	if len(lines) == 0 {
		return "InterruptError: All fibers interrupted without error"
	}
	return strings.Join(lines, "\n")
}

// report is what the runner writes on stderr when `main` fails, as the
// JS runner's `conclude` does.
func report(c *Cause) string {
	if !c.has(causeFail) && !c.has(causeDie) {
		return "error: main was interrupted"
	}
	if !c.has(causeDie) {
		for _, r := range c.reasons() {
			if d, ok := r.err.(*Data); ok && r.kind == causeFail && d.Tag == appErrorTag {
				return "error: " + d.Fields[0].(*Record).Get("message").(string)
			}
		}
	}
	return pretty(c)
}
