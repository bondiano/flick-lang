package flickrt

// The bindings of Channel's transformations: Flick callbacks are curried,
// an effectful one gives a task, a filter gives a `Result`, and bytes are
// lists of numbers.

func ChannelTap(f, self Value) Value {
	return channelMap(channelOf(self), func(fb *Fiber, o Value) Value {
		Run(fb, Apply(fb, f, o))
		return o
	})
}

func ChannelMapDone(f, self Value) Value {
	return channelMapDone(channelOf(self), func(fb *Fiber, d Value) Value { return Apply(fb, f, d) })
}

func ChannelMapDoneEffect(f, self Value) Value {
	return channelMapDone(channelOf(self), func(fb *Fiber, d Value) Value { return Run(fb, Apply(fb, f, d)) })
}

func ChannelMapInput(f, self Value) Value {
	return channelMapInput(channelOf(self), func(fb *Fiber, i Value) Value { return Run(fb, Apply(fb, f, i)) })
}

func ChannelMapInputError(f, self Value) Value {
	return channelMapInputError(channelOf(self), func(fb *Fiber, err Value) Value { return Run(fb, Apply(fb, f, err)) })
}

// ChannelSwitchMap is Effect's: the channels of the elements merged one at
// a time, a new one interrupting the running one.
func ChannelSwitchMap(f, self Value) Value {
	return channelMergeAll(channelOf(ChannelMap(f, self)), 1, 16, true)
}

func ChannelFlatten(self Value) Value {
	return channelFlatMap(channelOf(self), func(_ *Fiber, o Value) *Channel { return channelOf(o) })
}

func ChannelConcat(that, self Value) Value {
	return channelConcatWith(channelOf(self), func(*Fiber, Value) *Channel { return channelOf(that) })
}

func ChannelOrElseIfEmpty(f, self Value) Value {
	return channelOrElseIfEmpty(channelOf(self), func(fb *Fiber, d Value) *Channel { return channelOf(Apply(fb, f, d)) })
}

func ChannelDrain(self Value) Value { return channelDrain(channelOf(self)) }

func ChannelForever(self Value) Value { return repeatLoop(channelOf(self), func(*Fiber, Value) {}) }

// passed is what a filter's `Result` passes.
func passed(r Value) (Value, bool) {
	if isOk(r) {
		return value(r), true
	}
	return nil, false
}

func predicate(f Value) func(fb *Fiber, o Value) (Value, bool) {
	return func(fb *Fiber, o Value) (Value, bool) { return o, Apply(fb, f, o).(bool) }
}

func predicateEffect(f Value) func(fb *Fiber, o Value) (Value, bool) {
	return func(fb *Fiber, o Value) (Value, bool) { return o, Run(fb, Apply(fb, f, o)).(bool) }
}

func filterOf(f Value) func(fb *Fiber, o Value) (Value, bool) {
	return func(fb *Fiber, o Value) (Value, bool) { return passed(Apply(fb, f, o)) }
}

func filterEffectOf(f Value) func(fb *Fiber, o Value) (Value, bool) {
	return func(fb *Fiber, o Value) (Value, bool) { return passed(Run(fb, Apply(fb, f, o))) }
}

func ChannelFilter(f, self Value) Value { return channelFilterMap(channelOf(self), predicate(f)) }

func ChannelFilterMap(f, self Value) Value { return channelFilterMap(channelOf(self), filterOf(f)) }

func ChannelFilterEffect(f, self Value) Value {
	return channelFilterMap(channelOf(self), predicateEffect(f))
}

func ChannelFilterMapEffect(f, self Value) Value {
	return channelFilterMap(channelOf(self), filterEffectOf(f))
}

func ChannelFilterArray(f, self Value) Value {
	return channelFilterMapArray(channelOf(self), predicate(f))
}

func ChannelFilterMapArray(f, self Value) Value {
	return channelFilterMapArray(channelOf(self), filterOf(f))
}

func ChannelFilterArrayEffect(f, self Value) Value {
	return channelFilterMapArray(channelOf(self), predicateEffect(f))
}

func ChannelFilterMapArrayEffect(f, self Value) Value {
	return channelFilterMapArray(channelOf(self), filterEffectOf(f))
}

func ChannelScan(initial, f, self Value) Value {
	return channelScan(channelOf(self), initial, func(fb *Fiber, s, o Value) Value { return apply2(fb, f, s, o) })
}

func ChannelScanEffect(initial, f, self Value) Value {
	return channelScan(channelOf(self), initial, func(fb *Fiber, s, o Value) Value { return Run(fb, apply2(fb, f, s, o)) })
}

func ChannelBuffer(options, self Value) Value {
	o := record(options)
	q := queueOptions(o.Get("capacity"), o.Get("strategy"))
	return channelBuffer(channelOf(self), q, func(fb *Fiber, q *Queue, x Value) { q.offer(fb, x) }, q.take)
}

func ChannelBufferArray(options, self Value) Value {
	o := record(options)
	q := queueOptions(o.Get("capacity"), o.Get("strategy"))
	return channelBuffer(channelOf(self), q, func(fb *Fiber, q *Queue, xs Value) { q.offerAll(fb, xs.([]Value)) }, q.takeAll)
}

func ChannelSplitLines() Value { return channelSplitLines() }

func ChannelDecodeText(encoding Value) Value { return channelDecodeText(encoding.(string)) }

func ChannelEncodeText() Value { return channelEncodeText }
