package flickrt

import (
	"slices"
	"sync"
	"testing"
	"time"
)

// Moving a test clock wakes the sleepers due in order and waits until what
// they do settles; a sleeper not yet due stays asleep.
func TestTestClockWakesSleepersInOrder(t *testing.T) {
	tc := newTestClock()
	fb := rootFiber()
	fb.owner = newOwner()
	fb.env = fb.env.with(clockKey, tc)
	fb.state.clock = tc
	var mu sync.Mutex
	var seen []string
	for _, s := range []struct {
		ms   int
		name string
	}{{30, "c"}, {10, "a"}, {20, "b"}} {
		fork(fb, fb.owner, Eff(func(fb *Fiber) Value {
			sleep(fb, time.Duration(s.ms)*time.Millisecond, false)
			// work after the wake, which adjust must wait for
			for range 1000 {
				EffectYieldNow(fb)
			}
			mu.Lock()
			seen = append(seen, s.name)
			mu.Unlock()
			return Unit
		}), nil)
	}
	check := func(want ...string) {
		t.Helper()
		mu.Lock()
		defer mu.Unlock()
		if !slices.Equal(seen, want) {
			t.Fatalf("woke %v, want %v", seen, want)
		}
	}
	TestAdjust(fb, fromMillis(25))
	check("a", "b")
	if now := tc.millis(); now != 25 {
		t.Fatalf("now %v", now)
	}
	TestSetTime(fb, 40.0)
	check("a", "b", "c")
	if n := tc.monotonic().Int64(); n != 40e6 {
		t.Fatalf("monotonic %v", n)
	}
}

// Seeded draws are Effect's ISAAC: the same numbers as Random.withSeed on
// Node for an integer, a fraction, a negative and a large seed.
func TestIsaacMatchesEffect(t *testing.T) {
	for _, c := range []struct {
		seed float64
		want []float64
	}{
		{42, []float64{0.2272248550901319, 0.8207217622702011, 0.16356462006064776, 0.30035612566630077}},
		{1.5, []float64{0.3359475394676412, 0.9818145376269317, 0.6165858143387948, 0.7939063381687923}},
		{-3, []float64{0.8904632322045111, 0.4393073422987235, 0.63885311862174, 0.12032002202612435}},
		{1e20, []float64{0.4703651328121178, 0.4431257598514613, 0.19487403354193167, 0.6156539065620302}},
	} {
		g := newIsaac(c.seed)
		for i, want := range c.want {
			if got := g.next(); got != want {
				t.Fatalf("seed %v draw %d: %v, want %v", c.seed, i, got, want)
			}
		}
	}
}
