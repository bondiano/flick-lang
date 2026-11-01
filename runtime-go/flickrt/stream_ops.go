package flickrt

import (
	"math"
	"sync"
	"time"
)

// The operations of Effect's Stream (Stream.ts) the others are built of:
// streams of channels, sequential and concurrent maps, threaded states,
// recovery and the latches their fibers meet at.

func onChannel(s Value, f func(self *Channel) *Channel) Value { return &Stream{f(streamOf(s).ch)} }

// streamFlatMap is Effect's sequential `flatMap`: the stream of each
// element, one after the other.
func streamFlatMap(self *Stream, f func(fb *Fiber, a Value) *Stream) *Stream {
	return &Stream{channelFlatMap(channelFlattenArray(self.ch), func(fb *Fiber, a Value) *Channel { return f(fb, a).ch })}
}

// streamSuspend is the stream `f` makes each time it runs.
func streamSuspend(f func(fb *Fiber) *Stream) *Stream {
	return &Stream{channelSuspend(func(fb *Fiber) *Channel { return f(fb).ch })}
}

func streamFailCause(c *Cause) *Stream { return &Stream{channelFailCause(c)} }

func streamFail(err Value) *Stream { return streamFailCause(&Cause{kind: causeFail, err: err}) }

// streamFromPull is Effect's `fromPull(Effect.succeed(pull))`.
func streamFromPullOf(p Pull) *Stream { return &Stream{channelFromPullOf(p)} }

// streamConcat is `self` and then `that`, each in a scope of its own.
func streamConcat(self, that *Stream) *Stream {
	return &Stream{channelFlatMap(channelFromArray([]Value{self.ch, that.ch}),
		func(_ *Fiber, c Value) *Channel { return c.(*Channel) })}
}

func streamCatchCause(self *Stream, f func(fb *Fiber, c *Cause) *Stream) *Stream {
	return &Stream{channelCatchCause(self.ch, func(fb *Fiber, c *Cause) *Channel { return f(fb, c).ch })}
}

func streamCatchError(self *Stream, f func(fb *Fiber, err Value) *Stream) *Stream {
	return &Stream{channelCatchError(self.ch, func(fb *Fiber, err Value) *Channel { return f(fb, err).ch })}
}

// streamMapEffect is Effect's `mapEffect`: each element's result a chunk
// of its own, `n` (at most; 1 or less: in turn) at once.
func streamMapEffect(self *Stream, f func(fb *Fiber, a Value) Value, n int, unordered bool) *Stream {
	elements := channelFlattenArray(self.ch)
	var mapped *Channel
	if n <= 1 {
		mapped = channelMap(elements, f)
	} else {
		mapped = channelMapEffectPar(elements, f, n, unordered)
	}
	return &Stream{channelMap(mapped, func(_ *Fiber, b Value) Value { return []Value{b} })}
}

// concurrencyOf is a concurrency option, `None` being no bound.
func concurrencyOf(o Value) int {
	if n, ok := some(o); ok {
		return int(n.(int32))
	}
	return math.MaxInt
}

// fiberSet is Effect's `Fiber.runIn(scope)` for fibers forked one by one:
// those still running when the scope closes are interrupted and awaited,
// and one that ended is forgotten.
type fiberSet struct {
	mu      sync.Mutex
	running map[*child]struct{}
	closed  bool
}

func runIn(fb *Fiber, scope *Owner) *fiberSet {
	s := &fiberSet{running: map[*child]struct{}{}}
	scope.add(fb, func(fb *Fiber, _ *Cause) Value {
		s.mu.Lock()
		s.closed = true
		left := make([]*child, 0, len(s.running))
		for c := range s.running {
			left = append(left, c)
		}
		s.mu.Unlock()
		stop(fb, left...)
		return Unit
	})
	return s
}

// fork runs `task` in a fiber of the set, interrupted at once when the
// scope has closed.
func (s *fiberSet) fork(fb *Fiber, task func(fb *Fiber) Value) *child {
	s.mu.Lock()
	c := fork(fb, fb.owner, Eff(task), func(c *child) {
		s.mu.Lock()
		delete(s.running, c)
		s.mu.Unlock()
	})
	closed := s.closed
	if !closed {
		s.running[c] = struct{}{}
	}
	s.mu.Unlock()
	if closed {
		stop(fb, c)
	}
	return c
}

