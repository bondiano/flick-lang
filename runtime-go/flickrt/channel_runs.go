package flickrt

// Running channels beyond the core: heads and lasts, effectful folds, and
// running into queues and hubs.

func runHead(fb *Fiber, self *Channel) Value {
	head := None
	return runWith(fb, self, func(fb *Fiber, pull Pull) {
		head = Some(pull(fb))
		halt(Unit)
	}, func(*Fiber, Value) Value { return head })
}

func runLast(fb *Fiber, self *Channel) Value {
	last := None
	return runWith(fb, self, func(fb *Fiber, pull Pull) {
		for {
			last = Some(pull(fb))
		}
	}, func(*Fiber, Value) Value { return last })
}

// runIntoQueue offers every element with `offer`, then ends the queue, or
// fails it as the channel failed; only the run is interruptible.
func runIntoQueue(fb *Fiber, self *Channel, q *Queue, offer func(fb *Fiber, o Value)) Value {
	restore := fb.state.mask
	return masked(fb, restore+1, func() Value {
		_, c := catch(func() Value {
			return masked(fb, restore, func() Value { return runForEach(fb, self, offer) })
		})
		if c == nil {
			q.end()
		} else {
			q.failCause(c)
		}
		return Unit
	})
}

// toQueue runs the channel into a queue in a fiber of the owner's scope,
// which shuts the queue down when it closes.
func toQueue(fb *Fiber, self *Channel, options Value, offer func(q *Queue) func(*Fiber, Value)) Value {
	o := record(options)
	q := queueOptions(o.Get("capacity"), o.Get("strategy"))
	scope := fb.owner
	scope.add(fb, func(*Fiber, *Cause) Value { q.shutdown(); return Unit })
	forkIn(fb, scope, func(fb *Fiber) Value { return runIntoQueue(fb, self, q, offer(q)) })
	return q
}

// makePubSub is the hub of the options, shut down with the owner's scope.
func makePubSub(fb *Fiber, o *Record) *PubSub {
	replay := hubReplay(o.Get("replay"))
	var h *PubSub
	if c, ok := some(o.Get("capacity")); !ok {
		h = newPubSub(makeAtomicUnbounded(replay), hubDropping)
	} else {
		strategy := hubBackPressure
		if s, ok := some(o.Get("strategy")); ok {
			switch queueStrategies[s.(*Data).Tag] {
			case queueDropping:
				strategy = hubDropping
			case queueSliding:
				strategy = hubSliding
			}
		}
		h = newPubSub(makeAtomicBounded(int(c.(int32)), replay), strategy)
	}
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value { h.shutdown(fb); return Unit })
	return h
}

// runIntoPubSub publishes with `publish` each element, shutting the hub
// down at the end when `shutdownOnEnd`; the done value.
func runIntoPubSub(fb *Fiber, self *Channel, h *PubSub, shutdownOnEnd bool, publish func(fb *Fiber, o Value)) Value {
	v, c := catch(func() Value { return runForEach(fb, self, publish) })
	if shutdownOnEnd {
		h.shutdown(fb)
	}
	if c != nil {
		panic(&Failure{c})
	}
	return v
}

// toPubSub runs the channel into a new hub in a fiber of the owner's
// scope.
func toPubSub(fb *Fiber, self *Channel, options Value, array bool) Value {
	o := record(options)
	h := makePubSub(fb, o)
	shutdownOnEnd := o.Get("shutdownOnEnd").(bool)
	forkIn(fb, fb.owner, func(fb *Fiber) Value {
		return runIntoPubSub(fb, self, h, shutdownOnEnd, publisher(h, array))
	})
	return h
}

func publisher(h *PubSub, array bool) func(fb *Fiber, o Value) {
	if array {
		return func(fb *Fiber, o Value) { h.publishAll(fb, o.([]Value)) }
	}
	return func(fb *Fiber, o Value) { h.publish(fb, o) }
}

func offerer(q *Queue) func(*Fiber, Value) {
	return func(fb *Fiber, o Value) { q.offer(fb, o) }
}

func allOfferer(q *Queue) func(*Fiber, Value) {
	return func(fb *Fiber, o Value) { q.offerAll(fb, o.([]Value)) }
}

// The bindings.

func ChannelRunCount(fb *Fiber, self Value) Value {
	return runFold(fb, channelOf(self), int32(0), func(_ *Fiber, n, _ Value) Value { return n.(int32) + 1 })
}

func ChannelRunHead(fb *Fiber, self Value) Value { return runHead(fb, channelOf(self)) }
func ChannelRunLast(fb *Fiber, self Value) Value { return runLast(fb, channelOf(self)) }

func ChannelRunFoldEffect(fb *Fiber, initial, f, self Value) Value {
	return runFold(fb, channelOf(self), Apply(fb, initial, Unit), func(fb *Fiber, z, o Value) Value {
		return Run(fb, apply2(fb, f, z, o))
	})
}

func ChannelRunForEachWhile(fb *Fiber, f, self Value) Value {
	return runForEach(fb, channelOf(self), func(fb *Fiber, o Value) {
		if !Run(fb, Apply(fb, f, o)).(bool) {
			halt(Unit)
		}
	})
}

// ChannelMkUint8Array is the bytes of the chunks, concatenated.
func ChannelMkUint8Array(fb *Fiber, self Value) Value {
	var out []byte
	runFold(fb, channelOf(self), Unit, func(_ *Fiber, z, chunk Value) Value {
		for _, bytes := range chunk.([]Value) {
			out = append(out, byteString(bytes)...)
		}
		return z
	})
	return byteList(out)
}

func ChannelRunIntoQueue(fb *Fiber, q, self Value) Value {
	return runIntoQueue(fb, channelOf(self), queueOf(q), offerer(queueOf(q)))
}

func ChannelRunIntoQueueArray(fb *Fiber, q, self Value) Value {
	return runIntoQueue(fb, channelOf(self), queueOf(q), allOfferer(queueOf(q)))
}

func ChannelToQueue(fb *Fiber, options, self Value) Value {
	return toQueue(fb, channelOf(self), options, offerer)
}

func ChannelToQueueArray(fb *Fiber, options, self Value) Value {
	return toQueue(fb, channelOf(self), options, allOfferer)
}

func ChannelToPubSub(fb *Fiber, options, self Value) Value {
	return toPubSub(fb, channelOf(self), options, false)
}

func ChannelToPubSubArray(fb *Fiber, options, self Value) Value {
	return toPubSub(fb, channelOf(self), options, true)
}

func ChannelRunIntoPubSub(fb *Fiber, options, hub, self Value) Value {
	h := pubSubOf(hub)
	return runIntoPubSub(fb, channelOf(self), h, record(options).Get("shutdownOnEnd").(bool), publisher(h, false))
}

func ChannelRunIntoPubSubArray(fb *Fiber, options, hub, self Value) Value {
	h := pubSubOf(hub)
	return runIntoPubSub(fb, channelOf(self), h, record(options).Get("shutdownOnEnd").(bool), publisher(h, true))
}
