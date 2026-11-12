package flickrt

import "testing"

func failure(f func()) (c *Cause) {
	defer func() {
		if r := recover(); r != nil {
			c = causeOf(r)
		}
	}()
	f()
	return nil
}

func TestTryHandlesTypedFailuresOnly(t *testing.T) {
	fb := rootFiber()
	handle := func(_ *Fiber, err Value) Value {
		if err == "boom" {
			return "handled"
		}
		return Unmatched
	}
	if got := Try(fb, func(*Fiber) Value { return Fail("boom") }, handle); got != "handled" {
		t.Errorf("a typed failure an arm matches is handled: %v", got)
	}
	c := failure(func() { Try(fb, func(*Fiber) Value { return Fail("other") }, handle) })
	if c == nil || c.reasons()[0].err != "other" {
		t.Error("no matching arm: the failure goes on")
	}
	c = failure(func() { Try(fb, func(*Fiber) Value { panic(Defect{"broken"}) }, handle) })
	if c == nil || !c.has(causeDie) {
		t.Error("a defect is no typed failure")
	}
	if report(&Cause{kind: causeFail, err: NewData(appErrorTag, Rec("message", "no port", "details", None))}) != "error: no port" {
		t.Error("an AppError is reported by its message")
	}
}

// `Exit.find_defect` gives what the JS target's describeDefect gives: an
// error's message, without its name and the Flick lines it unwound.
func TestFindDefectGivesTheMessage(t *testing.T) {
	for defect, want := range map[string]string{
		"RangeError: Int.unsafe_div: division by zero\n    at App.fl:3\n": "Int.unsafe_div: division by zero",
		"Error: App:4: one item: [0,1]\n":                                 "App:4: one item: [0,1]",
		"broken":                                                          "broken",
		"Service not found: Users\n    at App.fl:9\n":                     "Service not found: Users",
	} {
		exit := &Exit{cause: &Cause{kind: causeDie, defect: defect}}
		if got := ExitFindDefect(exit).(*Data).Fields()[0]; got != want {
			t.Errorf("%q: got %q, want %q", defect, got, want)
		}
	}
}

// Combining keeps each reason once (docs/formal/runtime.md); races and a
// scope's finalizers keep repeats.
func TestCombineKeepsEachReasonOnce(t *testing.T) {
	fail := func(v Value) *Cause { return &Cause{kind: causeFail, err: v} }
	die := &Cause{kind: causeDie, defect: "boom"}
	count := func(c *Cause) int { return len(c.reasons()) }
	if n := count(Sequential(fail(1.0), fail(1.0))); n != 1 {
		t.Errorf("equal typed failures are one: %d", n)
	}
	if n := count(Parallel(Sequential(fail(1.0), fail(2.0)), fail(1.0))); n != 2 {
		t.Errorf("sequential and parallel are one combination: %d", n)
	}
	if n := count(Sequential(die, &Cause{kind: causeDie, defect: "boom"})); n != 1 {
		t.Errorf("two defects of one message are one, as two equal strings in JS: %d", n)
	}
	thrown := func() *Cause { return causeOf(Defect{Message: "boom"}) }
	if n := count(Sequential(thrown(), thrown())); n != 2 {
		t.Errorf("two thrown errors of one text are two errors: %d", n)
	}
	if n := count(concat(fail(1.0), fail(1.0))); n != 2 {
		t.Errorf("concatenation keeps repeats: %d", n)
	}
	if Sequential(nil, die) != die || Sequential(die, nil) != die {
		t.Error("the empty cause is the unit")
	}
}

