package flickrt

import (
	"math"
	"sync"
)

// Streams zipped, as Effect's (Stream.ts): chunk by chunk, with indices
// and neighbours, and with the latest of each side, and their bindings.

// joinBoth is Effect's `Fiber.joinAll` of both pulls forked into the
// scope: both values, or the first failure as it comes.
func joinBoth(fb *Fiber, fibers *fiberSet, left, right Pull) (Value, Value) {
	l := fibers.fork(fb, func(fb *Fiber) Value { return left(fb) })
	r := fibers.fork(fb, func(fb *Fiber) Value { return right(fb) })
	ldone, rdone := l.done, r.done
	for ldone != nil || rdone != nil {
		switch firstClosed(fb, ldone, rdone, signalOf(fb)) {
		case 0:
			if l.cause != nil {
				panic(&Failure{l.cause})
			}
			ldone = nil
		case 1:
			if r.cause != nil {
				panic(&Failure{r.cause})
			}
			rdone = nil
		default:
			panic(fb.state.stopped())
		}
	}
	return l.value, r.value
}

// streamZipWithArray zips chunk by chunk: `f` gives the output and what is
// left of either side, which waits for the other's next chunk.
func streamZipWithArray(left, right *Stream, f func(fb *Fiber, l, r []Value) ([]Value, []Value, []Value)) *Stream {
	return &Stream{fromTransformBracket(func(fb *Fiber, _ Pull, scope, _ *Owner) Pull {
		pullLeft := toPullScoped(fb, left.ch, scope)
		pullRight := toPullScoped(fb, right.ch, scope)
		fibers := runIn(fb, scope)
		var leftArray, rightArray []Value
		return func(fb *Fiber) Value {
			var l, r []Value
			switch {
			case leftArray != nil:
				l, r = leftArray, pullRight(fb).([]Value)
			case rightArray != nil:
				l, r = pullLeft(fb).([]Value), rightArray
			default:
				lv, rv := joinBoth(fb, fibers, pullLeft, pullRight)
				l, r = lv.([]Value), rv.([]Value)
			}
			out, leftover, rightover := f(fb, l, r)
			leftArray, rightArray = nil, nil
			if len(leftover) > 0 {
				leftArray = leftover
			} else if len(rightover) > 0 {
				rightArray = rightover
			}
			return out
		}
	})}
}

// zipArrays pairs the elements by `f` up to the shorter chunk.
func zipArrays(f func(fb *Fiber, a, b Value) Value) func(fb *Fiber, l, r []Value) ([]Value, []Value, []Value) {
	return func(fb *Fiber, l, r []Value) ([]Value, []Value, []Value) {
		n := min(len(l), len(r))
		out := make([]Value, n)
		for i := range out {
			out[i] = f(fb, l[i], r[i])
		}
		return out, l[n:], r[n:]
	}
}

func StreamZipWith(that, f, s Value) Value {
	return streamZipWithArray(streamOf(s), streamOf(that), zipArrays(func(fb *Fiber, a, b Value) Value { return apply2(fb, f, a, b) }))
}

func StreamZip(that, s Value) Value {
	return streamZipWithArray(streamOf(s), streamOf(that), zipArrays(func(_ *Fiber, a, b Value) Value { return []Value{a, b} }))
}

func StreamZipFlatten(that, s Value) Value {
	return streamZipWithArray(streamOf(s), streamOf(that), zipArrays(func(_ *Fiber, a, b Value) Value {
		return append(append([]Value(nil), List(a)...), b)
	}))
}

func StreamZipWithArray(that, f, s Value) Value {
	return streamZipWithArray(streamOf(s), streamOf(that), func(fb *Fiber, l, r []Value) ([]Value, []Value, []Value) {
		t := List(apply2(fb, f, l, r))
		return List(t[0]), List(t[1]), List(t[2])
	})
}

// zipSide keeps one side's elements up to the shorter chunk.
func zipSide(left bool) func(*Fiber, []Value, []Value) ([]Value, []Value, []Value) {
	return func(_ *Fiber, l, r []Value) ([]Value, []Value, []Value) {
		n := min(len(l), len(r))
		out := r[:n:n]
		if left {
			out = l[:n:n]
		}
		return out, l[n:], r[n:]
	}
}

func StreamZipLeft(right, s Value) Value {
	return streamZipWithArray(streamOf(s), streamOf(right), zipSide(true))
}

func StreamZipRight(right, s Value) Value {
	return streamZipWithArray(streamOf(s), streamOf(right), zipSide(false))
}

