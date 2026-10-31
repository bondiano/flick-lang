package flickrt

import "testing"

// A check passes, finds a failure it can replay, or runs out of values.
func TestArbitraryCheck(t *testing.T) {
	fb := rootFiber()
	ints := ArbitrarySchema(SchemaInt)
	options := func(seed Value, replay Value) Value {
		return Rec("runs", None, "size", None, "maxDiscards", Some(int32(5)), "maxShrinks", None, "seed", seed, "replay", replay)
	}
	small := Fn(func(_ *Fiber, n Value) Value { return Succeed(n.(int32) < 50) })
	passed := ArbitraryCheck(fb, options(Some(1.0), None), Fn(func(*Fiber, Value) Value { return Succeed(true) }), ints)
	if ArbitraryFormatCheckFailure(passed) != None || record(passed.(*Data).Fields[0]).Get("runs") != int32(100) {
		t.Fatalf("a true property gave %v", Format(passed))
	}
	failed := ArbitraryCheck(fb, options(Some(1.0), None), small, ints).(*Data)
	if failed.Tag != falsifiedTag {
		t.Fatalf("a false property gave %v", Format(failed))
	}
	replay := record(failed.Fields[0]).Get("replay")
	again := ArbitraryCheck(fb, options(None, Some(replay)), small, ints).(*Data)
	if again.Tag != falsifiedTag || record(again.Fields[0]).Get("shrunkInput") != record(failed.Fields[0]).Get("shrunkInput") {
		t.Fatalf("the replay gave %v", Format(again))
	}
	none := ArbitraryFilter(Fn(func(*Fiber, Value) Value { return false }), ints)
	exhausted := ArbitraryCheck(fb, options(Some(3.0), None), small, none)
	report, _ := some(ArbitraryFormatCheckFailure(exhausted))
	if report != "Property exhausted after 0 run(s) and 6 discard(s)\nSeed: \"3\"" {
		t.Fatalf("an exhausted check reported %q", report)
	}
}
