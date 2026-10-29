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
