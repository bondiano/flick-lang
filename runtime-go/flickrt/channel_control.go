package flickrt

// How a channel fails, ends, repeats and runs beyond the core, as
// Effect's (Channel.ts): recovery, finalizers, retries, services and
// spans.

// channelCatchError is Effect's `catch`: the channel `f` makes of the
// first typed error goes on; another failure fails as it is.
func channelCatchError(self *Channel, f func(fb *Fiber, err Value) *Channel) *Channel {
	return channelCatchCause(self, func(fb *Fiber, c *Cause) *Channel {
		if r := c.firstOf(causeFail); r != nil {
			return f(fb, r.err)
		}
		return channelFailCause(c)
	})
}

// channelTapError runs `f` of the first typed error that is no end, then
// fails as before.
func channelTapError(self *Channel, f func(fb *Fiber, err Value)) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value {
			v, c := catch(func() Value { return pull(fb) })
			if c == nil {
				return v
			}
			if r := c.firstOf(causeFail); r != nil && !isDoneReason(r) {
				f(fb, r.err)
			}
			panic(&Failure{c})
		}
	})
}

// channelTapCause runs `f` of a failure that is no end, then fails with it.
func channelTapCause(self *Channel, f func(fb *Fiber, c *Cause)) *Channel {
	return channelCatchCause(self, func(_ *Fiber, c *Cause) *Channel {
		return fromPull(func(fb *Fiber) Pull {
			f(fb, c)
			panic(&Failure{c})
		})
	})
}

// channelOnFirst runs `f` of the first element.
func channelOnFirst(self *Channel, f func(fb *Fiber, o Value)) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		first := true
		return func(fb *Fiber) Value {
			o := pull(fb)
			if first {
				first = false
				f(fb, o)
			}
			return o
		}
	})
}

// channelOnEnd runs `task` when the channel ends without failing.
func channelOnEnd(self *Channel, task func(fb *Fiber)) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value {
			return catchDone(fb, pull, func(fb *Fiber, d Value) Value {
				task(fb)
				return halt(d)
			})
		}
	})
}

// channelHaltWhen ends the channel, between its elements, once the task
// forked beside it has ended, as it ended.
func channelHaltWhen(self *Channel, task func(fb *Fiber) Value) *Channel {
	return fromTransformBracket(func(fb *Fiber, upstream Pull, scope, forked *Owner) Pull {
		pull := self.transform(fb, upstream, scope)
		c := forkIn(fb, forked, task)
		return func(fb *Fiber) Value {
			if !isClosed(c.done) {
				return pull(fb)
			}
			if c.cause != nil {
				panic(&Failure{c.cause})
			}
			return halt(c.value)
		}
	})
}

// channelRetry runs the channel again after a typed error while the
// schedule, given the error, goes on; an element restarts the schedule.
func channelRetry(self *Channel, s *Schedule) *Channel {
	return channelSuspend(func(*Fiber) *Channel {
		var step func(fb *Fiber, input Value) (metadata, bool)
		meta := defaultMetadata
		withMeta := channelProvideServiceEffect(self, scheduleMetadataKey, func(*Fiber) Value { return meta.record() })
		withReset := channelOnFirst(withMeta, func(*Fiber, Value) { step = nil })
		var loop *Channel
		loop = channelCatchError(withReset, func(_ *Fiber, err Value) *Channel {
			return channelUnwrap(func(fb *Fiber) *Channel {
				if step == nil {
					step = s.stepper(fb)
				}
				m, done := step(fb, err)
				if done {
					return channelFailCause(&Cause{kind: causeFail, err: err})
				}
				meta = m
				return loop
			})
		})
		return loop
	})
}

// channelProvideServices builds and pulls the channel with the services.
func channelProvideServices(self *Channel, s Services) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		pull := self.transform(provided(fb, s), upstream, scope)
		return func(fb *Fiber) Value { return pull(provided(fb, s)) }
	})
}

// channelProvideServiceEffect provides the service `make` gives each time
// the channel runs.
func channelProvideServiceEffect(self *Channel, key string, make func(fb *Fiber) Value) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		return channelProvideServices(self, Services{key: make(fb)}).transform(fb, upstream, scope)
	})
}

// channelProvideLayer builds the layer into the channel's scope each time
// it runs.
func channelProvideLayer(self *Channel, l *Layer) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		s := buildWith(fb, l, currentMemo(fb), scope)
		return channelProvideServices(self, s).transform(fb, upstream, scope)
	})
}

