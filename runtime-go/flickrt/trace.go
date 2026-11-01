package flickrt

import (
	"os"
	"strconv"
	"sync"
	"sync/atomic"
)

// The runtime trace of docs/formal/runtime.md (X4): with FLICK_TRACE set
// to a file, each step of the owner contract λRT names is written there as
// one JSON object per line, in the order the steps happened. The JS
// runtime writes the same events (runtime/src/trace.ts); flick-model's
// monitor checks a trace against λRT, and the e2e harness compares the
// projections of both. A disabled trace costs one check of `tracing`.
//
// Events, with `o` an owner, `f` a child, `i` the place of a finalizer
// among those of its owner, `l` a layer, `t` a foreign call:
//
//	open o p k     an owner `k` (main, scope, provide) opens inside `p` (0: none)
//	phase o c      its close reaches phase `c` (C0…C6, §3.3)
//	spawn o f      a child and its watcher are registered
//	refuse o       a spawn into a closing owner fails
//	interrupt o f  C2 asks a child to stop
//	end o f x      a child ends: ok, fail (a typed failure or a defect) or int
//	sup o f        its supervisor is called
//	watched o f    its watcher is done
//	fin o i        a finalizer is added to the stack
//	now o i        a finalizer is added after C5 and runs at once
//	run o i        a finalizer starts
//	track o t      the owner waits for a foreign call
//	settle o t     the call settled
//	build l        a memoized layer builds; release l: its scope releases it
//	exit x         main ends: ok, fail or int
var (
	tracePath = os.Getenv("FLICK_TRACE")
	tracing   = tracePath != ""
)

// a process this one starts writes no trace over this one's
func init() { _ = os.Unsetenv("FLICK_TRACE") }

var (
	traceMu   sync.Mutex
	traceFile *os.File
	traceBuf  []byte
	traceIDs  atomic.Int64
	traceKids sync.Map // *child → its id
	traceLays sync.Map // *Layer → its id
)

// traceID is a fresh id of an owner, a child, a layer or a foreign call.
func traceID() int64 { return traceIDs.Add(1) }

// traceEvent writes one event: its name, then pairs of a key and an int64
// or a string.
func traceEvent(event string, fields ...any) {
	traceMu.Lock()
	defer traceMu.Unlock()
	if traceFile == nil {
		f, err := os.Create(tracePath)
		if err != nil {
			tracing = false
			return
		}
		traceFile = f
	}
	b := append(traceBuf[:0], `{"e":`...)
	b = strconv.AppendQuote(b, event)
	for i := 0; i+1 < len(fields); i += 2 {
		b = append(b, ',')
		b = strconv.AppendQuote(b, fields[i].(string))
		b = append(b, ':')
		switch v := fields[i+1].(type) {
		case int64:
			b = strconv.AppendInt(b, v, 10)
		case string:
			b = strconv.AppendQuote(b, v)
		}
	}
	b = append(b, '}', '\n')
	// unbuffered: `os.Exit` ends the program without flushing
	_, _ = traceFile.Write(b)
	traceBuf = b
}

// traceOpen gives `o` an id and writes that it opened inside `parent`.
func traceOpen(o, parent *Owner, kind string) {
	o.trace = traceID()
	var p int64
	if parent != nil {
		p = parent.trace
	}
	traceEvent("open", "o", o.trace, "p", p, "k", kind)
}

func (o *Owner) tracePhase(phase string) {
	if tracing && o.trace != 0 {
		traceEvent("phase", "o", o.trace, "c", phase)
	}
}

// traceChild is the id of a child, given at its spawn.
func traceChild(c *child) int64 {
	if id, ok := traceKids.Load(c); ok {
		return id.(int64)
	}
	id := traceID()
	traceKids.Store(c, id)
	return id
}

// traceExit is how a cause ends a fiber, as the events name it.
func traceExit(c *Cause) string {
	switch {
	case c == nil:
		return "ok"
	case c.has(causeFail) || c.has(causeDie):
		return "fail"
	}
	return "int"
}

// traceLayer is the id of a layer, given when it first builds.
func traceLayer(l *Layer) int64 {
	id, _ := traceLays.LoadOrStore(l, traceID())
	return id.(int64)
}
