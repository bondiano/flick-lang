package flickrt

// More of Effect's Stream: constructors, transformations and runs of
// elements, and the byte streams of files and stdin.

import (
	"io"
	"math/big"
	"os"
	"strings"
	"sync"
)

func StreamSucceed(x Value) Value { return chunkStream([]Value{x}) }

func StreamMake(xs Value) Value { return chunkStream(List(xs)) }

// mapChunks is the stream of each chunk mapped by `f`; an empty result is
// skipped.
func mapChunks(src *Stream, f func(fb *Fiber, chunk []Value) []Value) *Stream {
	return streamTransformPull(src, func(_ *Fiber, up Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value {
			for {
				if out := f(fb, up(fb).([]Value)); len(out) > 0 {
					return out
				}
			}
		}
	})
}

func StreamMap(f, s Value) Value {
	return mapChunks(streamOf(s), func(fb *Fiber, chunk []Value) []Value {
		out := make([]Value, len(chunk))
		for i, x := range chunk {
			out[i] = Apply(fb, f, x)
		}
		return out
	})
}

func StreamFilter(p, s Value) Value {
	return mapChunks(streamOf(s), func(fb *Fiber, chunk []Value) []Value {
		var out []Value
		for _, x := range chunk {
			if Apply(fb, p, x).(bool) {
				out = append(out, x)
			}
		}
		return out
	})
}

// StreamTake is the first `n` elements; it ends without pulling more.
func StreamTake(n, s Value) Value {
	left := int(n.(int32))
	if left <= 0 {
		return streamEmpty
	}
	return streamTransformPull(streamOf(s), func(_ *Fiber, up Pull, _ *Owner) Pull {
		left := left
		return func(fb *Fiber) Value {
			if left <= 0 {
				return halt(Unit)
			}
			chunk := up(fb).([]Value)
			if len(chunk) > left {
				chunk = chunk[:left]
			}
			left -= len(chunk)
			return chunk
		}
	})
}

// StreamMergeEffect is Effect's `mergeEffect`: the task runs beside the
// stream; its failure fails the stream, and the stream's end interrupts it.
func StreamMergeEffect(task, s Value) Value {
	return streamMergeEffect(streamOf(s), func(fb *Fiber) { Run(fb, task) })
}

func StreamRunCount(fb *Fiber, s Value) Value {
	n := int32(0)
	runStream(fb, streamOf(s), func(chunk []Value) { n += int32(len(chunk)) })
	return n
}

func StreamMkString(fb *Fiber, s Value) Value {
	parts := []Value{}
	runStream(fb, streamOf(s), func(chunk []Value) { parts = append(parts, chunk...) })
	return StringJoin("", parts)
}

func StreamMkBytes(fb *Fiber, s Value) Value {
	out := []Value{}
	runStream(fb, streamOf(s), func(chunk []Value) {
		for _, x := range chunk {
			out = append(out, List(x)...)
		}
	})
	return out
}

// StreamSplitLines splits the text at `\n`, `\r\n` and `\r`, across chunks,
// as Effect's splitLines.
func StreamSplitLines(s Value) Value {
	src := streamOf(s)
	return streamOpen(func(fb *Fiber) pull {
		up := src.open(fb)
		var rest strings.Builder
		ended, cr := false, false
		return func(fb *Fiber) ([]Value, bool) {
			for !ended {
				chunk, ok := up(fb)
				if !ok {
					ended = true
					if rest.Len() > 0 {
						return []Value{rest.String()}, true
					}
					return nil, false
				}
				var lines []Value
				for _, x := range chunk {
					for _, c := range []byte(x.(string)) {
						switch {
						case c == '\n' && cr:
							cr = false
						case c == '\n' || c == '\r':
							lines = append(lines, rest.String())
							rest.Reset()
							cr = c == '\r'
						default:
							rest.WriteByte(c)
							cr = false
						}
					}
				}
				if len(lines) > 0 {
					return lines, true
				}
			}
			return nil, false
		}
	})
}

