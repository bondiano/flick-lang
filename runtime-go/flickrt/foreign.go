package flickrt

import "context"

// Context is what a Go function behind a `foreign async` binding takes
// first: it is cancelled when the calling fiber is interrupted.
type Context = context.Context

// ForeignAsync runs the Go side of a `foreign async` binding in a goroutine
// of its own. An interruption cancels its context and fails at once; the
// call may still be running, so the innermost owner waits for it to end
// before releasing resources, as the JS runtime waits for a promise.
func ForeignAsync(fb *Fiber, call func(Context) Value) Value {
	ctx, cancel := context.WithCancel(context.Background())
	type result struct {
		value Value
		cause *Cause
	}
	ended := make(chan result, 1)
	done := make(chan struct{})
	go func() {
		defer close(done)
		v, c := catch(func() Value { return call(ctx) })
		ended <- result{v, c}
	}()
	signal := signalOf(fb)
	if c := parking(fb); c != nil {
		defer c.park(func() bool { return len(ended) > 0 || isClosed(signal) })()
	}
	select {
	case r := <-ended:
		cancel()
		if r.cause != nil {
			panic(&Failure{r.cause})
		}
		return r.value
	case <-signal:
		cancel()
		fb.owner.track(done)
		panic(fb.state.stopped())
	}
}
