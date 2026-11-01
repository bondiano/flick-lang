package flickrt

import (
	"sync"
	"sync/atomic"
)

// Transactions as Effect 4's (Effect.ts `tx`, `txRetry`; TxRef.ts): an
// attempt reads and writes TxRefs through a journal of `{version, value,
// written}` and commits it at once, or runs again after `retry` or a
// conflict.
//
// JS interleaves fibers, where Go runs them in parallel, so commits are
// TL2's: a commit publishes each value it writes with a new stamp of
// `txClock`, and an attempt that reads a cell stamped after it began stops
// at once as a conflict, so that its body never sees half of another
// commit. Validating a journal and publishing it happen under `txMu`,
// which also guards the waiters of every ref. A write always takes a new
// version: Go has no `Object.is` over every value.

// txCell is a committed value of a TxRef and the stamp of the commit that
// wrote it; never mutated.
type txCell struct {
	version uint64
	value   Value
}

// TxRef is Effect's TxRef.
type TxRef struct {
	cell    atomic.Pointer[txCell]
	waiters map[*txWaiter]struct{} // attempts waiting after `retry` for a change; under txMu
}

var (
	txMu    sync.Mutex
	txClock atomic.Uint64 // the stamp of the last commit that wrote
)

func newTxRef(v Value) *TxRef {
	r := &TxRef{}
	r.cell.Store(&txCell{value: v})
	return r
}

func txRefOf(v Value) *TxRef { return v.(*TxRef) }

// txWaiter is an attempt waiting for a commit to write one of `refs`,
// which closes `ch` once.
type txWaiter struct {
	ch   chan struct{}
	refs []*TxRef
}

// detach takes the waiter off its refs; the caller holds txMu.
func (w *txWaiter) detach() {
	for _, r := range w.refs {
		delete(r.waiters, w)
	}
}

type txEntry struct {
	version uint64
	value   Value
	written bool
}

// tx is Effect's TransactionMetaInner: the journal of the current attempt
// and its metadata. A body may fork fibers that share it, so `mu` guards
// it; take `mu` before `txMu`, never after.
type tx struct {
	mu        sync.Mutex
	journal   map[*TxRef]*txEntry
	readStamp uint64 // txClock as the attempt began
	retry     bool   // the body called `retry`
	conflict  bool   // the body read a cell committed after readStamp
	done      atomic.Bool

	attempt                            int
	reason                             Value // None, or Some of the previous attempt's Tx.RetryReason
	start, now, elapsed, sincePrevious float64
}

const txKey = "$transaction"

var (
	txRetryReason    = Some(Nullary(Tag("Tx.RetryReason.Retry")))
	txConflictReason = Some(Nullary(Tag("Tx.RetryReason.Conflict")))
)

// txOf is the transaction the fiber runs in, nil outside one: a boundary
// that finished leaves the fibers it forked to start their own.
func txOf(fb *Fiber) *tx {
	if v, ok := fb.env.get(txKey); ok {
		if t := v.(*tx); !t.done.Load() {
			return t
		}
	}
	return nil
}

// TxAtomically is Effect's `tx`: the task runs as one transaction, or as
// part of the fiber's.
func TxAtomically(fb *Fiber, task Value) Value {
	if txOf(fb) != nil {
		return Run(fb, task)
	}
	return atomically(fb, func(fb *Fiber) Value { return Run(fb, task) })
}

// txDo runs `f` in the fiber's transaction, or in one of its own, as each
// of Effect's operations on transactional values is wrapped in `tx`.
func txDo(fb *Fiber, f func(*Fiber, *tx) Value) Value {
	if t := txOf(fb); t != nil {
		return f(fb, t)
	}
	return atomically(fb, func(fb *Fiber) Value { return f(fb, txOf(fb)) })
}

// atomically runs `body` in a new transaction until an attempt commits or
// fails while consistent.
func atomically(fb *Fiber, body func(*Fiber) Value) Value {
	clock := clockOf(fb)
	now := clock.millis()
	t := &tx{journal: map[*TxRef]*txEntry{}, attempt: 1, reason: None, start: now, now: now}
	inner := *fb
	inner.env = fb.env.with(txKey, t)
	defer t.done.Store(true)
	for {
		t.begin(clock)
		out, cause := catch(func() Value { return body(&inner) })
		t.mu.Lock()
		retry, conflict := t.retry, t.conflict
		t.mu.Unlock()
		if cause != nil && retry && !conflict {
			if interruptsOnly(cause) {
				t.await(fb)
			} else {
				// `retry` interrupted the body, but it also failed: roll back and fail
				t.clear()
				retry = false
			}
		}
		if !retry && !conflict {
			if cause != nil && t.consistent() {
				t.clear()
				panic(&Failure{cause})
			}
			if cause == nil && t.commit() {
				t.clear()
				return out
			}
		}
		t.mu.Lock()
		t.attempt++
		t.reason = txConflictReason
		if retry && !conflict {
			t.reason = txRetryReason
		}
		t.mu.Unlock()
		t.clear()
	}
}

func interruptsOnly(c *Cause) bool {
	return c.has(causeInterrupt) && !c.has(causeFail) && !c.has(causeDie)
}

// begin starts an attempt: Effect stamps the times of every attempt but
// the first.
func (t *tx) begin(clock Clock) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.attempt > 1 {
		now := clock.millis()
		t.sincePrevious = now - t.now
		t.now = now
		t.elapsed = now - t.start
	}
	t.readStamp = txClock.Load()
}

func (t *tx) clear() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.retry, t.conflict = false, false
	clear(t.journal)
}

