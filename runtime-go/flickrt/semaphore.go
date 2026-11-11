package flickrt

import "sync"

// Semaphore is `permits` in all, `taken` of them held; a release wakes the
// fibers waiting for permits, each checking again.
type Semaphore struct {
	mu      sync.Mutex
	permits int
	taken   int
	wake    chan struct{}
}

func SemaphoreMake(_ *Fiber, permits Value) Value {
	return &Semaphore{permits: int(permits.(int32)), wake: make(chan struct{})}
}

func sem(v Value) *Semaphore { return v.(*Semaphore) }

// releaseLocked gives back `n` permits and wakes the waiters: the permits
// free now. The caller holds the lock.
func (s *Semaphore) releaseLocked(n int) int {
	s.taken -= n
	close(s.wake)
	s.wake = make(chan struct{})
	return s.permits - s.taken
}

func (s *Semaphore) release(n int) int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.releaseLocked(n)
}

// take waits until `n` permits are free and takes them.
func (s *Semaphore) take(fb *Fiber, n int) {
	for {
		s.mu.Lock()
		if s.permits-s.taken >= n {
			s.taken += n
			s.mu.Unlock()
			return
		}
		wake := s.wake
		s.mu.Unlock()
		wait(fb, wake)
	}
}

func (s *Semaphore) tryTake(n int) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.permits-s.taken < n {
		return false
	}
	s.taken += n
	return true
}

func SemaphoreTake(fb *Fiber, n, s Value) Value {
	sem(s).take(fb, int(n.(int32)))
	return n
}

func SemaphoreTakeIfAvailable(_ *Fiber, n, s Value) Value { return sem(s).tryTake(int(n.(int32))) }
func SemaphoreRelease(_ *Fiber, n, s Value) Value         { return int32(sem(s).release(int(n.(int32)))) }

func SemaphoreReleaseAll(_ *Fiber, s Value) Value {
	x := sem(s)
	x.mu.Lock()
	defer x.mu.Unlock()
	return int32(x.releaseLocked(x.taken))
}

func SemaphoreResize(_ *Fiber, n, s Value) Value {
	x := sem(s)
	x.mu.Lock()
	defer x.mu.Unlock()
	x.permits = int(n.(int32))
	if x.permits >= x.taken {
		x.releaseLocked(0)
	}
	return Unit
}

// holding runs `task` with the permits a successful `acquire` took,
// giving them back however it ends; the wait for them is interruptible as
// the fiber is around the call.
func holding(fb *Fiber, acquire func() bool, release func(), task Value) (Value, bool) {
	outer := fb.state.mask
	if !acquire() {
		return nil, false
	}
	defer release()
	return masked(fb, outer, func() Value { return Run(fb, task) }), true
}

func SemaphoreWithPermits(fb *Fiber, s, n, task Value) Value {
	x, k := sem(s), int(n.(int32))
	v, _ := holding(fb, func() bool { x.take(fb, k); return true }, func() { x.release(k) }, task)
	return v
}

func SemaphoreWithPermit(fb *Fiber, s, task Value) Value {
	return SemaphoreWithPermits(fb, s, int32(1), task)
}

func SemaphoreWithPermitsIfAvailable(fb *Fiber, s, n, task Value) Value {
	x, k := sem(s), int(n.(int32))
	if v, ok := holding(fb, func() bool { return x.tryTake(k) }, func() { x.release(k) }, task); ok {
		return Some(v)
	}
	return None
}

// Partitioned is a semaphore partitioned by key: waiters queue by key, and
// released permits go to the keys in turn, one permit at a time.
type Partitioned struct {
	mu      sync.Mutex
	max     int
	total   int // permits free
	waiting int // permits the waiters still need
	pending int // released permits not yet handed out
	keys    []Value
	waiters map[int][]*partWaiter // by index into keys
	cursor  int
}

type partWaiter struct {
	permits int
	done    chan struct{}
}

