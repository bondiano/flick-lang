//go:build windows

package flickrt

import (
	"bufio"
	"errors"
	"os"
	"os/exec"
	"syscall"
	"testing"
)

// On windows a console's Ctrl+Break reaches the program as an interrupt:
// main's scopes close and it exits with 130, as on SIGINT elsewhere.
func TestCtrlBreakInterruptsMain(t *testing.T) {
	if os.Getenv("FLICK_CTRL_BREAK_CHILD") == "1" {
		Main(func(fb *Fiber) {
			fb.owner.add(fb, func(*Fiber, *Cause) Value {
				os.Stdout.WriteString("cleanup\n")
				return Unit
			})
			os.Stdout.WriteString("started\n")
			wait(fb, make(chan struct{}))
		})
	}
	cmd := exec.Command(os.Args[0], "-test.run=^TestCtrlBreakInterruptsMain$")
	cmd.Env = append(os.Environ(), "FLICK_CTRL_BREAK_CHILD=1")
	// a group of its own, so the event reaches the child and not the test
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: syscall.CREATE_NEW_PROCESS_GROUP}
	out, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	lines := bufio.NewScanner(out)
	if !lines.Scan() || lines.Text() != "started" {
		t.Fatalf("first line %q", lines.Text())
	}
	event := syscall.NewLazyDLL("kernel32.dll").NewProc("GenerateConsoleCtrlEvent")
	if ok, _, err := event.Call(syscall.CTRL_BREAK_EVENT, uintptr(cmd.Process.Pid)); ok == 0 {
		t.Fatalf("GenerateConsoleCtrlEvent: %v", err)
	}
	if !lines.Scan() || lines.Text() != "cleanup" {
		t.Fatalf("after Ctrl+Break %q, want the scope closed", lines.Text())
	}
	var exit *exec.ExitError
	if err := cmd.Wait(); !errors.As(err, &exit) || exit.ExitCode() != 130 {
		t.Fatalf("exit %v, want 130", err)
	}
}

// Windows errnos are not Unix's: a missing directory still reads as Node's
// ENOENT, whichever separator the path uses.
func TestWindowsFileErrorsReadAsNodes(t *testing.T) {
	for _, path := range []string{`missing/dir/file.txt`, `missing\dir\file.txt`} {
		_, err := os.Open(path)
		want := "ENOENT: no such file or directory, open '" + path + "'"
		if got := nodeMessage(err, "open", path); got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	}
}