// entry is the journal's entry of `r`, made as the attempt first reads it;
// reading a cell committed after the attempt began ends the attempt as a
// conflict. The caller holds t.mu.
func (t *tx) entry(fb *Fiber, r *TxRef) *txEntry {
	if e, ok := t.journal[r]; ok {
		return e
	}
	c := r.cell.Load()
	if c.version > t.readStamp {
		t.conflict = true
		panic(&Failure{&Cause{kind: causeInterrupt, fiber: fb.state.id}})
	}
	e := &txEntry{version: c.version, value: c.value}
	t.journal[r] = e
	return e
}

// get is the value of `r` the attempt sees.
func (t *tx) get(fb *Fiber, r *TxRef) Value {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.entry(fb, r).value
}

// set writes `v` to `r` in the journal; as Effect's, it reads `r` first.
func (t *tx) set(fb *Fiber, r *TxRef, v Value) {
	t.mu.Lock()
	defer t.mu.Unlock()
	e := t.entry(fb, r)
	e.value, e.written = v, true
}

// retryNow is Effect's `txRetry`: the attempt ends, to run again once a
// ref it read changes.
func (t *tx) retryNow(fb *Fiber) Value {
	t.mu.Lock()
	t.retry = true
	t.mu.Unlock()
	panic(&Failure{&Cause{kind: causeInterrupt, fiber: fb.state.id}})
}

// consistent is whether no commit wrote a ref the attempt read.
func (t *tx) consistent() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.unchanged()
}

// unchanged is `consistent`, the caller holding t.mu.
func (t *tx) unchanged() bool {
	for r, e := range t.journal {
		if r.cell.Load().version != e.version {
			return false
		}
	}
	return true
}

// commit publishes the journal's writes if the attempt is consistent,
// and wakes the attempts waiting for them; whether it did.
func (t *tx) commit() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	txMu.Lock()
	if !t.unchanged() {
		txMu.Unlock()
		return false
	}
	stamp, wrote := txClock.Load()+1, false
	var woken []*txWaiter
	for r, e := range t.journal {
		if !e.written {
			continue
		}
		wrote = true
		r.cell.Store(&txCell{stamp, e.value})
		for w := range r.waiters {
			woken = append(woken, w)
			w.detach()
		}
	}
	if wrote {
		// after the cells: an attempt that sees this stamp sees all of them
		txClock.Store(stamp)
	}
	txMu.Unlock()
	for _, w := range woken {
		close(w.ch)
	}
	return true
}

// await is Effect's awaitPendingTransaction: it waits, interruptibly,
// until a commit writes a ref the attempt read, unless one already has.
func (t *tx) await(fb *Fiber) {
	w := &txWaiter{ch: make(chan struct{})}
	t.mu.Lock()
	txMu.Lock()
	stale := !t.unchanged()
	if !stale {
		for r := range t.journal {
			if r.waiters == nil {
				r.waiters = map[*txWaiter]struct{}{}
			}
			r.waiters[w] = struct{}{}
			w.refs = append(w.refs, r)
		}
	}
	txMu.Unlock()
	t.mu.Unlock()
	if stale {
		return
	}
	defer func() {
		txMu.Lock()
		w.detach()
		txMu.Unlock()
	}()
	wait(fb, w.ch)
}

// txIn is the fiber's transaction, which the types of `Tx.Transaction`
// operations guarantee.
func txIn(fb *Fiber) *tx {
	if t := txOf(fb); t != nil {
		return t
	}
	panic(Defect{"Service not found: effect/Effect/Transaction"})
}

func TxRetry(fb *Fiber) Value { return txIn(fb).retryNow(fb) }

// TxCurrent is the metadata of the current attempt, without its journal.
func TxCurrent(fb *Fiber) Value {
	t := txIn(fb)
	t.mu.Lock()
	defer t.mu.Unlock()
	return Rec(
		"attempt", int32(t.attempt),
		"retryReason", t.reason,
		"start", t.start,
		"now", t.now,
		"elapsed", t.elapsed,
		"elapsedSincePrevious", t.sincePrevious,
	)
}

// The functions of `TxRef`.

func TxRefMake(v Value) Value { return newTxRef(v) }

func TxRefGet(fb *Fiber, r Value) Value { return txRead(fb, txRefOf(r), Identity) }

func TxRefSet(fb *Fiber, v, r Value) Value {
	return txUpdate(fb, txRefOf(r), func(Value) Value { return v })
}

func TxRefUpdate(fb *Fiber, f, r Value) Value {
	return txUpdate(fb, txRefOf(r), func(v Value) Value { return Apply(fb, f, v) })
}

func TxRefModify(fb *Fiber, f, r Value) Value { return txModify(fb, txRefOf(r), txPair(fb, f)) }

// txPair is a Flick function giving a pair `(out, next)`, for `modify`.
func txPair(fb *Fiber, f Value) func(Value) (Value, Value) {
	return func(v Value) (Value, Value) {
		p := Apply(fb, f, v).([]Value)
		return p[0], p[1]
	}
}

// txRead, txUpdate and txModify are TxRef's `get`, `update` and `modify`
// of `r` by Go functions, in the fiber's transaction or one of their own,
// for the structures over TxRefs.
func txRead(fb *Fiber, r *TxRef, f func(Value) Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return f(t.get(fb, r)) })
}

func txUpdate(fb *Fiber, r *TxRef, f func(Value) Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		t.update(fb, r, f)
		return Unit
	})
}

func txModify(fb *Fiber, r *TxRef, f func(Value) (Value, Value)) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value { return t.modify(fb, r, f) })
}

func (t *tx) update(fb *Fiber, r *TxRef, f func(Value) Value) {
	t.set(fb, r, f(t.get(fb, r)))
}

func (t *tx) modify(fb *Fiber, r *TxRef, f func(Value) (Value, Value)) Value {
	out, next := f(t.get(fb, r))
	t.set(fb, r, next)
	return out
}
