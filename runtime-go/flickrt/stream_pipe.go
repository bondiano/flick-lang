package flickrt

import (
	"math/big"
	"sync"
)

// Streams through sinks and channels: sinks run again and again over a
// stream, beside it or once at its head, and execution plans, and their
// bindings.

// StreamTransduce runs the sink again and again over the stream, each
// time from the last one's leftovers, emitting each value; a failure of
// the stream ends the input of the run it falls in, then fails.
func StreamTransduce(sink, s Value) Value {
	k := sinkOf(sink)
	return streamTransformPull(streamOf(s), func(_ *Fiber, upstream Pull, scope *Owner) Pull {
		var done *Cause
		var leftover []Value
		withLeftover := func(fb *Fiber) Value {
			if leftover != nil {
				chunk := leftover
				leftover = nil
				return chunk
			}
			return catchError(fb, upstream, func(_ *Fiber, err Value) Value {
				done = &Cause{kind: causeFail, err: err}
				return halt(Unit)
			})
		}
		return func(fb *Fiber) Value {
			if done != nil {
				panic(&Failure{done})
			}
			v, l := k.transform(fb, withLeftover, scope)
			if len(l) > 0 {
				leftover = l
			}
			return []Value{v}
		}
	})
}

func StreamAggregate(sink, s Value) Value {
	return streamAggregateWithin(streamOf(s), sinkOf(sink), ScheduleForever.(*Schedule))
}

func StreamAggregateWithin(sink, sched, s Value) Value {
	return streamAggregateWithin(streamOf(s), sinkOf(sink), scheduleOf(sched))
}

// scheduleStep is what the schedule offers the buffer to end a run of the
// sink.
type scheduleStepMarker struct{ _ byte }

var scheduleStepped = &scheduleStepMarker{}

// streamAggregateWithin runs the sink again and again over the stream
// pulled ahead into a buffer, a step of the schedule (given the last
// value) ending a run early; the schedule's clock leaves out the time
// between runs.
func streamAggregateWithin(self *Stream, sink *Sink, schedule *Schedule) *Stream {
	return &Stream{fromTransformBracket(func(fb *Fiber, _ Pull, outer, scope *Owner) Pull {
		pull := toPullScoped(fb, self.ch, outer)
		pullLatch := newLatch()
		buffer := newQueue(0, queueSuspend)
		forkIn(fb, scope, func(fb *Fiber) Value {
			_, c := catch(func() Value {
				for {
					LatchAwait(fb, pullLatch)
					arr := pull(fb)
					LatchClose(fb, pullLatch)
					buffer.offer(fb, arr)
				}
			})
			buffer.failCause(c)
			return Unit
		})
		clock := clockOf(fb)
		step := schedule.start(fb)
		hasInput := newLatch()
		var mu sync.Mutex // guards the state the schedule's fiber reads
		scheduleTime, openedAt := clock.millis(), 0.0
		lastOutput := None
		var leftover []Value
		state := func() (queueState, *Cause) {
			buffer.mu.Lock()
			defer buffer.mu.Unlock()
			return buffer.state, buffer.exit
		}
		onInput := func(fb *Fiber, chunk Value) Value {
			if LatchOpen(fb, hasInput).(bool) {
				mu.Lock()
				openedAt = clock.millis()
				mu.Unlock()
			}
			return chunk
		}
		stepToBuffer := Eff(func(fb *Fiber) Value {
			LatchAwait(fb, hasInput)
			mu.Lock()
			now, last := scheduleTime, lastOutput
			mu.Unlock()
			if _, delay, done := step(fb, now, last); done {
				// end the input so the sink drains its leftovers without pulling more
				buffer.end()
			} else {
				sleepFor(fb, delay)
				buffer.offer(fb, scheduleStepped)
			}
			return EffectNever(fb)
		})
		sinkUpstream := func(fb *Fiber) Value {
			mu.Lock()
			chunk := leftover
			leftover = nil
			mu.Unlock()
			if chunk != nil {
				return onInput(fb, chunk)
			}
			if st, _ := state(); st == queueOpen {
				LatchOpen(fb, pullLatch)
			}
			arr := buffer.take(fb)
			if arr == Value(scheduleStepped) {
				return halt(Unit)
			}
			return onInput(fb, arr)
		}
		run := Eff(func(fb *Fiber) Value {
			value, left := sink.transform(fb, sinkUpstream, scope)
			if LatchIsOpen(hasInput).(bool) {
				mu.Lock()
				scheduleTime += clock.millis() - openedAt
				mu.Unlock()
			} else if st, _ := state(); st == queueDone {
				return halt(Unit)
			}
			mu.Lock()
			lastOutput = Some(value)
			if len(left) > 0 {
				leftover = left
			}
			mu.Unlock()
			return []Value{value}
		})
		return func(fb *Fiber) Value {
			mu.Lock()
			pending := leftover != nil
			mu.Unlock()
			st, exit := state()
			if st == queueDone && !pending {
				panic(&Failure{exit})
			}
			LatchClose(fb, hasInput)
			if st == queueOpen {
				return race(fb, run, stepToBuffer, false)
			}
			return Run(fb, run)
		}
	})}
}

