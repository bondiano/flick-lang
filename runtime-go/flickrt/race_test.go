package flickrt

import (
	"slices"
	"sync"
	"testing"
	"time"
)

// A race starts its branches in order, each once the one before blocks, as
// Effect runs a branch at once until it suspends: a loser listed before an
// immediate winner has started, so it is interrupted and its finalizer
// runs; a branch listed after a winner never starts.
func TestRaceStartsBranchesInTurn(t *testing.T) {
	fb := rootFiber()
	fb.owner = newOwner()
	var mu sync.Mutex
	var log []string
	note := func(s string) Value {
		mu.Lock()
		log = append(log, s)
		mu.Unlock()
		return Unit
	}
	loser := Eff(func(fb *Fiber) Value {
		return EffectEnsuring(fb, Eff(func(*Fiber) Value { return note("loser ended") }), Eff(func(fb *Fiber) Value {
			sleep(fb, time.Minute, false)
			return "slow"
		}))
	})
	for range 200 {
		log = nil
		if got := RaceFirst(fb, loser, Succeed("fast")); got != "fast" {
			t.Fatalf("winner %v", got)
		}
		if !slices.Equal(log, []string{"loser ended"}) {
			t.Fatalf("log %v", log)
		}
	}
	later := Eff(func(*Fiber) Value { return note("later started") })
	log = nil
	RaceFirst(fb, Succeed("first"), later)
	RaceSuccess(fb, Succeed("first"), later)
	if len(log) != 0 {
		t.Fatalf("a branch after the winner started: %v", log)
	}
}
