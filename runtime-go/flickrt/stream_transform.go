package flickrt

// Stream's transformations of elements, as Effect's (Stream.ts): maps,
// taps, filters, takes and drops, and their bindings. Flick callbacks are
// curried, an effectful one gives a task, and a filter gives a `Result`.

func StreamAs(b, s Value) Value {
	return mapChunks(streamOf(s), func(_ *Fiber, chunk []Value) []Value {
		out := make([]Value, len(chunk))
		for i := range out {
			out[i] = b
		}
		return out
	})
}

func StreamMapError(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelMapError(f, c)) })
}

func StreamMapBoth(options, s Value) Value {
	o := record(options)
	return StreamMapError(o.Get("onError"), StreamMap(o.Get("onElement"), s))
}

func StreamMapArray(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel {
		return channelMap(c, func(fb *Fiber, chunk Value) Value { return Apply(fb, f, chunk) })
	})
}

func StreamMapArrayEffect(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel {
		return channelMap(c, func(fb *Fiber, chunk Value) Value { return Run(fb, Apply(fb, f, chunk)) })
	})
}

func StreamMapEffectPar(options, f, s Value) Value {
	o := record(options)
	return streamMapEffect(streamOf(s), func(fb *Fiber, a Value) Value { return Run(fb, Apply(fb, f, a)) },
		concurrencyOf(o.Get("concurrency")), o.Get("unordered").(bool))
}

func StreamFlattenEffect(s Value) Value {
	return streamMapEffect(streamOf(s), func(fb *Fiber, task Value) Value { return Run(fb, task) }, 1, false)
}

// streamResult is each element as `Ok`, then a failure's error as `Err`.
func StreamResult(s Value) Value {
	ok := StreamMap(Fn(func(_ *Fiber, a Value) Value { return Ok(a) }), s)
	return streamCatchError(streamOf(ok), func(_ *Fiber, err Value) *Stream { return chunkStream([]Value{Err(err)}) })
}

func streamTap(self *Stream, f func(fb *Fiber, a Value)) *Stream {
	return streamMapEffect(self, func(fb *Fiber, a Value) Value {
		f(fb, a)
		return a
	}, 1, false)
}

func StreamTap(f, s Value) Value {
	return streamTap(streamOf(s), func(fb *Fiber, a Value) { Run(fb, Apply(fb, f, a)) })
}

func StreamTapBoth(options, s Value) Value {
	o := record(options)
	return StreamTap(o.Get("onElement"), StreamTapError(o.Get("onError"), s))
}

func StreamFilterMap(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelFilterMapArray(c, filterOf(f)) })
}

func StreamFilterEffect(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelFilterMapArray(c, predicateEffect(f)) })
}

func StreamFilterMapEffect(f, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelFilterMapArray(c, filterEffectOf(f)) })
}

// streamTakeUntil is the elements up to the first one `stop` holds of,
// included unless `excludeLast`; it ends without pulling more.
func streamTakeUntil(self *Stream, stop func(fb *Fiber, a Value) bool, excludeLast bool) *Stream {
	return streamTransformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		done := false
		return func(fb *Fiber) Value {
			if done {
				return halt(Unit)
			}
			chunk := pull(fb).([]Value)
			for j, a := range chunk {
				if stop(fb, a) {
					done = true
					end := j + 1
					if excludeLast {
						end = j
					}
					if end == 0 {
						return halt(Unit)
					}
					return chunk[:end:end]
				}
			}
			return chunk
		}
	})
}

func StreamTakeUntil(f, s Value) Value {
	return streamTakeUntil(streamOf(s), func(fb *Fiber, a Value) bool { return Apply(fb, f, a).(bool) }, false)
}

func StreamTakeUntilEffect(f, s Value) Value {
	return streamTakeUntil(streamOf(s), func(fb *Fiber, a Value) bool { return Run(fb, Apply(fb, f, a)).(bool) }, false)
}

func StreamTakeWhileEffect(f, s Value) Value {
	return streamTakeUntil(streamOf(s), func(fb *Fiber, a Value) bool { return !Run(fb, Apply(fb, f, a)).(bool) }, true)
}

// streamTakeWhile is what `f` passes of the elements up to the first it
// refuses.
func streamTakeWhile(self *Stream, f func(fb *Fiber, a Value) (Value, bool)) *Stream {
	return streamTransformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		done := false
		return func(fb *Fiber) Value {
			for {
				if done {
					return halt(Unit)
				}
				var out []Value
				for _, a := range pull(fb).([]Value) {
					b, ok := f(fb, a)
					if !ok {
						done = true
						break
					}
					out = append(out, b)
				}
				if len(out) > 0 {
					return out
				}
			}
		}
	})
}

func StreamTakeWhile(f, s Value) Value { return streamTakeWhile(streamOf(s), predicate(f)) }

func StreamTakeWhileFilter(f, s Value) Value { return streamTakeWhile(streamOf(s), filterOf(f)) }

// StreamTakeRight keeps the last `n` elements, emitted at the end.
func StreamTakeRight(n, s Value) Value {
	count := int(n.(int32))
	if count <= 0 {
		return streamEmpty
	}
	return streamMapAccumArray(streamOf(s), func(*Fiber) Value { return &[]Value{} },
		func(_ *Fiber, state Value, chunk []Value) (Value, []Value) {
			list := state.(*[]Value)
			*list = append(*list, chunk...)
			if len(*list) > count {
				*list = append([]Value(nil), (*list)[len(*list)-count:]...)
			}
			return state, nil
		}, func(_ *Fiber, state Value) []Value { return *state.(*[]Value) })
}