// channelMapEffectPar is Effect's `mapEffectConcurrent`: `f` of each
// element in a fiber of its own, `n` at once, the results in their order
// unless `unordered`.
func channelMapEffectPar(self *Channel, f func(fb *Fiber, o Value) Value, n int, unordered bool) *Channel {
	return fromTransformBracket(func(fb *Fiber, upstream Pull, scope, forked *Owner) Pull {
		pull := self.transform(fb, upstream, scope)
		queue := newQueue(0, queueSuspend)
		forked.add(fb, func(*Fiber, *Cause) Value { queue.shutdown(); return Unit })
		fibers := runIn(fb, forked)
		if unordered {
			permits := &Semaphore{permits: n, wake: make(chan struct{})}
			forkIn(fb, forked, func(fb *Fiber) Value {
				_, c := catch(func() Value {
					for {
						permits.take(fb, 1)
						o := pull(fb)
						fibers.fork(fb, func(fb *Fiber) Value {
							if v, c := catch(func() Value { return f(fb, o) }); c != nil {
								queue.failCause(c)
							} else {
								queue.offer(fb, v)
							}
							permits.release(1)
							return Unit
						})
					}
				})
				// the end goes out once the running ones have offered theirs
				permits.take(fb, n-1)
				queue.failCause(c)
				permits.release(n - 1)
				return Unit
			})
			return queue.take
		}
		// room for n - 2: one more is offered after its fiber starts, one is joined
		effects := newQueue(max(0, n-2), queueSuspend)
		forked.add(fb, func(*Fiber, *Cause) Value { effects.shutdown(); return Unit })
		forkIn(fb, forked, func(fb *Fiber) Value {
			_, c := catch(func() Value {
				for {
					join := effects.take(fb).(func(*Fiber) Value)
					queue.offer(fb, join(fb))
				}
			})
			queue.failCause(c)
			return Unit
		})
		var mu sync.Mutex
		var failed *Cause
		forkIn(fb, forked, func(fb *Fiber) Value {
			_, c := catch(func() Value {
				for {
					o := pull(fb)
					mu.Lock()
					cause := failed
					mu.Unlock()
					if cause != nil {
						panic(&Failure{cause})
					}
					running := fibers.fork(fb, func(fb *Fiber) Value { return f(fb, o) })
					running.observe(func(c *child) {
						if c.cause != nil {
							mu.Lock()
							failed = c.cause
							mu.Unlock()
							queue.failCause(c.cause)
						}
					})
					effects.offer(fb, func(fb *Fiber) Value {
						wait(fb, running.done)
						return running.result()
					})
				}
			})
			effects.offer(fb, func(*Fiber) Value { panic(&Failure{c}) })
			effects.failCause(c)
			return Unit
		})
		return queue.take
	})
}

// channelMapAccumHalt is Effect's `Channel.mapAccum` with `onHalt`: once
// the channel ends or fails, what `onHalt` gives of the last state goes
// out first.
func channelMapAccumHalt(self *Channel, initial func(fb *Fiber) Value, f func(fb *Fiber, s, o Value) (Value, []Value), onHalt func(fb *Fiber, s Value) []Value) *Channel {
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
					if onHalt != nil {
						current = onHalt(fb, state)
					}
					if len(current) == 0 {
						panic(&Failure{c})
					}
					continue
				}
				state, current = f(fb, state, o)
			}
			next := current[0]
			current = current[1:]
			return next
		}
	})
}

// nonEmptyChunk is the values as one chunk, or none when there are none.
func nonEmptyChunk(values []Value) []Value {
	if len(values) == 0 {
		return nil
	}
	return []Value{values}
}

// streamMapAccumArray is Effect's `mapAccumArray`: `f` of the state and
// each chunk gives the next state and the elements to emit as a chunk.
func streamMapAccumArray(self *Stream, initial func(fb *Fiber) Value, f func(fb *Fiber, s Value, chunk []Value) (Value, []Value), onHalt func(fb *Fiber, s Value) []Value) *Stream {
	var last func(fb *Fiber, s Value) []Value
	if onHalt != nil {
		last = func(fb *Fiber, s Value) []Value { return nonEmptyChunk(onHalt(fb, s)) }
	}
	return &Stream{channelMapAccumHalt(self.ch, initial, func(fb *Fiber, s, o Value) (Value, []Value) {
		s, values := f(fb, s, o.([]Value))
		return s, nonEmptyChunk(values)
	}, last)}
}

// statePair is the state and the values of a Flick `(s, List b)`.
func statePair(v Value) (Value, []Value) {
	pair := List(v)
	return pair[0], List(pair[1])
}

func newLatch() *Latch { return &Latch{wake: make(chan struct{})} }

// sleepOrOpen is `Effect.raceFirst(Effect.sleep(ms), latch.await)`.
func sleepOrOpen(fb *Fiber, ms float64, l *Latch) {
	l.mu.Lock()
	if l.open {
		l.mu.Unlock()
		return
	}
	wake := l.wake
	l.mu.Unlock()
	if ms <= 0 {
		EffectYieldNow(fb)
		return
	}
	timer, cancel := clockOf(fb).timer(time.Duration(ms * float64(time.Millisecond)))
	defer cancel()
	if firstClosed(fb, timer, wake, signalOf(fb)) == 2 {
		panic(fb.state.stopped())
	}
}
