package flickrt

import (
	"cmp"
	"slices"
	"sync"
)

// Request is Effect's: a request is made of its payload, so equal payloads
// make equal requests, and a resolver answers the requests made at the
// same time in one batch.
//
// Effect runs fibers one at a time in the order they were scheduled, so
// the requests of fibers forked together reach their resolver in that
// order, and a batch's fiber runs after them. Here they arrive at once, in
// any order: a resolver keeps them in a window, which a fiber taking its
// turn (startup.go) after them sorts into batches as Effect's `addEntry`
// would have, in the order the requesting fibers were forked.

var requestTag = Tag("Request.Request.Request")

func RequestOf(payload Value) Value           { return NewData(requestTag, payload) }
func RequestPayload(request Value) Value      { return request.(*Data).Fields[0] }
func RequestRequest(entry Value) Value        { return RequestPayload(entry.(*requestEntry).request) }
func entryOf(v Value) *requestEntry           { return v.(*requestEntry) }
func batchEntries(es []*requestEntry) []Value { return toValues(es) }

func toValues[T any](xs []T) []Value {
	out := make([]Value, len(xs))
	for i, x := range xs {
		out[i] = x
	}
	return out
}

// requests guards the windows and batches of every resolver, their entries
// and the caches of `with_cache`.
var requests sync.Mutex

// A requestEntry is a request waiting for its resolver, which completes it
// once.
type requestEntry struct {
	request         Value
	by              int64 // the requesting fiber, which orders the window
	span            Value // the requesting fiber's span, an Option
	key             Value // its batch's
	uninterruptible bool
	complete        func(exit *Exit) // `with_cache` wraps it
	batch           *batch
	completed       bool
	done            chan struct{} // closed once `exit` is set
	exit            *Exit
}

// settle is how an entry is completed: once, and only while its batch, if
// any, still waits for it.
func (e *requestEntry) settle(exit *Exit) {
	requests.Lock()
	if e.completed || e.batch != nil && !e.batch.dropPending(e) {
		e.completed = true
		requests.Unlock()
		return
	}
	e.completed = true
	e.exit = exit
	requests.Unlock()
	close(e.done)
}

// A batch is the entries a resolver collected under one key: `entries` go
// to `runAll`, `pending` are those it has not completed.
type batch struct {
	key      Value
	resolver *resolver
	entries  []*requestEntry
	pending  []*requestEntry
	fiber    *child
	run      int  // which of the fibers forked for it runs it: the last one
	started  bool // it takes no more entries
}

// dropPending removes `e` from the pending entries; whether it was there.
// With the lock.
func (b *batch) dropPending(e *requestEntry) bool {
	i := slices.Index(b.pending, e)
	if i >= 0 {
		b.pending = slices.Delete(b.pending, i, i+1)
	}
	return i >= 0
}

// A resolver is Effect's RequestResolver; `window` is the requests not
// yet sorted into its batches, `pending` its batches still collecting.
type resolver struct {
	delay        func(fb *Fiber)
	batchKey     func(fb *Fiber, e *requestEntry) Value
	equalKeys    bool // batch keys compare by `==`, else as JS's Map does
	preCheck     func(fb *Fiber, e *requestEntry) bool
	collectWhile func(fb *Fiber, entries []*requestEntry) bool
	runAll       func(fb *Fiber, entries []Value, key Value)
	window       []*requestEntry
	sorting      bool // a fiber is forked to sort the window
	pending      []*batch
}

// with is a resolver of the same batching, without its requests.
func (r *resolver) with(f func(*resolver)) Value {
	out := *r
	out.window, out.sorting, out.pending = nil, false, nil
	f(&out)
	return &out
}

func resolverOf(v Value) *resolver { return v.(*resolver) }

// sameKey is whether two batch keys are one: by `==` for grouped
// resolvers, else as JS's Map compares keys.
func (r *resolver) sameKey(a, b Value) bool {
	if r.equalKeys {
		return Equals(a, b)
	}
	if x, ok := a.(float64); ok && x != x {
		y, ok := b.(float64)
		return ok && y != y
	}
	return identical(a, b)
}

// find is the pending batch of `key`, nil without one. With the lock.
func (r *resolver) find(key Value) *batch {
	for _, b := range r.pending {
		if r.sameKey(b.key, key) {
			return b
		}
	}
	return nil
}

func (r *resolver) drop(b *batch) {
	if i := slices.Index(r.pending, b); i >= 0 {
		r.pending = slices.Delete(r.pending, i, i+1)
	}
}

// sort sorts the window into batches, in the order the requesting fibers
// were forked.
func (r *resolver) sort(fb *Fiber) {
	requests.Lock()
	window := r.window
	r.window, r.sorting = nil, false
	requests.Unlock()
	slices.SortStableFunc(window, func(a, b *requestEntry) int { return cmp.Compare(a.by, b.by) })
	for _, e := range window {
		r.addEntry(fb, e)
	}
}