// StreamPipeThrough is the leftovers of the sink run on the stream.
func StreamPipeThrough(sink, s Value) Value {
	piped := channelPipeToOrFail(streamOf(s).ch, sinkOf(sink).toChannel())
	return &Stream{channelConcatWith(piped, func(_ *Fiber, d Value) *Channel {
		if end := d.(sinkEnd); len(end.leftover) > 0 {
			return channelSucceed(end.leftover)
		}
		return channelEmpty
	})}
}

// StreamTapSink feeds each chunk to the sink, run beside the stream, before
// emitting it; once the sink ends, the stream goes on alone, and its
// failure fails the stream.
func StreamTapSink(sink, s Value) Value {
	k := sinkOf(sink)
	return streamTransformPullBracket(streamOf(s), func(fb *Fiber, pull Pull, _, scope *Owner) Pull {
		upstreamLatch, sinkLatch := newLatch(), newLatch()
		var mu sync.Mutex
		var chunk []Value
		var causeSink *Cause
		sinkDone, streamDone := false, false
		status := func() (*Cause, bool) {
			mu.Lock()
			defer mu.Unlock()
			return causeSink, sinkDone
		}
		sinkUpstream := func(fb *Fiber) Value {
			LatchAwait(fb, upstreamLatch)
			mu.Lock()
			arr, ended := chunk, streamDone
			chunk = nil
			mu.Unlock()
			if arr == nil {
				return halt(Unit)
			}
			if !ended {
				LatchClose(fb, upstreamLatch)
			}
			LatchOpen(fb, sinkLatch)
			return arr
		}
		forkIn(fb, scope, func(fb *Fiber) Value {
			_, c := catch(func() Value {
				k.transform(fb, sinkUpstream, scope)
				return Unit
			})
			mu.Lock()
			sinkDone, causeSink = true, c
			mu.Unlock()
			LatchOpen(fb, sinkLatch)
			return Unit
		})
		pullAndOffer := func(fb *Fiber) Value {
			arr, c := catch(func() Value { return pull(fb) })
			if c != nil {
				if _, rest, ok := filterDone(c); !ok {
					panic(&Failure{rest})
				}
			}
			mu.Lock()
			if causeSink != nil {
				cs := causeSink
				mu.Unlock()
				panic(&Failure{cs})
			}
			if sinkDone {
				mu.Unlock()
				if c != nil {
					return halt(Unit)
				}
				return arr
			}
			if c == nil {
				chunk = arr.([]Value)
			} else {
				streamDone = true
			}
			mu.Unlock()
			LatchClose(fb, sinkLatch)
			LatchOpen(fb, upstreamLatch)
			LatchAwait(fb, sinkLatch)
			if c == nil {
				return arr
			}
			if cs, _ := status(); cs != nil {
				panic(&Failure{cs})
			}
			return halt(Unit)
		}
		return func(fb *Fiber) Value {
			switch cs, done := status(); {
			case cs != nil:
				panic(&Failure{cs})
			case done:
				return pull(fb)
			}
			return pullAndOffer(fb)
		}
	})
}

// StreamPeel runs the sink on the head of the stream, in the owner's
// scope: its value, and the rest of the stream from its leftovers.
func StreamPeel(fb *Fiber, sink, s Value) Value {
	var failed *Cause
	originalPull := toPull(fb, streamOf(s).ch)
	pull := func(fb *Fiber) Value {
		v, c := catch(func() Value { return originalPull(fb) })
		if c != nil {
			failed = c
			panic(&Failure{c})
		}
		return v
	}
	scope := newOwner()
	end, cause := catch(func() Value {
		v, l := sinkOf(sink).transform(fb, pull, scope)
		return sinkEnd{v, l}
	})
	if cause = scope.close(fb, cause); cause != nil {
		panic(&Failure{cause})
	}
	rest := streamEmpty
	if failed == nil {
		rest = streamFromPullOf(originalPull)
	}
	if l := end.(sinkEnd).leftover; len(l) > 0 {
		rest = streamConcat(chunkStream(l), rest)
	}
	return []Value{end.(sinkEnd).value, rest}
}

