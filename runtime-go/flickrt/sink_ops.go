package flickrt

import "math/big"

// Sink's constructors and transformations, as Effect's (Sink.ts): a
// transform gives the value and the leftovers (nil: none) it ends with.

func endOf(v Value) (Value, []Value) { return v, nil }

// sinkOfEffect is Effect's `fromEffectEnd`: the task's end, consuming
// nothing.
func sinkOfEffect(task func(fb *Fiber) (Value, []Value)) *Sink {
	return sinkFromTransform(func(fb *Fiber, _ Pull, _ *Owner) (Value, []Value) { return task(fb) })
}

// sinkMapEnd maps the end the sink gives.
func sinkMapEnd(self *Sink, f func(fb *Fiber, v Value, l []Value) (Value, []Value)) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		v, l := self.transform(fb, upstream, scope)
		return f(fb, v, l)
	})
}

// sinkMapInputArray maps each chunk before the sink reads it.
func sinkMapInputArray(self *Sink, f func(fb *Fiber, chunk Value) Value) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		return self.transform(fb, func(fb *Fiber) Value { return f(fb, upstream(fb)) }, scope)
	})
}

// upstreamTracked is the upstream, marking `ended` once it fails or ends.
func upstreamTracked(upstream Pull, ended *bool) Pull {
	return func(fb *Fiber) Value {
		v, c := catch(func() Value { return upstream(fb) })
		if c != nil {
			*ended = true
			panic(&Failure{c})
		}
		return v
	}
}

// sinkFlatMap runs the sink `f` makes of the value on its leftovers, then
// on the rest of the upstream.
func sinkFlatMap(self *Sink, f func(fb *Fiber, a Value) *Sink) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		ended := false
		a, leftover := self.transform(fb, upstreamTracked(upstream, &ended), scope)
		v, l := f(fb, a).transform(fb, func(fb *Fiber) Value {
			switch {
			case leftover != nil:
				arr := leftover
				leftover = nil
				return arr
			case ended:
				return halt(Unit)
			}
			return upstream(fb)
		}, scope)
		if leftover == nil {
			return v, l
		}
		return v, append(append([]Value{}, l...), leftover...)
	})
}

// sinkOrElse runs the sink `f` makes of a typed error on the rest of the
// upstream.
func sinkOrElse(self *Sink, f func(fb *Fiber, err Value) *Sink) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		ended := false
		var l []Value
		v := catchError(fb, func(fb *Fiber) Value {
			v, left := self.transform(fb, upstreamTracked(upstream, &ended), scope)
			l = left
			return v
		}, func(fb *Fiber, err Value) Value {
			v, left := f(fb, err).transform(fb, func(fb *Fiber) Value {
				if ended {
					return halt(Unit)
				}
				return upstream(fb)
			}, scope)
			l = left
			return v
		})
		return v, l
	})
}

// sinkCatch gives, on a failure `pick` takes, the value of `f` of what it
// picked, without leftovers.
func sinkCatch(self *Sink, f func(fb *Fiber, c *Cause) (Value, bool)) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		var l []Value
		v, c := catch(func() Value {
			v, left := self.transform(fb, upstream, scope)
			l = left
			return v
		})
		if c == nil {
			return v, l
		}
		if v, ok := f(fb, c); ok {
			return v, nil
		}
		panic(&Failure{c})
	})
}

// sinkOnExit runs `f` of how the sink ended, its value or its failure,
// uninterruptibly.
func sinkOnExit(self *Sink, f func(fb *Fiber, exit *Exit)) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		var l []Value
		v, c := catch(func() Value {
			v, left := self.transform(fb, upstream, scope)
			l = left
			return v
		})
		_, fc := catch(func() Value {
			return masked(fb, fb.state.mask+1, func() Value { f(fb, &Exit{value: v, cause: c}); return nil })
		})
		if c = finalized(c, fc); c != nil { // as Effect's onExit
			panic(&Failure{c})
		}
		return v, l
	})
}

// sinkSummarized runs `summary` before and after the sink, its value
// paired with `f` of both.
func sinkSummarized(self *Sink, summary func(fb *Fiber) Value, f func(fb *Fiber, start, end Value) Value) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		start := summary(fb)
		v, l := self.transform(fb, upstream, scope)
		end := summary(fb)
		return []Value{v, f(fb, start, end)}, l
	})
}

func sinkWithDuration(self *Sink) *Sink {
	return sinkSummarized(self, func(fb *Fiber) Value { return clockOf(fb).monotonic() },
		func(_ *Fiber, start, end Value) Value {
			return fromNanos(new(big.Int).Sub(end.(*big.Int), start.(*big.Int)))
		})
}

