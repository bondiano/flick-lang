package flickrt

// The standard input for all its readers: `Stream.stdin`, `Terminal`'s
// read_line and `Cli.Prompt`. One read is on the fly at a time, on a
// goroutine of its own, so a reader waits interruptibly: interrupted, it
// stops waiting, and what the read brings stays for the next reader, as
// Node's stdin stream buffers it. Nothing waits for that read at exit.
// shortcut: on a Windows console, a cooked read_line interrupted leaves a ReadConsole on the fly that takes the next line before Cli.Prompt's key records; try CancelSynchronousIo when a program meets it

import (
	"bytes"
	"io"
	"os"
	"strings"
	"sync"
	"time"
)

type stdinPump struct {
	src     io.Reader
	mu      sync.Mutex
	buf     []byte        // read and not yet taken
	err     error         // the end of the source, which is read no more
	ready   chan struct{} // closed as the read on the fly ends; nil with none
	scratch []byte        // the read on the fly's, there being one at a time
}

func newPump(src io.Reader) *stdinPump { return &stdinPump{src: src, scratch: make([]byte, 64<<10)} }

// processStdin is the pump of the process's stdin.
var processStdin = sync.OnceValue(func() *stdinPump { return newPump(os.Stdin) })

// pumpOf is the pump of an input: the process's one for stdin.
func pumpOf(input *os.File) *stdinPump {
	if input == os.Stdin {
		return processStdin()
	}
	return newPump(input)
}

func (p *stdinPump) fill(ready chan struct{}) {
	n, err := p.src.Read(p.scratch)
	p.mu.Lock()
	p.buf = append(p.buf, p.scratch[:n]...)
	p.err, p.ready = err, nil
	p.mu.Unlock()
	close(ready)
}

// wait waits until `has` holds of the bytes read or the source ended;
// false when `timeout`, unless negative, ran out first.
func (p *stdinPump) wait(fb *Fiber, timeout time.Duration, has func([]byte) bool) bool {
	var expired <-chan time.Time
	if timeout >= 0 {
		timer := time.NewTimer(timeout)
		defer timer.Stop()
		expired = timer.C
	}
	for {
		p.mu.Lock()
		if has(p.buf) || p.err != nil {
			p.mu.Unlock()
			return true
		}
		if p.ready == nil {
			p.ready = make(chan struct{})
			go p.fill(p.ready)
		}
		ready := p.ready
		p.mu.Unlock()
		signal := signalOf(fb)
		done := park(fb, nil)
		select {
		case <-ready:
			done()
		case <-expired:
			done()
			return false
		case <-signal:
			done()
			panic(fb.state.stopped())
		}
	}
}

func someBytes(b []byte) bool { return len(b) > 0 }

// chunk is every byte read, once there is one; nil at the end, with the
// source's error other than EOF.
func (p *stdinPump) chunk(fb *Fiber) ([]byte, error) {
	p.wait(fb, -1, someBytes)
	p.mu.Lock()
	defer p.mu.Unlock()
	out := p.buf
	p.buf = nil
	if len(out) > 0 {
		return out, nil
	}
	if p.err == io.EOF {
		return nil, nil
	}
	return nil, p.err
}

// line is the next line without its line break, the last one ending
// with the input; false at the end.
func (p *stdinPump) line(fb *Fiber) (string, bool) {
	p.wait(fb, -1, func(b []byte) bool { return bytes.IndexByte(b, '\n') >= 0 })
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.buf) == 0 {
		return "", false
	}
	end := bytes.IndexByte(p.buf, '\n') + 1
	if end == 0 {
		end = len(p.buf)
	}
	line := string(p.buf[:end])
	p.buf = p.buf[end:]
	return strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r"), true
}

// byte is the next byte, waiting at most `timeout` milliseconds unless it
// is negative: false without an error when none came.
func (p *stdinPump) byte(fb *Fiber, timeout int) (byte, bool, error) {
	if !p.wait(fb, time.Duration(timeout)*time.Millisecond, someBytes) {
		return 0, false, nil
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.buf) == 0 {
		return 0, false, p.err
	}
	b := p.buf[0]
	p.buf = p.buf[1:]
	return b, true, nil
}
