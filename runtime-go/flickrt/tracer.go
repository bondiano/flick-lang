package flickrt

import (
	"fmt"
	"math/big"
	"math/rand/v2"
	"sync"
)

// Effect's Tracer: a span runs around a task under the fiber's span, with
// attributes, links, events and a status that ends with the task. Every
// tracer is Effect's native one, which keeps its spans in memory; one that
// exports (`--trace`) also gets each span as it ends.

// A tracer makes the spans; `ended`, if any, gets each one as it ends.
type tracer struct {
	ended func(*span)
}

var TracerNative Value = &tracer{}

type span struct {
	name         Value // an Option String: none for an external span
	spanId       string
	traceId      string
	sampled      bool
	parent       Value // an Option of a span
	external     bool
	kind         string
	noop         bool // made with tracing off: it records nothing
	unpropagated bool // the spans inside skip it when they look for their parent
	start        *big.Int
	tracer       *tracer

	mu     sync.Mutex
	ended  bool
	end    *big.Int
	exit   *Cause
	attrs  []spanAttr
	links  []Value // `Tracer.SpanLink` records
	events []spanEvent
}

type spanAttr struct {
	key   string
	value Value
}

type spanEvent struct {
	name  string
	time  *big.Int
	attrs []spanAttr
}

const (
	parentSpanKey         = "effect/ParentSpan"
	tracerKey             = "effect/Tracer"
	traceLevelKey         = "effect/Tracer/CurrentTraceLevel"
	minimumTraceLevelKey  = "effect/Tracer/MinimumTraceLevel"
	disablePropagationKey = "effect/Tracer/DisablePropagation"
	tracerEnabledKey      = "effect/References/TracerEnabled"
	tracerTimingKey       = "effect/References/TracerTimingEnabled"
	spanAnnotationsKey    = "effect/References/TracerSpanAnnotations"
	spanLinksKey          = "effect/References/TracerSpanLinks"
	stackFrameKey         = "effect/References/CurrentStackFrame"
)

var (
	spanStartedTag   = Tag("Tracer.SpanStatus.Started")
	spanEndedTag     = Tag("Tracer.SpanStatus.Ended")
	noAttributes     = newRecord(0)
	emptySpanLinks   = []Value{}
	defaultSpanLevel = "Info"
)

func spanOf(v Value) *span { return v.(*span) }

// envOr is the fiber's setting of `key`, or `def`.
func envOr(fb *Fiber, key string, def Value) Value {
	if v, ok := fb.env.get(key); ok {
		return v
	}
	return def
}

// optField is the field `k` of a record that may lack it, or `def`.
func optField(o *Record, k string, def Value) Value {
	if i := o.index(k); i >= 0 {
		return o.vals[i]
	}
	return def
}

// currentSpan is the fiber's span, if it has one.
func currentSpan(fb *Fiber) Value {
	if s, ok := fb.env.get(parentSpanKey); ok {
		return Some(s)
	}
	return None
}

// currentSpanLocal is the fiber's span unless it is external, else nil.
func currentSpanLocal(fb *Fiber) *span {
	if s, ok := fb.env.get(parentSpanKey); ok && !spanOf(s).external {
		return spanOf(s)
	}
	return nil
}

func currentTracer(fb *Fiber) *tracer { return envOr(fb, tracerKey, TracerNative).(*tracer) }

// propagating is Effect's filterDisablePropagation: the span, or the first
// ancestor whose propagation is not disabled.
func propagating(s Value) Value {
	for {
		p, ok := some(s)
		if !ok {
			return None
		}
		sp := spanOf(p)
		if !sp.unpropagated {
			return s
		}
		if sp.external {
			return None
		}
		s = sp.parent
	}
}

