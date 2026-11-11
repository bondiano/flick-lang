package flickrt

import (
	"math"
	"math/big"
	"time"
)

// Clock is the fiber's source of time, which its sleeps, timeouts and
// schedules wait on. It lives in the fiber's environment, so that a test
// runs on a clock of its own.
type Clock interface {
	millis() float64
	nanos() *big.Int
	monotonic() *big.Int
	// timer is a channel closed, never sent on, once `d` has passed, and
	// the function that cancels it.
	timer(d time.Duration) (<-chan struct{}, func())
}

const clockKey = "$clock"

// liveClock is the system's clock.
type liveClock struct{}

var (
	systemClock Clock = liveClock{}
	clockStart        = time.Now()
)

func (liveClock) millis() float64     { return float64(time.Now().UnixMilli()) }
func (liveClock) nanos() *big.Int     { return big.NewInt(time.Now().UnixNano()) }
func (liveClock) monotonic() *big.Int { return big.NewInt(int64(time.Since(clockStart))) }

func (liveClock) timer(d time.Duration) (<-chan struct{}, func()) {
	ch := make(chan struct{})
	t := time.AfterFunc(d, func() { close(ch) })
	return ch, func() { t.Stop() }
}

// clockOf is the fiber's clock.
func clockOf(fb *Fiber) Clock {
	if c, ok := fb.env.get(clockKey); ok {
		return c.(Clock)
	}
	return systemClock
}

func ClockWith(fb *Fiber, f Value) Value { return Run(fb, Apply(fb, f, clockOf(fb))) }

func ClockMillisOf(c Value) Value         { return c.(Clock).millis() }
func ClockNanosOf(c Value) Value          { return c.(Clock).nanos() }
func ClockMonotonicNanosOf(c Value) Value { return c.(Clock).monotonic() }

func ClockCurrentTimeMillis(fb *Fiber) Value  { return clockOf(fb).millis() }
func ClockCurrentTimeNanos(fb *Fiber) Value   { return clockOf(fb).nanos() }
func ClockMonotonicTimeNanos(fb *Fiber) Value { return clockOf(fb).monotonic() }

// millisOf is the milliseconds of a duration.
func millisOf(d *Duration) float64 {
	switch d.tag {
	case durationMillis:
		return d.millis
	case durationNanos:
		return bigToFloat(d.nanos) / 1e6
	case durationInfinite:
		return math.Inf(1)
	}
	return math.Inf(-1)
}
