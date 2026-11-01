package flickrt

import (
	"math/big"
	"strconv"
	"sync"
	"sync/atomic"
	"time"
)

// Event listeners: a listener queues each event for a worker, a child of
// its owner, that runs the handler one event at a time; a failing handler
// or a full queue tells the supervisor. Listeners stop with their owner.

type listener struct {
	events chan Value
	// done is closed when the worker has stopped; events sent after are dropped
	done chan struct{}
	// closed is set when the owner it is attached in begins to close
	closed atomic.Bool
	fail   func(*Cause)
}

func EventsQueue(capacity Value) Value { return int(capacity.(int32)) }

func EventsListen(fb *Fiber, owner, policy, supervisor, handler Value) Value {
	l := &listener{events: make(chan Value, policy.(int)), done: make(chan struct{})}
	// the supervisor runs with the services of where the listener was made,
	// in a fiber of its own on each call: the worker and senders call it
	o := owner.(*Owner)
	l.fail = func(c *Cause) {
		supervising := *fb
		supervising.state, supervising.owner = newState(), o
		if _, failed := catch(func() Value { return Run(&supervising, Apply(&supervising, supervisor, failureOf(c))) }); failed != nil {
			o.report(failed)
		}
	}
	FiberSpawn(fb, owner, Fn(func(fb *Fiber, _ Value) Value { return Succeed(Unit) }), Eff(func(w *Fiber) Value {
		defer close(l.done)
		for {
			suspended(w.state)
			select {
			case e := <-l.events:
				_, cause := catch(func() Value { return Run(w, Apply(w, handler, e)) })
				if cause != nil && (cause.has(causeFail) || cause.has(causeDie)) {
					l.fail(cause)
				}
			case <-w.state.signal:
				return Unit
			}
		}
	}))
	return l
}

// EventsAttach registers the listener by `add`. When the owner begins to
// close, later events are ignored and `remove` gets the same listener,
// with the services of where it was attached.
func EventsAttach(fb *Fiber, l, add, remove Value) Value {
	Run(fb, Apply(fb, add, l))
	attached := *fb
	fb.owner.onClose(fb, func(closer *Fiber) {
		l.(*listener).closed.Store(true)
		inner := attached
		inner.state = closer.state
		Run(&inner, Apply(&inner, remove, l))
	})
	return Unit
}

// send queues an event, as the function JS gets for a listener does.
func (l *listener) send(e Value) {
	if l.closed.Load() {
		return
	}
	select {
	case <-l.done:
		return
	default:
	}
	select {
	case l.events <- e:
	default:
		l.fail(&Cause{kind: causeDie, defect: "Error: event queue full: " + strconv.Itoa(cap(l.events)) + " events are waiting"})
	}
}

// An error reporter sends each failure and defect of a cause it is given,
// once, to a listener as an `ErrorReporter.Report`.
type errorReporter struct {
	listener *listener
	mu       sync.Mutex
	reported map[*Cause]bool
}

const reportersKey = "effect/ErrorReporter/CurrentErrorReporters"

func ErrorReporterMake(l Value) Value {
	return &errorReporter{listener: l.(*listener), reported: map[*Cause]bool{}}
}

// ErrorReporterLayer is the reporters in effect for the body, with those
// already in effect when `mergeWithExisting`.
func ErrorReporterLayer(reporters, options Value) Value {
	return &Layer{func(fb *Fiber, _ *MemoMap, _ *Owner) Services {
		var current []Value
		if record(options).Get("mergeWithExisting").(bool) {
			if rs, ok := fb.env.get(reportersKey); ok {
				current = append(current, rs.([]Value)...)
			}
		}
		return Services{reportersKey: append(current, List(reporters)...)}
	}}
}

var severityInfo Value = "Info"

func (r *errorReporter) report(c *Cause) {
	r.mu.Lock()
	seen := r.reported[c]
	r.reported[c] = true
	r.mu.Unlock()
	if seen {
		return
	}
	for _, reason := range c.reasons() {
		if reason.kind == causeInterrupt {
			continue
		}
		message := reason.defect
		if reason.kind == causeFail {
			message = Format(reason.err)
		}
		r.listener.send(Rec(
			"cause", failureOf(reason),
			"message", message,
			"severity", severityInfo,
			"attributes", []Value{},
			"timestamp", big.NewInt(time.Now().UnixNano()),
		))
	}
}

func ErrorReporterReport(fb *Fiber, failure Value) Value {
	if rs, ok := fb.env.get(reportersKey); ok {
		for _, r := range rs.([]Value) {
			r.(*errorReporter).report(causeOfFailure(failure))
		}
	}
	return Unit
}
