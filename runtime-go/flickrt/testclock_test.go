package flickrt

import (
	"slices"
	"sync"
	"testing"
)

// onTestClock runs `body` as the task of a clock test, in a fiber of its own.
func onTestClock(body func(fb *Fiber)) {
	fb := &Fiber{state: newState(), owner: newOwner()}
	Run(fb, TestClock("t", Eff(func(fb *Fiber) Value {
		body(fb)
		return Unit
	})).(*test).task)
}

type notes struct {
	mu   sync.Mutex
	seen []string
}

func (n *notes) add(s string) {
	n.mu.Lock()
	n.seen = append(n.seen, s)
	n.mu.Unlock()
}

func (n *notes) get() []string {
	n.mu.Lock()
	defer n.mu.Unlock()
	return slices.Clone(n.seen)
}

func TestTestClockWakesSleepersInOrderAndSettles(t *testing.T) {
	onTestClock(func(fb *Fiber) {
		var n notes
		sleeper := func(ms float64, name string, again float64) *child {
			c := fork(fb, fb.owner, Eff(func(fb *Fiber) Value {
				sleepFor(fb, fromMillis(ms))
				n.add(name)
				if again > 0 {
					sleepFor(fb, fromMillis(again))
					n.add(name + " again")
				}
				return Unit
			}), nil)
			// moving by nothing lets the fiber register its sleep first
			TestAdjust(fb, DurationZero)
			return c
		}
		a, b, c := sleeper(10, "a", 5), sleeper(20, "b", 0), sleeper(10, "c", 1)
		TestAdjust(fb, fromMillis(12))
		if got := n.get(); !slices.Equal(got, []string{"a", "c", "c again"}) {
			t.Fatalf("after 12ms: %v", got)
		}
		if now := ClockCurrentTimeMillis(fb); now != 12.0 {
			t.Fatalf("now %v", now)
		}
		TestAdjust(fb, fromMillis(10))
		if got := n.get(); !slices.Equal(got, []string{"a", "c", "c again", "a again", "b"}) {
			t.Fatalf("after 22ms: %v", got)
		}
		for _, f := range []*child{a, b, c} {
			FiberJoin(fb, f)
		}
		if nanos := ClockCurrentTimeNanos(fb).(interface{ Int64() int64 }).Int64(); nanos != 22_000_000 {
			t.Fatalf("nanos %d", nanos)
		}
		TestSetTime(fb, 5.0)
		if now := ClockCurrentTimeMillis(fb); now != 5.0 {
			t.Fatalf("set back: %v", now)
		}
	})
}

func TestTestClockDrivesTimeoutsAndSchedules(t *testing.T) {
	onTestClock(func(fb *Fiber) {
		var n notes
		timed := fork(fb, fb.owner, Eff(func(fb *Fiber) Value {
			return TimeoutOption(fb, 50.0, Eff(func(fb *Fiber) Value {
				sleepFor(fb, fromMillis(100))
				return "late"
			}))
		}), nil)
		calls := 0
		retried := fork(fb, fb.owner, Eff(func(fb *Fiber) Value {
			return EffectRetry(fb, ScheduleSpaced(fromMillis(10)), Eff(func(fb *Fiber) Value {
				calls++
				n.add("call")
				if calls < 3 {
					return Fail("again")
				}
				return "done"
			}))
		}), nil)
		TestAdjust(fb, DurationZero)
		if got := n.get(); len(got) != 1 {
			t.Fatalf("before any time passed: %v", got)
		}
		TestAdjust(fb, fromMillis(10))
		if got := n.get(); len(got) != 2 {
			t.Fatalf("after 10ms: %v", got)
		}
		TestAdjust(fb, fromMillis(40))
		if v := FiberJoin(fb, retried); v != "done" {
			t.Fatalf("retried: %v", v)
		}
		if v := FiberJoin(fb, timed); v != None {
			t.Fatalf("timed out: %v", v)
		}
	})
}
