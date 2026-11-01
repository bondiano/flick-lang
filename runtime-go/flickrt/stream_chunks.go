package flickrt

// Stream's transformations of chunks, as Effect's (Stream.ts): states
// threaded through the elements, windows and groups, and their bindings.

func StreamMapAccum(initial, f, s Value) Value {
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return initial },
		func(fb *Fiber, state Value, chunk []Value) (Value, []Value) {
			var acc []Value
			for _, a := range chunk {
				var values []Value
				state, values = statePair(apply2(fb, f, state, a))
				acc = append(acc, values...)
			}
			return state, acc
		}, nil)
}

func StreamMapAccumArray(initial, f, s Value) Value {
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return initial },
		func(fb *Fiber, state Value, chunk []Value) (Value, []Value) {
			return statePair(apply2(fb, f, state, chunk))
		}, nil)
}

// StreamMapAccumEffect emits the values of each element as a chunk.
func StreamMapAccumEffect(initial, f, s Value) Value {
	return &Stream{channelMapAccumHalt(channelFlattenArray(streamOf(s).ch), func(*Fiber) Value { return initial },
		func(fb *Fiber, state, a Value) (Value, []Value) {
			state, values := statePair(Run(fb, apply2(fb, f, state, a)))
			return state, nonEmptyChunk(values)
		}, nil)}
}

func StreamMapAccumArrayEffect(initial, f, s Value) Value {
	return &Stream{channelMapAccumHalt(streamOf(s).ch, func(*Fiber) Value { return initial },
		func(fb *Fiber, state, chunk Value) (Value, []Value) {
			state, values := statePair(Run(fb, apply2(fb, f, state, chunk)))
			return state, nonEmptyChunk(values)
		}, nil)}
}

// StreamScan emits the initial state, then the state after each element.
func StreamScan(initial, f, s Value) Value {
	states := &Stream{channelMapAccumHalt(streamOf(s).ch, func(*Fiber) Value { return initial },
		func(fb *Fiber, state, chunk Value) (Value, []Value) {
			arr := chunk.([]Value)
			out := make([]Value, len(arr))
			for i, a := range arr {
				state = apply2(fb, f, state, a)
				out[i] = state
			}
			return state, []Value{out}
		}, nil)}
	return streamConcat(chunkStream([]Value{initial}), states)
}

func StreamScanEffect(initial, f, s Value) Value {
	scanned := channelScan(channelFlattenArray(streamOf(s).ch), initial, func(fb *Fiber, state, a Value) Value {
		return Run(fb, apply2(fb, f, state, a))
	})
	return &Stream{channelMap(scanned, func(_ *Fiber, state Value) Value { return []Value{state} })}
}

// StreamAccumulate emits all the elements so far after each chunk.
func StreamAccumulate(s Value) Value {
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return []Value{} },
		func(_ *Fiber, acc Value, chunk []Value) (Value, []Value) {
			prev := acc.([]Value)
			combined := append(append(make([]Value, 0, len(prev)+len(chunk)), prev...), chunk...)
			return combined, []Value{combined}
		}, nil)
}

func StreamSliding(size, s Value) Value { return StreamSlidingSize(size, int32(1), s) }

// StreamSlidingSize emits windows of `size` elements, `step` apart; at the
// end, what is left when no window held it yet.
func StreamSlidingSize(size, step, s Value) Value {
	windowSize, stepSize := max(1, int(size.(int32))), max(1, int(step.(int32)))
	return streamTransformPull(streamOf(s), func(_ *Fiber, upstream Pull, _ *Owner) Pull {
		var cause *Cause
		var list []Value
		emitted, skip := false, 0
		return func(fb *Fiber) Value {
			if cause != nil {
				panic(&Failure{cause})
			}
			for {
				arr, c := catch(func() Value { return upstream(fb) })
				if c != nil {
					// the buffer has already advanced by step after a full window
					if len(list) == 0 || (emitted && len(list) <= windowSize-stepSize) {
						panic(&Failure{c})
					}
					cause = c
					out := list
					list = nil
					return []Value{out}
				}
				list = append(list, arr.([]Value)...)
				if skip > 0 {
					length := len(list)
					list = list[min(skip, length):]
					skip = max(0, skip-length)
				}
				if len(list) < windowSize {
					continue
				}
				emitted = true
				var windows []Value
				for len(list) >= windowSize {
					windows = append(windows, append([]Value(nil), list[:windowSize]...))
					length := len(list)
					list = list[min(stepSize, length):]
					skip = max(0, stepSize-length)
				}
				return windows
			}
		}
	})
}

// StreamSplit emits the runs of elements between those `p` holds of.
func StreamSplit(p, s Value) Value {
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return []Value(nil) },
		func(fb *Fiber, state Value, chunk []Value) (Value, []Value) {
			acc := state.([]Value)
			var out []Value
			for _, a := range chunk {
				if Apply(fb, p, a).(bool) {
					if len(acc) > 0 {
						out = append(out, acc)
						acc = nil
					}
				} else {
					acc = append(acc, a)
				}
			}
			return acc, out
		}, func(_ *Fiber, state Value) []Value { return nonEmptyChunk(state.([]Value)) })
}

func StreamGrouped(n, s Value) Value {
	return StreamChunks(streamRechunk(streamOf(s), int(n.(int32))))
}

func StreamGroupedWithin(n, d, s Value) Value {
	take := sinkOf(SinkTake(int32(max(1, int(n.(int32))))))
	return streamAggregateWithin(streamOf(s), take, scheduleOf(ScheduleSpaced(d)))
}

// StreamGroupAdjacentBy emits each run of elements of equal keys with its
// key.
func StreamGroupAdjacentBy(f, s Value) Value {
	return streamTransformPull(streamOf(s), func(_ *Fiber, pull Pull, _ *Owner) Pull {
		var key Value
		var group []Value
		done := false
		loop := func(fb *Fiber) Value {
			for {
				var out []Value
				for _, a := range pull(fb).([]Value) {
					k := Apply(fb, f, a)
					switch {
					case group == nil:
						key, group = k, []Value{a}
					case Equals(k, key):
						group = append(group, a)
					default:
						out = append(out, []Value{key, group})
						key, group = k, []Value{a}
					}
				}
				if len(out) > 0 {
					return out
				}
			}
		}
		return func(fb *Fiber) Value {
			if done {
				return halt(Unit)
			}
			return catchDone(fb, loop, func(*Fiber, Value) Value {
				done = true
				last := group
				group = nil
				if len(last) == 0 {
					return halt(Unit)
				}
				return []Value{[]Value{key, last}}
			})
		}
	})
}
