package flickrt

// Running channels, as Effect's `runWith` (Channel.ts:10537): the channel
// runs in a scope of its own, closed with how the run ended, in the fiber
// of the run, whose owner stays the lexical one.

// runWith gives the channel's pull to `f`, which pulls until it ends; the
// done value goes to `onHalt` (nil: it is the result).
func runWith(fb *Fiber, self *Channel, f func(fb *Fiber, pull Pull), onHalt func(fb *Fiber, d Value) Value) Value {
	scope := newOwner()
	out, cause := catch(func() Value {
		pull := self.transform(fb, haltPull, scope)
		return catchDone(fb, func(fb *Fiber) Value { f(fb, pull); return Unit }, func(fb *Fiber, d Value) Value {
			if onHalt == nil {
				return d
			}
			return onHalt(fb, d)
		})
	})
	if cause = scope.close(fb, cause); cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// runFold folds the elements into the state; pure, so it checks for an
// interruption at each one.
func runFold(fb *Fiber, self *Channel, initial Value, f func(fb *Fiber, z, o Value) Value) Value {
	state := initial
	return runWith(fb, self, func(fb *Fiber, pull Pull) {
		for {
			Check(fb)
			state = f(fb, state, pull(fb))
		}
	}, func(*Fiber, Value) Value { return state })
}

// runForEach runs `f` of each element; the done value is the result.
func runForEach(fb *Fiber, self *Channel, f func(fb *Fiber, o Value)) Value {
	return runWith(fb, self, func(fb *Fiber, pull Pull) {
		for {
			f(fb, pull(fb))
		}
	}, nil)
}

func runDrain(fb *Fiber, self *Channel) Value {
	return runWith(fb, self, func(fb *Fiber, pull Pull) {
		for {
			Check(fb)
			pull(fb)
		}
	}, nil)
}

func runCollect(fb *Fiber, self *Channel) []Value {
	return runFold(fb, self, []Value{}, func(_ *Fiber, z, o Value) Value { return append(z.([]Value), o) }).([]Value)
}

func ChannelRunFold(fb *Fiber, initial, f, self Value) Value {
	return runFold(fb, channelOf(self), Apply(fb, initial, Unit), func(fb *Fiber, z, o Value) Value {
		return Apply(fb, Apply(fb, f, z), o)
	})
}

func ChannelRunForEach(fb *Fiber, f, self Value) Value {
	return runForEach(fb, channelOf(self), func(fb *Fiber, o Value) { Run(fb, Apply(fb, f, o)) })
}

func ChannelRunDrain(fb *Fiber, self Value) Value { return runDrain(fb, channelOf(self)) }

func ChannelRunCollect(fb *Fiber, self Value) Value { return runCollect(fb, channelOf(self)) }