func StreamPipeThroughChannelOrFail(c, s Value) Value {
	return &Stream{channelPipeToOrFail(streamOf(s).ch, channelOf(c))}
}

// streamRetryWithoutReset is the stream again after each typed error while
// the schedule, never restarted, goes on.
func streamRetryWithoutReset(self *Stream, schedule *Schedule) *Stream {
	return &Stream{channelUnwrap(func(fb *Fiber) *Channel {
		step, meta := schedule.stepper(fb), defaultMetadata
		var loop func() *Stream
		loop = func() *Stream {
			withMeta := &Stream{channelProvideServiceEffect(self.ch, scheduleMetadataKey, func(*Fiber) Value { return meta.record() })}
			return streamCatchError(withMeta, func(fb *Fiber, err Value) *Stream {
				m, done := step(fb, err)
				if done {
					return streamFail(err)
				}
				meta = m
				EffectYieldNow(fb)
				return loop()
			})
		}
		return loop().ch
	})}
}

// StreamWithExecutionPlan runs the stream with the services of the plan's
// steps in turn, each retried by its schedule, until one does not fail,
// telling `onEvent` of each attempt; with `prevent`, a stream that failed
// after emitting fails as it did.
func StreamWithExecutionPlan(plan, prevent, onEvent, s Value) Value {
	self, steps, preventFallback := streamOf(s), plan.([]*planStep), prevent.(bool)
	return streamSuspend(func(*Fiber) *Stream {
		i, attempt, lastStepIndex, stepAttempt := 0, int32(0), int32(-1), int32(0)
		var meta Value = Rec("attempt", int32(0), "stepIndex", int32(0))
		emit := func(fb *Fiber, event Value) { catch(func() Value { return Run(fb, Apply(fb, onEvent, event)) }) }
		provideMeta := func(st *Stream) *Stream {
			return &Stream{channelProvideServiceEffect(st.ch, metadataKey, func(*Fiber) Value {
				attempt++
				meta = Rec("attempt", attempt, "stepIndex", int32(i))
				return meta
			})}
		}
		var info Value
		var started *big.Int
		instrument := func(st *Stream) *Stream {
			begun := StreamOnStart(Eff(func(fb *Fiber) Value {
				m := record(meta)
				index := m.Get("stepIndex").(int32)
				if index != lastStepIndex {
					lastStepIndex, stepAttempt = index, 0
				}
				stepAttempt++
				info = Rec("attempt", m.Get("attempt"), "stepAttempt", stepAttempt, "stepIndex", index)
				started = clockOf(fb).monotonic()
				emit(fb, NewData(attemptStart, info))
				return Unit
			}), st)
			return &Stream{channelOnExit(streamOf(begun).ch, func(fb *Fiber, exit *Exit) Value {
				if info == nil {
					return Unit
				}
				attemptInfo := info
				info = nil
				took := fromNanos(new(big.Int).Sub(clockOf(fb).monotonic(), started))
				if exit.cause == nil {
					emit(fb, NewData(attemptSuccess, attemptInfo, took))
				} else {
					emit(fb, NewData(attemptFailure, attemptInfo, took, failureOf(exit.cause)))
				}
				return Unit
			})}
		}
		var lastError Value
		hasError := false
		var loop *Stream
		loop = streamSuspend(func(*Fiber) *Stream {
			if i >= len(steps) {
				return streamFail(lastError)
			}
			step := steps[i]
			next := provideMeta(instrument(&Stream{channelProvideLayer(self.ch, layerOf(step.provide))}))
			received := false
			if hasError {
				err, attempted, wrapped := lastError, false, next
				next = streamSuspend(func(*Fiber) *Stream {
					if attempted {
						return wrapped
					}
					attempted = true
					return streamFail(err)
				})
				next = streamRetryWithoutReset(next, step.retries(false))
			} else if sch := step.retries(true); sch != nil {
				next = streamRetryWithoutReset(next, sch)
			}
			if preventFallback {
				next = &Stream{channelOnFirst(next.ch, func(*Fiber, Value) { received = true })}
			}
			return streamCatchError(next, func(_ *Fiber, err Value) *Stream {
				i++
				if preventFallback && received {
					return streamFail(err)
				}
				lastError, hasError = err, true
				return loop
			})
		})
		return loop
	})
}
