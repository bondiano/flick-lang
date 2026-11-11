package flickrt

import "strings"

// Channel's transformations beyond the core: inputs, concatenation,
// repetition, filters, scans, buffers and text.

// channelMapInput maps each element of the upstream before the channel
// reads it.
func channelMapInput(self *Channel, f func(fb *Fiber, i Value) Value) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		return self.transform(fb, func(fb *Fiber) Value { return f(fb, upstream(fb)) }, scope)
	})
}

// catchError recovers from a typed error: `f` of the first typed error of
// a failure that has one; another failure goes on.
func catchError(fb *Fiber, p Pull, f func(fb *Fiber, err Value) Value) Value {
	v, c := catch(func() Value { return p(fb) })
	if c == nil {
		return v
	}
	r := c.firstOf(causeFail)
	if r == nil {
		panic(&Failure{c})
	}
	return f(fb, r.err)
}

// channelMapInputError maps the typed errors of the upstream; its end
// goes on as it is.
func channelMapInputError(self *Channel, f func(fb *Fiber, err Value) Value) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		return self.transform(fb, func(fb *Fiber) Value {
			return catchError(fb, upstream, func(fb *Fiber, err Value) Value {
				if _, ok := err.(*done); ok {
					panic(&Failure{&Cause{kind: causeFail, err: err}})
				}
				return Fail(f(fb, err))
			})
		}, scope)
	})
}

// channelOrElseIfEmpty is the channel, or the one `f` makes of its done
// value when it ends before emitting.
func channelOrElseIfEmpty(self *Channel, f func(fb *Fiber, d Value) *Channel) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		forked := forkScope(fb, scope)
		var current Pull
		return func(fb *Fiber) Value {
			if current != nil {
				return current(fb)
			}
			pull := self.transform(fb, upstream, forked)
			current = func(fb *Fiber) Value {
				return catchDone(fb, func(fb *Fiber) Value {
					v := pull(fb)
					current = pull
					return v
				}, func(fb *Fiber, d Value) Value {
					closeScope(fb, forked, nil)
					next := f(fb, d).transform(fb, upstream, scope)
					current = next
					return next(fb)
				})
			}
			return current(fb)
		}
	})
}

// channelDrain pulls the channel to its end, emitting nothing.
func channelDrain(self *Channel) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value {
			for {
				Check(fb)
				pull(fb)
			}
		}
	})
}

// repeatLoop runs the channel again and again: each run in a scope of its
// own, closed when it ends, after which `onDone` of its done value goes on
// or halts.
func repeatLoop(self *Channel, onDone func(fb *Fiber, d Value)) *Channel {
	return fromTransform(func(_ *Fiber, upstream Pull, scope *Owner) Pull {
		var current Pull
		var runScope *Owner
		return func(fb *Fiber) Value {
			for {
				if current == nil {
					runScope = forkScope(fb, scope)
					current = self.transform(fb, upstream, runScope)
				}
				v, c := catch(func() Value { return current(fb) })
				if c == nil {
					return v
				}
				d, rest, ok := filterDone(c)
				if !ok {
					panic(&Failure{rest})
				}
				current = nil
				closeScope(fb, runScope, nil)
				onDone(fb, d)
			}
		}
	})
}

// channelRepeat runs the channel again while the schedule, given each
// done value, goes on; each run reads the step's metadata.
func channelRepeat(self *Channel, s *Schedule) *Channel {
	return channelUnwrap(func(fb *Fiber) *Channel {
		step, meta := s.stepper(fb), defaultMetadata
		withMeta := channelProvideServiceEffect(self, scheduleMetadataKey, func(*Fiber) Value { return meta.record() })
		return repeatLoop(withMeta, func(fb *Fiber, d Value) {
			m, done := step(fb, d)
			if done {
				halt(d)
			}
			meta = m
		})
	})
}

// channelSchedule steps the schedule with each element before emitting
// it; the schedule's end ends the channel with its last output.
func channelSchedule(self *Channel, s *Schedule) *Channel {
	return transformPull(self, func(fb *Fiber, pull Pull, _ *Owner) Pull {
		step := s.stepper(fb)
		return func(fb *Fiber) Value {
			o := pull(fb)
			if m, done := step(fb, o); done {
				return halt(m.output)
			}
			return o
		}
	})
}

// channelFilterMap emits what `f` passes of each element.
func channelFilterMap(self *Channel, f func(fb *Fiber, o Value) (Value, bool)) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value {
			for {
				if b, ok := f(fb, pull(fb)); ok {
					return b
				}
			}
		}
	})
}

// channelFilterMapArray keeps what `f` passes of each chunk, skipping the
// chunks it empties.
func channelFilterMapArray(self *Channel, f func(fb *Fiber, o Value) (Value, bool)) *Channel {
	return channelFilterMap(self, func(fb *Fiber, arr Value) (Value, bool) {
		passes := []Value{}
		for _, o := range arr.([]Value) {
			if b, ok := f(fb, o); ok {
				passes = append(passes, b)
			}
		}
		return passes, len(passes) > 0
	})
}

