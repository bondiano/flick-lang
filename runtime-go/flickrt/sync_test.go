package flickrt

import (
	"sync"
	"testing"
)

func TestRefModifyIsAtomic(t *testing.T) {
	cell := RefMake(float64(0))
	inc := Fn(func(n Value) Value { return []Value{Unit, n.(float64) + 1} })
	var wg sync.WaitGroup
	for range 1000 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			RefModify(cell, inc)
		}()
	}
	wg.Wait()
	if got := RefGet(cell); got != float64(1000) {
		t.Fatalf("got %v, want 1000", got)
	}
}

func TestDeferredWakesEveryWaiter(t *testing.T) {
	d := DeferredMake(nil)
	var wg sync.WaitGroup
	for range 10 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if got := DeferredAwait(rootFiber(), d); got != "done" {
				t.Errorf("got %v", got)
			}
		}()
	}
	if DeferredSucceed(nil, "done", d) != true || DeferredSucceed(nil, "again", d) != false {
		t.Fatal("a deferred completes once")
	}
	wg.Wait()
}
