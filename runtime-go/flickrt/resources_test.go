package flickrt

import (
	"slices"
	"testing"
)

// counting is a lookup of a key that notes each call.
func counting(n *notes) Value {
	return Fn(func(_ *Fiber, key Value) Value {
		return Eff(func(*Fiber) Value {
			n.add("lookup " + key.(string))
			return key
		})
	})
}

func TestCacheEntriesExpireOnTheFibersClock(t *testing.T) {
	onTestClock(func(fb *Fiber) {
		var n notes
		c := CacheMake(fb, Rec("lookup", counting(&n), "capacity", int32(10), "timeToLive", Some(fromMillis(1000))))
		CacheGet(fb, "a", c)
		TestAdjust(fb, fromMillis(999))
		CacheGet(fb, "a", c)
		TestAdjust(fb, fromMillis(1))
		if CacheHas(fb, "a", c).(bool) {
			t.Fatal("an entry outlived its time to live")
		}
		CacheGet(fb, "a", c)
		if got := n.get(); !slices.Equal(got, []string{"lookup a", "lookup a"}) {
			t.Fatalf("lookups %v", got)
		}
	})
}

// releasing acquires `key` with a release noted when its owner closes.
func releasing(n *notes) Value {
	return Fn(func(_ *Fiber, key Value) Value {
		return Eff(func(fb *Fiber) Value {
			fb.owner.add(fb, func(*Fiber, *Cause) Value { n.add("release " + key.(string)); return Unit })
			return key
		})
	})
}

func TestRcMapReleasesAKeyAfterItsIdleTime(t *testing.T) {
	onTestClock(func(fb *Fiber) {
		var n notes
		idle := Fn(func(*Fiber, Value) Value { return fromMillis(100) })
		m := RcMapMake(fb, Rec("lookup", releasing(&n), "idleTimeToLive", Some(idle), "capacity", None))
		Scope(fb, func(fb *Fiber, _, _ Value) Value { return RcMapGet(fb, "a", m) })
		TestAdjust(fb, fromMillis(60))
		RcMapTouch(fb, "a", m)
		TestAdjust(fb, fromMillis(60))
		if !RcMapHas(fb, "a", m).(bool) || len(n.get()) > 0 {
			t.Fatalf("released before its idle time: %v", n.get())
		}
		TestAdjust(fb, fromMillis(40))
		if RcMapHas(fb, "a", m).(bool) || !slices.Equal(n.get(), []string{"release a"}) {
			t.Fatalf("kept after its idle time: %v", n.get())
		}
	})
}

func TestRcRefReleasesAfterItsIdleTimeUnlessHeldAgain(t *testing.T) {
	onTestClock(func(fb *Fiber) {
		var n notes
		acquire := Apply(fb, releasing(&n), "shared")
		r := RcRefMake(fb, Rec("acquire", acquire, "idleTimeToLive", Some(fromMillis(100))))
		hold := func() { Scope(fb, func(fb *Fiber, _, _ Value) Value { return RcRefGet(fb, r) }) }
		hold()
		TestAdjust(fb, fromMillis(50))
		hold()
		TestAdjust(fb, fromMillis(99))
		if len(n.get()) > 0 {
			t.Fatalf("released while its idle time ran again: %v", n.get())
		}
		TestAdjust(fb, fromMillis(1))
		if !slices.Equal(n.get(), []string{"release shared"}) {
			t.Fatalf("kept after its idle time: %v", n.get())
		}
	})
}
