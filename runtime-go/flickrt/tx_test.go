package flickrt

import (
	"sync"
	"testing"
)

// txTask is a transaction body of Go code.
func txTask(f func(fb *Fiber) Value) Value { return Eff(f) }

func TestTxTransfersKeepTheirSumAndReadersNeverSeeHalfACommit(t *testing.T) {
	const accounts, total, workers, rounds = 6, 600, 8, 400
	refs := make([]*TxRef, accounts)
	for i := range refs {
		refs[i] = newTxRef(int32(total / accounts))
	}
	sum := func(fb *Fiber) int32 {
		s := int32(0)
		for _, r := range refs {
			s += TxRefGet(fb, r).(int32)
		}
		return s
	}
	var wg sync.WaitGroup
	for w := range workers {
		wg.Add(1)
		go func() {
			defer wg.Done()
			fb := rootFiber()
			for i := range rounds {
				from, to := refs[(w+i)%accounts], refs[(w+2*i+1)%accounts]
				TxAtomically(fb, txTask(func(fb *Fiber) Value {
					n := TxRefGet(fb, from).(int32)
					TxRefSet(fb, n-1, from)
					TxRefUpdate(fb, Fn(func(_ *Fiber, m Value) Value { return m.(int32) + 1 }), to)
					return Unit
				}))
			}
		}()
	}
	for range 2 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			fb := rootFiber()
			for range rounds {
				// checked inside the body: an attempt that would not commit sees a consistent state too
				TxAtomically(fb, txTask(func(fb *Fiber) Value {
					if s := sum(fb); s != int32(total) {
						t.Errorf("an attempt saw the sum %v", s)
					}
					return Unit
				}))
			}
		}()
	}
	wg.Wait()
	if s := TxAtomically(rootFiber(), txTask(func(fb *Fiber) Value { return sum(fb) })); s != int32(total) {
		t.Fatalf("sum %v after the transfers, want %v", s, total)
	}
}

func TestTxRetryWaitsForAWriteOfWhatItRead(t *testing.T) {
	counter, unrelated := newTxRef(int32(0)), newTxRef(int32(0))
	const target = 50
	got := make(chan Value)
	go func() {
		got <- TxAtomically(rootFiber(), txTask(func(fb *Fiber) Value {
			n := TxRefGet(fb, counter).(int32)
			if n < target {
				return TxRetry(fb)
			}
			return TxCurrent(fb)
		}))
	}()
	var wg sync.WaitGroup
	for range 5 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			fb := rootFiber()
			for range target / 5 {
				inc := Fn(func(_ *Fiber, m Value) Value { return m.(int32) + 1 })
				TxAtomically(fb, txTask(func(fb *Fiber) Value { return TxRefUpdate(fb, inc, unrelated) }))
				TxAtomically(fb, txTask(func(fb *Fiber) Value { return TxRefUpdate(fb, inc, counter) }))
			}
		}()
	}
	wg.Wait()
	meta := record(<-got)
	if meta.Get("attempt").(int32) < 2 || meta.Get("retryReason") == None {
		t.Fatalf("the retrying transaction ran once: %v", meta.Get("attempt"))
	}
}

func TestTxFailureRollsBackAndNestedTransactionsJoin(t *testing.T) {
	fb := rootFiber()
	r := newTxRef("before")
	_, cause := catch(func() Value {
		return TxAtomically(fb, txTask(func(fb *Fiber) Value {
			TxAtomically(fb, txTask(func(fb *Fiber) Value { return TxRefSet(fb, "inner", r) }))
			return Fail("stop")
		}))
	})
	if cause == nil || cause.err != "stop" {
		t.Fatalf("the failure did not reach the caller: %v", cause)
	}
	if v := TxRefGet(fb, r); v != "before" {
		t.Fatalf("a failed transaction committed %v", v)
	}
	meta := record(TxAtomically(fb, txTask(TxCurrent)))
	if meta.Get("attempt") != int32(1) || meta.Get("retryReason") != None {
		t.Fatal("a first attempt has no retry reason")
	}
}

func TestTxQueueProducersAndConsumersMeetThroughRetry(t *testing.T) {
	const producers, consumers, each = 4, 3, 60
	q := TxQueueBounded(int32(3))
	var wg sync.WaitGroup
	for p := range producers {
		wg.Add(1)
		go func() {
			defer wg.Done()
			fb := rootFiber()
			for i := range each {
				TxQueueOffer(fb, int32(p*each+i), q)
			}
		}()
	}
	sums := make(chan int, consumers)
	for range consumers {
		go func() {
			fb, sum := rootFiber(), 0
			for range producers * each / consumers {
				sum += int(TxQueueTake(fb, q).(int32))
			}
			sums <- sum
		}()
	}
	wg.Wait()
	total := 0
	for range consumers {
		total += <-sums
	}
	if n := producers * each; total != n*(n-1)/2 {
		t.Fatalf("took a sum of %v, want %v", total, n*(n-1)/2)
	}
}