// sinkDrain consumes everything.
var sinkDrain = sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
	catchDone(fb, func(fb *Fiber) Value {
		for {
			Check(fb)
			upstream(fb)
		}
	}, func(*Fiber, Value) Value { return nil })
	return Unit, nil
})

// The bindings: an `End` reaches Flick as a pair of the value and its
// leftovers in an Option.

func flickEnd(v Value, l []Value) Value { return fromSinkEnd(nil, sinkEnd{v, l}) }

func sinkEndOf(end Value) (Value, []Value) {
	e := toSinkEnd(nil, end).(sinkEnd)
	return e.value, e.leftover
}

// SinkMake is the sink of what `f` does with the stream of its input, in
// the sink's scope.
func SinkMake(f Value) Value {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		input := &Stream{channelFromPullOf(upstream)}
		return endOf(Run(Within(fb, scope), Apply(fb, f, input)))
	})
}

func SinkFromEffectEnd(task Value) Value {
	return sinkOfEffect(func(fb *Fiber) (Value, []Value) { return sinkEndOf(Run(fb, task)) })
}

func SinkFromEffect(task Value) Value {
	return sinkOfEffect(func(fb *Fiber) (Value, []Value) { return endOf(Run(fb, task)) })
}

// SinkFromQueue offers each chunk to the queue and ends it at the end.
func SinkFromQueue(q Value) Value {
	x := queueOf(q)
	return sinkFromTransform(func(fb *Fiber, upstream Pull, _ *Owner) (Value, []Value) {
		catchDone(fb, func(fb *Fiber) Value {
			for {
				x.offerAll(fb, upstream(fb).([]Value))
			}
		}, func(*Fiber, Value) Value { return nil })
		x.end()
		return Unit, nil
	})
}

func SinkFromPubSub(hub Value) Value {
	h := pubSubOf(hub)
	return sinkForEachArray(func(fb *Fiber, chunk []Value) { h.publishAll(fb, chunk) })
}

func SinkSucceed(a Value) Value { return sinkOfEffect(func(*Fiber) (Value, []Value) { return a, nil }) }

func SinkSync(f Value) Value {
	return sinkOfEffect(func(fb *Fiber) (Value, []Value) { return endOf(Apply(fb, f, Unit)) })
}

func SinkSuspend(f Value) Value {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		return sinkOf(Apply(fb, f, Unit)).transform(fb, upstream, scope)
	})
}

func SinkFail(err Value) Value {
	return sinkOfEffect(func(*Fiber) (Value, []Value) { return endOf(Fail(err)) })
}

func SinkFailSync(f Value) Value {
	return sinkOfEffect(func(fb *Fiber) (Value, []Value) { return endOf(Fail(Apply(fb, f, Unit))) })
}

func SinkFailCause(failure Value) Value {
	return sinkOfEffect(func(*Fiber) (Value, []Value) { panic(&Failure{causeOfFailure(failure)}) })
}

func SinkFailCauseSync(f Value) Value {
	return sinkOfEffect(func(fb *Fiber) (Value, []Value) { panic(&Failure{causeOfFailure(Apply(fb, f, Unit))}) })
}

var SinkNever Value = sinkOfEffect(func(fb *Fiber) (Value, []Value) { return endOf(EffectNever(fb)) })

func SinkIgnoreLeftover(self Value) Value {
	return sinkMapEnd(sinkOf(self), func(_ *Fiber, v Value, _ []Value) (Value, []Value) { return v, nil })
}

var SinkDrain Value = sinkDrain

func SinkMap(f, self Value) Value {
	return sinkMapEnd(sinkOf(self), func(fb *Fiber, v Value, l []Value) (Value, []Value) { return Apply(fb, f, v), l })
}

func SinkAs(b, self Value) Value {
	return sinkMapEnd(sinkOf(self), func(_ *Fiber, _ Value, l []Value) (Value, []Value) { return b, l })
}

func mapChunk(fb *Fiber, f func(*Fiber, Value) Value, chunk Value) Value {
	xs := chunk.([]Value)
	out := make([]Value, len(xs))
	for i, x := range xs {
		out[i] = f(fb, x)
	}
	return out
}

func SinkMapInput(f, self Value) Value {
	return sinkMapInputArray(sinkOf(self), func(fb *Fiber, chunk Value) Value {
		return mapChunk(fb, func(fb *Fiber, x Value) Value { return Apply(fb, f, x) }, chunk)
	})
}

