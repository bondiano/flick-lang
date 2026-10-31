package flickrt

// Channel's transformations, as Effect's (Channel.ts), and their bindings.

func channelMap(self *Channel, f func(fb *Fiber, o Value) Value) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value { return f(fb, pull(fb)) }
	})
}

// channelMapDone maps the done value.
func channelMapDone(self *Channel, f func(fb *Fiber, d Value) Value) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value {
			return catchDone(fb, pull, func(fb *Fiber, d Value) Value { return halt(f(fb, d)) })
		}
	})
}

// channelMapAccum emits the values `f` gives of each element with the
// state, which it threads; after a failure, each pull fails as it did.
func channelMapAccum(self *Channel, initial func(fb *Fiber) Value, f func(fb *Fiber, s, o Value) (Value, []Value)) *Channel {
	return transformPull(self, func(fb *Fiber, pull Pull, _ *Owner) Pull {
		state := initial(fb)
		var current []Value
		var cause *Cause
		return func(fb *Fiber) Value {
			for len(current) == 0 {
				if cause != nil {
					panic(&Failure{cause})
				}
				o, c := catch(func() Value { return pull(fb) })
				if c != nil {
					cause = c
					panic(&Failure{c})
				}
				state, current = f(fb, state, o)
			}
			next := current[0]
			current = current[1:]
			return next
		}
	})
}

// channelFlattenArray emits the elements of each list.
func channelFlattenArray(self *Channel) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		var array []Value
		return func(fb *Fiber) Value {
			for len(array) == 0 {
				array = pull(fb).([]Value)
			}
			next := array[0]
			array = array[1:]
			return next
		}
	})
}

// channelFlatMap emits what the channel of each element emits, in turn;
// each runs in a scope forked from the channel's, closed when it ends if
// it acquired anything.
func channelFlatMap(self *Channel, f func(fb *Fiber, o Value) *Channel) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		pull := self.transform(fb, upstream, scope)
		var child Pull
		var childScope *Owner
		return func(fb *Fiber) Value {
			for {
				if child == nil {
					o := pull(fb)
					if childScope == nil {
						childScope = forkScope(fb, scope)
					}
					child = f(fb, o).transform(fb, upstream, childScope)
				}
				v, c := catch(func() Value { return child(fb) })
				if c == nil {
					return v
				}
				if _, rest, ok := filterDone(c); !ok {
					panic(&Failure{rest})
				}
				child = nil
				if !childScope.empty() {
					s := childScope
					childScope = nil
					closeScope(fb, s, nil)
				}
			}
		}
	})
}

// channelConcatWith emits what the channel emits, then what the channel
// `f` makes of its done value emits.
func channelConcatWith(self *Channel, f func(fb *Fiber, d Value) *Channel) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		forked := forkScope(fb, scope)
		var current Pull
		return func(fb *Fiber) Value {
			if current == nil {
				first := self.transform(fb, upstream, forked)
				current = func(fb *Fiber) Value {
					return catchDone(fb, first, func(fb *Fiber, d Value) Value {
						closeScope(fb, forked, nil)
						next := f(fb, d).transform(fb, upstream, scope)
						current = next
						return next(fb)
					})
				}
			}
			return current(fb)
		}
	})
}

// channelPipeTo feeds what `self` emits to `that`.
func channelPipeTo(self, that *Channel) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		return that.transform(fb, self.transform(fb, upstream, scope), scope)
	})
}

// piped is the failure of the channel piped to another one, which crosses
// it as a defect, as Effect's `Cause.Done` of a cause.
type piped struct{ cause *Cause }

// channelPipeToOrFail is channelPipeTo where `that` does not see the
// failures of `self`, which fail the whole as they are.
func channelPipeToOrFail(self, that *Channel) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		up := self.transform(fb, upstream, scope)
		upstreamPull := func(fb *Fiber) Value {
			v, c := catch(func() Value { return up(fb) })
			if c != nil && !isDoneCause(c) {
				panic(&Failure{&Cause{kind: causeDie, defect: "Done", err: &piped{c}}})
			}
			if c != nil {
				panic(&Failure{c})
			}
			return v
		}
		pull := that.transform(fb, upstreamPull, scope)
		return func(fb *Fiber) Value {
			v, c := catch(func() Value { return pull(fb) })
			if c == nil {
				return v
			}
			if d := c.firstOf(causeDie); d != nil {
				if p, ok := d.err.(*piped); ok {
					panic(&Failure{p.cause})
				}
				panic(&Failure{d})
			}
			panic(&Failure{c})
		}
	})
}

