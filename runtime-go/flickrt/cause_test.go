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