// openSpan is Effect's makeSpanUnsafe: a span of `name` made as the
// `Tracer.SpanOptions` say, which may lack fields, under the fiber's span.
func openSpan(fb *Fiber, name, options Value) *span {
	o := record(options)
	parent := optField(o, "parent", None)
	if !isSome(parent) {
		if optField(o, "root", false).(bool) {
			parent = None
		} else {
			parent = propagating(currentSpan(fb))
		}
	}
	if !envOr(fb, tracerEnabledKey, true).(bool) || optField(o, "disablePropagation", false).(bool) {
		return &span{name: Some(name), spanId: "noop", traceId: "noop", parent: parent, kind: "internal",
			noop: true, unpropagated: true, start: bigZero, ended: true, end: bigZero}
	}
	level := envOr(fb, traceLevelKey, defaultSpanLevel).(string)
	if l, ok := some(optField(o, "level", None)); ok {
		level = l.(string)
	}
	links := append([]Value{}, envOr(fb, spanLinksKey, emptySpanLinks).([]Value)...)
	links = append(links, optField(o, "links", emptySpanLinks).([]Value)...)
	start := bigZero
	if envOr(fb, tracerTimingKey, true).(bool) {
		start = clockOf(fb).nanos()
	}
	sampled := false
	if v, ok := some(optField(o, "sampled", None)); ok {
		sampled = v.(bool)
	} else if p, ok := some(parent); ok && !spanOf(p).sampled {
		sampled = false
	} else {
		sampled = logLevelOrdinal(envOr(fb, minimumTraceLevelKey, "All").(string)) <= logLevelOrdinal(level)
	}
	// random ids, as Effect's Hex.random
	s := &span{name: Some(name), spanId: fmt.Sprintf("%016x", rand.Uint64()),
		traceId: fmt.Sprintf("%016x%016x", rand.Uint64(), rand.Uint64()),
		sampled: sampled, parent: parent, kind: optField(o, "kind_", "internal").(string), start: start,
		tracer: currentTracer(fb), links: links}
	if p, ok := some(parent); ok {
		s.traceId = spanOf(p).traceId
	}
	annotations := envOr(fb, spanAnnotationsKey, noAttributes).(*Record)
	for i, k := range annotations.keys {
		s.attribute(k, annotations.vals[i])
	}
	for _, kv := range List(optField(o, "attributes", emptySpanLinks)) {
		s.attribute(At(kv, 0).(string), At(kv, 1))
	}
	return s
}