// channelOnExit runs `finalizer` of how the channel ended, when its scope
// closes; a done value ends it with a success of that value.
func channelOnExit(self *Channel, finalizer func(fb *Fiber, exit *Exit) Value) *Channel {
	return fromTransformBracket(func(fb *Fiber, upstream Pull, scope, forked *Owner) Pull {
		var ended Value = Unit
		forked.add(fb, func(fb *Fiber, exit *Cause) Value {
			if exit != nil {
				return finalizer(fb, &Exit{cause: exit})
			}
			return finalizer(fb, &Exit{value: ended})
		})
		pull := self.transform(fb, upstream, scope)
		return func(fb *Fiber) Value {
			return catchDone(fb, pull, func(_ *Fiber, d Value) Value {
				ended = d
				return halt(d)
			})
		}
	})
}

// channelCatchCause goes on, after a failure that is no end, with the
// channel `f` makes of its cause; the failed one's scope is closed first.
func channelCatchCause(self *Channel, f func(fb *Fiber, c *Cause) *Channel) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		forked := forkScope(fb, scope)
		pull := self.transform(fb, upstream, forked)
		var current Pull
		current = func(fb *Fiber) Value {
			v, c := catch(func() Value { return pull(fb) })
			if c == nil {
				return v
			}
			if isDoneCause(c) {
				panic(&Failure{c})
			}
			toClose := forked
			forked = forkScope(fb, scope)
			closeScope(fb, toClose, c)
			next := f(fb, c).transform(fb, upstream, forked)
			current = next
			return next(fb)
		}
		return func(fb *Fiber) Value { return current(fb) }
	})
}

// The bindings: Flick callbacks are curried, and a task is run where
// Effect would yield it.

func ChannelMap(f, self Value) Value {
	return channelMap(channelOf(self), func(fb *Fiber, o Value) Value { return Apply(fb, f, o) })
}

func ChannelMapEffect(f, self Value) Value {
	return channelMap(channelOf(self), func(fb *Fiber, o Value) Value { return Run(fb, Apply(fb, f, o)) })
}

func ChannelMapAccum(initial, f, self Value) Value {
	return channelMapAccum(channelOf(self), func(fb *Fiber) Value { return Apply(fb, initial, Unit) },
		func(fb *Fiber, s, o Value) (Value, []Value) {
			pair := List(Run(fb, Apply(fb, Apply(fb, f, s), o)))
			return pair[0], List(pair[1])
		})
}

func ChannelFlattenArray(self Value) Value { return channelFlattenArray(channelOf(self)) }

func ChannelFlatMap(f, self Value) Value {
	return channelFlatMap(channelOf(self), func(fb *Fiber, o Value) *Channel { return channelOf(Apply(fb, f, o)) })
}

func ChannelConcatWith(f, self Value) Value {
	return channelConcatWith(channelOf(self), func(fb *Fiber, d Value) *Channel { return channelOf(Apply(fb, f, d)) })
}

func ChannelPipeTo(that, self Value) Value { return channelPipeTo(channelOf(self), channelOf(that)) }

func ChannelPipeToOrFail(that, self Value) Value {
	return channelPipeToOrFail(channelOf(self), channelOf(that))
}

// ChannelOnExit gives the finalizer a Flick `Exit`.
func ChannelOnExit(f, self Value) Value {
	return channelOnExit(channelOf(self), func(fb *Fiber, exit *Exit) Value { return Run(fb, Apply(fb, f, exit)) })
}

// ChannelCatchCause gives the handler a `Failure`.
func ChannelCatchCause(f, self Value) Value {
	return channelCatchCause(channelOf(self), func(fb *Fiber, c *Cause) *Channel {
		return channelOf(Apply(fb, f, failureOf(c)))
	})
}