func StreamDrop(n, s Value) Value {
	count := int(n.(int32))
	if count <= 0 {
		return s
	}
	return streamTransformPull(streamOf(s), func(_ *Fiber, pull Pull, _ *Owner) Pull {
		dropped := 0
		return func(fb *Fiber) Value {
			for {
				chunk := pull(fb).([]Value)
				if dropped >= count {
					return chunk
				}
				dropped += len(chunk)
				if dropped > count {
					return chunk[len(chunk)-(dropped-count):]
				}
			}
		}
	})
}

// streamDropWhile drops the elements up to the first `drop` refuses.
func streamDropWhile(self *Stream, drop func(fb *Fiber, a Value) bool) *Stream {
	return streamTransformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		dropping := true
		return func(fb *Fiber) Value {
			for dropping {
				chunk := pull(fb).([]Value)
				for i, a := range chunk {
					if !drop(fb, a) {
						dropping = false
						return chunk[i:]
					}
				}
			}
			return pull(fb)
		}
	})
}

func StreamDropWhile(f, s Value) Value {
	return streamDropWhile(streamOf(s), func(fb *Fiber, a Value) bool { return Apply(fb, f, a).(bool) })
}

func StreamDropWhileEffect(f, s Value) Value {
	return streamDropWhile(streamOf(s), func(fb *Fiber, a Value) bool { return Run(fb, Apply(fb, f, a)).(bool) })
}

func StreamDropWhileFilter(f, s Value) Value {
	return streamDropWhile(streamOf(s), func(fb *Fiber, a Value) bool { return isOk(Apply(fb, f, a)) })
}

func StreamDropUntil(f, s Value) Value {
	return StreamDrop(int32(1), streamDropWhile(streamOf(s), func(fb *Fiber, a Value) bool { return !Apply(fb, f, a).(bool) }))
}

func StreamDropUntilEffect(f, s Value) Value {
	return StreamDrop(int32(1), streamDropWhile(streamOf(s), func(fb *Fiber, a Value) bool { return !Run(fb, Apply(fb, f, a)).(bool) }))
}

// StreamDropRight holds the last `n` elements back, emitting the others.
func StreamDropRight(n, s Value) Value {
	count := int(n.(int32))
	if count <= 0 {
		return s
	}
	return streamTransformPull(streamOf(s), func(_ *Fiber, pull Pull, _ *Owner) Pull {
		var list []Value
		return func(fb *Fiber) Value {
			for {
				list = append(list, pull(fb).([]Value)...)
				if take := len(list) - count; take > 0 {
					items := list[:take:take]
					list = append([]Value(nil), list[take:]...)
					return items
				}
			}
		}
	})
}

func StreamWhen(test, s Value) Value {
	return &Stream{channelUnwrap(func(fb *Fiber) *Channel {
		if Run(fb, test).(bool) {
			return streamOf(s).ch
		}
		return channelEmpty
	})}
}

func StreamCollect(s Value) Value {
	return &Stream{channelFromEffect(func(fb *Fiber) Value { return []Value{StreamRunCollect(fb, s)} })}
}

// streamChangesWith drops each element `same` holds of with the last one
// emitted.
func streamChangesWith(self *Stream, same func(fb *Fiber, a, last Value) bool) *Stream {
	return streamTransformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		first := true
		var last Value
		return func(fb *Fiber) Value {
			for {
				chunk := pull(fb).([]Value)
				var out []Value
				i := 0
				if first {
					first, last, i = false, chunk[0], 1
					out = append(out, last)
				}
				for ; i < len(chunk); i++ {
					if a := chunk[i]; !same(fb, a, last) {
						last = a
						out = append(out, a)
					}
				}
				if len(out) > 0 {
					return out
				}
			}
		}
	})
}

func StreamChanges(s Value) Value {
	return streamChangesWith(streamOf(s), func(_ *Fiber, a, last Value) bool { return Equals(a, last) })
}

func StreamChangesWith(f, s Value) Value {
	return streamChangesWith(streamOf(s), func(fb *Fiber, a, last Value) bool { return apply2(fb, f, a, last).(bool) })
}

func StreamChangesWithEffect(f, s Value) Value {
	return streamChangesWith(streamOf(s), func(fb *Fiber, a, last Value) bool {
		return Run(fb, apply2(fb, f, a, last)).(bool)
	})
}

// StreamIntersperse puts the element between each two; a chunk after the
// first starts with it.
func StreamIntersperse(element, s Value) Value {
	return streamTransformPull(streamOf(s), func(_ *Fiber, pull Pull, _ *Owner) Pull {
		first := true
		return func(fb *Fiber) Value {
			chunk := pull(fb).([]Value)
			out := make([]Value, 0, 2*len(chunk))
			if !first {
				out = append(out, element)
			}
			first = false
			for j, a := range chunk {
				out = append(out, a)
				if j < len(chunk)-1 {
					out = append(out, element)
				}
			}
			return out
		}
	})
}

func StreamIntersperseAffixes(options, s Value) Value {
	o := record(options)
	start := chunkStream([]Value{o.Get("start")})
	middle := streamOf(StreamIntersperse(o.Get("middle"), s))
	return streamConcat(streamConcat(start, middle), chunkStream([]Value{o.Get("end")}))
}

func StreamBuffer(options, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelBufferArray(options, c)) })
}

func StreamBufferArray(options, s Value) Value {
	return onChannel(s, func(c *Channel) *Channel { return channelOf(ChannelBuffer(options, c)) })
}
