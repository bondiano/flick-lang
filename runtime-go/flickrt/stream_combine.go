package flickrt

import (
	"strings"
	"sync"
)

// Streams combined: one after another, nested, repeated, merged, raced and
// interleaved, and their bindings. What runs concurrently runs in fibers
// of the stream's scope.

func StreamConcat(that, s Value) Value { return streamConcat(streamOf(s), streamOf(that)) }

func StreamPrepend(values, s Value) Value {
	return streamConcat(chunkStream(List(values)), streamOf(s))
}

func StreamFlatMap(f, s Value) Value {
	return streamFlatMap(streamOf(s), func(fb *Fiber, a Value) *Stream { return streamOf(Apply(fb, f, a)) })
}

// streamFlatMapPar is `flatMap` with options: in turn when at most one
// runs at once, merged otherwise.
func streamFlatMapPar(self *Stream, f func(fb *Fiber, a Value) *Stream, options Value) *Stream {
	o := record(options)
	n := concurrencyOf(o.Get("concurrency"))
	if n <= 1 {
		return streamFlatMap(self, f)
	}
	bufferSize := 16
	if b, ok := some(o.Get("bufferSize")); ok {
		bufferSize = int(b.(int32))
	}
	inner := channelMap(channelFlattenArray(self.ch), func(fb *Fiber, a Value) Value { return f(fb, a).ch })
	return &Stream{channelMergeAll(inner, n, bufferSize, false)}
}

func StreamFlatMapPar(options, f, s Value) Value {
	return streamFlatMapPar(streamOf(s), func(fb *Fiber, a Value) *Stream { return streamOf(Apply(fb, f, a)) }, options)
}

func StreamSwitchMap(f, s Value) Value {
	inner := channelMap(channelFlattenArray(streamOf(s).ch), func(fb *Fiber, a Value) Value { return streamOf(Apply(fb, f, a)).ch })
	return &Stream{channelMergeAll(inner, 1, 16, true)}
}

func StreamFlatten(s Value) Value {
	return streamFlatMap(streamOf(s), func(_ *Fiber, inner Value) *Stream { return streamOf(inner) })
}

func StreamFlattenArray(s Value) Value { return onChannel(s, channelFlattenArray) }

func StreamFlattenIterable(s Value) Value {
	return streamFlatMap(streamOf(s), func(_ *Fiber, xs Value) *Stream { return chunkStream(List(xs)) })
}

func StreamDrain(s Value) Value { return onChannel(s, channelDrain) }

// streamMergeEffect runs `task` beside the stream, whose end ends the
// merge.
func streamMergeEffect(self *Stream, task func(fb *Fiber)) *Stream {
	drained := channelFromPullOf(func(fb *Fiber) Value {
		task(fb)
		return halt(Unit)
	})
	return &Stream{channelMerge(self.ch, drained, "left")}
}

func runDrainOf(s *Stream) func(fb *Fiber) { return func(fb *Fiber) { runDrain(fb, s.ch) } }

func StreamDrainFork(that, s Value) Value {
	return streamMergeEffect(streamOf(s), runDrainOf(streamOf(that)))
}

func StreamMergeLeft(right, s Value) Value {
	return streamMergeEffect(streamOf(s), runDrainOf(streamOf(right)))
}

func StreamMergeRight(right, s Value) Value {
	return streamMergeEffect(streamOf(right), runDrainOf(streamOf(s)))
}

func StreamRepeat(sched, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelRepeat(c, scheduleOf(sched)) })
}

// StreamRepeatElements emits each element again while a run of the
// schedule, given the element, goes on.
func StreamRepeatElements(sched, s Value) Value {
	schedule, self := scheduleOf(sched), streamOf(s)
	return &Stream{fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		pullElement := channelFlattenArray(self.ch).transform(fb, upstream, scope)
		var pullRepeat, pull Pull
		pull = func(fb *Fiber) Value {
			element := pullElement(fb)
			chunk := []Value{element}
			step := schedule.stepper(fb)
			pullRepeat = func(fb *Fiber) Value {
				if _, done := step(fb, element); done {
					pullRepeat = nil
					return pull(fb)
				}
				return chunk
			}
			return chunk
		}
		return func(fb *Fiber) Value {
			if pullRepeat != nil {
				return pullRepeat(fb)
			}
			return pull(fb)
		}
	})}
}

