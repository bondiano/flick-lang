package flickrt

import (
	"container/heap"
	"math"
	"math/big"
	"runtime"
	"sync"
	"time"
)

// TestClock is Effect's TestClock: virtual time that starts at 0 and moves
// only by `adjust` and `set_time`, which wake the sleepers due by then in
// order. After waking each one, and before moving at all, it waits until
// every fiber on the clock is blocked or has ended, as Effect waits for the
// work a wake schedules: it counts the fibers that run.
type TestClock struct {
	mu      sync.Mutex
	run     sync.Mutex // one move at a time
	now     float64    // millis
	wall    *big.Int
	mono    *big.Int
	sleeps  sleepers
	seq     int64
	busy    int           // fibers on the clock not blocked
	resumes int64         // each time one resumed, to see a wake that raced a settle
	idle    chan struct{} // signalled when busy drops to 0
}

// A sleeper is a fiber waiting for a time of a virtual clock.
type sleeper struct {
	clock *TestClock
	at    float64
	seq   int64 // registration order breaks ties
	index int   // in the heap, -1 once out
	ch    chan time.Time
	fired bool
}

type sleepers []*sleeper

func (h sleepers) Len() int { return len(h) }
func (h sleepers) Less(i, j int) bool {
	return h[i].at < h[j].at || h[i].at == h[j].at && h[i].seq < h[j].seq
}
func (h sleepers) Swap(i, j int) {
	h[i], h[j] = h[j], h[i]
	h[i].index, h[j].index = i, j
}
func (h *sleepers) Push(x any) {
	s := x.(*sleeper)
	s.index = len(*h)
	*h = append(*h, s)
}
func (h *sleepers) Pop() any {
	old := *h
	s := old[len(old)-1]
	*h, s.index = old[:len(old)-1], -1
	return s
}

func newTestClock() *TestClock {
	return &TestClock{wall: new(big.Int), mono: new(big.Int), busy: 1, idle: make(chan struct{}, 1)}
}

func (tc *TestClock) millis() float64 {
	tc.mu.Lock()
	defer tc.mu.Unlock()
	return tc.now
}

func (tc *TestClock) nanos() *big.Int {
	tc.mu.Lock()
	defer tc.mu.Unlock()
	return new(big.Int).Set(tc.wall)
}

func (tc *TestClock) monotonic() *big.Int {
	tc.mu.Lock()
	defer tc.mu.Unlock()
	return new(big.Int).Set(tc.mono)
}

// fired is the channel of an alarm already due.
var fired = func() chan time.Time {
	ch := make(chan time.Time)
	close(ch)
	return ch
}()

func (tc *TestClock) after(d time.Duration) alarm {
	tc.mu.Lock()
	defer tc.mu.Unlock()
	end := tc.now + float64(d)/1e6
	if end <= tc.now {
		return alarm{C: fired}
	}
	tc.seq++
	s := &sleeper{clock: tc, at: end, seq: tc.seq, ch: make(chan time.Time)}
	heap.Push(&tc.sleeps, s)
	return alarm{C: s.ch, entry: s}
}

// cancel takes a sleeper off the clock; one that went off was counted as
// running when it was woken, and its fiber counted itself again since.
func (tc *TestClock) cancel(s *sleeper) {
	tc.mu.Lock()
	defer tc.mu.Unlock()
	if s.fired {
		tc.add(-1)
	} else if s.index >= 0 {
		heap.Remove(&tc.sleeps, s.index)
	}
}

// add counts fibers that start or resume running (n > 0) or block or end.
func (tc *TestClock) add(n int) {
	tc.busy += n
	if n > 0 {
		tc.resumes++
	}
	if tc.busy == 0 {
		select {
		case tc.idle <- struct{}{}:
		default:
		}
	}
}

func (tc *TestClock) count(n int) {
	tc.mu.Lock()
	tc.add(n)
	tc.mu.Unlock()
}