// StreamLimitBytes is the chunks of bytes while their total stays within
// the limit; past it, the stream `onLimit` instead of the rest.
func StreamLimitBytes(limit, onLimit, s Value) Value {
	max := limit.(*big.Int).Int64()
	src := streamOf(s)
	return streamOpen(func(fb *Fiber) pull {
		up, total, owner := src.open(fb), int64(0), fb.owner
		var instead pull
		return func(fb *Fiber) ([]Value, bool) {
			if instead != nil {
				return instead(fb)
			}
			chunk, ok := up(fb)
			if !ok {
				return nil, false
			}
			for i, x := range chunk {
				if total += int64(len(List(x))); total > max {
					instead = streamOf(onLimit).open(Within(fb, owner))
					if i > 0 {
						return chunk[:i], true
					}
					return instead(fb)
				}
			}
			return chunk, true
		}
	})
}

// byteSizeOption is a ByteSize option as a count, or `fallback`.
func byteSizeOption(o Value, fallback int64) int64 {
	if n, ok := some(o); ok {
		return n.(*big.Int).Int64()
	}
	return fallback
}

// StreamReadFile is the bytes of the file in chunks, closed when the
// stream ends.
func StreamReadFile(options, path Value) Value {
	o, p := record(options), path.(string)
	offset := byteSizeOption(o.Get("offset"), 0)
	left := byteSizeOption(o.Get("bytesToRead"), -1)
	size := int64(64 << 10)
	if n, ok := some(o.Get("chunkSize")); ok {
		size = int64(n.(int32))
	}
	return streamOpen(func(fb *Fiber) pull {
		f, err := os.Open(p)
		if err != nil {
			Fail(systemError(err, "readFile", "open", p))
		}
		fb.owner.add(fb, func(*Fiber, *Cause) Value { f.Close(); return Unit })
		pos := offset
		return func(fb *Fiber) ([]Value, bool) {
			want := size
			if left >= 0 {
				want = min(want, left)
			}
			if want <= 0 {
				return nil, false
			}
			buf := make([]byte, want)
			n, err := f.ReadAt(buf, pos)
			if err != nil && err != io.EOF {
				Fail(systemError(err, "readFile", "read", p))
			}
			if n == 0 {
				return nil, false
			}
			pos += int64(n)
			if left >= 0 {
				left -= int64(n)
			}
			return []Value{byteList(buf[:n])}, true
		}
	})
}

var (
	stdioNodesMu sync.Mutex
	stdioNodes   = map[*Record]bool{} // the Stdio services `Stdio.node` made
)

// StreamStdin is the bytes of stdin in chunks; it needs the Stdio of
// `Stdio.node`, and fails with NotFound under another.
// shortcut: the read blocks its goroutine and ignores interruption until input comes; read stdin on a goroutine of its own if a program interrupts it
var StreamStdin Value = streamOpen(func(fb *Fiber) pull {
	impl, ok := fb.env.get("Stdio.Stdio")
	stdioNodesMu.Lock()
	native := ok && stdioNodes[impl.(*Record)]
	stdioNodesMu.Unlock()
	if !native {
		Fail(NewData(platformErrorTag, NewData(systemReasonTag, Rec(
			"tag", Nullary(Tag("PlatformError.SystemErrorTag.NotFound")), "module_", "Stdio", "method", "stdin",
			"description", Some("the implementation of the service has no native streams"),
			"syscall", None, "pathOrDescriptor", Some("stdin")))))
	}
	buf := make([]byte, 64<<10)
	return func(fb *Fiber) ([]Value, bool) {
		defer park(fb, nil)()
		n, err := os.Stdin.Read(buf)
		if n > 0 {
			return []Value{byteList(buf[:n])}, true
		}
		if err != nil && err != io.EOF {
			Fail(systemError(err, "stdin", "read", "stdin"))
		}
		return nil, false
	}
})
