package flickrt

// Running streams, as Effect's (Stream.ts): folds, heads and lasts, loops
// over the elements, and running into queues and hubs, and their bindings.

func StreamRunSum(fb *Fiber, s Value) Value {
	return runFold(fb, streamOf(s).ch, 0.0, func(_ *Fiber, acc, chunk Value) Value {
		sum := acc.(float64)
		for _, x := range chunk.([]Value) {
			sum += x.(float64)
		}
		return sum
	})
}

func StreamRunDrain(fb *Fiber, s Value) Value {
	runDrain(fb, streamOf(s).ch)
	return Unit
}

func StreamRunHead(fb *Fiber, s Value) Value {
	if chunk, ok := some(runHead(fb, streamOf(s).ch)); ok {
		return Some(chunk.([]Value)[0])
	}
	return None
}

func StreamRunLast(fb *Fiber, s Value) Value {
	if chunk, ok := some(runLast(fb, streamOf(s).ch)); ok {
		arr := chunk.([]Value)
		return Some(arr[len(arr)-1])
	}
	return None
}

func StreamRunFoldEffect(fb *Fiber, initial, f, s Value) Value {
	return runFold(fb, streamOf(s).ch, initial, func(fb *Fiber, z, chunk Value) Value {
		for _, x := range chunk.([]Value) {
			z = Run(fb, apply2(fb, f, z, x))
		}
		return z
	})
}

func StreamRunForEach(fb *Fiber, f, s Value) Value {
	runForEach(fb, streamOf(s).ch, func(fb *Fiber, chunk Value) {
		for _, x := range chunk.([]Value) {
			Run(fb, Apply(fb, f, x))
		}
	})
	return Unit
}

// StreamRunForEachWhile stops at the first element `f` refuses.
func StreamRunForEachWhile(fb *Fiber, f, s Value) Value {
	runForEach(fb, streamOf(s).ch, func(fb *Fiber, chunk Value) {
		for _, x := range chunk.([]Value) {
			if !Run(fb, Apply(fb, f, x)).(bool) {
				halt(Unit)
			}
		}
	})
	return Unit
}

func StreamRunForEachArray(fb *Fiber, f, s Value) Value {
	runForEach(fb, streamOf(s).ch, func(fb *Fiber, chunk Value) { Run(fb, Apply(fb, f, chunk)) })
	return Unit
}

// StreamRunIntoPubSub publishes each chunk, leaving the hub open.
func StreamRunIntoPubSub(fb *Fiber, hub, s Value) Value {
	h := pubSubOf(hub)
	runIntoPubSub(fb, streamOf(s).ch, h, false, publisher(h, true))
	return Unit
}

func StreamRunIntoQueue(fb *Fiber, q, s Value) Value {
	return runIntoQueue(fb, streamOf(s).ch, queueOf(q), allOfferer(queueOf(q)))
}

// StreamToPubSub runs the stream into a new hub in a fiber of the owner's
// scope, shutting the hub down at its end.
func StreamToPubSub(fb *Fiber, options, s Value) Value {
	h := makePubSub(fb, record(options))
	ch := streamOf(s).ch
	forkIn(fb, fb.owner, func(fb *Fiber) Value { return runIntoPubSub(fb, ch, h, true, publisher(h, true)) })
	return h
}

func StreamToQueue(fb *Fiber, options, s Value) Value {
	return toQueue(fb, streamOf(s).ch, options, allOfferer)
}