// settle waits, holding tc.mu, until no fiber on the clock runs. A fiber a
// closed channel woke may not have counted itself yet, so quiet must last
// over a few rounds of the scheduler.
//
// shortcut: a fiber busy for over a second without blocking stops the wait without Effect's warning log; log it if tests need to see why
func (tc *TestClock) settle() {
	limit := time.Now().Add(time.Second)
	for {
		for tc.busy > 0 {
			tc.mu.Unlock()
			left := time.Until(limit)
			if left <= 0 {
				tc.mu.Lock()
				return
			}
			t := time.NewTimer(left)
			select {
			case <-tc.idle:
			case <-t.C:
			}
			t.Stop()
			tc.mu.Lock()
		}
		seen := tc.resumes
		tc.mu.Unlock()
		for range 8 {
			runtime.Gosched()
		}
		tc.mu.Lock()
		if tc.busy == 0 && tc.resumes == seen {
			return
		}
	}
}

// move moves the clock to `step` of its time, waking the sleepers due by
// then, earliest first; with `adjust` nanoseconds, as Effect's adjust,
// the wall and monotonic clocks end that far from where they began.
func (tc *TestClock) move(step func(float64) float64, adjust *big.Int) {
	tc.run.Lock()
	defer tc.run.Unlock()
	tc.mu.Lock()
	defer tc.mu.Unlock()
	tc.add(-1) // the mover blocks while time moves
	tc.settle()
	wall, mono := new(big.Int).Set(tc.wall), new(big.Int).Set(tc.mono)
	end := step(tc.now)
	for tc.sleeps.Len() > 0 && tc.sleeps[0].at <= end {
		s := heap.Pop(&tc.sleeps).(*sleeper)
		tc.advance(s.at)
		s.fired = true
		tc.add(1) // its fiber, until it counts itself
		close(s.ch)
		tc.settle()
	}
	tc.advance(end)
	if adjust != nil && !math.IsInf(end, 0) && end == end {
		tc.wall.Add(wall, adjust)
		if adjust.Sign() > 0 {
			tc.mono.Add(mono, adjust)
		}
	}
	tc.add(1)
}

func (tc *TestClock) advance(at float64) {
	if delta := at - tc.now; delta > 0 && !math.IsInf(delta, 0) {
		tc.mono.Add(tc.mono, big.NewInt(int64(math.Round(delta*1e6))))
	}
	if !math.IsInf(at, 0) && at == at {
		whole := math.Floor(at)
		n := new(big.Int).Mul(exactInt(whole), bigMillion)
		tc.wall = n.Add(n, big.NewInt(int64(math.Floor((at-whole)*1e6))))
	}
	tc.now = at
}

// park and unpark tell the fiber's virtual clock, if any, that the fiber
// blocks and resumes; park also writes out stdout first, as idle does,
// and tells a race waiting for the fiber to start that it has.
func park(fb *Fiber) {
	idle()
	s := fb.state
	if s.starting != nil {
		close(s.starting)
		s.starting = nil
	}
	if tc := s.clock; tc != nil {
		tc.count(-1)
	}
}

func unpark(fb *Fiber) {
	if tc := fb.state.clock; tc != nil {
		tc.count(1)
	}
}

// The test clock of `Test`.

func testClockOf(fb *Fiber) *TestClock {
	if tc, ok := clockOf(fb).(*TestClock); ok {
		return tc
	}
	panic(Defect{"Error: Test.adjust and Test.set_time need the virtual clock of a Test.clock_test"})
}

// TestClockTest is `Test.clock_test`: an effectful test on a clock of its own.
func TestClockTest(name, task Value) Value {
	return &test{name: name.(string), task: Eff(func(fb *Fiber) Value {
		tc := newTestClock()
		inner := *fb
		inner.env = fb.env.with(clockKey, tc)
		fb.state.clock = tc
		defer func() { fb.state.clock = nil }()
		return Run(&inner, task)
	})}
}

func TestAdjust(fb *Fiber, d Value) Value {
	x := dur(d)
	var nanos *big.Int
	millis := math.Inf(1)
	if !x.infinite() {
		nanos = x.toNanos()
		millis = DurationToMillis(d).(*Data).Fields[0].(float64)
	}
	testClockOf(fb).move(func(now float64) float64 { return now + millis }, nanos)
	return Unit
}

func TestSetTime(fb *Fiber, millis Value) Value {
	testClockOf(fb).move(func(float64) float64 { return millis.(float64) }, nil)
	return Unit
}
