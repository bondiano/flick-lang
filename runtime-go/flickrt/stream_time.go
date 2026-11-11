package flickrt

import (
	"math"
	"math/big"
	"sync"
)

// Stream's operations in time and concurrency: concurrent maps,
// debouncing, throttling and timeouts, on the fiber's clock, waiting only
// where the test clock sees it.

// StreamDebounce emits the last element of each burst once `d` passes
// without another.
func StreamDebounce(d, s Value) Value {
	ms := millisOf(dur(d))
	return streamTransformPull(streamOf(s), func(fb *Fiber, pull Pull, scope *Owner) Pull {
		clock := clockOf(fb)
		var mu sync.Mutex // guards the state the pulling fiber shares
		var lastArr []Value
		var cause *Cause
		emitAt := math.Inf(1)
		pullLatch, emitLatch, endLatch := newLatch(), newLatch(), newLatch()
		forkIn(fb, scope, func(fb *Fiber) Value {
			_, c := catch(func() Value {
				for {
					LatchAwait(fb, pullLatch)
					arr := pull(fb).([]Value)
					mu.Lock()
					lastArr, emitAt = arr, clock.millis()+ms
					mu.Unlock()
					LatchOpen(fb, emitLatch)
				}
			})
			mu.Lock()
			cause, emitAt = c, clock.millis()
			mu.Unlock()
			LatchOpen(fb, emitLatch)
			LatchOpen(fb, endLatch)
			return Unit
		})
		// last takes the last element of the last chunk; the caller holds mu
		last := func() Value {
			out := []Value{lastArr[len(lastArr)-1]}
			lastArr = nil
			return out
		}
		sleepLoop := func(fb *Fiber) Value {
			for {
				mu.Lock()
				now, at := clock.millis(), emitAt
				mu.Unlock()
				wait := ms
				if at >= now {
					wait = math.Min(ms, at-now)
				}
				sleepOrOpen(fb, wait, endLatch)
				mu.Lock()
				switch {
				case clock.millis() < emitAt:
				case lastArr != nil:
					LatchClose(fb, emitLatch)
					LatchClose(fb, pullLatch)
					out := last()
					mu.Unlock()
					return out
				case cause != nil:
					c := cause
					mu.Unlock()
					panic(&Failure{c})
				}
				mu.Unlock()
			}
		}
		return func(fb *Fiber) Value {
			mu.Lock()
			if cause != nil {
				if lastArr != nil {
					out := last()
					mu.Unlock()
					return out
				}
				c := cause
				mu.Unlock()
				panic(&Failure{c})
			}
			mu.Unlock()
			LatchOpen(fb, pullLatch)
			LatchAwait(fb, emitLatch)
			return sleepLoop(fb)
		}
	})
}

// streamThrottle throttles by the Flick options: a token bucket of `units`
// per `duration` and `burst` more; `Enforce` (the default) drops a chunk
// that costs too much, `Shape` delays it.
func streamThrottle(self *Stream, o *Record, cost func(fb *Fiber, chunk Value) float64) *Stream {
	units := o.Get("units").(float64)
	durationMs := millisOf(dur(o.Get("duration")))
	burst := 0.0
	if b, ok := some(o.Get("burst")); ok {
		burst = b.(float64)
	}
	enforce := true
	if st, ok := some(o.Get("strategy")); ok {
		enforce = ConstructorName(st.(*Data).Tag) != "Shape"
	}
	most := units + burst
	if most < 0 {
		most = math.Inf(1)
	}
	if enforce {
		return streamTransformPull(self, func(fb *Fiber, pull Pull, _ *Owner) Pull {
			clock := clockOf(fb)
			tokens, timestamp := units, clock.millis()
			return func(fb *Fiber) Value {
				for {
					arr := pull(fb)
					weight := cost(fb, arr)
					current := clock.millis()
					sum := tokens + (current-timestamp)/durationMs*units
					available := most
					if sum >= 0 {
						available = math.Min(sum, most)
					}
					if weight <= available {
						tokens, timestamp = available-weight, current
						return arr
					}
				}
			}
		})
	}
	return streamTransformPull(self, func(fb *Fiber, pull Pull, _ *Owner) Pull {
		clock := clockOf(fb)
		tokens, timestamp := units, clock.monotonic()
		return func(fb *Fiber) Value {
			arr := pull(fb)
			weight := cost(fb, arr)
			current := clock.monotonic()
			elapsed := bigToFloat(new(big.Int).Sub(current, timestamp)) / 1e6
			remaining := math.Min(tokens+elapsed/durationMs*units, most) - weight
			if delay := -remaining / units * durationMs; remaining < 0 && delay > 0 {
				sleepFor(fb, fromMillis(delay))
			}
			tokens, timestamp = remaining, current
			return arr
		}
	})
}

func StreamThrottle(options, s Value) Value {
	o := record(options)
	return streamThrottle(streamOf(s), o, func(fb *Fiber, chunk Value) float64 {
		return Apply(fb, o.Get("cost"), chunk).(float64)
	})
}

func StreamThrottleEffect(options, s Value) Value {
	o := record(options)
	return streamThrottle(streamOf(s), o, func(fb *Fiber, chunk Value) float64 {
		return Run(fb, Apply(fb, o.Get("cost"), chunk)).(float64)
	})
}
