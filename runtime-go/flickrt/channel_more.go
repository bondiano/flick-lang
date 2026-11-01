package flickrt

// Channel's constructors beyond the core, as Effect's (Channel.ts): from
// effects, queues, hubs and schedules, and their bindings.

// channelFromPullOf is the channel whose pull is `p`, as Effect's
// `fromPull(Effect.succeed(p))`.
func channelFromPullOf(p Pull) *Channel { return fromPull(func(*Fiber) Pull { return p }) }

var channelNever = channelFromPullOf(func(fb *Fiber) Value { return EffectNever(fb) })

// channelFromSubscription is Effect's: the take of the subscription, an
// interruption of which ends the channel.
func channelFromSubscription(s *Subscription) *Channel {
	return channelFromPullOf(func(fb *Fiber) Value {
		return onInterruptDone(fb, func(fb *Fiber) Value {
			s.hub.mu.Lock()
			return s.take(fb)
		})
	})
}

func channelFromSubscriptionArray(s *Subscription) *Channel {
	return channelFromPullOf(func(fb *Fiber) Value {
		return onInterruptDone(fb, func(fb *Fiber) Value { return s.takeAll(fb) })
	})
}

// onInterruptDone is `Effect.onInterrupt(f, () => Cause.done())`.
func onInterruptDone(fb *Fiber, f func(fb *Fiber) Value) Value {
	v, c := catch(func() Value { return f(fb) })
	if c != nil {
		if c.has(causeInterrupt) {
			c = Sequential(c, doneCause(Unit))
		}
		panic(&Failure{c})
	}
	return v
}

// channelFromPubSub subscribes when first pulled, in the channel's scope.
func channelFromPubSub(h *PubSub, array bool) *Channel {
	return channelUnwrap(func(fb *Fiber) *Channel {
		if array {
			return channelFromSubscriptionArray(h.subscribe(fb))
		}
		return channelFromSubscription(h.subscribe(fb))
	})
}

// channelFromSchedule emits each output of the schedule after its delay
// and ends with the last.
func channelFromSchedule(s *Schedule) *Channel {
	return fromPull(func(fb *Fiber) Pull {
		step := s.stepper(fb)
		return func(fb *Fiber) Value {
			m, done := step(fb, Unit)
			if done {
				return halt(m.output)
			}
			return m.output
		}
	})
}

// channelCallback is Effect's `callback`: `f` runs in a fiber of the
// channel's scope with a queue the channel takes from; its failure fails
// the queue.
func channelCallback(q *Queue, f func(fb *Fiber, q *Queue), take Pull) *Channel {
	return fromTransform(func(fb *Fiber, _ Pull, scope *Owner) Pull {
		scope.add(fb, func(*Fiber, *Cause) Value { q.shutdown(); return Unit })
		inner := Within(fb, scope)
		forkIn(inner, scope, func(fb *Fiber) Value {
			if _, c := catch(func() Value { f(fb, q); return nil }); c != nil {
				q.failCause(c)
			}
			return Unit
		})
		return take
	})
}

// channelAcquireUseRelease releases what `acquire` gave, uninterruptibly
// acquired, with how the channel `use` makes of it ended.
func channelAcquireUseRelease(acquire func(fb *Fiber) Value, use func(fb *Fiber, a Value) *Channel, release func(fb *Fiber, a Value, exit *Exit) Value) *Channel {
	return fromTransformBracket(func(fb *Fiber, upstream Pull, scope, forked *Owner) Pull {
		var acquired, ended Value
		has := false
		forked.add(fb, func(fb *Fiber, exit *Cause) Value {
			if !has {
				return Unit
			}
			if exit != nil {
				return release(fb, acquired, &Exit{cause: exit})
			}
			return release(fb, acquired, &Exit{value: ended})
		})
		value := masked(fb, fb.state.mask+1, func() Value {
			acquired = acquire(fb)
			has = true
			return acquired
		})
		pull := use(fb, value).transform(fb, upstream, scope)
		return func(fb *Fiber) Value {
			return catchDone(fb, pull, func(_ *Fiber, d Value) Value {
				ended = d
				return halt(d)
			})
		}
	})
}

// The bindings.

var ChannelDefaultChunkSize Value = int32(streamChunkSize)

func ChannelFromChunk(c Value) Value { return channelFromArray(c.(*Chunk).items) }

func ChannelSync(f Value) Value {
	return channelFromEffect(func(fb *Fiber) Value { return Apply(fb, f, Unit) })
}

func ChannelEndSync(f Value) Value {
	return fromPull(func(fb *Fiber) Pull {
		d := Apply(fb, f, Unit)
		return func(*Fiber) Value { return halt(d) }
	})
}

var ChannelNever Value = channelNever

func ChannelFailSync(f Value) Value {
	return fromPull(func(fb *Fiber) Pull { return Fail(Apply(fb, f, Unit)).(Pull) })
}

func ChannelFailCauseSync(f Value) Value {
	return fromPull(func(fb *Fiber) Pull { panic(&Failure{causeOfFailure(Apply(fb, f, Unit))}) })
}

func ChannelFromEffect(task Value) Value {
	return channelFromEffect(func(fb *Fiber) Value { return Run(fb, task) })
}

func ChannelFromEffectDone(task Value) Value {
	return channelFromPullOf(func(fb *Fiber) Value { return halt(Run(fb, task)) })
}

func ChannelFromEffectDrain(task Value) Value {
	return fromPull(func(fb *Fiber) Pull {
		Run(fb, task)
		return haltPull
	})
}

func ChannelFromQueue(q Value) Value      { return channelFromPullOf(queueOf(q).take) }
func ChannelFromQueueArray(q Value) Value { return channelFromPullOf(queueOf(q).takeAll) }

func ChannelFromSubscription(sub Value) Value { return channelFromSubscription(subscriptionOf(sub)) }

func ChannelFromSubscriptionArray(sub Value) Value {
	return channelFromSubscriptionArray(subscriptionOf(sub))
}

func ChannelFromPubSub(hub Value) Value      { return channelFromPubSub(pubSubOf(hub), false) }
func ChannelFromPubSubArray(hub Value) Value { return channelFromPubSub(pubSubOf(hub), true) }

func ChannelFromSchedule(s Value) Value { return channelFromSchedule(scheduleOf(s)) }

// callbackOf is `Channel.callback` of the options' queue, `f` a Flick
// function of the queue to a task.
func callbackOf(options, f Value, array bool) Value {
	o := record(options)
	q := queueOptions(o.Get("bufferSize"), o.Get("strategy"))
	take := Pull(q.take)
	if array {
		take = q.takeAll
	}
	return channelCallback(q, func(fb *Fiber, q *Queue) { Run(fb, Apply(fb, f, q)) }, take)
}

func ChannelCallback(options, f Value) Value      { return callbackOf(options, f, false) }
func ChannelCallbackArray(options, f Value) Value { return callbackOf(options, f, true) }

// ChannelAcquireUseRelease gives the release a Flick `Exit`.
func ChannelAcquireUseRelease(acquire, use, release Value) Value {
	return channelAcquireUseRelease(
		func(fb *Fiber) Value { return Run(fb, acquire) },
		func(fb *Fiber, a Value) *Channel { return channelOf(Apply(fb, use, a)) },
		func(fb *Fiber, a Value, exit *Exit) Value { return Run(fb, apply2(fb, release, a, exit)) },
	)
}