// attribute sets an attribute: a key it has keeps its place, as in a Map.
func (s *span) attribute(key string, value Value) {
	if s.noop {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	for i := range s.attrs {
		if s.attrs[i].key == key {
			s.attrs[i].value = value
			return
		}
	}
	s.attrs = append(s.attrs, spanAttr{key, value})
}

func (s *span) event(name string, time *big.Int, attrs []spanAttr) {
	if s.noop {
		return
	}
	s.mu.Lock()
	s.events = append(s.events, spanEvent{name, time, attrs})
	s.mu.Unlock()
}

// endSpan is Effect's: it ends a running span with the exit, at the
// clock's time when timing is on.
func endSpan(s *span, exit *Cause, clock Clock, timing bool) {
	if s.noop || s.external {
		return
	}
	s.mu.Lock()
	if s.ended {
		s.mu.Unlock()
		return
	}
	s.ended, s.exit, s.end = true, exit, bigZero
	if timing {
		s.end = clock.nanos()
	}
	s.mu.Unlock()
	if s.tracer != nil && s.tracer.ended != nil {
		s.tracer.ended(s)
	}
}

// A stackFrame is `References.CurrentStackFrame`: the span a task runs in.
// shortcut: no stack line, as Go frames are not the program's; capture Flick's location when `captureStackTrace` asks for it
type stackFrame struct {
	name   string
	parent Value // an Option of a stackFrame
}

// withParentSpan runs `task` with the span as the parent of its spans.
func withParentSpan(fb *Fiber, s *span, task Value) Value {
	inner := *fb
	if !s.external {
		name, _ := some(s.name)
		frame := &stackFrame{name.(string), envOptional(fb, stackFrameKey)}
		inner.env = inner.env.with(stackFrameKey, frame)
	}
	inner.env = inner.env.with(parentSpanKey, s)
	return Run(&inner, task)
}

// envOptional is the fiber's setting of `key` as an Option.
func envOptional(fb *Fiber, key string) Value {
	if v, ok := fb.env.get(key); ok {
		return Some(v)
	}
	return None
}

// useSpan runs `body` with a new span, ended with how it ends.
func useSpan(fb *Fiber, name, options Value, body func(*span) Value) Value {
	return spanning(fb, openSpan(fb, name, options), body)
}

// spanning runs `body` with the span, ended with how it ends.
func spanning(fb *Fiber, s *span, body func(*span) Value) Value {
	clock, timing := clockOf(fb), envOr(fb, tracerTimingKey, true).(bool)
	out, cause := catch(func() Value { return body(s) })
	endSpan(s, cause, clock, timing)
	if cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// scopedSpan is Effect's makeSpanScoped: a span ended when `scope` closes.
func scopedSpan(fb *Fiber, name, options Value, scope *Owner) *span {
	s := openSpan(fb, name, options)
	clock, timing := clockOf(fb), envOr(fb, tracerTimingKey, true).(bool)
	scope.add(fb, func(_ *Fiber, exit *Cause) Value { endSpan(s, exit, clock, timing); return Unit })
	return s
}

func TracerWithSpan(fb *Fiber, name, options, task Value) Value {
	return useSpan(fb, name, options, func(s *span) Value { return withParentSpan(fb, s, task) })
}

func TracerParentSpan(fb *Fiber) Value { return currentSpan(fb) }

func TracerExternalSpan(options Value) Value {
	o := record(options)
	return &span{
		name:     None,
		spanId:   o.Get("spanId").(string),
		traceId:  o.Get("traceId").(string),
		sampled:  o.Get("sampled").(bool),
		parent:   None,
		external: true,
	}
}

func TracerSpanId(s Value) Value   { return spanOf(s).spanId }
func TracerTraceId(s Value) Value  { return spanOf(s).traceId }
func TracerSampled(s Value) Value  { return spanOf(s).sampled }
func TracerSpanName(s Value) Value { return spanOf(s).name }
func TracerParent(s Value) Value   { return spanOf(s).parent }

func TracerSpanKind(s Value) Value {
	if sp := spanOf(s); !sp.external {
		return Some(sp.kind)
	}
	return None
}

// attrText is an attribute's value as text: Node's inspection of one that
// is not a string.
func attrText(v Value) string {
	if s, ok := v.(string); ok {
		return s
	}
	return inspect(v)
}

func TracerLinks(s Value) Value {
	sp := spanOf(s)
	if sp.external {
		return []Value{}
	}
	sp.mu.Lock()
	defer sp.mu.Unlock()
	return append([]Value{}, sp.links...)
}

// TracerStatus is the span's status; an external one runs since 0.
func TracerStatus(s Value) Value {
	sp := spanOf(s)
	sp.mu.Lock()
	defer sp.mu.Unlock()
	switch {
	case sp.external:
		return NewData(spanStartedTag, bigZero)
	case sp.ended:
		return NewData(spanEndedTag, sp.start, sp.end)
	}
	return NewData(spanStartedTag, sp.start)
}

func TracerAttributes(s Value) Value {
	sp := spanOf(s)
	sp.mu.Lock()
	defer sp.mu.Unlock()
	out := make([]Value, len(sp.attrs))
	for i, a := range sp.attrs {
		out[i] = []Value{a.key, attrText(a.value)}
	}
	return out
}

func TracerWithTracer(fb *Fiber, t, task Value) Value { return withEnv(fb, tracerKey, t, task) }

func TracerWithTraceLevel(fb *Fiber, level, task Value) Value {
	return withEnv(fb, traceLevelKey, level, task)
}

func TracerWithMinimumTraceLevel(fb *Fiber, level, task Value) Value {
	return withEnv(fb, minimumTraceLevelKey, level, task)
}

// ending is the span of `Layer.Trace`'s options, ended when `scope`
// closes, after its `onEnd`.
func ending(fb *Fiber, name, options Value, scope *Owner) *span {
	o := record(options)
	s := scopedSpan(fb, name, o.Get("span"), scope)
	if onEnd, ok := some(o.Get("onEnd")); ok {
		scope.add(fb, func(fb *Fiber, exit *Cause) Value {
			how := None
			if exit != nil {
				how = Some(failureOf(exit))
			}
			return Run(fb, Apply(fb, Apply(fb, onEnd, s), how))
		})
	}
	return s
}

// LayerSpan is a layer whose span is the parent of the body's spans.
func LayerSpan(name, options Value) Value {
	return fromBuild(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		return Services{parentSpanKey: ending(fb, name, options, scope)}
	})
}

func LayerParentSpan(s Value) Value { return constant(Services{parentSpanKey: s}) }

// LayerWithSpan builds `self` inside a span of the name.
func LayerWithSpan(name, options, self Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		s := ending(fb, name, options, scope)
		return layerOf(self).build(provided(fb, Services{parentSpanKey: s}), memo, scope)
	})
}

func LayerWithParentSpan(s, self Value) Value {
	return fromBuild(func(fb *Fiber, memo *MemoMap, scope *Owner) Services {
		return layerOf(self).build(provided(fb, Services{parentSpanKey: s}), memo, scope)
	})
}
