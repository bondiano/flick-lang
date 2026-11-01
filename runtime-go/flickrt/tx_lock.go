package flickrt

import "maps"

// TxReentrantLock and TxSemaphore as Effect 4's: a TxRef of who holds the
// lock, and a TxRef of the permits left.

// TxReentrantLock is Effect's TxReentrantLock.
type TxReentrantLock struct{ ref *TxRef }

// txLockState is Effect's LockState: the read locks of each fiber, and
// the fiber holding the write lock (0: none) with its count; never
// mutated.
type txLockState struct {
	readers map[int64]int32
	writer  int64
	writes  int32
}

func txLockOf(v Value) *TxRef { return v.(*TxReentrantLock).ref }

func lockState(v Value) *txLockState { return v.(*txLockState) }

// withReaders is the state with `n` read locks of fiber `id`.
func (s *txLockState) withReaders(id int64, n int32) *txLockState {
	readers := maps.Clone(s.readers)
	if readers == nil {
		readers = map[int64]int32{}
	}
	if n == 0 {
		delete(readers, id)
	} else {
		readers[id] = n
	}
	return &txLockState{readers, s.writer, s.writes}
}

func TxReentrantLockMake() Value { return &TxReentrantLock{newTxRef(&txLockState{})} }

// acquireRead takes a read lock for fiber `id`, retrying while another
// fiber writes; its read locks then.
func acquireRead(fb *Fiber, l Value, id int64) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		s := lockState(t.get(fb, txLockOf(l)))
		if s.writer != 0 && s.writer != id {
			return t.retryNow(fb)
		}
		n := s.readers[id] + 1
		t.set(fb, txLockOf(l), s.withReaders(id, n))
		return n
	})
}

// acquireWrite takes the write lock for fiber `id`, retrying while
// another fiber reads or writes; its write locks then.
func acquireWrite(fb *Fiber, l Value, id int64) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		s := lockState(t.get(fb, txLockOf(l)))
		if s.writer != 0 && s.writer != id {
			return t.retryNow(fb)
		}
		for reader, n := range s.readers {
			if reader != id && n > 0 {
				return t.retryNow(fb)
			}
		}
		n := int32(1)
		if s.writer != 0 {
			n = s.writes + 1
		}
		t.set(fb, txLockOf(l), &txLockState{s.readers, id, n})
		return n
	})
}

func releaseRead(fb *Fiber, l Value, id int64) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		s := lockState(t.get(fb, txLockOf(l)))
		n := s.readers[id]
		if n <= 0 {
			return int32(0)
		}
		t.set(fb, txLockOf(l), s.withReaders(id, n-1))
		return n - 1
	})
}

func releaseWrite(fb *Fiber, l Value, id int64) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		s := lockState(t.get(fb, txLockOf(l)))
		if s.writer == 0 || s.writer != id {
			return int32(0)
		}
		next := &txLockState{s.readers, id, s.writes - 1}
		if next.writes <= 0 {
			next.writer, next.writes = 0, 0
		}
		t.set(fb, txLockOf(l), next)
		return s.writes - 1
	})
}

func TxReentrantLockAcquireRead(fb *Fiber, l Value) Value  { return acquireRead(fb, l, fb.state.id) }
func TxReentrantLockAcquireWrite(fb *Fiber, l Value) Value { return acquireWrite(fb, l, fb.state.id) }
func TxReentrantLockReleaseRead(fb *Fiber, l Value) Value  { return releaseRead(fb, l, fb.state.id) }
func TxReentrantLockReleaseWrite(fb *Fiber, l Value) Value { return releaseWrite(fb, l, fb.state.id) }

// acquireScoped is Effect's interruptible acquireRelease: `release` of the
// acquiring fiber runs as the scope closes.
func acquireScoped(fb *Fiber, l Value, acquire, release func(*Fiber, Value, int64) Value) Value {
	id := fb.state.id
	n := acquire(fb, l, id)
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { return release(fb, l, id) })
	return n
}

func TxReentrantLockReadLock(fb *Fiber, l Value) Value {
	return acquireScoped(fb, l, acquireRead, releaseRead)
}

func TxReentrantLockWriteLock(fb *Fiber, l Value) Value {
	return acquireScoped(fb, l, acquireWrite, releaseWrite)
}

