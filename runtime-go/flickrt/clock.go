package flickrt

import (
	"math/big"
	"time"
)

// Clock is Effect's: the time a fiber reads, sleeps and times out by. It
// is a service of the fiber's environment: the system's by default, a
// virtual one in a `Test.clock_test`.
type Clock interface {
	millis() float64     // since 1970-01-01 UTC
	nanos() *big.Int     // since 1970-01-01 UTC
	monotonic() *big.Int // for elapsed time; its origin means nothing
	after(d time.Duration) alarm
}

// An alarm is a timer of a clock: its channel delivers when the time is
// up; stop releases it, whether or not it went off.
type alarm struct {
	C     <-chan time.Time
	timer *time.Timer
	entry *sleeper // of a virtual clock
}

func (a alarm) stop() {
	if a.timer != nil {
		a.timer.Stop()
	} else if a.entry != nil {
		a.entry.clock.cancel(a.entry)
	}
}

const clockKey = "$clock"

// clockOf is the clock of the fiber's environment.
func clockOf(fb *Fiber) Clock {
	if c, ok := fb.env.get(clockKey); ok {
		return c.(Clock)
	}
	return systemClock{}
}

type systemClock struct{}

// started anchors the monotonic clock, as Node's hrtime has an arbitrary origin.
var started = time.Now()

func (systemClock) millis() float64     { return float64(time.Now().UnixMilli()) }
func (systemClock) nanos() *big.Int     { return big.NewInt(time.Now().UnixNano()) }
func (systemClock) monotonic() *big.Int { return big.NewInt(int64(time.Since(started))) }

func (systemClock) after(d time.Duration) alarm {
	t := time.NewTimer(d)
	return alarm{C: t.C, timer: t}
}

// elapsed is the time from a monotonic reading of the clock to now.
func elapsed(c Clock, from *big.Int) *Duration {
	return fromNanos(new(big.Int).Sub(c.monotonic(), from))
}

// toGoDuration is a finite, non-negative duration as Go's, saturated.
func toGoDuration(d *Duration) time.Duration {
	n := d.toNanos()
	if !n.IsInt64() {
		return time.Duration(1<<63 - 1)
	}
	return time.Duration(n.Int64())
}

// The `Clock` module.

func ClockWith(fb *Fiber, f Value) Value { return Run(fb, Apply(f, clockOf(fb))) }

func ClockMillisOf(_ *Fiber, c Value) Value    { return c.(Clock).millis() }
func ClockNanosOf(_ *Fiber, c Value) Value     { return c.(Clock).nanos() }
func ClockMonotonicOf(_ *Fiber, c Value) Value { return c.(Clock).monotonic() }

func ClockCurrentMillis(fb *Fiber) Value  { return clockOf(fb).millis() }
func ClockCurrentNanos(fb *Fiber) Value   { return clockOf(fb).nanos() }
func ClockMonotonicNanos(fb *Fiber) Value { return clockOf(fb).monotonic() }
