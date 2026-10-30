package flickrt

import (
	"fmt"
	"sync/atomic"
)

// Spans, as far as layers and the fiber's current span go: a span is the
// parent of the spans opened while it is the fiber's, and ends with how
// its region ended.
//
// shortcut: spans are kept, not exported, and carry no attributes, links, status or level; port Effect's Tracer with a tracer that exports them
type span struct {
	name     Value // an Option String: none for an external span
	spanId   string
	traceId  string
	sampled  bool
	parent   Value // an Option of a span
	external bool
}

const parentSpanKey = "effect/ParentSpan"

var spanIds atomic.Uint64

func spanOf(v Value) *span { return v.(*span) }

// currentSpan is the fiber's span, if it has one.
func currentSpan(fb *Fiber) Value {
	if s, ok := fb.env.get(parentSpanKey); ok {
		return Some(s)
	}
	return None
}

// openSpan is a span of `name` under the fiber's, or under the parent the
// options give; a root span has none.
func openSpan(fb *Fiber, name, options Value) *span {
	o := record(options)
	parent := o.Get("parent")
	if _, ok := some(parent); !ok && !o.Get("root").(bool) {
		parent = currentSpan(fb)
	}
	n := spanIds.Add(1)
	traceId := fmt.Sprintf("%032x", n)
	if p, ok := some(parent); ok {
		traceId = spanOf(p).traceId
	}
	return &span{
		name:    Some(name),
		spanId:  fmt.Sprintf("%016x", n),
		traceId: traceId,
		sampled: true,
		parent:  parent,
	}
}

func TracerWithSpan(fb *Fiber, name, options, task Value) Value {
	return withEnv(fb, parentSpanKey, openSpan(fb, name, options), task)
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

// ending is the span of `Layer.Trace`'s options, ended with `scope`.
func ending(fb *Fiber, name, options Value, scope *Owner) *span {
	o := record(options)
	s := openSpan(fb, name, o.Get("span"))
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