// withAcquired is Effect's: `task` runs once `acquire` did, interruptibly
// as the caller, and `release` after it, uninterruptibly.
func withAcquired(fb *Fiber, acquire, release func(*Fiber), task Value) Value {
	acquire(fb)
	out, cause := catch(func() Value { return Run(fb, task) })
	_, released := catch(func() Value {
		return masked(fb, fb.state.mask+1, func() Value {
			release(fb)
			return Unit
		})
	})
	if cause = Sequential(cause, released); cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// lockHeld is `with_read_lock` or `with_write_lock` by its acquire and
// release.
func lockHeld(fb *Fiber, task, l Value, acquire, release func(*Fiber, Value, int64) Value) Value {
	id := fb.state.id
	return withAcquired(fb,
		func(fb *Fiber) { acquire(fb, l, id) },
		func(fb *Fiber) { release(fb, l, id) },
		task)
}

func TxReentrantLockWithReadLock(fb *Fiber, task, l Value) Value {
	return lockHeld(fb, task, l, acquireRead, releaseRead)
}

func TxReentrantLockWithWriteLock(fb *Fiber, task, l Value) Value {
	return lockHeld(fb, task, l, acquireWrite, releaseWrite)
}

func txLockRead(fb *Fiber, l Value, f func(*txLockState) Value) Value {
	return txRead(fb, txLockOf(l), func(v Value) Value { return f(lockState(v)) })
}

func TxReentrantLockReadLocks(fb *Fiber, l Value) Value {
	return txLockRead(fb, l, func(s *txLockState) Value {
		total := int32(0)
		for _, n := range s.readers {
			total += n
		}
		return total
	})
}

func TxReentrantLockWriteLocks(fb *Fiber, l Value) Value {
	return txLockRead(fb, l, func(s *txLockState) Value { return s.writes })
}

func TxReentrantLockLocked(fb *Fiber, l Value) Value {
	return txLockRead(fb, l, func(s *txLockState) Value { return len(s.readers) > 0 || s.writer != 0 })
}

func TxReentrantLockReadLocked(fb *Fiber, l Value) Value {
	return txLockRead(fb, l, func(s *txLockState) Value { return len(s.readers) > 0 })
}

func TxReentrantLockWriteLocked(fb *Fiber, l Value) Value {
	return txLockRead(fb, l, func(s *txLockState) Value { return s.writer != 0 })
}

// TxSemaphore is Effect's TxSemaphore.
type TxSemaphore struct {
	ref      *TxRef
	capacity int32
}

func txSemaphoreOf(v Value) *TxSemaphore { return v.(*TxSemaphore) }

func TxSemaphoreMake(permits Value) Value {
	n := permits.(int32)
	if n < 0 {
		panic(Defect{"Permits must be non-negative"})
	}
	return &TxSemaphore{newTxRef(n), n}
}

func TxSemaphoreAvailable(fb *Fiber, s Value) Value {
	return txRead(fb, txSemaphoreOf(s).ref, Identity)
}

func TxSemaphoreCapacity(s Value) Value { return txSemaphoreOf(s).capacity }

// positive is a number of permits, which must be positive.
func positive(n Value) int32 {
	if k := n.(int32); k > 0 {
		return k
	}
	panic(Defect{"Number of permits must be positive"})
}

// acquirePermits takes `n` permits, retrying while there are fewer.
func acquirePermits(fb *Fiber, s Value, n int32) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		r := txSemaphoreOf(s).ref
		permits := t.get(fb, r).(int32)
		if permits < n {
			return t.retryNow(fb)
		}
		t.set(fb, r, permits-n)
		return Unit
	})
}

func tryAcquirePermits(fb *Fiber, s Value, n int32) Value {
	return txModify(fb, txSemaphoreOf(s).ref, func(v Value) (Value, Value) {
		if permits := v.(int32); permits >= n {
			return true, permits - n
		}
		return false, v
	})
}

// releasePermits gives `n` permits back, up to the capacity.
func releasePermits(fb *Fiber, s Value, n int32) Value {
	sem := txSemaphoreOf(s)
	return txUpdate(fb, sem.ref, func(v Value) Value { return min(v.(int32)+n, sem.capacity) })
}

func TxSemaphoreAcquire(fb *Fiber, s Value) Value     { return acquirePermits(fb, s, 1) }
func TxSemaphoreAcquireN(fb *Fiber, n, s Value) Value { return acquirePermits(fb, s, positive(n)) }
func TxSemaphoreTryAcquire(fb *Fiber, s Value) Value  { return tryAcquirePermits(fb, s, 1) }

func TxSemaphoreTryAcquireN(fb *Fiber, n, s Value) Value {
	return tryAcquirePermits(fb, s, positive(n))
}

func TxSemaphoreRelease(fb *Fiber, s Value) Value     { return releasePermits(fb, s, 1) }
func TxSemaphoreReleaseN(fb *Fiber, n, s Value) Value { return releasePermits(fb, s, positive(n)) }

func TxSemaphoreWithPermit(fb *Fiber, s, task Value) Value {
	return TxSemaphoreWithPermits(fb, s, int32(1), task)
}

func TxSemaphoreWithPermits(fb *Fiber, s, n, task Value) Value {
	k := positive(n)
	return withAcquired(fb,
		func(fb *Fiber) { acquirePermits(fb, s, k) },
		func(fb *Fiber) { releasePermits(fb, s, k) },
		task)
}

func TxSemaphoreWithPermitScoped(fb *Fiber, s Value) Value {
	acquirePermits(fb, s, 1)
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { return releasePermits(fb, s, 1) })
	return Unit
}