func StreamForever(s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return repeatLoop(c, func(*Fiber, Value) {}) })
}

func StreamSchedule(sched, s Value) Value {
	scheduled := channelSchedule(channelFlattenArray(streamOf(s).ch), scheduleOf(sched))
	return &Stream{channelMap(scheduled, func(_ *Fiber, a Value) Value { return []Value{a} })}
}

func StreamMerge(that, s Value) Value {
	return &Stream{channelMerge(streamOf(s).ch, streamOf(that).ch, "both")}
}

// StreamMergeWith ends as the strategy says, `Left` being this stream.
func StreamMergeWith(strategy, that, s Value) Value {
	side := strings.ToLower(ConstructorName(strategy.(*Data).Tag))
	return &Stream{channelMerge(streamOf(s).ch, streamOf(that).ch, side)}
}

func StreamMergeResult(that, s Value) Value {
	left := StreamMap(Fn(func(_ *Fiber, a Value) Value { return Ok(a) }), s)
	right := StreamMap(Fn(func(_ *Fiber, b Value) Value { return Err(b) }), that)
	return StreamMerge(right, left)
}

// StreamMergeAll flattens the stream of the streams given, by the options.
func StreamMergeAll(options, streams Value) Value {
	return streamFlatMapPar(chunkStream(List(streams)), func(_ *Fiber, inner Value) *Stream { return streamOf(inner) }, options)
}

func StreamCross(right, s Value) Value {
	return StreamCrossWith(right, Fn(func(fb *Fiber, l Value) Value {
		return Fn(func(_ *Fiber, r Value) Value { return []Value{l, r} })
	}), s)
}

func StreamCrossWith(right, f, s Value) Value {
	return streamFlatMap(streamOf(s), func(fb *Fiber, l Value) *Stream {
		g := Apply(fb, f, l)
		return streamOf(StreamMap(g, right))
	})
}

// streamRaceAll pulls each stream in a scope of its own; the first to emit
// wins, the others are interrupted and their scopes closed.
func streamRaceAll(streams []Value) *Stream {
	return &Stream{fromTransform(func(_ *Fiber, _ Pull, scope *Owner) Pull {
		var mu sync.Mutex
		var winner Pull
		won := func() Pull {
			mu.Lock()
			defer mu.Unlock()
			return winner
		}
		race := func(fb *Fiber) Value {
			tasks := make([]Value, len(streams))
			for i, s := range streams {
				tasks[i] = Eff(func(fb *Fiber) Value {
					childScope := forkScope(fb, scope)
					var pull Pull
					chunk, c := catch(func() Value {
						pull = toPullScoped(fb, streamOf(s).ch, childScope)
						return pull(fb)
					})
					return masked(fb, fb.state.mask+1, func() Value {
						if c == nil {
							mu.Lock()
							first := winner == nil
							if first {
								winner = pull
							}
							mu.Unlock()
							if !first {
								closeScope(fb, childScope, nil)
							}
							return chunk
						}
						closeScope(fb, childScope, c)
						panic(&Failure{c})
					})
				})
			}
			return raceAll(fb, tasks, false)
		}
		return func(fb *Fiber) Value {
			if w := won(); w != nil {
				return w(fb)
			}
			return race(fb)
		}
	})}
}

func StreamRaceAll(streams Value) Value { return streamRaceAll(List(streams)) }

func StreamRace(that, s Value) Value { return streamRaceAll([]Value{s, that}) }

// alternating is the stream of true, false, true and so on without end.
var alternating = &Stream{fromPull(func(*Fiber) Pull {
	return func(*Fiber) Value {
		chunk := make([]Value, streamChunkSize)
		for i := range chunk {
			chunk[i] = i%2 == 0
		}
		return chunk
	}
})}

