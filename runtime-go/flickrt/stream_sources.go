package flickrt

// Stream's sources, as Effect's (Stream.ts): from values, effects,
// schedules, queues, hubs and references, and their bindings. What a
// Flick task acquires belongs to its lexical scope, as in the JS adapter.

var StreamEmpty Value = streamEmpty
var StreamNever Value = &Stream{channelNever}

// StreamFromArrays emits each non-empty list as a chunk.
func StreamFromArrays(arrays Value) Value {
	var chunks []Value
	for _, a := range List(arrays) {
		if len(List(a)) > 0 {
			chunks = append(chunks, a)
		}
	}
	return &Stream{channelFromArray(chunks)}
}

func StreamSuspend(f Value) Value {
	return streamSuspend(func(fb *Fiber) *Stream { return streamOf(Apply(fb, f, Unit)) })
}

func StreamFail(err Value) Value          { return streamFail(err) }
func StreamFailCause(failure Value) Value { return streamFailCause(causeOfFailure(failure)) }

func StreamDie(message Value) Value {
	return streamFailCause(&Cause{kind: causeDie, defect: message.(string)})
}

func StreamFromEffect(task Value) Value {
	return &Stream{channelFromEffect(func(fb *Fiber) Value { return []Value{Run(fb, task)} })}
}

func StreamFromEffectDrain(task Value) Value {
	return streamFromPullOf(func(fb *Fiber) Value {
		Run(fb, task)
		return halt(Unit)
	})
}

func StreamFromEffectRepeat(task Value) Value {
	return streamFromPullOf(func(fb *Fiber) Value { return []Value{Run(fb, task)} })
}

// StreamFromEffectSchedule runs the task, then again after each step of
// the schedule, which gets the last value; each run reads the step's
// metadata.
func StreamFromEffectSchedule(s, task Value) Value {
	sched := scheduleOf(s)
	return &Stream{fromPull(func(fb *Fiber) Pull {
		step := sched.stepper(fb)
		last := runWithMetadata(fb, defaultMetadata, task)
		initial := true
		return func(fb *Fiber) Value {
			if initial {
				initial = false
				return []Value{last}
			}
			m, done := step(fb, last)
			if done {
				return halt(m.output)
			}
			last = runWithMetadata(fb, m, task)
			return []Value{last}
		}
	})}
}

func StreamFromListEffectRepeat(task Value) Value {
	return streamFlatMap(streamOf(StreamFromEffectRepeat(task)), func(_ *Fiber, xs Value) *Stream {
		return chunkStream(List(xs))
	})
}

// StreamUnwrap is the JS adapter's `flatten(fromEffect(task))`: the task
// runs in the stream's fiber, acquiring into its lexical scope.
func StreamUnwrap(task Value) Value {
	return streamFlatMap(streamOf(StreamFromEffect(task)), func(_ *Fiber, s Value) *Stream { return streamOf(s) })
}

// StreamCallback is Effect's `callback` with an unbounded queue, the
// producer forked into the stream's scope but acquiring into its lexical
// one, as the JS adapter gives it.
func StreamCallback(f Value) Value {
	return &Stream{fromTransform(func(fb *Fiber, _ Pull, scope *Owner) Pull {
		q := newQueue(unboundedCapacity, queueSuspend)
		scope.add(fb, func(*Fiber, *Cause) Value { q.shutdown(); return Unit })
		forkIn(fb, scope, func(fb *Fiber) Value {
			if _, c := catch(func() Value { return Run(fb, Apply(fb, f, q)) }); c != nil {
				q.failCause(c)
			}
			return Unit
		})
		return q.takeAll
	})}
}

func StreamTick(interval Value) Value {
	d := dur(interval)
	return &Stream{fromPull(func(*Fiber) Pull {
		first := true
		return func(fb *Fiber) Value {
			if first {
				first = false
			} else {
				sleepFor(fb, d)
			}
			return []Value{Unit}
		}
	})}
}

func StreamFromSchedule(s Value) Value {
	sched := scheduleOf(s)
	return &Stream{fromPull(func(fb *Fiber) Pull {
		step := sched.stepper(fb)
		return func(fb *Fiber) Value {
			m, done := step(fb, Unit)
			if done {
				return halt(Unit)
			}
			return []Value{m.output}
		}
	})}
}

func StreamFromQueue(q Value) Value { return streamFromPullOf(queueOf(q).takeAll) }

func StreamFromPubSub(hub Value) Value { return &Stream{channelFromPubSub(pubSubOf(hub), true)} }

func StreamUnfold(seed, f Value) Value {
	return &Stream{fromPull(func(*Fiber) Pull {
		state := seed
		return func(fb *Fiber) Value {
			next, ok := some(Run(fb, Apply(fb, f, state)))
			if !ok {
				return halt(Unit)
			}
			pair := List(next)
			state = pair[1]
			return []Value{pair[0]}
		}
	})}
}

func StreamPaginate(seed, f Value) Value {
	return &Stream{fromPull(func(*Fiber) Pull {
		state, done := seed, false
		return func(fb *Fiber) Value {
			for {
				if done {
					return halt(Unit)
				}
				pair := List(Run(fb, Apply(fb, f, state)))
				if next, ok := some(pair[1]); ok {
					state = next
				} else {
					done = true
				}
				if page := List(pair[0]); len(page) > 0 {
					return page
				}
			}
		}
	})}
}

func StreamIterate(value, next Value) Value {
	return &Stream{fromPull(func(*Fiber) Pull {
		state := value
		return func(fb *Fiber) Value {
			a := state
			state = Apply(fb, next, a)
			return []Value{a}
		}
	})}
}

// StreamRange is the integers from `lo` to `hi` in chunks of the default
// size.
func StreamRange(lo, hi Value) Value {
	low, high := int(lo.(int32)), int(hi.(int32))
	if low > high {
		return streamEmpty
	}
	return &Stream{fromPull(func(*Fiber) Pull {
		start, done := low, false
		return func(*Fiber) Value {
			if done {
				return halt(Unit)
			}
			n := high - start + 1
			if n > streamChunkSize {
				n = streamChunkSize
			} else {
				done = true
			}
			chunk := make([]Value, n)
			for i := range chunk {
				chunk[i] = int32(start + i)
			}
			start += n
			return chunk
		}
	})}
}

// SubscriptionRefMake is Effect's: the value, and a hub replaying the last
// one, to which each new value is published.
func SubscriptionRefMake(fb *Fiber, v Value) Value {
	h := newPubSub(makeAtomicUnbounded(1), hubDropping)
	h.publish(fb, v)
	return &SyncRef{value: v, hub: h}
}

// SubscriptionRefChanges is the current value, then each new one.
func SubscriptionRefChanges(r Value) Value {
	return &Stream{channelFromPubSub(r.(*SyncRef).hub, true)}
}

// TxSubscriptionRefChangesStream subscribes in the stream's scope and
// takes each change in a transaction of its own.
func TxSubscriptionRefChangesStream(s Value) Value {
	return &Stream{channelUnwrap(func(fb *Fiber) *Channel {
		q := TxSubscriptionRefChanges(fb, s)
		return channelFromPullOf(func(fb *Fiber) Value { return []Value{TxQueueTake(fb, q)} })
	})}
}