// channelUpdateService runs the channel with the service of `key` changed
// by `f`.
func channelUpdateService(self *Channel, key string, f func(fb *Fiber, impl Value) Value) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		impl, ok := fb.env.get(key)
		if !ok {
			panic(Defect{"Service not found: " + key})
		}
		return channelProvideServices(self, Services{key: f(fb, impl)}).transform(fb, upstream, scope)
	})
}

// channelWithSpan runs the channel in a span of the name, the parent of
// the spans it opens.
//
// shortcut: Go's spans are not exported, so ending one records nothing; end it with its exit once the go target has a tracer
func channelWithSpan(self *Channel, name, options Value) *Channel {
	return channelAcquireUseRelease(
		func(fb *Fiber) Value { return openSpan(fb, name, options) },
		func(_ *Fiber, s Value) *Channel { return channelProvideServices(self, Services{parentSpanKey: s}) },
		func(*Fiber, Value, *Exit) Value { return Unit },
	)
}

// The bindings: Flick callbacks are curried, an effectful one gives a
// task, and a cause reaches Flick as a `Failure`.

func ChannelCatch(f, self Value) Value {
	return channelCatchError(channelOf(self), func(fb *Fiber, err Value) *Channel { return channelOf(Apply(fb, f, err)) })
}

func ChannelTapError(f, self Value) Value {
	return channelTapError(channelOf(self), func(fb *Fiber, err Value) { Run(fb, Apply(fb, f, err)) })
}

func ChannelTapCause(f, self Value) Value {
	return channelTapCause(channelOf(self), func(fb *Fiber, c *Cause) { Run(fb, Apply(fb, f, failureOf(c))) })
}

func ChannelMapError(f, self Value) Value {
	return channelCatchError(channelOf(self), func(fb *Fiber, err Value) *Channel {
		return channelFailCause(&Cause{kind: causeFail, err: Apply(fb, f, err)})
	})
}

func ChannelIgnore(self Value) Value {
	return channelCatchError(channelOf(self), func(*Fiber, Value) *Channel { return channelEmpty })
}

func ChannelIgnoreCause(self Value) Value {
	return channelCatchCause(channelOf(self), func(*Fiber, *Cause) *Channel { return channelEmpty })
}

func ChannelEnsuring(task, self Value) Value {
	return channelOnExit(channelOf(self), func(fb *Fiber, _ *Exit) Value { return Run(fb, task) })
}

func ChannelOnError(f, self Value) Value {
	return channelOnExit(channelOf(self), func(fb *Fiber, exit *Exit) Value {
		if exit.cause == nil {
			return Unit
		}
		return Run(fb, Apply(fb, f, failureOf(exit.cause)))
	})
}

func ChannelOnStart(task, self Value) Value {
	return channelUnwrap(func(fb *Fiber) *Channel {
		Run(fb, task)
		return channelOf(self)
	})
}

func ChannelOnFirst(f, self Value) Value {
	return channelOnFirst(channelOf(self), func(fb *Fiber, o Value) { Run(fb, Apply(fb, f, o)) })
}

func ChannelOnEnd(task, self Value) Value {
	return channelOnEnd(channelOf(self), func(fb *Fiber) { Run(fb, task) })
}

func ChannelHaltWhen(task, self Value) Value {
	return channelHaltWhen(channelOf(self), func(fb *Fiber) Value { return Run(fb, task) })
}

func ChannelMergeEffect(task, self Value) Value {
	return channelMerge(channelOf(self), channelOf(ChannelFromEffectDrain(task)), "left")
}

func ChannelRepeat(s, self Value) Value { return channelRepeat(channelOf(self), scheduleOf(s)) }
func ChannelRetry(s, self Value) Value  { return channelRetry(channelOf(self), scheduleOf(s)) }

func ChannelSchedule(s, self Value) Value { return channelSchedule(channelOf(self), scheduleOf(s)) }

func ChannelProvide(layer, self Value) Value {
	return channelProvideLayer(channelOf(self), layerOf(layer))
}

func ChannelProvideService(key, impl, self Value) Value {
	return channelProvideServices(channelOf(self), Services{key.(string): impl})
}

func ChannelProvideServiceEffect(key, task, self Value) Value {
	return channelProvideServiceEffect(channelOf(self), key.(string), func(fb *Fiber) Value { return Run(fb, task) })
}

func ChannelUpdateService(key, f, self Value) Value {
	return channelUpdateService(channelOf(self), key.(string), func(fb *Fiber, impl Value) Value { return Apply(fb, f, impl) })
}

func ChannelWithSpan(name, options, self Value) Value {
	return channelWithSpan(channelOf(self), name, options)
}