func StreamZipWithIndex(s Value) Value {
	return streamTransformPull(streamOf(s), func(_ *Fiber, pull Pull, _ *Owner) Pull {
		i := int32(0)
		return func(fb *Fiber) Value {
			chunk := pull(fb).([]Value)
			out := make([]Value, len(chunk))
			for j, a := range chunk {
				out[j] = []Value{a, i}
				i++
			}
			return out
		}
	})
}

// StreamZipWithNext pairs each element with the next one, if any.
func StreamZipWithNext(s Value) Value {
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return None },
		func(_ *Fiber, acc Value, chunk []Value) (Value, []Value) {
			i := 0
			if !isSome(acc) {
				i, acc = 1, Some(chunk[0])
			}
			var pairs []Value
			for ; i < len(chunk); i++ {
				prev := value(acc)
				acc = Some(chunk[i])
				pairs = append(pairs, []Value{prev, acc})
			}
			return acc, pairs
		}, func(_ *Fiber, acc Value) []Value {
			if isSome(acc) {
				return []Value{[]Value{value(acc), None}}
			}
			return nil
		})
}

func StreamZipWithPrevious(s Value) Value {
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return None },
		func(_ *Fiber, acc Value, chunk []Value) (Value, []Value) {
			pairs := make([]Value, len(chunk))
			for i, a := range chunk {
				pairs[i] = []Value{acc, a}
				acc = Some(a)
			}
			return acc, pairs
		}, nil)
}

// StreamZipWithPreviousAndNext gives each element its neighbours.
func StreamZipWithPreviousAndNext(s Value) Value {
	type around struct{ prev, current Value }
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return around{None, None} },
		func(_ *Fiber, state Value, chunk []Value) (Value, []Value) {
			acc := state.(around)
			i := 0
			var current Value
			if !isSome(acc.current) {
				i, current = 1, chunk[0]
				acc.current = Some(current)
			} else {
				current = value(acc.current)
			}
			var triples []Value
			for ; i < len(chunk); i++ {
				acc.current = Some(chunk[i])
				triples = append(triples, []Value{acc.prev, current, acc.current})
				acc.prev = Some(current)
				current = chunk[i]
			}
			return acc, triples
		}, func(_ *Fiber, state Value) []Value {
			if acc := state.(around); isSome(acc.current) {
				return []Value{[]Value{acc.prev, value(acc.current), None}}
			}
			return nil
		})
}

// streamZipLatestAll emits the latest element of every stream, in their
// order, whenever one emits once each has; a stream that ends without
// emitting ends the zip.
func streamZipLatestAll(streams []Value) *Stream {
	return &Stream{channelSuspend(func(*Fiber) *Channel {
		var mu sync.Mutex
		latest := make([]Value, len(streams))
		emitted := map[int]bool{}
		ready, empty := newLatch(), newLatch()
		channels := make([]Value, len(streams))
		for i, s := range streams {
			each := channelMap(channelFlattenArray(streamOf(s).ch), func(fb *Fiber, a Value) Value {
				mu.Lock()
				latest[i] = a
				if !emitted[i] {
					emitted[i] = true
					if len(emitted) < len(streams) {
						mu.Unlock()
						LatchAwait(fb, ready)
						return nil
					}
					out := append([]Value(nil), latest...)
					mu.Unlock()
					LatchOpen(fb, ready)
					return []Value{out}
				}
				out := append([]Value(nil), latest...)
				mu.Unlock()
				return []Value{out}
			})
			kept := channelFilterMap(each, func(_ *Fiber, o Value) (Value, bool) { return o, o != nil })
			channels[i] = channelMapDone(kept, func(fb *Fiber, d Value) Value {
				mu.Lock()
				has := emitted[i]
				mu.Unlock()
				if !has {
					LatchOpen(fb, empty)
				}
				return Unit
			})
		}
		merged := channelMergeAll(channelFromArray(channels), math.MaxInt, 0, false)
		return channelInterruptWhen(merged, func(fb *Fiber) Value { return LatchAwait(fb, empty) })
	})}
}

func StreamZipLatestAll(streams Value) Value { return streamZipLatestAll(List(streams)) }

func StreamZipLatest(that, s Value) Value { return streamZipLatestAll([]Value{s, that}) }

func StreamZipLatestWith(that, f, s Value) Value {
	return StreamMap(Fn(func(fb *Fiber, pair Value) Value {
		p := List(pair)
		return apply2(fb, f, p[0], p[1])
	}), streamZipLatestAll([]Value{s, that}))
}
