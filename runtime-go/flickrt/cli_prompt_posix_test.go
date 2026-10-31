//go:build darwin || linux

package flickrt

import (
	"io"
	"os"
	"testing"
	"time"
)

func promptTestPipe(t *testing.T) (*promptTerminalState, *os.File) {
	t.Helper()
	input, writer, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { input.Close(); writer.Close() })
	tty, err := promptOpenTerminal(input, writer)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := promptRestoreTerminal(tty); err != nil {
			t.Errorf("restore: %v", err)
		}
	})
	return tty, writer
}

func TestPromptProcessInputPipeKeysAndEOF(t *testing.T) {
	tty, writer := promptTestPipe(t)
	if _, err := io.WriteString(writer, "é🙂\x1b[D\x7f\rnext"); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"é", "🙂", "left", "backspace", "return", "n", "e", "x", "t"} {
		value, err := promptTerminalRead(rootFiber(), tty)
		if err != nil {
			t.Fatalf("reading %q: %v", want, err)
		}
		if got := value.(*Record).Get("key").(*Record).Get("name"); got != want {
			t.Fatalf("got key %v, want %s", got, want)
		}
	}
	if _, err := promptTerminalRead(rootFiber(), tty); err != io.EOF {
		t.Fatalf("end of pipe: %v", err)
	}
}

func TestPromptProcessInputCanBeInterruptedWithoutInput(t *testing.T) {
	tty, _ := promptTestPipe(t)
	fb := rootFiber()
	started := make(chan struct{})
	original := tty.keys.read
	tty.keys.read = func(fb *Fiber, timeout int) (byte, bool, error) {
		close(started)
		return original(fb, timeout)
	}
	result := make(chan *Cause, 1)
	go func() {
		_, cause := catch(func() Value {
			value, err := promptTerminalRead(fb, tty)
			if err != nil {
				panic(err)
			}
			return value
		})
		result <- cause
	}()
	<-started
	fb.state.interrupt(73)
	select {
	case cause := <-result:
		if cause == nil || cause.kind != causeInterrupt || cause.fiber != 73 {
			t.Fatalf("lost interruption: %v", cause)
		}
	case <-time.After(time.Second):
		t.Fatal("terminal input ignored fiber interruption")
	}
}

func TestPromptProcessServiceQuitsOnCtrlC(t *testing.T) {
	native, writer := promptTestPipe(t)
	if _, err := io.WriteString(writer, "a\x03"); err != nil {
		t.Fatal(err)
	}
	tty := &promptProcessTty{native: native, readGate: make(chan struct{}, 1)}
	tty.readGate <- struct{}{}
	service := promptProcessService(tty)
	fb := rootFiber()
	if got := promptCall(fb, service, "read_key", Unit).(*Record).Get("key").(*Record).Get("name"); got != "a" {
		t.Fatalf("got key %v", got)
	}
	_, cause := catch(func() Value { return promptCall(fb, service, "read_key", Unit) })
	if cause == nil || cause.kind != causeFail || cause.err != promptQuit {
		t.Fatalf("Ctrl+C did not quit: %v", cause)
	}
}
