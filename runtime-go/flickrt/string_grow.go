package flickrt

// Building a string by `++`, a piece at a time, as V8's ropes make cheap on
// the JS target: the string `++` last made from a buffer ends where the
// buffer's bytes end, so `++` onto it writes the piece into the buffer's
// spare room instead of copying the whole string. The bytes of a string
// made are never written again; only the room after them is.

import (
	"sync"
	"unsafe"
)

// growFrom is the length from which `++` keeps room to grow into.
const growFrom = 64

// growLimit bounds the buffers remembered; past it they are forgotten,
// and a string built further starts a buffer of its own.
const growLimit = 256

var (
	growMu sync.Mutex
	// the buffers by the start of their bytes, each with the bytes in use
	grown = map[*byte][]byte{}
)

// stringConcat is `x + y`, into the room after `x` when `x` is the latest
// string of a buffer.
func stringConcat(x, y string) string {
	if len(x)+len(y) < growFrom || len(y) == 0 || len(x) == 0 {
		return x + y
	}
	start := unsafe.StringData(x)
	growMu.Lock()
	defer growMu.Unlock()
	buf, ok := grown[start]
	if !ok || len(buf) != len(x) {
		if len(grown) >= growLimit {
			clear(grown)
		}
		buf = make([]byte, 0, 2*(len(x)+len(y)))
		buf = append(buf, x...)
	} else {
		delete(grown, start)
	}
	buf = append(buf, y...)
	grown[unsafe.SliceData(buf)] = buf
	return unsafe.String(unsafe.SliceData(buf), len(buf))
}
