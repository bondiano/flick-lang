package flickrt

// Channel is Effect's (Channel.ts): given the pull of its upstream and the
// scope it runs in, the pull of what it emits. Building the pull runs in
// the fiber that runs the channel, whose owner stays the lexical one, as a
// Flick acquisition belongs to the scope its region names; a failure while
// building is a pull that fails.
type Channel struct {
	transform func(fb *Fiber, upstream Pull, scope *Owner) Pull
}

func channelOf(v Value) *Channel { return v.(*Channel) }

func fromTransform(transform func(fb *Fiber, upstream Pull, scope *Owner) Pull) *Channel {
	return &Channel{transform: func(fb *Fiber, upstream Pull, scope *Owner) (p Pull) {
		if _, c := catch(func() Value { p = transform(fb, upstream, scope); return nil }); c != nil {
			return failPull(c)
		}
		return p
	}}
}

// transformPull is the channel of `f` of the pull of `self`.
func transformPull(self *Channel, f func(fb *Fiber, pull Pull, scope *Owner) Pull) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		return f(fb, self.transform(fb, upstream, scope), scope)
	})
}

// fromPull is the channel of the pull `f` builds, without upstream.
func fromPull(f func(fb *Fiber) Pull) *Channel {
	return fromTransform(func(fb *Fiber, _ Pull, _ *Owner) Pull { return f(fb) })
}

// fromTransformBracket gives `f` a scope of its own, `forked`, closed with
// how the pull ended once it ends or fails.
func fromTransformBracket(f func(fb *Fiber, upstream Pull, scope, forked *Owner) Pull) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		forked := forkScope(fb, scope)
		onError := func(fb *Fiber, c *Cause) *Failure {
			_, closed := catch(func() Value { closeScope(fb, forked, doneExit(c).cause); return nil })
			return &Failure{finalized(c, closed)} // as Effect's onError
		}
		var p Pull
		if _, c := catch(func() Value { p = f(fb, upstream, scope, forked); return nil }); c != nil {
			panic(onError(fb, c))
		}
		return func(fb *Fiber) Value {
			v, c := catch(func() Value { return p(fb) })
			if c != nil {
				panic(onError(fb, c))
			}
			return v
		}
	})
}

// toPullScoped is the channel's pull, run in `scope`.
func toPullScoped(fb *Fiber, self *Channel, scope *Owner) Pull {
	return self.transform(fb, haltPull, scope)
}

// toPull is the channel's pull in the owner of `fb`, with its services,
// shared by fibers one at a time (Channel.ts:11886).
func toPull(fb *Fiber, self *Channel) Pull {
	env, owner := fb.env, fb.owner
	p := serial(toPullScoped(fb, self, owner))
	return func(fb *Fiber) Value {
		inner := *fb
		inner.env, inner.owner = env, owner
		return p(&inner)
	}
}

// channelFromArray emits the elements one at a time.
func channelFromArray(xs []Value) *Channel {
	return fromPull(func(*Fiber) Pull {
		i := 0
		return func(*Fiber) Value {
			if i >= len(xs) {
				return halt(Unit)
			}
			i++
			return xs[i-1]
		}
	})
}

// channelFromIterableArray emits the elements in chunks of `size`.
func channelFromIterableArray(xs []Value, size int) *Channel {
	size = max(1, size)
	return fromPull(func(*Fiber) Pull {
		i := 0
		return func(*Fiber) Value {
			if i >= len(xs) {
				return halt(Unit)
			}
			end := min(i+size, len(xs))
			chunk := xs[i:end:end]
			i = end
			return chunk
		}
	})
}

// channelFromEffect emits what `task` gives, once.
func channelFromEffect(task func(fb *Fiber) Value) *Channel {
	return fromPull(func(*Fiber) Pull {
		ran := false
		return func(fb *Fiber) Value {
			if ran {
				return halt(Unit)
			}
			ran = true
			return task(fb)
		}
	})
}

func channelSucceed(x Value) *Channel {
	return channelFromEffect(func(*Fiber) Value { return x })
}

func channelEnd(d Value) *Channel {
	return fromPull(func(*Fiber) Pull { return func(*Fiber) Value { return halt(d) } })
}

var channelEmpty = channelEnd(Unit)

func channelFailCause(c *Cause) *Channel {
	return fromPull(func(*Fiber) Pull { return failPull(c) })
}

// channelSuspend is the channel `evaluate` makes each time it runs.
func channelSuspend(evaluate func(fb *Fiber) *Channel) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		return evaluate(fb).transform(fb, upstream, scope)
	})
}

// channelUnwrap is the channel `task` makes when first pulled, acquiring
// into the channel's scope.
func channelUnwrap(task func(fb *Fiber) *Channel) *Channel {
	return fromTransform(func(_ *Fiber, upstream Pull, scope *Owner) Pull {
		var p Pull
		return func(fb *Fiber) Value {
			if p == nil {
				ch := task(Within(fb, scope))
				p = ch.transform(fb, upstream, scope)
			}
			return p(fb)
		}
	})
}

// channelIdentity passes its upstream through.
var channelIdentity = fromTransform(func(_ *Fiber, upstream Pull, _ *Owner) Pull { return upstream })

// embedInput runs `input` of the upstream beside the channel, which then
// reads none.
func embedInput(self *Channel, input func(fb *Fiber, upstream Pull)) *Channel {
	return fromTransformBracket(func(fb *Fiber, upstream Pull, scope, forked *Owner) Pull {
		forkIn(fb, forked, func(fb *Fiber) Value { input(fb, upstream); return Unit })
		return self.transform(fb, haltPull, scope)
	})
}

// The bindings of Channel's constructors.

func ChannelFromArray(xs Value) Value { return channelFromArray(List(xs)) }

func ChannelFromListChunked(size, xs Value) Value {
	return channelFromIterableArray(List(xs), int(size.(int32)))
}

func ChannelSucceed(x Value) Value { return channelSucceed(x) }
func ChannelEnd(d Value) Value     { return channelEnd(d) }

var ChannelEmpty Value = channelEmpty

func ChannelFail(err Value) Value { return channelFailCause(&Cause{kind: causeFail, err: err}) }

func ChannelFailCause(failure Value) Value { return channelFailCause(causeOfFailure(failure)) }

func ChannelSuspend(f Value) Value {
	return channelSuspend(func(fb *Fiber) *Channel { return channelOf(Apply(fb, f, Unit)) })
}

func ChannelUnwrap(task Value) Value {
	return channelUnwrap(func(fb *Fiber) *Channel { return channelOf(Run(fb, task)) })
}

func ChannelIdentity() Value { return channelIdentity }

// ChannelAcquireRelease emits the value `acquire` gives, released with the
// channel's scope.
func ChannelAcquireRelease(release, acquire Value) Value {
	return channelUnwrap(func(fb *Fiber) *Channel { return channelSucceed(AcquireRelease(fb, acquire, release)) })
}
