package flickrt

// How a stream fails and what runs around it, as Effect's (Stream.ts):
// recovery, taps, finalizers, retries, services and spans, and their
// bindings. A cause reaches Flick as a `Failure`, a defect as a `Failure`
// of the defect alone, as the JS adapter gives them.

// findDefect is Effect's `Cause.findDefect`: the first defect of a cause.
func findDefect(c *Cause) *Cause {
	for _, r := range c.reasons() {
		if r.kind == causeDie {
			return r
		}
	}
	return nil
}

func StreamCatch(f, s Value) Value {
	return streamCatchError(streamOf(s), func(fb *Fiber, err Value) *Stream { return streamOf(Apply(fb, f, err)) })
}

func StreamCatchIf(p, f, s Value) Value {
	return streamCatchError(streamOf(s), func(fb *Fiber, err Value) *Stream {
		if Apply(fb, p, err).(bool) {
			return streamOf(Apply(fb, f, err))
		}
		return streamFail(err)
	})
}

func StreamCatchFilter(filter, f, s Value) Value {
	return streamCatchError(streamOf(s), func(fb *Fiber, err Value) *Stream {
		r := Apply(fb, filter, err)
		if !isOk(r) {
			return streamFail(value(r))
		}
		return streamOf(Apply(fb, f, value(r)))
	})
}

func StreamCatchCause(f, s Value) Value {
	return streamCatchCause(streamOf(s), func(fb *Fiber, c *Cause) *Stream {
		return streamOf(Apply(fb, f, failureOf(c)))
	})
}

func StreamCatchCauseIf(p, f, s Value) Value {
	return streamCatchCause(streamOf(s), func(fb *Fiber, c *Cause) *Stream {
		if failure := failureOf(c); Apply(fb, p, failure).(bool) {
			return streamOf(Apply(fb, f, failure))
		}
		return streamFailCause(c)
	})
}

func StreamCatchCauseFilter(filter, f, s Value) Value {
	return streamCatchCause(streamOf(s), func(fb *Fiber, c *Cause) *Stream {
		failure := failureOf(c)
		r := Apply(fb, filter, failure)
		if !isOk(r) {
			return streamFailCause(causeOfFailure(value(r)))
		}
		return streamOf(apply2(fb, f, value(r), failure))
	})
}

func StreamCatchDefect(f, s Value) Value {
	return streamCatchCause(streamOf(s), func(fb *Fiber, c *Cause) *Stream {
		if d := findDefect(c); d != nil {
			return streamOf(Apply(fb, f, failureOf(d)))
		}
		return streamFailCause(c)
	})
}

func StreamTapError(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelTapError(f, c)) })
}

func StreamTapCause(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelTapCause(f, c)) })
}

func StreamTapDefect(f, s Value) Value {
	return onChannel(s, func(self *Channel) *Channel {
		return channelTapCause(self, func(fb *Fiber, c *Cause) {
			if d := findDefect(c); d != nil {
				Run(fb, Apply(fb, f, failureOf(d)))
			}
		})
	})
}

func StreamOrElseIfEmpty(fallback, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel {
		return channelOrElseIfEmpty(c, func(*Fiber, Value) *Channel { return streamOf(fallback).ch })
	})
}

func StreamOrElseSucceed(f, s Value) Value {
	return streamCatchError(streamOf(s), func(fb *Fiber, err Value) *Stream {
		return chunkStream([]Value{Apply(fb, f, err)})
	})
}

// StreamOrDie is Effect's `catch(self, die)`: the first typed error a
// defect.
func StreamOrDie(s Value) Value {
	return streamCatchError(streamOf(s), func(_ *Fiber, err Value) *Stream {
		return streamFailCause(dieOf(err))
	})
}

func StreamIgnore(s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelIgnore(c)) })
}

func StreamIgnoreCause(s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelIgnoreCause(c)) })
}

func StreamRetry(sched, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelRetry(c, scheduleOf(sched)) })
}

func StreamOnExit(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelOnExit(f, c)) })
}

func StreamOnError(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelOnError(f, c)) })
}

func StreamOnStart(task, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelOnStart(task, c)) })
}

func StreamOnFirst(f, s Value) Value {
	return onChannel(s, func(self *Channel) *Channel {
		return channelOnFirst(self, func(fb *Fiber, chunk Value) { Run(fb, Apply(fb, f, chunk.([]Value)[0])) })
	})
}

func StreamOnEnd(task, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelOnEnd(task, c)) })
}

func StreamEnsuring(task, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelEnsuring(task, c)) })
}

func StreamProvide(layer, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelProvideLayer(c, layerOf(layer)) })
}

func StreamUpdateService(key, f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelUpdateService(key, f, c)) })
}

// StreamWithSpan opens the span with the options Effect defaults to.
func StreamWithSpan(name, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel {
		return channelWithSpan(c, name, Rec("parent", None, "root", false))
	})
}
