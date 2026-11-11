package flickrt

import (
	"io"
	"iter"
	"unicode/utf8"
)

// Stream is a channel of chunks, non-empty lists of the elements, ended
// with Unit. Its resources go to the scope a run gives the channel; the
// Flick tasks it runs see the lexical owner. The operations are in the
// stream_*.go files.
type Stream struct{ ch *Channel }

func streamOf(v Value) *Stream { return v.(*Stream) }

// pull is a pull of the streams written before channels: the next chunk,
// or false once the stream ended.
type pull func(fb *Fiber) ([]Value, bool)

// newPull is the Pull of chunks of an old pull; an empty chunk is skipped.
func newPull(next pull) Pull {
	return func(fb *Fiber) Value {
		for {
			chunk, ok := next(fb)
			if !ok {
				return halt(Unit)
			}
			if len(chunk) > 0 {
				return chunk
			}
		}
	}
}

// oldPull is the old pull of a Pull of chunks, ended by (nil, false).
func oldPull(p Pull) pull {
	return func(fb *Fiber) ([]Value, bool) {
		var chunk []Value
		ended := false
		catchDone(fb, func(fb *Fiber) Value { chunk = p(fb).([]Value); return nil },
			func(*Fiber, Value) Value { ended = true; return nil })
		return chunk, !ended
	}
}

// streamOpen is the stream `open` pulls, opened with the channel's scope
// as the owner of what it acquires.
func streamOpen(open func(fb *Fiber) pull) *Stream {
	return &Stream{fromTransform(func(fb *Fiber, _ Pull, scope *Owner) Pull {
		return newPull(open(Within(fb, scope)))
	})}
}

// open is the stream's pull in the owner of `fb`.
func (s *Stream) open(fb *Fiber) pull { return oldPull(toPullScoped(fb, s.ch, fb.owner)) }

func streamFromPull(f func(fb *Fiber) Pull) *Stream { return &Stream{fromPull(f)} }

// streamTransformPull is the stream of the pull `f` makes of the stream's.
func streamTransformPull(self *Stream, f func(fb *Fiber, pull Pull, scope *Owner) Pull) *Stream {
	return &Stream{fromTransform(func(fb *Fiber, _ Pull, scope *Owner) Pull {
		return f(fb, toPullScoped(fb, self.ch, scope), scope)
	})}
}

func streamTransformPullBracket(self *Stream, f func(fb *Fiber, pull Pull, scope, forked *Owner) Pull) *Stream {
	return &Stream{fromTransformBracket(func(fb *Fiber, _ Pull, scope, forked *Owner) Pull {
		return f(fb, toPullScoped(fb, self.ch, scope), scope, forked)
	})}
}

var streamEmpty = &Stream{channelEmpty}

// chunkStream is the stream of one chunk, or none when it is empty.
func chunkStream(chunk []Value) *Stream {
	if len(chunk) == 0 {
		return streamEmpty
	}
	return &Stream{channelSucceed(chunk)}
}

