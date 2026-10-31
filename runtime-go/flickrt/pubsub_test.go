package flickrt

import (
	"slices"
	"testing"
)

func ints(xs ...int32) []Value {
	out := make([]Value, len(xs))
	for i, x := range xs {
		out[i] = x
	}
	return out
}

func TestBoundedPubSubPublisherWaitsForTheSlowestSubscriber(t *testing.T) {
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		h := newPubSub(makeAtomicBounded(2, 0), hubBackPressure)
		fast, slow := h.subscribe(fb), h.subscribe(fb)
		published := make(chan bool)
		go func() { published <- h.publishAll(rootFiber(), ints(1, 2, 3, 4)) }()
		var got []Value
		for range 4 {
			got = append(got, PubSubTake(fb, fast), PubSubTake(fb, slow))
		}
		if !slices.Equal(got, ints(1, 1, 2, 2, 3, 3, 4, 4)) || !<-published {
			t.Errorf("the subscribers took %v", got)
		}
		return Unit
	})
}

func TestSlidingPubSubReplaysWhatItSlid(t *testing.T) {
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		h := newPubSub(makeAtomicBounded(2, 3), hubSliding)
		early := h.subscribe(fb)
		h.publishAll(fb, ints(1, 2, 3, 4))
		late := h.subscribe(fb)
		if got := early.takeUpTo(fb, 9); !slices.Equal(got, ints(3, 4)) {
			t.Errorf("the early subscriber took %v", got)
		}
		if got := late.takeUpTo(fb, 9); !slices.Equal(got, ints(2, 3, 4)) {
			t.Errorf("the late subscriber replayed %v", got)
		}
		return Unit
	})
}

func TestShutdownInterruptsAWaitingTaker(t *testing.T) {
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		h := newPubSub(makeAtomicUnbounded(0), hubDropping)
		s := h.subscribe(fb)
		taken := make(chan *Cause)
		go func() {
			_, c := catch(func() Value { return PubSubTake(rootFiber(), s) })
			taken <- c
		}()
		h.shutdown(fb)
		if c := <-taken; c == nil || !c.has(causeInterrupt) {
			t.Errorf("the taker ended with %v", c)
		}
		return Unit
	})
}
