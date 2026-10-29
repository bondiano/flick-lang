package flickrt

import (
	"sync/atomic"
	"testing"
	"time"
)

func TestOwnerWaitsForAnInterruptedForeignCall(t *testing.T) {
	var finished atomic.Bool
	var releasedAfter bool
	fb := rootFiber()
	owned(fb, func(fb *Fiber, o *Owner) Value {
		o.add(fb, func(*Fiber, *Cause) Value {
			releasedAfter = finished.Load()
			return Unit
		})
		go func() {
			time.Sleep(5 * time.Millisecond)
			fb.state.interrupt(0)
		}()
		_, cause := catch(func() Value {
			return ForeignAsync(fb, func(ctx Context) Value {
				<-ctx.Done()
				time.Sleep(20 * time.Millisecond) // a call slow to notice the cancellation
				finished.Store(true)
				return Unit
			})
		})
		if cause == nil || !cause.has(causeInterrupt) {
			t.Errorf("the call ends with the interruption, not %v", cause)
		}
		return Unit
	})
	if !releasedAfter {
		t.Fatal("the owner released its resources before the foreign call ended")
	}
}
