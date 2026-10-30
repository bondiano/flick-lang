package flickrt

import (
	"io"
	"iter"
	"unicode/utf8"
)

// Stream as Effect's: a description of elements pulled in chunks, run anew
// each time it is opened. Opening may acquire resources into the owner of
// the fiber it opens in, so a run opens it in a scope of its own whose
// closing releases them, as an Effect run closes the stream's scope. A
// pull fails as the stream does, by panicking with its failure.
//
// shortcut: only the operations the HTTP streams need are here; port more of Effect's Stream as programs use them on Go

// pull is the next chunk, never empty, or false once the stream ended.
type pull func(fb *Fiber) ([]Value, bool)

type Stream struct{ open func(fb *Fiber) pull }

// Channel is the one kind of Effect channel the go target builds: a
// transformation of the chunks of a stream into others.
type Channel struct {
	transform func(fb *Fiber, upstream pull) pull
}

func streamOf(v Value) *Stream { return v.(*Stream) }

// chunkStream is the stream of one chunk, or none when it is empty.
func chunkStream(chunk []Value) *Stream {
	return &Stream{open: func(*Fiber) pull {
		done := len(chunk) == 0
		return func(*Fiber) ([]Value, bool) {
			if done {
				return nil, false
			}
			done = true
			return chunk, true
		}
	}}
}

func StreamFromList(xs Value) Value { return chunkStream(List(xs)) }

// StreamFromListEffect is the list the task gives as one chunk, the task
// run when the stream is pulled.
func StreamFromListEffect(task Value) Value {
	return &Stream{open: func(fb *Fiber) pull {
		var next pull
		return func(fb *Fiber) ([]Value, bool) {
			if next == nil {
				next = chunkStream(List(Run(fb, task))).open(fb)
			}
			return next(fb)
		}
	}}
}

// streamChunkSize is Effect's chunk size of a stream of an iterable.
const streamChunkSize = 4096

// StreamFromSeq pulls the sequence's elements as the stream runs.
func StreamFromSeq(s Value) Value {
	return &Stream{open: func(fb *Fiber) pull {
		next, stop := iter.Pull(Iter(s))
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
	}}
}

// runStream runs the stream in a scope of its own, giving each chunk to
// `each` as it comes.
func runStream(fb *Fiber, s *Stream, each func(chunk []Value)) {
	owned(fb, func(fb *Fiber, _ *Owner) Value {
		next := s.open(fb)
		for {
			Check(fb)
			chunk, ok := next(fb)
			if !ok {
				return Unit
			}
			each(chunk)
		}
	})
}

func StreamRunCollect(fb *Fiber, s Value) Value {
	out := []Value{}
	runStream(fb, streamOf(s), func(chunk []Value) { out = append(out, chunk...) })
	return out
}

// StreamMapEffect runs the task of each element in turn, each result a
// chunk of its own, as Effect's sequential `mapEffect` emits them.
func StreamMapEffect(f, s Value) Value {
	src := streamOf(s)
	return &Stream{open: func(fb *Fiber) pull {
		up := src.open(fb)
		var chunk []Value
		return func(fb *Fiber) ([]Value, bool) {
			for len(chunk) == 0 {
				var ok bool
				if chunk, ok = up(fb); !ok {
					return nil, false
				}
			}
			x := chunk[0]
			chunk = chunk[1:]
			return []Value{Run(fb, Apply(f, x))}, true
		}
	}}
}

func StreamPipeThroughChannel(c, s Value) Value {
	ch, src := c.(*Channel), streamOf(s)
	return &Stream{open: func(fb *Fiber) pull { return ch.transform(fb, src.open(fb)) }}
}

// mapElements is the stream of each element mapped by the function a run
// makes, which may keep state across the run's elements.
func mapElements(src *Stream, start func() func(Value) Value) *Stream {
	return &Stream{open: func(fb *Fiber) pull {
		up, f := src.open(fb), start()
		return func(fb *Fiber) ([]Value, bool) {
			chunk, ok := up(fb)
			if !ok {
				return nil, false
			}
			out := make([]Value, len(chunk))
			for i, x := range chunk {
				out[i] = f(x)
			}
			return out, true
		}
	}}
}

// StreamEncodeText is each string's UTF-8 bytes, a lone surrogate a
// U+FFFD, as TextEncoder writes it.
func StreamEncodeText(s Value) Value {
	return mapElements(streamOf(s), func() func(Value) Value {
		return func(x Value) Value { return byteList([]byte(wellFormed(x.(string)))) }
	})
}

// StreamDecodeText decodes as a streaming TextDecoder: a leading byte
// order mark dropped, a character split across chunks decoded whole, an
// incomplete one at the end left out.
func StreamDecodeText(s Value) Value {
	return mapElements(streamOf(s), func() func(Value) Value {
		var d textDecoder
		return func(x Value) Value { return d.decode([]byte(byteString(x))) }
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