// addEntry is Effect's: it adds an entry to its batch, forking the batch's
// fiber for a new one, or running the batch at once when it takes no more.
func (r *resolver) addEntry(fb *Fiber, e *requestEntry) {
	if r.preCheck != nil && !r.preCheck(fb, e) {
		return
	}
	e.key = r.batchKey(fb, e)
	requests.Lock()
	b := r.find(e.key)
	if b == nil {
		b = &batch{key: e.key, resolver: r}
		r.pending = append(r.pending, b)
		b.fork(fb, r.delay)
	}
	e.batch = b
	b.pending = append(b.pending, e)
	b.entries = append(b.entries, e)
	if r.collectWhile(fb, b.entries) {
		requests.Unlock()
		return
	}
	// Effect runs the batch at once, before another entry comes
	r.drop(b)
	previous := b.fiber
	b.fork(fb, func(*Fiber) {})
	requests.Unlock()
	previous.state.interrupt(fb.state.id)
}

// fork forks the fiber that runs the batch after `delay`, in a turn of its
// own; it replaces the one forked before. With the lock.
func (b *batch) fork(fb *Fiber, delay func(*Fiber)) {
	b.run++
	run := b.run
	b.fiber = forkInTurn(fb, newOwner(), func(fb *Fiber) {
		delay(fb)
		b.runAll(fb, run)
	})
}

// runAll runs the resolver on the batch unless a later fiber of it does;
// a failure fails every entry it left incomplete, a success dies for each.
func (b *batch) runAll(fb *Fiber, run int) {
	r := b.resolver
	requests.Lock()
	if b.run != run || b.started {
		requests.Unlock()
		return
	}
	b.started = true
	r.drop(b)
	entries := batchEntries(b.entries)
	requests.Unlock()
	_, cause := catch(func() Value { r.runAll(fb, entries, b.key); return Unit })
	masked(fb, fb.state.mask+1, func() Value {
		exit := &Exit{cause: cause}
		if cause == nil {
			exit.cause = &Cause{kind: causeDie, defect: "Error: Effect.request: RequestResolver did not complete request", thrown: true}
		}
		requests.Lock()
		rest := slices.Clone(b.pending)
		b.entries = nil
		requests.Unlock()
		for _, e := range rest {
			e.complete(exit)
		}
		return Unit
	})
}

// removeEntry takes an interrupted request out of the window or its batch;
// a batch's fiber stops when it has no entries left.
func removeEntry(fb *Fiber, r *resolver, e *requestEntry) {
	requests.Lock()
	if i := slices.Index(r.window, e); i >= 0 {
		r.window = slices.Delete(r.window, i, i+1)
		requests.Unlock()
		e.complete(&Exit{cause: &Cause{kind: causeInterrupt}})
		return
	}
	b := r.find(e.key)
	if e.uninterruptible || e.batch == nil || b == nil {
		requests.Unlock()
		return
	}
	i := slices.Index(b.entries, e)
	if i < 0 {
		requests.Unlock()
		return
	}
	b.entries = slices.Delete(b.entries, i, i+1)
	b.dropPending(e)
	var stopped *child
	if len(b.entries) == 0 {
		r.drop(b)
		stopped = b.fiber
	}
	requests.Unlock()
	e.complete(&Exit{cause: &Cause{kind: causeInterrupt}})
	if stopped != nil {
		stopped.state.interrupt(fb.state.id)
	}
}

// RequestRun runs `request` through `resolver`.
func RequestRun(fb *Fiber, resolver, request Value) Value {
	r := resolverOf(resolver)
	e := &requestEntry{request: request, by: fb.state.id, span: currentSpan(fb), done: make(chan struct{})}
	e.complete = e.settle
	requests.Lock()
	r.window = append(r.window, e)
	if !r.sorting {
		r.sorting = true
		forkInTurn(fb, newOwner(), r.sort)
	}
	requests.Unlock()
	if _, cause := catch(func() Value { wait(fb, e.done); return Unit }); cause != nil {
		masked(fb, fb.state.mask+1, func() Value { removeEntry(fb, r, e); return Unit })
		panic(&Failure{cause})
	}
	if e.exit.cause != nil {
		panic(&Failure{e.exit.cause})
	}
	return e.exit.value
}

func RequestComplete(_ *Fiber, exit, entry Value) Value {
	entryOf(entry).complete(ex(exit))
	return Unit
}

// RequestCompleteEffect completes `entry` as `task` ends; a defect or an
// interruption fails the call instead, as Effect's `matchEffect` lets them.
func RequestCompleteEffect(fb *Fiber, task, entry Value) Value {
	exit := exitOf(fb, task)
	if exit.cause.has(causeDie) || exit.cause.has(causeInterrupt) {
		panic(&Failure{exit.cause})
	}
	entryOf(entry).complete(exit)
	return Unit
}

func RequestSucceed(_ *Fiber, v, entry Value) Value {
	return RequestComplete(nil, ExitSucceed(v), entry)
}
func RequestFail(_ *Fiber, err, entry Value) Value { return RequestComplete(nil, ExitFail(err), entry) }

func RequestFailCause(_ *Fiber, failure, entry Value) Value {
	return RequestComplete(nil, ExitFailCause(failure), entry)
}