func SinkMapInputEffect(f, self Value) Value {
	return sinkMapInputArray(sinkOf(self), func(fb *Fiber, chunk Value) Value {
		return mapChunk(fb, func(fb *Fiber, x Value) Value { return Run(fb, Apply(fb, f, x)) }, chunk)
	})
}

func SinkMapInputArray(f, self Value) Value {
	return sinkMapInputArray(sinkOf(self), func(fb *Fiber, chunk Value) Value { return Apply(fb, f, chunk) })
}

func SinkMapInputArrayEffect(f, self Value) Value {
	return sinkMapInputArray(sinkOf(self), func(fb *Fiber, chunk Value) Value { return Run(fb, Apply(fb, f, chunk)) })
}

func SinkMapEnd(f, self Value) Value {
	return sinkMapEnd(sinkOf(self), func(fb *Fiber, v Value, l []Value) (Value, []Value) {
		return sinkEndOf(Apply(fb, f, flickEnd(v, l)))
	})
}

func SinkMapEffectEnd(f, self Value) Value {
	return sinkMapEnd(sinkOf(self), func(fb *Fiber, v Value, l []Value) (Value, []Value) {
		return sinkEndOf(Run(fb, Apply(fb, f, flickEnd(v, l))))
	})
}

func SinkMapEffect(f, self Value) Value {
	return sinkMapEnd(sinkOf(self), func(fb *Fiber, v Value, l []Value) (Value, []Value) {
		return Run(fb, Apply(fb, f, v)), l
	})
}

// SinkMapError maps the first typed error, as Effect's `mapError`.
func SinkMapError(f, self Value) Value {
	s := sinkOf(self)
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		var l []Value
		v := catchError(fb, func(fb *Fiber) Value {
			v, left := s.transform(fb, upstream, scope)
			l = left
			return v
		}, func(fb *Fiber, err Value) Value { return Fail(Apply(fb, f, err)) })
		return v, l
	})
}

func SinkMapLeftover(f, self Value) Value {
	return sinkMapEnd(sinkOf(self), func(fb *Fiber, v Value, l []Value) (Value, []Value) {
		if l == nil {
			return v, nil
		}
		return v, mapChunk(fb, func(fb *Fiber, x Value) Value { return Apply(fb, f, x) }, l).([]Value)
	})
}

func SinkFlatMap(f, self Value) Value {
	return sinkFlatMap(sinkOf(self), func(fb *Fiber, a Value) *Sink { return sinkOf(Apply(fb, f, a)) })
}

func SinkUnwrap(task Value) Value {
	return sinkFromChannel(channelUnwrap(func(fb *Fiber) *Channel { return sinkOf(Run(fb, task)).toChannel() }))
}

func SinkSummarized(summary, f, self Value) Value {
	return sinkSummarized(sinkOf(self), func(fb *Fiber) Value { return Run(fb, summary) },
		func(fb *Fiber, start, end Value) Value { return apply2(fb, f, start, end) })
}

func SinkWithDuration(self Value) Value { return sinkWithDuration(sinkOf(self)) }

var SinkTimed Value = sinkMapEnd(sinkWithDuration(sinkDrain), func(_ *Fiber, v Value, l []Value) (Value, []Value) {
	return v.([]Value)[1], l
})

func SinkProvideService(key, impl, self Value) Value {
	s := sinkOf(self)
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		return s.transform(provided(fb, Services{key.(string): impl}), upstream, scope)
	})
}

func SinkOrElse(f, self Value) Value {
	return sinkOrElse(sinkOf(self), func(fb *Fiber, err Value) *Sink { return sinkOf(Apply(fb, f, err)) })
}

func SinkCatch(f, self Value) Value {
	return sinkCatch(sinkOf(self), func(fb *Fiber, c *Cause) (Value, bool) {
		r := c.firstOf(causeFail)
		if r == nil {
			return nil, false
		}
		return Run(fb, Apply(fb, f, r.err)), true
	})
}

func SinkCatchCause(f, self Value) Value {
	return sinkCatch(sinkOf(self), func(fb *Fiber, c *Cause) (Value, bool) {
		return Run(fb, Apply(fb, f, failureOf(c))), true
	})
}

func SinkOnExit(f, self Value) Value {
	return sinkOnExit(sinkOf(self), func(fb *Fiber, exit *Exit) { Run(fb, Apply(fb, f, exit)) })
}

func SinkEnsuring(task, self Value) Value {
	return sinkOnExit(sinkOf(self), func(fb *Fiber, _ *Exit) { Run(fb, task) })
}
