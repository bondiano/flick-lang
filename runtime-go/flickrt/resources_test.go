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

// failingRelease acquires `key` with a release that fails unless the key
// is "ok".
func failingRelease(_ *Fiber, key Value) Value {
	return Eff(func(fb *Fiber) Value {
		fb.owner.add(fb, func(*Fiber, *Cause) Value {
			if key == "ok" {
				return Unit
			}
			return Fail(key)
		})
		return key
	})
}

// unhandled runs `body` with a logger that notes each message logged with
// a cause, and the level it is logged at.
func unhandled(level Value, body func(fb *Fiber)) []string {
	var n notes
	l := &logger{log: func(_ *Fiber, m *logEntry) Value {
		if m.cause != nil {
			n.add(m.level + " " + m.message[0].(string))
		}
		return Unit
	}}
	fb := &Fiber{state: newState(), owner: newOwner()}
	task := Eff(func(fb *Fiber) Value { body(fb); return Unit })
	LoggerWithLoggers(fb, []Value{l}, Eff(func(fb *Fiber) Value { return ReferencesWithUnhandledLogLevel(fb, level, task) }))
	return n.get()
}

func TestAFailingFinalizerOfAPoolOrScopedCacheIsLoggedAsUnhandled(t *testing.T) {
	run := func(fb *Fiber) {
		Scope(fb, func(fb *Fiber, _, _ Value) Value {
			return PoolMake(fb, Rec("acquire", failingRelease(fb, "pool"), "size", int32(1), "concurrency", None, "targetUtilization", None))
		})
		Scope(fb, func(fb *Fiber, _, _ Value) Value {
			c := ScopedCacheMake(fb, Rec("lookup", Fn(failingRelease), "capacity", int32(1), "timeToLive", None))
			id := Fn(func(*Fiber, Value) Value { return Eff(func(*Fiber) Value { return Unit }) })
			ScopedCacheUse(fb, "cache", c, id)
			return ScopedCacheUse(fb, "ok", c, id)
		})
	}
	want := []string{"Warn Unhandled error in pool finalizer", "Warn Unhandled error in ScopedCache finalizer"}
	if got := unhandled(Some("Warn"), run); !slices.Equal(got, want) {
		t.Fatalf("logged %v, want %v", got, want)
	}
	if got := unhandled(None, run); len(got) > 0 {
		t.Fatalf("logged %v with no unhandled log level", got)
	}
}
