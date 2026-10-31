package flickrt

import (
	"math"
	"math/big"
	"runtime"
	"slices"
	"sort"
	"sync"
	"sync/atomic"
	"time"
)

// testClocks counts the test clocks in use: with none, a blocking wait only
// flushes stdout.
var testClocks atomic.Int64

// A testClock is Effect's TestClock: virtual time from 0 that moves only by
// `adjust` and `setTime`, which wake the sleepers due on the way in order.
// Where Effect tracks the tasks a wake schedules, this counts the fibers
// under the clock that are not parked: a wake is over once every one but
// the caller is parked on something that has not come.
type testClock struct {
	mu      sync.Mutex
	now     float64  // milliseconds
	wall    *big.Int // nanoseconds, replaced, never changed in place
	mono    *big.Int
	sleeps  []*sleeper // by descending time, so that the next to wake is last
	probes  map[*probe]struct{}
	running atomic.Int64
	runs    sync.Mutex // one adjustment at a time
}

type sleeper struct {
	at   float64
	done chan struct{}
}

// A probe tells, without consuming anything, whether what a parked fiber
// waits for has come.
type probe struct{ ready func() bool }

var closedChan = func() chan struct{} {
	ch := make(chan struct{})
	close(ch)
	return ch
}()

func newTestClock() *testClock {
	return &testClock{wall: new(big.Int), mono: new(big.Int), probes: map[*probe]struct{}{}}
}

func (c *testClock) millis() float64 {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.now
}

func (c *testClock) nanos() *big.Int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.wall
}

func (c *testClock) monotonic() *big.Int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.mono
}

// timer registers a sleeper; one due now or before wakes at once.
//
// shortcut: no warning is logged when a test sleeps without moving the clock, as Effect's warningStart does; log it from the sleeping fiber if a test needs it
func (c *testClock) timer(d time.Duration) (<-chan struct{}, func()) {
	c.mu.Lock()
	defer c.mu.Unlock()
	end := c.now + float64(d)/1e6
	if end <= c.now {
		return closedChan, func() {}
	}
	s := &sleeper{end, make(chan struct{})}
	// before the sleepers of the same time, so that those wake in the order they came
	at := sort.Search(len(c.sleeps), func(i int) bool { return c.sleeps[i].at <= end })
	c.sleeps = slices.Insert(c.sleeps, at, s)
	return s.done, func() {
		c.mu.Lock()
		if i := slices.Index(c.sleeps, s); i >= 0 {
			c.sleeps = slices.Delete(c.sleeps, i, i+1)
		}
		c.mu.Unlock()
	}
}

// park counts the calling fiber as parked until the function it gives
// runs; `ready` is nil for a wait on I/O.
func (c *testClock) park(ready func() bool) func() {
	var p *probe
	if ready != nil {
		p = &probe{ready}
		c.mu.Lock()
		c.probes[p] = struct{}{}
		c.mu.Unlock()
	}
	c.running.Add(-1)
	return func() {
		c.running.Add(1)
		if p != nil {
			c.mu.Lock()
			delete(c.probes, p)
			c.mu.Unlock()
		}
	}
}

func (c *testClock) anyReady() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	for p := range c.probes {
		if p.ready() {
			return true
		}
	}
	return false
}

const busyWarning = "A fiber woken by the test clock is still running " +
	"without suspending, so the test clock stopped waiting for it. Sleeps it " +
	"starts later may be scheduled relative to a later time."

// settle lets the fibers under the clock run until all but the caller are
// parked on what has not come, on two passes in a row; after a second of
// real time it stops waiting, warning when a wake left a fiber running.
func (c *testClock) settle(fb *Fiber, woken bool) {
	self := int64(0)
	if fb.state.clock == c {
		self = 1
	}
	deadline := time.Now().Add(time.Second)
	for quiet := 0; quiet < 2; {
		runtime.Gosched()
		if c.running.Load() <= self && !c.anyReady() {
			quiet++
			continue
		}
		quiet = 0
		if time.Now().After(deadline) {
			if woken {
				// logged on the live clock, as Effect's `withLive` does
				real := *fb
				real.env = fb.env.with(clockKey, systemClock)
				EffectLogWarning(&real, busyWarning)
			}
			return
		}
	}
}

// run moves the clock to `step` of its time, waking each sleeper due by
// then at its time once the fibers before it have settled; `nanos` is an
// adjustment's exact length.
func (c *testClock) run(fb *Fiber, step func(float64) float64, nanos *big.Int) {
	c.runs.Lock()
	defer c.runs.Unlock()
	c.settle(fb, false)
	c.mu.Lock()
	wall, mono := c.wall, c.mono
	end := step(c.now)
	for len(c.sleeps) > 0 {
		s := c.sleeps[len(c.sleeps)-1]
		if s.at > end {
			break
		}
		c.sleeps = c.sleeps[:len(c.sleeps)-1]
		c.advanceTo(s.at)
		c.mu.Unlock()
		close(s.done)
		c.settle(fb, true)
		c.mu.Lock()
	}
	c.advanceTo(end)
	if nanos != nil && finiteMillis(end) {
		c.wall = new(big.Int).Add(wall, nanos)
		if nanos.Sign() > 0 {
			c.mono = new(big.Int).Add(mono, nanos)
		}
	}
	c.mu.Unlock()
}

func finiteMillis(x float64) bool { return !math.IsInf(x, 0) && !math.IsNaN(x) }

func (c *testClock) advanceTo(at float64) {
	if delta := at - c.now; delta > 0 && finiteMillis(delta) {
		n, _ := new(big.Float).SetFloat64(math.Round(delta * 1e6)).Int(nil)
		c.mono = new(big.Int).Add(c.mono, n)
	}
	if finiteMillis(at) {
		whole := math.Floor(at)
		n, _ := new(big.Float).SetFloat64(whole).Int(nil)
		n.Mul(n, bigMillion)
		c.wall = n.Add(n, big.NewInt(int64(math.Floor((at-whole)*1e6))))
	}
	c.now = at
}

// TestClock is `Test.clock_test`: an effect test on a test clock of its
// own, which counts the test's fiber as running.
func TestClock(name, task Value) Value {
	return TestEffect(name, Eff(func(fb *Fiber) Value {
		c := newTestClock()
		c.running.Store(1)
		fb.state.clock = c
		testClocks.Add(1)
		defer testClocks.Add(-1)
		return withEnv(fb, clockKey, c, task)
	}))
}

func TestAdjust(fb *Fiber, d Value) Value {
	x := dur(d)
	millis := millisOf(x)
	var nanos *big.Int
	if finiteMillis(millis) {
		nanos = x.toNanos()
	}
	clockOf(fb).(*testClock).run(fb, func(now float64) float64 { return now + millis }, nanos)
	return Unit
}

func TestSetTime(fb *Fiber, millis Value) Value {
	clockOf(fb).(*testClock).run(fb, func(float64) float64 { return millis.(float64) }, nil)
	return Unit
}

// counted is the test clock a fiber forked by `fb` runs under, which counts
// it as running until it ends.
func counted(fb *Fiber) *testClock {
	if testClocks.Load() == 0 {
		return nil
	}
	c, _ := clockOf(fb).(*testClock)
	return c
}
