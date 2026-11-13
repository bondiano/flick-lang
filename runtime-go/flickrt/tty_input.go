package flickrt

import (
	"io"
	"os"
	"sync"
	"time"
	"unicode/utf8"
)

// The key presses of the process's stdin, decoded as Node's readline does
// with an `escapeCodeTimeout` of 50 milliseconds. It is read only while a
// prompt waits for a key, so input the prompts do not ask for stays with
// the system for whatever reads stdin next.

type ttyReader struct {
	in      io.Reader
	mu      sync.Mutex
	keys    []ttyKey
	pending []string // the characters of a key not complete yet
	partial []byte   // the bytes of a character not complete yet
	ended   bool
	reading bool
	timer   *time.Timer
	changed chan struct{} // closed when keys arrive or the input ends
}

var stdinKeys = newTtyReader(os.Stdin)

func newTtyReader(in io.Reader) *ttyReader {
	return &ttyReader{in: in, changed: make(chan struct{})}
}

// notify wakes the waiting readers; under the lock.
func (r *ttyReader) notify() {
	close(r.changed)
	r.changed = make(chan struct{})
}

// feed decodes the characters after those pending into keys; under the
// lock.
func (r *ttyReader) feed(chars []string) {
	r.pending = append(r.pending, chars...)
	for {
		k, used, ok := ttyDecodeKey(r.pending)
		if !ok {
			return
		}
		r.pending = r.pending[used:]
		if k.emit {
			r.keys = append(r.keys, k)
		}
	}
}

// completeUTF8 is how much of the bytes ends on a whole character.
func completeUTF8(b []byte) int {
	for i := len(b) - 1; i >= 0 && i >= len(b)-utf8.UTFMax; i-- {
		if !utf8.RuneStart(b[i]) {
			continue
		}
		if !utf8.FullRune(b[i:]) {
			return i
		}
		break
	}
	return len(b)
}

func (r *ttyReader) read() {
	buf := make([]byte, 4096)
	n, err := r.in.Read(buf)
	r.mu.Lock()
	defer r.mu.Unlock()
	r.reading = false
	if n > 0 {
		data := append(r.partial, buf[:n]...)
		cut := completeUTF8(data)
		r.partial = append([]byte(nil), data[cut:]...)
		chars := ttyChars(decodeUTF8(data[:cut]))
		if len(chars) > 0 && r.timer != nil {
			r.timer.Stop()
			r.timer = nil
		}
		r.feed(chars)
		// a lone escape at the end is the escape key unless more follows soon
		if len(chars) > 0 && chars[len(chars)-1] == ttyEscape {
			var timer *time.Timer
			timer = time.AfterFunc(50*time.Millisecond, func() {
				r.mu.Lock()
				defer r.mu.Unlock()
				if r.timer == timer {
					r.timer = nil
					r.feed([]string{ttyTimeout})
					r.notify()
				}
			})
			r.timer = timer
		}
	}
	if err != nil {
		r.ended = true
	}
	r.notify()
}

// next is the next key press, waiting for one; false once the input ended.
func (r *ttyReader) next(fb *Fiber) (ttyKey, bool) {
	for {
		r.mu.Lock()
		if len(r.keys) > 0 {
			k := r.keys[0]
			r.keys = r.keys[1:]
			r.mu.Unlock()
			return k, true
		}
		if r.ended && r.timer == nil {
			r.mu.Unlock()
			return ttyKey{}, false
		}
		if !r.reading && !r.ended {
			r.reading = true
			go r.read()
		}
		changed := r.changed
		r.mu.Unlock()
		wait(fb, changed)
	}
}

// userInputOf is the `Terminal.UserInput` of a key press.
func userInputOf(k ttyKey) Value {
	input := None
	if k.hasInput {
		input = Some(k.input)
	}
	return Rec("input", input, "key", Rec("name", k.name, "ctrl", k.ctrl, "meta", k.meta, "shift", k.shift))
}

// shouldQuit is Node's terminal's default: escape, Ctrl+C and Ctrl+D end
// the input after they are read.
func (k ttyKey) shouldQuit() bool {
	return k.name == "escape" || k.ctrl && (k.name == "c" || k.name == "d")
}
