package flickrt

import (
	"sync"
	"testing"
	"time"
)

// A turn waits for the fibers forked before it to suspend, and turns run
// in the order they were queued.
func TestTurnsComeAfterTheFibersForkedBefore(t *testing.T) {
	fb := rootFiber()
	fb.owner = newOwner()
	var mu sync.Mutex
	var log []string
	add := func(s string) {
		mu.Lock()
		log = append(log, s)
		mu.Unlock()
	}
	release := make(chan struct{})
	// runs without suspending until `release`, which is no fiber's wait
	busy := fork(fb, fb.owner, Eff(func(fb *Fiber) Value {
		<-release
		add("busy")
		return Unit
	}), nil)
	a := forkInTurn(fb, fb.owner, func(*Fiber) { add("first") })
	b := forkInTurn(fb, fb.owner, func(*Fiber) { add("second") })
	time.Sleep(20 * time.Millisecond)
	add("released")
	close(release)
	for _, c := range []*child{busy, a, b} {
		<-c.done
	}
	want := []string{"released", "busy", "first", "second"}
	if len(log) != len(want) {
		t.Fatalf("log %v, want %v", log, want)
	}
	for i := range want {
		if log[i] != want[i] {
			t.Fatalf("log %v, want %v", log, want)
		}
	}
}

// A fiber interrupted before it took its turn gives it up to the next one.
func TestAnInterruptedTurnLetsTheNextRun(t *testing.T) {
	fb := rootFiber()
	fb.owner = newOwner()
	release := make(chan struct{})
	busy := fork(fb, fb.owner, Eff(func(fb *Fiber) Value { <-release; return Unit }), nil)
	a := forkInTurn(fb, fb.owner, func(*Fiber) {})
	ran := make(chan struct{})
	b := forkInTurn(fb, fb.owner, func(*Fiber) { close(ran) })
	stop(fb, a)
	close(release)
	<-busy.done
	<-b.done
	if !isClosed(ran) {
		t.Fatal("the second turn never ran")
	}
}

// A turn the first task of `all` queues comes after the tasks forked after
// it, however soon that first one runs.
func TestATurnOfATaskOfAllComesAfterItsSiblings(t *testing.T) {
	for range 200 {
		fb := rootFiber()
		fb.owner = newOwner()
		var mu sync.Mutex
		var log []string
		add := func(s string) {
			mu.Lock()
			log = append(log, s)
			mu.Unlock()
		}
		tasks := []Value{Eff(func(fb *Fiber) Value {
			yieldTurn(fb)
			add("turn")
			return Unit
		})}
		for range 8 {
			tasks = append(tasks, Eff(func(*Fiber) Value { add("sibling"); return Unit }))
		}
		all(fb, tasks, 0)
		if log[len(log)-1] != "turn" {
			t.Fatalf("log %v: the turn ran before a sibling", log)
		}
	}
}

// `all` of no tasks forks nothing and never waits, so it holds nothing: a
// later `all` must not lose a hold on its forks that nobody would release.
func TestAnAllOfNoTasksLeavesNoHold(t *testing.T) {
	fb := rootFiber()
	fb.owner = newOwner()
	all(fb, nil, 0)
	if fb.state.forks.Load() != nil {
		t.Fatal("an empty all holds a cohort it will never suspend out of")
	}
	all(fb, nil, 0)
	all(fb, []Value{Eff(func(*Fiber) Value { return Unit })}, 0)
	ran := make(chan struct{})
	turn := forkInTurn(fb, fb.owner, func(*Fiber) { close(ran) })
	select {
	case <-ran:
	case <-time.After(2 * time.Second):
		t.Fatal("a turn queued after an empty all never ran")
	}
	<-turn.done
}

// Likewise `raceAll` of no tasks, which fails at once.
func TestARaceOfNoTasksLeavesNoHold(t *testing.T) {
	fb := rootFiber()
	fb.owner = newOwner()
	catch(func() Value { return raceAll(fb, nil, false) })
	if fb.state.forks.Load() != nil {
		t.Fatal("an empty race holds a cohort it will never suspend out of")
	}
}
