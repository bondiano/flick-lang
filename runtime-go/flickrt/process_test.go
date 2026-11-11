package flickrt

import (
	"runtime"
	"testing"
	"time"
)

// procShell is the command line in the system's shell.
func procShell(script string) Value {
	if runtime.GOOS == "windows" {
		return ChildProcessMake("cmd", []Value{"/d", "/c", script})
	}
	return ChildProcessMake("sh", []Value{"-c", script})
}

func procSlow() Value {
	if runtime.GOOS == "windows" {
		return procShell("ping -n 30 127.0.0.1 > nul")
	}
	return procShell("sleep 30")
}

func TestChildProcessExitCodeOutputAndPipeline(t *testing.T) {
	fb := rootFiber()
	if got := ChildProcessExitCode(fb, procShell("exit 3")); got != int32(3) {
		t.Fatalf("exit code %v", got)
	}
	lines := ChildProcessLines(fb, ChildProcessPipeTo(ChildProcessMake("sort", []Value{}), procShell("echo b&& echo a")))
	if got := Format(lines); got != `["a","b"]` {
		t.Fatalf("pipeline gave %s", got)
	}
	text := ChildProcessString(fb, ChildProcessSetStdin("x1\n", procShell(map[bool]string{true: "findstr x", false: "cat"}[runtime.GOOS == "windows"])))
	if got := text.(string); got != "x1\n" && got != "x1\r\n" {
		t.Fatalf("stdin echoed %q", got)
	}
}

// Killing a process ends its tree, and a closing scope stops what still runs.
func TestChildProcessKillAndScope(t *testing.T) {
	fb := rootFiber()
	var p *process
	start := time.Now()
	owned(fb, func(fb *Fiber, _ *Owner) Value {
		p = ChildProcessSpawn(fb, procSlow()).(*process)
		if !p.running() {
			t.Fatal("not running after spawn")
		}
		ChildProcessKill(fb, p)
		if p.running() {
			t.Fatal("still running after kill")
		}
		p = ChildProcessSpawn(fb, procSlow()).(*process)
		return Unit
	})
	if p.running() {
		t.Fatal("the scope left the process running")
	}
	if time.Since(start) > 10*time.Second {
		t.Fatalf("stopping took %v", time.Since(start))
	}
}

// Kill waits for the process, not for a member of its group that ignores
// the signal and still holds its stdout: that one gets a second.
func TestChildProcessKillAwaitsTheLeaderNotItsPipes(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("no signals to ignore")
	}
	fb := rootFiber()
	owned(fb, func(fb *Fiber, _ *Owner) Value {
		p := ChildProcessSpawn(fb, procShell(`(trap "" TERM; echo ready; sleep 30) & sleep 30`)).(*process)
		t.Cleanup(func() { processKill(p.cmds[0], "SIGKILL") })
		if _, ok := p.out.next(fb); !ok {
			t.Fatal("no ready line")
		}
		start := time.Now()
		ChildProcessKill(fb, p)
		if took := time.Since(start); took > 10*time.Second {
			t.Fatalf("kill took %v", took)
		}
		if p.running() {
			t.Fatal("still running after kill")
		}
		if !processGroupAlive(p.cmds[0]) {
			t.Fatal("the member ignoring SIGTERM is gone: the case tests nothing")
		}
		processKill(p.cmds[0], "SIGKILL")
		p.out.all(fb) // ends once the last holder of the pipe is gone
		return Unit
	})
}

func TestChildProcessMissingProgramIsNotFound(t *testing.T) {
	_, cause := catch(func() Value {
		return ChildProcessExitCode(rootFiber(), ChildProcessMake("flick-no-such-program", []Value{}))
	})
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("no failure: %v", cause)
	}
	reason := cause.err.(*Data).Fields[0].(*Data).Fields[0].(*Record)
	if tag := reason.Get("tag").(*Data).Tag; tag != Tag("PlatformError.SystemErrorTag.NotFound") {
		t.Fatalf("tag %s", tag)
	}
}

// Output nobody reads never blocks a child on the go target: it gathers
// in memory, where Node pauses the stream and the child waits on a full
// pipe (docs: ChildProcess in the standard library reference).
func TestChildProcessUnreadOutputDoesNotBlock(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("no head or /dev/zero")
	}
	fb := rootFiber()
	done := make(chan Value, 1)
	go func() {
		done <- owned(fb, func(fb *Fiber, _ *Owner) Value {
			p := ChildProcessSpawn(fb, procShell("head -c 1048576 /dev/zero")).(*process)
			return ChildProcessWait(fb, p)
		})
	}()
	select {
	case code := <-done:
		if code != int32(0) {
			t.Fatalf("exit code %v", code)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("a child writing 1 MB nobody read did not end")
	}
}
