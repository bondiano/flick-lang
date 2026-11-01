package flickrt

import "math/big"

// The part `Effect.Observe`: log annotations and spans, spans of the
// fiber's tracer, and metrics of how tasks end.

var noSuchElementTag = Tag("Cause.NoSuchElementError.NoSuchElementError")

func EffectWithLogger(fb *Fiber, l, task Value) Value {
	return withEnv(fb, loggersKey, loggerSet(loggersOf(fb), []Value{l}), task)
}

func EffectAnnotateLogs(fb *Fiber, key, value, task Value) Value {
	return withEnv(fb, logAnnotationsKey, logAnnotationsOf(fb).set(key.(string), value), task)
}

// EffectAnnotateLogsScoped annotates the rest of the innermost scope: the
// scope's fiber gets the annotation, which it loses as the scope closes
// unless something replaced it.
//
// shortcut: annotating a region other than the innermost one lasts only for that call, as its fiber is not the caller's; keep the annotations in the owner if a program does that
func EffectAnnotateLogsScoped(fb *Fiber, key, value Value) Value {
	k := key.(string)
	prev := logAnnotationsOf(fb)
	fb.env = fb.env.with(logAnnotationsKey, prev.set(k, value))
	fb.owner.add(fb, func(*Fiber, *Cause) Value {
		current := logAnnotationsOf(fb)
		if i := current.index(k); i < 0 || current.vals[i] != value {
			return Unit
		}
		next := current
		if j := prev.index(k); j >= 0 {
			next = current.set(k, prev.vals[j])
		} else {
			next = newRecord(len(current.keys))
			for i, other := range current.keys {
				if other != k {
					next.put(other, current.vals[i])
				}
			}
		}
		fb.env = fb.env.with(logAnnotationsKey, next)
		return Unit
	})
	return Unit
}

// EffectWithLogSpan runs the task in a log span that starts now.
func EffectWithLogSpan(fb *Fiber, label, task Value) Value {
	spans := append([]logSpan{{label.(string), clockOf(fb).millis()}}, logSpansOf(fb)...)
	return withEnv(fb, logSpansKey, spans, task)
}

func EffectTracer(fb *Fiber) Value { return currentTracer(fb) }

func EffectAnnotateSpans(fb *Fiber, key, value, task Value) Value {
	annotations := envOr(fb, spanAnnotationsKey, noAttributes).(*Record)
	return withEnv(fb, spanAnnotationsKey, annotations.set(key.(string), value), task)
}

func EffectAnnotateCurrentSpan(fb *Fiber, key, value Value) Value {
	if s := currentSpanLocal(fb); s != nil {
		s.attribute(key.(string), value)
	}
	return Unit
}

// noSuchElement fails with Effect's NoSuchElementError, whose message JS
// leaves undefined.
func noSuchElement() Value { return Fail(NewData(noSuchElementTag, "")) }

func EffectCurrentSpan(fb *Fiber) Value {
	if s := currentSpanLocal(fb); s != nil {
		return s
	}
	return noSuchElement()
}

func EffectCurrentParentSpan(fb *Fiber) Value {
	if s, ok := fb.env.get(parentSpanKey); ok {
		return s
	}
	return noSuchElement()
}

func EffectSpanLinks(fb *Fiber) Value {
	return append([]Value{}, envOr(fb, spanLinksKey, emptySpanLinks).([]Value)...)
}

func EffectLinkSpans(fb *Fiber, spans, attributes, task Value) Value {
	links := append([]Value{}, envOr(fb, spanLinksKey, emptySpanLinks).([]Value)...)
	for _, s := range List(spans) {
		links = append(links, Rec("span", s, "attributes", attributes))
	}
	return withEnv(fb, spanLinksKey, links, task)
}

func EffectMakeSpan(fb *Fiber, name, options Value) Value { return openSpan(fb, name, options) }

func EffectMakeSpanScoped(fb *Fiber, name, options Value) Value {
	return masked(fb, fb.state.mask+1, func() Value { return scopedSpan(fb, name, options, fb.owner) })
}

func EffectUseSpan(fb *Fiber, name, options, f Value) Value {
	return useSpan(fb, name, options, func(s *span) Value { return Run(fb, Apply(fb, f, s)) })
}

func EffectWithSpanScoped(fb *Fiber, name, options, task Value) Value {
	return withParentSpan(fb, spanOf(EffectMakeSpanScoped(fb, name, options)), task)
}

func EffectWithParentSpan(fb *Fiber, s, task Value) Value { return withParentSpan(fb, spanOf(s), task) }

// tracked runs the task, then updates the metric with what `input` makes
// of its exit, if anything, before it ends the same way.
func tracked(fb *Fiber, m, task Value, input func(*Exit) (Value, bool)) Value {
	out, cause := catch(func() Value { return Run(fb, task) })
	masked(fb, fb.state.mask+1, func() Value {
		if in, ok := input(&Exit{out, cause}); ok {
			MetricUpdate(fb, in, m)
		}
		return Unit
	})
	if cause != nil {
		panic(&Failure{cause})
	}
	return out
}

func EffectTrack(fb *Fiber, m, f, task Value) Value {
	return tracked(fb, m, task, func(e *Exit) (Value, bool) { return Apply(fb, f, e), true })
}

func EffectTrackSuccesses(fb *Fiber, m, f, task Value) Value {
	return tracked(fb, m, task, func(e *Exit) (Value, bool) {
		if e.cause != nil {
			return nil, false
		}
		return Apply(fb, f, e.value), true
	})
}

func EffectTrackErrors(fb *Fiber, m, f, task Value) Value {
	return tracked(fb, m, task, func(e *Exit) (Value, bool) {
		if e.cause == nil {
			return nil, false
		}
		r := e.cause.firstOf(causeFail)
		if r == nil {
			return nil, false
		}
		return Apply(fb, f, r.err), true
	})
}

func EffectTrackDefects(fb *Fiber, m, f, task Value) Value {
	return tracked(fb, m, task, func(e *Exit) (Value, bool) {
		if e.cause == nil {
			return nil, false
		}
		r := e.cause.firstOf(causeDie)
		if r == nil {
			return nil, false
		}
		return Apply(fb, f, r.message()), true
	})
}

// EffectTrackDuration updates the metric with how long the task ran, by
// the clock's monotonic time.
func EffectTrackDuration(fb *Fiber, m, f, task Value) Value {
	clock := clockOf(fb)
	start := clock.monotonic()
	return tracked(fb, m, task, func(*Exit) (Value, bool) {
		elapsed := new(big.Int).Sub(clock.monotonic(), start)
		return Apply(fb, f, fromNanos(elapsed)), true
	})
}

// EffectDie is `Effect.die`: a defect of the message.
func EffectDie(_ *Fiber, message Value) Value {
	panic(&Failure{&Cause{kind: causeDie, defect: message.(string)}})
}
