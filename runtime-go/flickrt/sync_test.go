package flickrt

import (
	"runtime"
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

func TestSemaphoreNeverExceedsItsPermits(t *testing.T) {
	s := SemaphoreMake(nil, int32(3))
	var mu sync.Mutex
	running, most := 0, 0
	work := Eff(func(*Fiber) Value {
		mu.Lock()
		running++
		most = max(most, running)
		mu.Unlock()
		runtime.Gosched()
		mu.Lock()
		running--
		mu.Unlock()
		return Unit
	})
	var wg sync.WaitGroup
	for range 100 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			SemaphoreWithPermit(rootFiber(), s, work)
		}()
	}
	wg.Wait()
	if most > 3 || most == 0 {
		t.Fatalf("at most %d ran at once, want 1..3", most)
	}
}

func TestSyncRefUpdateEffectSerializes(t *testing.T) {
	r := SyncRefMake(nil, float64(0))
	add := Fn(func(n Value) Value {
		return Eff(func(*Fiber) Value {
			runtime.Gosched() // another update would interleave here without the lock
			return n.(float64) + 1
		})
	})
	var wg sync.WaitGroup
	for range 200 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			SyncRefUpdateEffect(rootFiber(), add, r)
		}()
	}
	wg.Wait()
	if got := SyncRefGet(nil, r); got != float64(200) {
		t.Fatalf("got %v, want 200", got)
	}
}