// Only the first typed failure of a `Failure e` has an error of `e`.
func TestTheTypedViewOfACauseSeesItsHeadOnly(t *testing.T) {
	fb := rootFiber()
	c := Sequential(&Cause{kind: causeFail, err: "b"}, &Cause{kind: causeFail, err: "a"})
	var seen []Value
	mapped := c.mapErrors(fb, Fn(func(_ *Fiber, err Value) Value { seen = append(seen, err); return "b!" }))
	if len(seen) != 1 || seen[0] != "b" {
		t.Errorf("map sees the head only: %v", seen)
	}
	if r := mapped.reasons(); r[0].err != "b!" || r[1].err != "a" {
		t.Errorf("the later failure stays as it was: %v, %v", r[0].err, r[1].err)
	}
	reasons := CauseReasons(failureOf(c)).([]Value)
	if reasons[0].(*Cause).kind != causeFail || reasons[1].(*Cause).kind != causeDie {
		t.Error("reasons give a later typed failure as a defect")
	}
}

// A failure in a fiber asked to stop skips `try` while the fiber is
// interruptible: its typed failures go, its interruption comes (`docs/formal/runtime.md#an-interruption-that-skips-a-handler-erases-fail-effect`).
func TestAnInterruptedFiberSkipsTheHandler(t *testing.T) {
	fb := rootFiber()
	fb.state.interrupt(5)
	body := Eff(func(*Fiber) Value { return Fail(1.0) })
	ran := false
	handle := func(*Fiber, Value) Value { ran = true; return Unit }
	masked(fb, 1, func() Value {
		if _, c := catch(func() Value { return Try(fb, body, handle) }); c != nil || !ran {
			t.Errorf("a masked fiber handles its failure: %v", c)
		}
		return Unit
	})
	ran = false
	_, c := catch(func() Value { return Try(fb, body, handle) })
	if r := c.reasons(); ran || len(r) != 1 || r[0].kind != causeInterrupt || r[0].fiber != 5 {
		t.Errorf("the handler is skipped and the interruption stays: ran %v, %v", ran, r)
	}
}

// (Seq=Par): the kind of a node is never observed, so both combinations
// of any two causes have the same reasons (docs/formal/runtime.md §2.2).
func TestSequentialAndParallelAreOneCombination(t *testing.T) {
	atoms := []*Cause{
		{kind: causeFail, err: 1.0}, {kind: causeDie, defect: "d"}, {kind: causeInterrupt, fiber: 3}, nil,
	}
	for _, a := range atoms {
		for _, b := range atoms {
			for _, c := range atoms {
				left, right := concat(a, b), concat(b, c)
				seq, par := Sequential(left, right).reasons(), Parallel(left, right).reasons()
				if len(seq) != len(par) {
					t.Fatalf("%v and %v: %d and %d reasons", left, right, len(seq), len(par))
				}
				for i := range seq {
					if seq[i] != par[i] {
						t.Fatalf("%v and %v differ at %d", left, right, i)
					}
				}
			}
		}
	}
}

// The empty cause still fails (`docs/formal/runtime.md#go-an-empty-cause-is-not-a-failure`).
func TestTheEmptyCauseFails(t *testing.T) {
	if _, c := catch(func() Value { return FailureRethrow(nil, CauseEmpty) }); c == nil || len(c.reasons()) != 0 {
		t.Errorf("rethrowing the empty failure fails without reasons: %v", c)
	}
	if ExitIsFailure(ExitFailCause(CauseFromReasons([]Value{}))) != true {
		t.Error("an exit of the empty failure is a failure")
	}
	die := &Cause{kind: causeDie, defect: "d"}
	if causeOfFailure(CauseCombine(failureOf(die), CauseEmpty)) != die {
		t.Error("the empty cause is the unit of combine")
	}
	if report(emptyCause) != "InterruptError: All fibers interrupted without error" {
		t.Errorf("main failing without reasons is reported as the JS runner does: %q", report(emptyCause))
	}
}