func PartitionedMake(_ *Fiber, options Value) Value {
	n := max(0, int(options.(*Record).Get("permits").(int32)))
	return &Partitioned{max: n, total: n, waiters: map[int][]*partWaiter{}}
}

func part(v Value) *Partitioned { return v.(*Partitioned) }

func PartitionedCapacity(p Value) Value            { return int32(part(p).max) }
func PartitionedAvailable(_ *Fiber, p Value) Value { return int32(part(p).available()) }

func (p *Partitioned) available() int {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.total
}

func (p *Partitioned) keyIndex(key Value) int {
	for i, k := range p.keys {
		if Bool(Equals(k, key)) {
			return i
		}
	}
	p.keys = append(p.keys, key)
	return len(p.keys) - 1
}

// releaseLocked hands `n` released permits to the waiters, a permit to
// each key in turn; the caller holds the lock.
func (p *Partitioned) releaseLocked(n int) int {
	if n > 0 {
		p.pending += n
	}
	for p.pending > 0 && p.waiting > 0 {
		p.cursor %= len(p.keys)
		queue := p.waiters[p.cursor]
		if len(queue) == 0 {
			p.cursor++
			continue
		}
		w := queue[0]
		w.permits--
		p.waiting--
		p.pending--
		if w.permits == 0 {
			p.waiters[p.cursor] = queue[1:]
			close(w.done)
		}
		p.cursor++
	}
	if p.waiting == 0 {
		p.total = min(p.max, p.total+p.pending)
		p.pending = 0
	}
	return p.total
}

func PartitionedRelease(_ *Fiber, n, p Value) Value {
	x := part(p)
	x.mu.Lock()
	defer x.mu.Unlock()
	return int32(x.releaseLocked(int(n.(int32))))
}

// take waits for `n` permits under `key`.
func (p *Partitioned) take(fb *Fiber, key Value, n int) {
	if n <= 0 {
		return
	}
	if p.max < n {
		wait(fb, nil)
	}
	p.mu.Lock()
	if p.total >= n {
		p.total -= n
		p.mu.Unlock()
		return
	}
	w := &partWaiter{permits: n - p.total, done: make(chan struct{})}
	p.total = 0
	p.waiting += w.permits
	i := p.keyIndex(key)
	p.waiters[i] = append(p.waiters[i], w)
	p.mu.Unlock()
	defer func() {
		if r := recover(); r != nil {
			p.mu.Lock()
			queue := p.waiters[i]
			for j, other := range queue {
				if other == w {
					p.waiters[i] = append(queue[:j:j], queue[j+1:]...)
				}
			}
			p.waiting -= w.permits
			p.releaseLocked(n - w.permits)
			p.mu.Unlock()
			panic(r)
		}
	}()
	wait(fb, w.done)
}

func (p *Partitioned) tryTake(n int) bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	if n <= 0 {
		return true
	}
	if p.max < n || p.total < n {
		return false
	}
	p.total -= n
	return true
}

func (p *Partitioned) release(n int) {
	p.mu.Lock()
	p.releaseLocked(n)
	p.mu.Unlock()
}

func PartitionedTake(fb *Fiber, key, n, p Value) Value {
	part(p).take(fb, key, int(n.(int32)))
	return Unit
}

func PartitionedWithPermits(fb *Fiber, p, key, n, task Value) Value {
	x, k := part(p), int(n.(int32))
	if k <= 0 {
		return Run(fb, task)
	}
	v, _ := holding(fb, func() bool { x.take(fb, key, k); return true }, func() { x.release(k) }, task)
	return v
}

func PartitionedWithPermit(fb *Fiber, p, key, task Value) Value {
	return PartitionedWithPermits(fb, p, key, int32(1), task)
}

func PartitionedWithPermitsIfAvailable(fb *Fiber, p, n, task Value) Value {
	x, k := part(p), int(n.(int32))
	if v, ok := holding(fb, func() bool { return x.tryTake(k) }, func() { x.release(max(k, 0)) }, task); ok {
		return Some(v)
	}
	return None
}