func StreamInterleave(that, s Value) Value { return StreamInterleaveWith(that, alternating, s) }

// StreamInterleaveWith pulls this stream for each `true` of the decider,
// `that` for each `false`, skipping a side that ended.
func StreamInterleaveWith(that, decider, s Value) Value {
	self := streamOf(s)
	return &Stream{fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		pullDecider := channelFlattenArray(streamOf(decider).ch).transform(fb, upstream, scope)
		leftDone, rightDone := false, false
		side := func(p Pull, ended *bool) func(fb *Fiber) (Value, bool) {
			return func(fb *Fiber) (Value, bool) {
				ok := true
				v := catchDone(fb, p, func(*Fiber, Value) Value {
					*ended, ok = true, false
					return nil
				})
				return v, ok
			}
		}
		pullLeft := side(channelFlattenArray(self.ch).transform(fb, upstream, scope), &leftDone)
		pullRight := side(channelFlattenArray(streamOf(that).ch).transform(fb, upstream, scope), &rightDone)
		return func(fb *Fiber) Value {
			for {
				if leftDone && rightDone {
					return halt(Unit)
				}
				left := pullDecider(fb).(bool)
				if left && leftDone || !left && rightDone {
					continue
				}
				pull := pullRight
				if left {
					pull = pullLeft
				}
				if elem, ok := pull(fb); ok {
					return []Value{elem}
				}
			}
		}
	})}
}

func StreamInterruptWhen(task, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelInterruptWhen(task, c)) })
}

func StreamHaltWhen(task, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelHaltWhen(task, c)) })
}

// timeoutMarker is the defect a timed-out stream dies with.
type timeoutMarker struct{ _ byte }

var timedOut = &timeoutMarker{}

// streamTimeoutOrElse switches to `orElse` when `d` passes while an
// element is pulled.
func streamTimeoutOrElse(self *Stream, d *Duration, orElse func(fb *Fiber) *Stream) *Stream {
	if d.infinite() {
		return self
	}
	if !DurationIsPositive(d).(bool) {
		return streamSuspend(orElse)
	}
	ms := millisOf(d)
	timed := streamSuspend(func(fb *Fiber) *Stream {
		clock := clockOf(fb)
		var mu sync.Mutex
		deadline, pulling := 0.0, false
		latch := newLatch()
		left := transformPull(self.ch, func(_ *Fiber, pull Pull, _ *Owner) Pull {
			return func(fb *Fiber) Value {
				mu.Lock()
				deadline, pulling = clock.millis()+ms, true
				mu.Unlock()
				LatchOpen(fb, latch)
				arr := pull(fb)
				LatchClose(fb, latch)
				mu.Lock()
				pulling = false
				mu.Unlock()
				return arr
			}
		})
		ticking := func() (float64, bool) {
			mu.Lock()
			defer mu.Unlock()
			return deadline, pulling
		}
		right := channelFromPullOf(func(fb *Fiber) Value {
			for {
				LatchAwait(fb, latch)
				at, ok := ticking()
				if !ok {
					continue
				}
				sleepFor(fb, fromMillis(at-clock.millis()))
				if at, ok = ticking(); !ok || at-clock.millis() > 0 {
					continue
				}
				panic(&Failure{&Cause{kind: causeDie, defect: "timeout", err: timedOut}})
			}
		})
		return &Stream{channelMerge(left, right, "left")}
	})
	return streamCatchCause(timed, func(fb *Fiber, c *Cause) *Stream {
		for _, r := range c.reasons() {
			if r.kind == causeDie && r.err == Value(timedOut) {
				return orElse(fb)
			}
		}
		return streamFailCause(c)
	})
}

func StreamTimeout(d, s Value) Value {
	return streamTimeoutOrElse(streamOf(s), dur(d), func(*Fiber) *Stream { return streamEmpty })
}

func StreamTimeoutOrElse(d, fallback, s Value) Value {
	return streamTimeoutOrElse(streamOf(s), dur(d), func(*Fiber) *Stream { return streamOf(fallback) })
}
