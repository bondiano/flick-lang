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
		if got := ExitFindDefect(exit).(*Data).Fields[0]; got != want {
			t.Errorf("%q: got %q, want %q", defect, got, want)
		}
	}
}

// Combining keeps each reason once, as Effect's `Cause.combine` does
// (docs/formal/runtime.md §2.2); races and a scope's finalizers keep repeats.
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
	if n := count(Sequential(die, &Cause{kind: causeDie, defect: "boom"})); n != 2 {
		t.Errorf("two defects of one text are two errors: %d", n)
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