// channelScan emits the initial state and each state `f` folds.
func channelScan(self *Channel, initial Value, f func(fb *Fiber, s, o Value) Value) *Channel {
	return transformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		state, first := initial, true
		return func(fb *Fiber) Value {
			if first {
				first = false
				return state
			}
			state = f(fb, state, pull(fb))
			return state
		}
	})
}

// channelBuffer runs the channel ahead into a queue its reader takes from,
// with `offer` of each element; the queue fails as the channel does.
func channelBuffer(self *Channel, q *Queue, offer func(fb *Fiber, q *Queue, o Value), take Pull) *Channel {
	return fromTransform(func(fb *Fiber, upstream Pull, scope *Owner) Pull {
		pull := self.transform(fb, upstream, scope)
		scope.add(fb, func(*Fiber, *Cause) Value { q.shutdown(); return Unit })
		forkIn(fb, scope, func(fb *Fiber) Value {
			_, c := catch(func() Value {
				for {
					offer(fb, q, pull(fb))
				}
			})
			q.failCause(c)
			panic(&Failure{c})
		})
		return take
	})
}

// channelSplitLines is chunks of text as chunks of lines, a line split
// across chunks joined, "\r\n", "\r" and "\n" ending one.
func channelSplitLines() *Channel {
	return fromTransform(func(_ *Fiber, upstream Pull, _ *Owner) Pull {
		var builder string
		midCRLF, ended := false, false
		var leftover Value
		split := func(chunk []Value) []Value {
			var lines []Value
			push := func(segment string) {
				lines = append(lines, builder+segment)
				builder = ""
			}
			for _, x := range chunk {
				str := x.(string)
				if str == "" {
					continue
				}
				from := 0
				cr, lf := indexFrom(str, '\r', 0), indexFrom(str, '\n', 0)
				if midCRLF {
					if lf == 0 {
						from = 1
						lf = indexFrom(str, '\n', from)
					}
					midCRLF = false
				}
				for cr != -1 || lf != -1 {
					if cr == -1 || (lf != -1 && lf < cr) {
						push(str[from:lf])
						from = lf + 1
						lf = indexFrom(str, '\n', from)
						continue
					}
					push(str[from:cr])
					if len(str) == cr+1 {
						midCRLF, from, cr = true, len(str), -1
						continue
					}
					if lf == cr+1 {
						from = cr + 2
					} else {
						from = cr + 1
					}
					cr, lf = indexFrom(str, '\r', from), indexFrom(str, '\n', from)
				}
				builder += str[from:]
			}
			return lines
		}
		var pullOrFlush Pull
		pullOrFlush = func(fb *Fiber) Value {
			for {
				if ended {
					return halt(leftover)
				}
				chunk, c := catch(func() Value { return upstream(fb) })
				if c != nil {
					d, rest, ok := filterDone(c)
					if !ok {
						panic(&Failure{rest})
					}
					ended, leftover = true, d
					if builder != "" {
						last := builder
						builder, midCRLF = "", false
						return []Value{last}
					}
					return halt(d)
				}
				if lines := split(chunk.([]Value)); len(lines) > 0 {
					return lines
				}
			}
		}
		return pullOrFlush
	})
}

// indexFrom is JS's `indexOf` of an ASCII byte from `from`, -1 if absent.
func indexFrom(s string, b byte, from int) int {
	for i := from; i < len(s); i++ {
		if s[i] == b {
			return i
		}
	}
	return -1
}

// channelDecodeText decodes chunks of bytes as a streaming TextDecoder.
//
// shortcut: UTF-8 only, where Node's TextDecoder knows more encodings; decode others when a program needs them
func channelDecodeText(encoding string) *Channel {
	return fromTransform(func(_ *Fiber, upstream Pull, _ *Owner) Pull {
		switch strings.ToLower(strings.TrimSpace(encoding)) {
		case "utf-8", "utf8", "unicode-1-1-utf-8":
		default:
			panic(Defect{"RangeError: The \"" + encoding + "\" encoding is not supported"})
		}
		var d textDecoder
		return func(fb *Fiber) Value {
			chunk := upstream(fb).([]Value)
			out := make([]Value, len(chunk))
			for i, bytes := range chunk {
				out[i] = d.decode([]byte(byteString(bytes)))
			}
			return out
		}
	})
}

// channelEncodeText encodes chunks of text as UTF-8, as TextEncoder.
var channelEncodeText = fromTransform(func(_ *Fiber, upstream Pull, _ *Owner) Pull {
	return func(fb *Fiber) Value {
		chunk := upstream(fb).([]Value)
		out := make([]Value, len(chunk))
		for i, s := range chunk {
			out[i] = byteList([]byte(wellFormed(s.(string))))
		}
		return out
	}
})
