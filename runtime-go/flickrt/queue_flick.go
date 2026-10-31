package flickrt

// The bindings of `Queue`: the end of a queue reaches Flick as `None`.

var queueStrategies = map[string]queueStrategy{
	Tag("Queue.Strategy.Suspend"):  queueSuspend,
	Tag("Queue.Strategy.Dropping"): queueDropping,
	Tag("Queue.Strategy.Sliding"):  queueSliding,
}

// queueOptions is a queue of a capacity (`None`: unbounded) and a strategy
// (`None`: suspend), as `Queue.make` and `Channel.Buffer` take them.
func queueOptions(capacity, strategy Value) *Queue {
	n, s := unboundedCapacity, queueSuspend
	if c, ok := some(capacity); ok {
		n = int(c.(int32))
	}
	if x, ok := some(strategy); ok {
		s = queueStrategies[x.(*Data).Tag]
	}
	return newQueue(n, s)
}

func QueueMake(options Value) Value {
	o := record(options)
	return queueOptions(o.Get("capacity"), o.Get("strategy"))
}

func QueueBounded(n Value) Value  { return newQueue(int(n.(int32)), queueSuspend) }
func QueueSliding(n Value) Value  { return newQueue(int(n.(int32)), queueSliding) }
func QueueDropping(n Value) Value { return newQueue(int(n.(int32)), queueDropping) }
func QueueUnbounded() Value       { return newQueue(unboundedCapacity, queueSuspend) }

func QueueOffer(fb *Fiber, x, q Value) Value { return queueOf(q).offer(fb, x) }

func QueueOfferAll(fb *Fiber, xs, q Value) Value { return queueOf(q).offerAll(fb, List(xs)) }

func QueueFail(err, q Value) Value { return queueOf(q).failCause(&Cause{kind: causeFail, err: err}) }

func QueueFailCause(failure, q Value) Value { return queueOf(q).failCause(causeOfFailure(failure)) }

func QueueEnd(q Value) Value { return queueOf(q).end() }

func QueueInterrupt(fb *Fiber, q Value) Value {
	return queueOf(q).failCause(&Cause{kind: causeInterrupt, fiber: fb.state.id})
}

func QueueShutdown(q Value) Value { return queueOf(q).shutdown() }
func QueueClear(q Value) Value    { return queueOf(q).clear() }

func QueueTakeAll(fb *Fiber, q Value) Value { return orDone(fb, queueOf(q).takeAll) }

// QueueCollect takes messages until the queue ends.
func QueueCollect(fb *Fiber, q Value) Value {
	x, out := queueOf(q), []Value{}
	catchDone(fb, func(fb *Fiber) Value {
		for {
			out = append(out, x.takeAll(fb).([]Value)...)
		}
	}, func(*Fiber, Value) Value { return nil })
	return out
}

func QueueTakeN(fb *Fiber, n, q Value) Value { return QueueTakeBetween(fb, n, n, q) }

func QueueTakeBetween(fb *Fiber, lo, hi, q Value) Value {
	return orDone(fb, func(fb *Fiber) Value { return queueOf(q).takeBetween(fb, int(lo.(int32)), int(hi.(int32))) })
}

func QueueTake(fb *Fiber, q Value) Value { return orDone(fb, queueOf(q).take) }

func QueuePoll(q Value) Value {
	if x, ok := queueOf(q).poll(); ok {
		return Some(x)
	}
	return None
}

func QueuePeek(fb *Fiber, q Value) Value { return orDone(fb, queueOf(q).peek) }

func QueueFlush(q Value) Value {
	x := queueOf(q)
	x.mu.Lock()
	x.releaseTakers()
	x.mu.Unlock()
	return Unit
}

func QueueAwait(fb *Fiber, q Value) Value {
	queueOf(q).await(fb)
	return Unit
}

func QueueSize(q Value) Value { return int32(queueOf(q).size()) }

func QueueIsFull(q Value) Value {
	x := queueOf(q)
	return x.size() == x.capacity
}

// QueueInto runs the task, then ends the queue, or fails it as the task
// failed; whether this ended it.
func QueueInto(fb *Fiber, q, task Value) Value {
	if _, c := catch(func() Value { return Run(fb, task) }); c != nil {
		return queueOf(q).failCause(c)
	}
	return queueOf(q).end()
}
