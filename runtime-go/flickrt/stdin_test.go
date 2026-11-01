package flickrt

import (
	"errors"
	"io"
	"os"
	"os/exec"
	"testing"
	"time"
)

// stdinInterrupted runs `read` on a fiber of its own, interrupts it after
// 50 ms and fails unless it ends interrupted within 1 s.
func stdinInterrupted(t *testing.T, read func(*Fiber)) {
	t.Helper()
	fb := rootFiber()
	ended := make(chan *Cause, 1)
	go func() {
		_, cause := catch(func() Value { read(fb); return Unit })
		ended <- cause
	}()
	time.Sleep(50 * time.Millisecond)
	fb.state.interrupt(7)
	select {
	case cause := <-ended:
		if cause == nil || cause.kind != causeInterrupt {
			t.Fatalf("the read ended with %v", cause)
		}
	case <-time.After(time.Second):
		t.Fatal("the read ignored its interruption for 1 s")
	}
}

func TestStdinInterruptedChunkLeavesItsInputToTheNext(t *testing.T) {
	r, w := io.Pipe()
	defer w.Close()
	p := newPump(r)
	stdinInterrupted(t, func(fb *Fiber) { p.chunk(fb) })
	go io.WriteString(w, "abc")
	if got, err := p.chunk(rootFiber()); string(got) != "abc" || err != nil {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestStdinLinesAndChunksTakeTheirParts(t *testing.T) {
	r, w := io.Pipe()
	p := newPump(r)
	go func() {
		io.WriteString(w, "ab")
		io.WriteString(w, "c\r\nde")
		io.WriteString(w, "f\nlast")
		w.Close()
	}()
	// a pipe passes each write to one read whole
	fb := rootFiber()
	if line, ok := p.line(fb); line != "abc" || !ok {
		t.Fatalf("first line %q, %v", line, ok)
	}
	if got, err := p.chunk(fb); string(got) != "de" || err != nil {
		t.Fatalf("chunk %q, %v", got, err)
	}
	for _, want := range []string{"f", "last"} {
		if line, ok := p.line(fb); line != want || !ok {
			t.Fatalf("line %q, %v, want %q", line, ok, want)
		}
	}
	if line, ok := p.line(fb); ok {
		t.Fatalf("a line %q after the end", line)
	}
}

func TestStdinByteWaitsAtMostItsTimeout(t *testing.T) {
	r, w := io.Pipe()
	defer w.Close()
	p := newPump(r)
	fb := rootFiber()
	start := time.Now()
	if _, ok, err := p.byte(fb, 20); ok || err != nil {
		t.Fatalf("a byte without input: %v, %v", ok, err)
	}
	if waited := time.Since(start); waited > time.Second {
		t.Fatalf("waited %v for 20 ms", waited)
	}
	go io.WriteString(w, "z")
	if b, ok, err := p.byte(fb, -1); b != 'z' || !ok || err != nil {
		t.Fatalf("got %q, %v, %v", b, ok, err)
	}
	stdinInterrupted(t, func(fb *Fiber) { p.byte(fb, -1) })
}

type stdinCounted struct {
	reads int
	data  string
}

func (c *stdinCounted) Read(b []byte) (int, error) {
	c.reads++
	n := copy(b, c.data)
	c.data = c.data[n:]
	if n == 0 {
		return 0, io.EOF
	}
	return n, nil
}

func TestStdinReadsItsEndOnce(t *testing.T) {
	src := &stdinCounted{data: "a"}
	p := newPump(src)
	fb := rootFiber()
	if got, err := p.chunk(fb); string(got) != "a" || err != nil {
		t.Fatalf("got %q, %v", got, err)
	}
	for range 3 {
		if got, err := p.chunk(fb); got != nil || err != nil {
			t.Fatalf("at the end: %q, %v", got, err)
		}
		if _, ok, err := p.byte(fb, -1); ok || err != io.EOF {
			t.Fatalf("a byte at the end: %v, %v", ok, err)
		}
	}
	if _, ok := p.line(fb); ok {
		t.Fatal("a line at the end")
	}
	if src.reads != 2 {
		t.Fatalf("%d reads of a source of one byte", src.reads)
	}
}

// TestStdinInterruptedReadDoesNotHoldExit re-runs the test binary as a
// process that interrupts a read of its stdin, an open pipe, and exits.
func TestStdinInterruptedReadDoesNotHoldExit(t *testing.T) {
	if os.Getenv("FLICK_STDIN_CHILD") != "" {
		stdinInterrupted(t, func(fb *Fiber) { processStdin().chunk(fb) })
		return
	}
	cmd := exec.Command(os.Args[0], "-test.run=^TestStdinInterruptedReadDoesNotHoldExit$")
	cmd.Env = append(os.Environ(), "FLICK_STDIN_CHILD=1")
	stdin, err := cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	defer stdin.Close()
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	exited := make(chan error, 1)
	go func() { exited <- cmd.Wait() }()
	select {
	case err := <-exited:
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			t.Fatalf("the child failed: %v", err)
		}
	case <-time.After(2 * time.Second):
		cmd.Process.Kill()
		t.Fatal("an interrupted read held the exit for 2 s")
	}
}