// A typed error that became a defect is that value, as in JS: told by its
// constructor and equal to the same value (docs/formal/runtime.md#go-a-typed-error-turned-into-a-defect-loses-its-value).
func TestATypedErrorAsADefectIsItsValue(t *testing.T) {
	a := func(n float64) *Cause { return &Cause{kind: causeFail, err: NewData(Tag("App.A#1"), n)} }
	reasons := CauseReasons(failureOf(Sequential(a(1), a(2)))).([]Value)
	if got := CauseFindDefect(CauseFromReasons(reasons[1:])).(*Data).Fields()[0]; got != "A" {
		t.Errorf("the defect is told by its constructor: %v", got)
	}
	dies := Sequential(a(1), &Cause{kind: causeDie, defect: "d"}).failsToDies()
	if n := len(Sequential(dies, dieOf(a(1).err)).reasons()); n != 2 {
		t.Errorf("a defect of an equal value is the same reason: %d", n)
	}
	if n := len(Sequential(dieOf(a(1).err), dieOf(a(2).err)).reasons()); n != 2 {
		t.Errorf("defects of different values are two: %d", n)
	}
}

// Defects are elected where a failure is joined with a finalizer's, a late
// branch's or every racer's: each typed failure but a pull's end is then a
// defect; elsewhere, where causes are only combined, the kinds stay.
func TestElectDefects(t *testing.T) {
	fail := &Cause{kind: causeFail, err: "boom"}
	die := &Cause{kind: causeDie, defect: "d"}
	kinds := func(c *Cause) (out []causeKind) {
		for _, r := range c.reasons() {
			out = append(out, r.kind)
		}
		return out
	}
	if got := kinds(Sequential(fail, die).electDefects()); len(got) != 2 || got[0] != causeDie || got[1] != causeDie {
		t.Errorf("a failure beside a defect is one: %v", got)
	}
	if fail.electDefects() != fail {
		t.Error("a failure without a defect stays")
	}
	if c := Sequential(doneCause(Unit), die).electDefects(); !isDoneCause(c) {
		t.Error("a pull's end stays typed")
	}
	if !finalized(Sequential(fail, die), nil).has(causeFail) {
		t.Error("a finalizer that succeeds leaves the body's cause as it is")
	}
	if !finalized(nil, Sequential(fail, die)).has(causeFail) {
		t.Error("after a success, the finalizer's cause is not elected")
	}
	if !Sequential(fail, die).has(causeFail) {
		t.Error("Cause.combine does not elect")
	}

	fb := rootFiber()
	dies := Eff(func(*Fiber) Value { panic(Defect{"late"}) })
	fails := Eff(func(*Fiber) Value { return Fail("boom") })
	check := func(what string, c *Cause) {
		t.Helper()
		if c == nil || c.has(causeFail) || len(c.reasons()) != 2 {
			t.Errorf("%s: the failure and the defect are two defects: %v", what, kinds(c))
		}
	}
	// a Flick region joins with `Cause.combine`, as owner.ts does: no election
	if c := failure(func() {
		Scope(fb, func(fb *Fiber, _, _ Value) Value { AddFinalizer(fb, dies); return Fail("boom") })
	}); c == nil || !c.has(causeFail) || len(c.reasons()) != 2 {
		t.Errorf("scope: the failure and the defect stay as they are: %v", kinds(c))
	}
	check("owned", failure(func() {
		owned(fb, func(fb *Fiber, _ *Owner) Value { AddFinalizer(fb, dies); return Fail("boom") })
	}))
	check("ensuring", failure(func() { EffectEnsuring(fb, dies, fails) }))
	// the failure waits for the other branch to start: one interrupted before it starts never dies
	started, failed := make(chan struct{}), make(chan struct{})
	check("forEachConcurrent", failure(func() {
		ParallelPair(fb,
			Eff(func(*Fiber) Value { <-started; close(failed); return Fail("boom") }),
			Eff(func(*Fiber) Value { close(started); <-failed; panic(Defect{"late"}) }))
	}))
	check("race", failure(func() { RaceSuccess(fb, fails, dies) }))
	check("raceAll", failure(func() { RaceAll(fb, []Value{fails, dies}) }))
}
