package flickrt

import (
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// gatedLazy is a cell whose initializer counts its runs, waits for `gate`
// and then gives what `end` gives of its run.
func gatedLazy(gate chan struct{}, end func(run int64) Value) (Value, *atomic.Int64) {
	runs := &atomic.Int64{}
	return LazyMake(Eff(func(fb *Fiber) Value {
		run := runs.Add(1)
		wait(fb, gate)
		return end(run)
	})), runs
}

func forced(fb *Fiber, cell Value) *Exit {
	v, c := catch(func() Value { return LazyForce(fb, cell) })
	return &Exit{v, c}
}

// forcing forces `cell` in `fb` on a goroutine of its own; the channel gives
// how it ended.
func forcing(fb *Fiber, cell Value) <-chan *Exit {
	out := make(chan *Exit, 1)
	go func() { out <- forced(fb, cell) }()
	return out
}

func openGate() chan struct{} {
	ch := make(chan struct{})
	close(ch)
	return ch
}

func untilRuns(t *testing.T, runs *atomic.Int64, n int64) {
	t.Helper()
	for deadline := time.Now().Add(5 * time.Second); runs.Load() < n; time.Sleep(time.Millisecond) {
		if time.Now().After(deadline) {
			t.Fatalf("the initializer ran %d times, want %d", runs.Load(), n)
		}
	}
}

func TestLazyForceCachesTheValue(t *testing.T) {
	cell, runs := gatedLazy(openGate(), func(int64) Value { return 42.0 })
	for range 2 {
		if e := forced(rootFiber(), cell); e.cause != nil || e.value != 42.0 {
			t.Fatalf("got %v, %v", e.value, e.cause)
		}
	}
	if runs.Load() != 1 {
		t.Fatalf("ran %d times", runs.Load())
	}
}

func TestLazyForceCachesFailuresAndDefects(t *testing.T) {
	for _, end := range []func(int64) Value{
		func(int64) Value { return Fail("failed") },
		func(int64) Value { return Die("boom") },
	} {
		cell, runs := gatedLazy(openGate(), end)
		first := forced(rootFiber(), cell)
		second := forced(rootFiber(), cell)
		if first.cause == nil || second.cause != first.cause {
			t.Fatalf("got %v then %v, want the same failure", first.cause, second.cause)
		}
		if first.cause.has(causeInterrupt) || runs.Load() != 1 {
			t.Fatalf("cause %v, ran %d times", first.cause, runs.Load())
		}
	}
}

func TestLazyConcurrentForcesRunOnce(t *testing.T) {
	gate := make(chan struct{})
	cell, runs := gatedLazy(gate, func(int64) Value { return 42.0 })
	var wg sync.WaitGroup
	for range 3 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if e := forced(rootFiber(), cell); e.cause != nil || e.value != 42.0 {
				t.Errorf("got %v, %v", e.value, e.cause)
			}
		}()
	}
	untilRuns(t, runs, 1)
	time.Sleep(10 * time.Millisecond)
	close(gate)
	wg.Wait()
	if runs.Load() != 1 {
		t.Fatalf("ran %d times", runs.Load())
	}
}

func TestLazyInterruptedOwnerResetsTheCell(t *testing.T) {
	gate := make(chan struct{})
	cell, runs := gatedLazy(gate, func(run int64) Value { return float64(run) })
	owner := rootFiber()
	ownerExit := forcing(owner, cell)
	untilRuns(t, runs, 1)
	waiterExit := forcing(rootFiber(), cell)
	time.Sleep(10 * time.Millisecond)
	owner.state.interrupt(99)
	if e := <-ownerExit; e.cause == nil || !e.cause.has(causeInterrupt) {
		t.Fatalf("the owner ended with %v, %v", e.value, e.cause)
	}
	untilRuns(t, runs, 2)
	close(gate)
	if e := <-waiterExit; e.cause != nil || e.value != 2.0 {
		t.Fatalf("the waiter got %v, %v", e.value, e.cause)
	}
	if e := forced(rootFiber(), cell); e.cause != nil || e.value != 2.0 || runs.Load() != 2 {
		t.Fatalf("got %v, %v after %d runs", e.value, e.cause, runs.Load())
	}
}

func TestLazyInterruptedWaiterLeavesTheAttempt(t *testing.T) {
	gate := make(chan struct{})
	cell, runs := gatedLazy(gate, func(int64) Value { return 42.0 })
	ownerExit := forcing(rootFiber(), cell)
	untilRuns(t, runs, 1)
	waiter := rootFiber()
	waiterExit := forcing(waiter, cell)
	time.Sleep(10 * time.Millisecond)
	waiter.state.interrupt(99)
	if e := <-waiterExit; e.cause == nil || !e.cause.has(causeInterrupt) {
		t.Fatalf("the waiter ended with %v, %v", e.value, e.cause)
	}
	close(gate)
	if e := <-ownerExit; e.cause != nil || e.value != 42.0 {
		t.Fatalf("the owner got %v, %v", e.value, e.cause)
	}
	if e := forced(rootFiber(), cell); e.cause != nil || e.value != 42.0 || runs.Load() != 1 {
		t.Fatalf("got %v, %v after %d runs", e.value, e.cause, runs.Load())
	}
}

func TestLazySelfForceIsACachedDefect(t *testing.T) {
	var runs int
	var cell Value
	cell = LazyMake(Eff(func(fb *Fiber) Value {
		runs++
		return LazyForce(fb, cell)
	}))
	for range 2 {
		e := forced(rootFiber(), cell)
		if e.cause == nil || !strings.HasPrefix(e.cause.firstOf(causeDie).defect, lazySelfForce) {
			t.Fatalf("got %v, %v", e.value, e.cause)
		}
	}
	if runs != 1 {
		t.Fatalf("ran %d times", runs)
	}
}

func TestLazyRunsWithTheServicesOfTheForcingFiber(t *testing.T) {
	cell := LazyMake(Eff(func(fb *Fiber) Value {
		v, _ := fb.env.get("Test.Name")
		return v
	}))
	fb := rootFiber()
	fb.env = fb.env.with("Test.Name", "forcer")
	if e := forced(fb, cell); e.cause != nil || e.value != "forcer" {
		t.Fatalf("got %v, %v", e.value, e.cause)
	}
}