// streamRun runs the sink on the stream; the sink's value is the result.
func streamRun(fb *Fiber, self *Stream, sink *Sink) Value {
	scope := newOwner()
	out, cause := catch(func() Value {
		v, _ := sink.transform(fb, toPullScoped(fb, self.ch, scope), scope)
		return v
	})
	if cause = scope.close(fb, cause); cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// runStream runs the stream, giving each chunk to `each` as it comes.
func runStream(fb *Fiber, s *Stream, each func(chunk []Value)) {
	runForEach(fb, s.ch, func(fb *Fiber, chunk Value) {
		Check(fb)
		each(chunk.([]Value))
	})
}

// streamRechunk regroups the elements in chunks of `target`, the last one
// shorter.
func streamRechunk(self *Stream, target int) *Stream {
	target = max(1, target)
	return streamTransformPull(self, func(_ *Fiber, pull Pull, _ *Owner) Pull {
		var chunk, current []Value
		index, ended := 0, false
		loop := func(fb *Fiber) Value {
			for {
				if current == nil {
					arr := pull(fb).([]Value)
					switch {
					case len(chunk) == 0 && len(arr) == target:
						return arr
					case len(chunk)+len(arr) < target:
						chunk = append(chunk, arr...)
					default:
						current = arr
					}
					continue
				}
				for index < len(current) {
					chunk = append(chunk, current[index])
					index++
					if len(chunk) == target {
						out := chunk
						chunk = nil
						return out
					}
				}
				index, current = 0, nil
			}
		}
		return func(fb *Fiber) Value {
			if ended {
				return halt(Unit)
			}
			return catchDone(fb, loop, func(*Fiber, Value) Value {
				if len(chunk) == 0 {
					return halt(Unit)
				}
				out := chunk
				ended, chunk = true, nil
				return out
			})
		}
	})
}

func StreamFromChannel(ch Value) Value { return &Stream{channelOf(ch)} }
func StreamToChannel(s Value) Value    { return streamOf(s).ch }

func StreamRun(fb *Fiber, sink, s Value) Value { return streamRun(fb, streamOf(s), sinkOf(sink)) }

func StreamRunFold(fb *Fiber, initial, f, s Value) Value {
	return runFold(fb, streamOf(s).ch, initial, func(fb *Fiber, z, chunk Value) Value {
		for _, x := range chunk.([]Value) {
			z = Apply2(fb, f, z, x)
		}
		return z
	})
}

func StreamRechunk(size, s Value) Value { return streamRechunk(streamOf(s), int(size.(int32))) }

// StreamChunks emits each chunk as an element of its own.
func StreamChunks(s Value) Value {
	return &Stream{channelMap(streamOf(s).ch, func(_ *Fiber, chunk Value) Value { return []Value{chunk} })}
}

func StreamFromList(xs Value) Value { return chunkStream(List(xs)) }

// StreamFromListEffect is the list the task gives as one chunk, the task
// run when the stream is pulled.
func StreamFromListEffect(task Value) Value {
	return &Stream{channelUnwrap(func(fb *Fiber) *Channel { return chunkStream(List(Run(fb, task))).ch })}
}

// streamChunkSize is the chunk size of a stream of an iterable.
const streamChunkSize = 4096

// StreamFromSeq pulls the sequence's elements as the stream runs.
func StreamFromSeq(s Value) Value {
	return streamOpen(func(fb *Fiber) pull {
		next, stop := iter.Pull(Iter(fb, s))
		fb.owner.add(fb, func(*Fiber, *Cause) Value { stop(); return Unit })
		return func(*Fiber) ([]Value, bool) {
			var chunk []Value
			for len(chunk) < streamChunkSize {
				x, ok := next()
				if !ok {
					break
				}
				chunk = append(chunk, x)
			}
			return chunk, len(chunk) > 0
		}
	})
}

func StreamRunCollect(fb *Fiber, s Value) Value {
	out := []Value{}
	runStream(fb, streamOf(s), func(chunk []Value) { out = append(out, chunk...) })
	return out
}

// StreamMapEffect runs the task of each element in turn, each result a
// chunk of its own.
func StreamMapEffect(f, s Value) Value {
	return &Stream{channelMap(channelFlattenArray(streamOf(s).ch),
		func(fb *Fiber, x Value) Value { return []Value{Run(fb, Apply(fb, f, x))} })}
}

func StreamPipeThroughChannel(c, s Value) Value {
	return &Stream{channelPipeTo(streamOf(s).ch, channelOf(c))}
}

// mapElements is the stream of each element mapped by the function a run
// makes, which may keep state across the run's elements.
func mapElements(src *Stream, start func() func(*Fiber, Value) Value) *Stream {
	return streamTransformPull(src, func(_ *Fiber, up Pull, _ *Owner) Pull {
		f := start()
		return func(fb *Fiber) Value {
			chunk := up(fb).([]Value)
			out := make([]Value, len(chunk))
			for i, x := range chunk {
				out[i] = f(fb, x)
			}
			return out
		}
	})
}

// StreamEncodeText is each string's UTF-8 bytes, a lone surrogate a
// U+FFFD, as TextEncoder writes it.
func StreamEncodeText(s Value) Value {
	return mapElements(streamOf(s), func() func(*Fiber, Value) Value {
		return func(fb *Fiber, x Value) Value { return byteList([]byte(wellFormed(x.(string)))) }
	})
}

// StreamDecodeText decodes as a streaming TextDecoder: a leading byte
// order mark dropped, a character split across chunks decoded whole, an
// incomplete one at the end left out.
func StreamDecodeText(s Value) Value {
	return mapElements(streamOf(s), func() func(*Fiber, Value) Value {
		var d textDecoder
		return func(fb *Fiber, x Value) Value { return d.decode([]byte(byteString(x))) }
	})
}

type textDecoder struct {
	pending []byte
	started bool
}

func (d *textDecoder) decode(chunk []byte) string {
	b := append(d.pending, chunk...)
	if !d.started {
		bom := []byte("\xef\xbb\xbf")
		if len(b) < len(bom) && string(b) == string(bom[:len(b)]) {
			d.pending = b
			return ""
		}
		d.started = len(b) > 0
		if len(b) >= len(bom) && string(b[:len(bom)]) == string(bom) {
			b = b[len(bom):]
		}
	}
	cut := len(b)
	// the last lead byte, when the bytes after it do not complete it yet
	for i := len(b) - 1; i >= 0 && i >= len(b)-3; i-- {
		if c := b[i]; c&0xC0 != 0x80 {
			if c >= 0xC0 && !utf8.FullRune(b[i:]) {
				cut = i
			}
			break
		}
	}
	d.pending = append([]byte(nil), b[cut:]...)
	return decodeUTF8(b[:cut])
}

// pullReader reads a stream of chunks of bytes (`List Int`) in the fiber,
// failing as the stream does.
type pullReader struct {
	fb    *Fiber
	next  pull
	chunk []Value
	buf   []byte
}

func (r *pullReader) Read(p []byte) (int, error) {
	for len(r.buf) == 0 {
		if len(r.chunk) == 0 {
			chunk, ok := r.next(r.fb)
			if !ok {
				return 0, io.EOF
			}
			r.chunk = chunk
		}
		r.buf = []byte(byteString(r.chunk[0]))
		r.chunk = r.chunk[1:]
	}
	n := copy(p, r.buf)
	r.buf = r.buf[n:]
	return n, nil
}
